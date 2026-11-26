//! Global hotkeys (Windows `RegisterHotKey`) mapped to transcribers.

use crate::{Core, runtime::Event};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

#[derive(Default)]
pub struct Hotkeys {
    /// Shortcut id → transcriber id.
    by_id: Mutex<HashMap<u32, String>>,
    /// Transcriber id → why its hotkey couldn't be registered.
    errors: Mutex<HashMap<String, String>>,
    /// Set while the UI is capturing a new hotkey, so presses reach the window.
    paused: AtomicBool,
}

impl Hotkeys {
    /// Re-register every enabled transcriber's hotkey.
    ///
    /// Registration runs on the main thread (the plugin hops there and waits),
    /// so no lock of ours is held while it happens.
    pub fn apply(&self, core: &Core) {
        let shortcuts = core.app.global_shortcut();
        let _ = shortcuts.unregister_all();
        let mut by_id: HashMap<u32, String> = HashMap::new();
        let mut errors: HashMap<String, String> = HashMap::new();

        if !self.paused.load(Ordering::SeqCst) {
            let config = core.config.get();
            for t in config.transcribers.iter().filter(|t| t.enabled) {
                let accelerator = t.hotkey.trim();
                if accelerator.is_empty() {
                    continue;
                }
                let shortcut = match Shortcut::from_str(accelerator) {
                    Ok(s) => s,
                    Err(err) => {
                        errors.insert(t.id.clone(), format!("Invalid hotkey: {err}"));
                        continue;
                    }
                };
                if let Some(other) = by_id.get(&shortcut.id()) {
                    let name = config
                        .transcriber(other)
                        .map(|o| o.name.clone())
                        .unwrap_or_default();
                    errors.insert(t.id.clone(), format!("Same hotkey as \"{name}\""));
                    continue;
                }
                match shortcuts.register(shortcut) {
                    Ok(()) => {
                        by_id.insert(shortcut.id(), t.id.clone());
                    }
                    Err(err) => {
                        log::warn!("registering {accelerator}: {err}");
                        errors.insert(
                            t.id.clone(),
                            "Windows refused this hotkey. Another app or Windows itself already uses it."
                                .to_string(),
                        );
                    }
                }
            }
        }
        *self.by_id.lock().unwrap() = by_id;
        *self.errors.lock().unwrap() = errors;
    }

    pub fn set_paused(&self, core: &Core, paused: bool) {
        if self.paused.swap(paused, Ordering::SeqCst) != paused {
            self.apply(core);
        }
    }

    pub fn errors(&self) -> HashMap<String, String> {
        self.errors.lock().unwrap().clone()
    }

    fn transcriber_for(&self, shortcut_id: u32) -> Option<String> {
        self.by_id.lock().unwrap().get(&shortcut_id).cloned()
    }
}

/// Called by the global-shortcut plugin for every press and release.
pub fn on_event(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    let Some(core) = crate::try_core(app) else {
        return;
    };
    if let Some(transcriber) = core.hotkeys.transcriber_for(shortcut.id()) {
        core.runtime.send(Event::Hotkey {
            transcriber,
            pressed: event.state == ShortcutState::Pressed,
        });
    }
}
