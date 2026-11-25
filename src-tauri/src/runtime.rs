//! What happens when a transcriber's hotkey fires: record, transcribe, deliver.

use crate::{
    Core,
    audio::{self, Recording},
    config::{HotkeyMode, Transcriber},
    output, overlay,
    overlay::OverlayState,
    transcribe::{self, RunOptions, SAMPLE_RATE},
    tray,
};
use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, mpsc},
    thread,
    time::Duration,
};

enum Phase {
    Recording(Recording),
    Transcribing,
}

pub enum Event {
    Hotkey { transcriber: String, pressed: bool },
    Cancel { transcriber: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Idle,
    Recording,
    Busy,
}

#[derive(Default)]
pub struct Runtime {
    phases: Mutex<HashMap<String, Phase>>,
    events: OnceLock<mpsc::Sender<Event>>,
}

impl Runtime {
    pub fn send(&self, event: Event) {
        if let Some(tx) = self.events.get() {
            let _ = tx.send(event);
        }
    }

    pub fn activity(&self) -> Activity {
        let phases = self.phases.lock().unwrap();
        if phases.values().any(|p| matches!(p, Phase::Recording(_))) {
            Activity::Recording
        } else if !phases.is_empty() {
            Activity::Busy
        } else {
            Activity::Idle
        }
    }

    /// Transcriber id → `recording` / `transcribing`, for the UI.
    pub fn states(&self) -> HashMap<String, &'static str> {
        self.phases
            .lock()
            .unwrap()
            .iter()
            .map(|(id, phase)| {
                let state = match phase {
                    Phase::Recording(_) => "recording",
                    Phase::Transcribing => "transcribing",
                };
                (id.clone(), state)
            })
            .collect()
    }

    /// Drop all recordings, releasing the microphones.
    pub fn cancel_all(&self) {
        self.phases
            .lock()
            .unwrap()
            .retain(|_, phase| matches!(phase, Phase::Transcribing));
    }
}

/// Hotkey events are handled one at a time, in order, on a dedicated thread.
/// A quick tap in hold mode sends press and release almost together, and they must not race.
pub fn start_worker(core: Arc<Core>) {
    let (tx, rx) = mpsc::channel();
    let _ = core.runtime.events.set(tx);
    thread::Builder::new()
        .name("transcriber-events".into())
        .spawn(move || {
            for event in rx {
                match event {
                    Event::Hotkey {
                        transcriber,
                        pressed,
                    } => on_hotkey(&core, &transcriber, pressed),
                    Event::Cancel { transcriber } => {
                        let removed = {
                            let mut phases = core.runtime.phases.lock().unwrap();
                            match phases.get(&transcriber) {
                                Some(Phase::Recording(_)) => phases.remove(&transcriber),
                                _ => None,
                            }
                        };
                        if removed.is_some() {
                            overlay::hide(&core);
                            refresh(&core);
                        }
                    }
                }
            }
        })
        .expect("spawn transcriber worker");
}

fn on_hotkey(core: &Arc<Core>, id: &str, pressed: bool) {
    let config = core.config.get();
    let Some(t) = config.transcriber(id).cloned() else {
        return;
    };
    let mut phases = core.runtime.phases.lock().unwrap();
    let recording = phases.get(id).map(|p| matches!(p, Phase::Recording(_)));
    match (recording, t.mode, pressed) {
        (None, _, true) => {
            drop(phases);
            start(core, &t);
        }
        (Some(true), HotkeyMode::Toggle, true) | (Some(true), HotkeyMode::Hold, false) => {
            let Some(Phase::Recording(rec)) = phases.insert(t.id.clone(), Phase::Transcribing)
            else {
                return;
            };
            drop(phases);
            refresh(core);
            finish(core.clone(), t, rec);
        }
        (Some(false), _, true) => {
            drop(phases);
            overlay::show(
                core,
                OverlayState::info(&t.name, "Still transcribing the previous recording"),
            );
        }
        _ => {}
    }
}

fn start(core: &Arc<Core>, t: &Transcriber) {
    let config = core.config.get();
    let Some(profile) = config.profile(&t.profile_id).cloned() else {
        overlay::show(
            core,
            OverlayState::error(
                &t.name,
                "No profile selected. Pick one on the Transcribers page.",
            ),
        );
        return;
    };

    // Load the model while the user is still talking.
    if core.engine.touch(&profile.model).is_none() {
        let core = core.clone();
        thread::spawn(move || {
            if let Err(err) = core.engine.get(&core, &profile.model) {
                log::warn!("preloading {}: {err:#}", profile.model);
            }
        });
    }

    let rec = match audio::start_recording(t.microphone.as_ref()) {
        Ok(rec) => rec,
        Err(err) => {
            overlay::show(core, OverlayState::error(&t.name, &format!("{err:#}")));
            return;
        }
    };
    let detail = match (&t.microphone, rec.fell_back) {
        (Some(mic), true) => format!("{} not found, using {}", mic.name, rec.device_name),
        _ => match t.mode {
            HotkeyMode::Toggle => "Press the hotkey again to stop".to_string(),
            HotkeyMode::Hold => "Release the hotkey to stop".to_string(),
        },
    };
    core.runtime
        .phases
        .lock()
        .unwrap()
        .insert(t.id.clone(), Phase::Recording(rec));
    overlay::show(core, OverlayState::new("recording", &t.name, &detail));
    refresh(core);
    spawn_level_meter(core.clone(), t.id.clone());
}

/// Feed the overlay's level meter while recording.
fn spawn_level_meter(core: Arc<Core>, id: String) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(50));
            let level = match core.runtime.phases.lock().unwrap().get(&id) {
                Some(Phase::Recording(rec)) => rec.level(),
                _ => break,
            };
            overlay::level(&core, level);
        }
    });
}

fn finish(core: Arc<Core>, t: Transcriber, rec: Recording) {
    thread::spawn(move || {
        let result = transcribe_recording(&core, &t, rec);
        core.runtime.phases.lock().unwrap().remove(&t.id);
        match result {
            Ok(Some(text)) => overlay::show(&core, OverlayState::new("done", &t.name, &text)),
            Ok(None) => overlay::show(&core, OverlayState::info(&t.name, "Nothing was heard")),
            Err(err) => {
                overlay::show(&core, OverlayState::error(&t.name, &format!("{err:#}")));
                core.report_error(&t.name, &err);
            }
        }
        refresh(&core);
    });
}

fn transcribe_recording(
    core: &Arc<Core>,
    t: &Transcriber,
    rec: Recording,
) -> Result<Option<String>> {
    let samples = rec.stop()?;
    let seconds = samples.len() as f32 / SAMPLE_RATE as f32;
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    log::info!("{}: recorded {seconds:.1}s, peak {peak:.3}", t.name);
    // Accidental taps and silent (muted) mics would only make whisper hallucinate.
    if seconds < 0.3 || peak < 0.003 {
        return Ok(None);
    }

    let config = core.config.get();
    let profile = config
        .profile(&t.profile_id)
        .cloned()
        .context("the transcriber's profile was deleted")?;
    let stage = if core.engine.touch(&profile.model).is_some() {
        "Transcribing…"
    } else {
        "Loading model…"
    };
    overlay::show(core, OverlayState::new("transcribing", &t.name, stage));
    let ctx = core.engine.get(core, &profile.model)?;
    overlay::show(
        core,
        OverlayState::new("transcribing", &t.name, "Transcribing…"),
    );

    let segments = transcribe::run(
        &ctx,
        &profile,
        &samples,
        RunOptions {
            threads: config.settings.threads,
            ..Default::default()
        },
    )?;
    core.engine.touch(&profile.model);
    let text = transcribe::to_dictation(&segments);
    if text.is_empty() {
        return Ok(None);
    }
    *core.last_text.lock().unwrap() = Some(text.clone());

    let mut errors = Vec::new();
    if t.paste {
        let mut pasted = text.clone();
        if t.append_space {
            pasted.push(' ');
        }
        if let Err(err) =
            output::deliver_to_window(&pasted, t.paste_method, config.settings.restore_clipboard)
        {
            errors.push(format!("{err:#}"));
        }
    }
    if t.file_output
        && !t.file_path.trim().is_empty()
        && let Err(err) = output::append_to_file(&t.file_path, &text, t.file_timestamps)
    {
        errors.push(format!("{err:#}"));
    }
    if !errors.is_empty() {
        anyhow::bail!(errors.join("; "));
    }
    Ok(Some(text))
}

fn refresh(core: &Core) {
    tray::refresh(core);
    core.notify();
}
