//! Commands called from the manager window.

use crate::{
    Core,
    audio::{self, MicInfo},
    config::{Config, Profile, Settings, Transcriber},
    models::{self, ModelInfo},
    runtime::Event,
    transcribe::{self, RunOptions, SAMPLE_RATE},
    tray,
};
use anyhow::Context;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt as _;

type CmdResult<T> = Result<T, String>;

fn fail(err: anyhow::Error) -> String {
    format!("{err:#}")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    config: Config,
    models: Vec<ModelInfo>,
    hotkey_errors: HashMap<String, String>,
    activity: HashMap<String, &'static str>,
    models_dir: String,
    ffmpeg: Option<String>,
    gpu_backend: &'static str,
    whisper_version: &'static str,
    app_version: &'static str,
    autostart: bool,
}

const GPU_BACKEND: &str = if cfg!(feature = "cuda") {
    "CUDA"
} else if cfg!(feature = "vulkan") {
    "Vulkan"
} else {
    "CPU only"
};

#[derive(Serialize)]
pub struct Component {
    name: &'static str,
    license: &'static str,
    note: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notices {
    app_license: &'static str,
    components: Vec<Component>,
    /// Notice that only applies to this build flavour.
    extra: Option<&'static str>,
    third_party: &'static str,
}

/// Licence texts are embedded at build time; THIRD-PARTY-NOTICES.txt comes from scripts/notices.mjs.
const APP_LICENSE: &str = include_str!("../../LICENSE");
const THIRD_PARTY: &str = include_str!("../../THIRD-PARTY-NOTICES.txt");

const EXTRA_NOTICE: Option<&str> = if cfg!(feature = "cuda") {
    Some(
        "This build includes NVIDIA CUDA libraries (cuBLAS and cuBLASLt), redistributed under the NVIDIA CUDA \
         Toolkit End User License Agreement: https://docs.nvidia.com/cuda/eula/\n\
         cuBLAS is copyright NVIDIA Corporation.",
    )
} else {
    None
};

#[tauri::command]
pub fn get_notices() -> Notices {
    Notices {
        app_license: APP_LICENSE,
        components: vec![
            Component {
                name: "whisper.cpp",
                license: "MIT",
                note: "the speech recognition engine, used through the whisper-rs bindings",
            },
            Component {
                name: "OpenAI Whisper models",
                license: "MIT",
                note: "not bundled; downloaded on request from the ggerganov/whisper.cpp repository on Hugging Face",
            },
            Component {
                name: "Tauri",
                license: "MIT or Apache-2.0",
                note: "desktop application framework (window, tray, hotkeys)",
            },
            Component {
                name: "Svelte",
                license: "MIT",
                note: "user interface",
            },
            Component {
                name: "ffmpeg",
                license: "LGPL/GPL, depends on the build",
                note: "not bundled; if installed, it is run as a separate program to read audio and video files",
            },
        ],
        extra: EXTRA_NOTICE,
        third_party: THIRD_PARTY,
    }
}

#[tauri::command]
pub fn get_snapshot(app: AppHandle, core: State<'_, Arc<Core>>) -> Snapshot {
    let config = core.config.get();
    Snapshot {
        ffmpeg: audio::find_ffmpeg(&config.settings.ffmpeg_path).map(|p| p.display().to_string()),
        models: models::list(&core),
        hotkey_errors: core.hotkeys.errors(),
        activity: core.runtime.states(),
        models_dir: core.models_dir().display().to_string(),
        gpu_backend: GPU_BACKEND,
        whisper_version: whisper_rs::WHISPER_CPP_VERSION,
        app_version: env!("CARGO_PKG_VERSION"),
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        config,
    }
}

#[tauri::command]
pub async fn list_mics() -> Vec<MicInfo> {
    tauri::async_runtime::spawn_blocking(audio::list_mics)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn save_settings(core: State<'_, Arc<Core>>, settings: Settings) -> CmdResult<()> {
    let old = core.config.get().settings;
    core.config
        .update(|c| {
            c.settings = settings.clone();
            Ok(())
        })
        .map_err(fail)?;
    // Loaded models were created with the old GPU settings or from the old folder.
    if old.use_gpu != settings.use_gpu
        || old.flash_attn != settings.flash_attn
        || old.models_dir != settings.models_dir
    {
        core.engine.unload_all();
        tray::refresh(&core);
    }
    core.notify();
    Ok(())
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, core: State<'_, Arc<Core>>, enabled: bool) -> CmdResult<()> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|e| e.to_string())?;
    core.notify();
    Ok(())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

#[tauri::command]
pub fn save_profile(core: State<'_, Arc<Core>>, mut profile: Profile) -> CmdResult<Profile> {
    if profile.id.is_empty() {
        profile.id = new_id();
    }
    profile.name = profile.name.trim().to_string();
    if profile.name.is_empty() {
        profile.name = "Profile".into();
    }
    let saved = profile.clone();
    core.config
        .update(move |c| {
            match c.profiles.iter_mut().find(|p| p.id == profile.id) {
                Some(existing) => *existing = profile,
                None => c.profiles.push(profile),
            }
            Ok(())
        })
        .map_err(fail)?;
    core.notify();
    Ok(saved)
}

#[tauri::command]
pub fn delete_profile(core: State<'_, Arc<Core>>, id: String) -> CmdResult<()> {
    core.config
        .update(|c| {
            let users: Vec<&str> = c
                .transcribers
                .iter()
                .filter(|t| t.profile_id == id)
                .map(|t| t.name.as_str())
                .collect();
            if !users.is_empty() {
                anyhow::bail!(
                    "This profile is used by {}. Give them another profile first.",
                    users.join(", ")
                );
            }
            c.profiles.retain(|p| p.id != id);
            Ok(())
        })
        .map_err(fail)?;
    core.notify();
    Ok(())
}

#[tauri::command]
pub fn save_transcriber(
    core: State<'_, Arc<Core>>,
    mut transcriber: Transcriber,
) -> CmdResult<Transcriber> {
    if transcriber.id.is_empty() {
        transcriber.id = new_id();
    }
    transcriber.name = transcriber.name.trim().to_string();
    if transcriber.name.is_empty() {
        transcriber.name = "Transcriber".into();
    }
    if transcriber.file_output && transcriber.file_path.trim().is_empty() {
        return Err("Choose a text file to write to, or turn off file output.".into());
    }
    let saved = transcriber.clone();
    core.config
        .update(move |c| {
            match c.transcribers.iter_mut().find(|t| t.id == transcriber.id) {
                Some(existing) => *existing = transcriber,
                None => c.transcribers.push(transcriber),
            }
            Ok(())
        })
        .map_err(fail)?;
    core.hotkeys.apply(&core);
    core.notify();
    Ok(saved)
}

#[tauri::command]
pub fn delete_transcriber(core: State<'_, Arc<Core>>, id: String) -> CmdResult<()> {
    core.runtime.send(Event::Cancel {
        transcriber: id.clone(),
    });
    core.config
        .update(|c| {
            c.transcribers.retain(|t| t.id != id);
            Ok(())
        })
        .map_err(fail)?;
    core.hotkeys.apply(&core);
    core.notify();
    Ok(())
}

#[tauri::command]
pub fn set_hotkeys_paused(core: State<'_, Arc<Core>>, paused: bool) {
    core.hotkeys.set_paused(&core, paused);
}

#[tauri::command]
pub fn cancel_recording(core: State<'_, Arc<Core>>, id: String) {
    core.runtime.send(Event::Cancel { transcriber: id });
}

#[tauri::command]
pub async fn refresh_catalog(core: State<'_, Arc<Core>>) -> CmdResult<()> {
    models::refresh_catalog(&core).await.map_err(fail)
}

#[tauri::command]
pub fn download_model(core: State<'_, Arc<Core>>, file: String) {
    let core = core.inner().clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = models::download(core.clone(), file.clone()).await {
            core.report_error(&format!("Downloading {file}"), &err);
        }
    });
}

#[tauri::command]
pub fn cancel_download(core: State<'_, Arc<Core>>, file: String) {
    core.models.cancel_download(&file);
}

#[tauri::command]
pub fn delete_model(core: State<'_, Arc<Core>>, file: String) -> CmdResult<()> {
    models::delete(&core, &file).map_err(fail)?;
    core.config
        .update(|c| {
            c.keep_loaded.retain(|f| f != &file);
            Ok(())
        })
        .map_err(fail)?;
    tray::refresh(&core);
    core.notify();
    Ok(())
}

fn load_in_background(core: Arc<Core>, file: String) {
    thread::spawn(move || {
        if let Err(err) = core.engine.get(&core, &file) {
            core.report_error(&format!("Loading {file}"), &err);
        }
        tray::refresh(&core);
    });
}

#[tauri::command]
pub fn load_model(core: State<'_, Arc<Core>>, file: String) {
    load_in_background(core.inner().clone(), file);
}

#[tauri::command]
pub fn unload_model(core: State<'_, Arc<Core>>, file: String) {
    core.engine.unload(&file);
    tray::refresh(&core);
    core.notify();
}

#[tauri::command]
pub fn set_keep_loaded(core: State<'_, Arc<Core>>, file: String, keep: bool) -> CmdResult<()> {
    core.config
        .update(|c| {
            c.keep_loaded.retain(|f| f != &file);
            if keep {
                c.keep_loaded.push(file.clone());
            }
            Ok(())
        })
        .map_err(fail)?;
    core.notify();
    if keep {
        load_in_background(core.inner().clone(), file);
    }
    Ok(())
}

#[tauri::command]
pub fn open_models_dir(app: AppHandle, core: State<'_, Arc<Core>>) -> CmdResult<()> {
    let dir = core.models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_text_file(path: String, contents: String) -> CmdResult<()> {
    std::fs::write(&path, contents).map_err(|e| format!("Couldn't save {path}: {e}"))
}

#[tauri::command]
pub fn copy_text(text: String) -> CmdResult<()> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| e.to_string())
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct FileJobEvent {
    id: String,
    /// `decoding`, `loading`, `transcribing`, `done`, `cancelled` or `error`.
    stage: &'static str,
    progress: i32,
    message: String,
    text: Option<String>,
    srt: Option<String>,
    duration_secs: f32,
}

#[tauri::command]
pub fn transcribe_file(
    core: State<'_, Arc<Core>>,
    path: String,
    profile_id: String,
    max_len: u32,
) -> CmdResult<String> {
    let job_id = new_id();
    let cancel = Arc::new(AtomicBool::new(false));
    core.jobs
        .lock()
        .unwrap()
        .insert(job_id.clone(), cancel.clone());
    let core = core.inner().clone();
    let id = job_id.clone();

    thread::spawn(move || {
        let emit = {
            let core = core.clone();
            let id = id.clone();
            move |event: FileJobEvent| {
                let _ = core.app.emit(
                    "file-job",
                    FileJobEvent {
                        id: id.clone(),
                        ..event
                    },
                );
            }
        };
        let result = (|| -> anyhow::Result<(String, String, f32)> {
            let config = core.config.get();
            let profile = config
                .profile(&profile_id)
                .cloned()
                .context("Pick a profile first")?;
            let ffmpeg = audio::find_ffmpeg(&config.settings.ffmpeg_path).context(
                "ffmpeg was not found. Install it (winget install Gyan.FFmpeg) or set its path in Settings.",
            )?;
            emit(FileJobEvent {
                stage: "decoding",
                message: "Reading audio…".into(),
                ..Default::default()
            });
            let samples = audio::decode_file(&ffmpeg, Path::new(&path), &cancel)?;
            let duration = samples.len() as f32 / SAMPLE_RATE as f32;

            if core.engine.touch(&profile.model).is_none() {
                emit(FileJobEvent {
                    stage: "loading",
                    message: format!("Loading {}…", profile.model),
                    duration_secs: duration,
                    ..Default::default()
                });
            }
            let ctx = core.engine.get(&core, &profile.model)?;

            let progress_emit = emit.clone();
            let progress = Box::new(move |percent: i32| {
                progress_emit(FileJobEvent {
                    stage: "transcribing",
                    progress: percent,
                    duration_secs: duration,
                    ..Default::default()
                });
            });
            progress(0);
            let segments = transcribe::run(
                &ctx,
                &profile,
                &samples,
                RunOptions {
                    threads: config.settings.threads,
                    max_len,
                    timestamps: true,
                    progress: Some(progress),
                    cancel: Some(cancel.clone()),
                },
            )?;
            core.engine.touch(&profile.model);
            Ok((
                transcribe::to_plain_text(&segments),
                transcribe::to_srt(&segments),
                duration,
            ))
        })();

        core.jobs.lock().unwrap().remove(&id);
        let event = match result {
            Ok((text, srt, duration)) => FileJobEvent {
                stage: "done",
                progress: 100,
                text: Some(text),
                srt: Some(srt),
                duration_secs: duration,
                ..Default::default()
            },
            Err(_) if cancel.load(Ordering::Relaxed) => FileJobEvent {
                stage: "cancelled",
                ..Default::default()
            },
            Err(err) => FileJobEvent {
                stage: "error",
                message: format!("{err:#}"),
                ..Default::default()
            },
        };
        emit(event);
    });
    Ok(job_id)
}

#[tauri::command]
pub fn cancel_file_job(core: State<'_, Arc<Core>>, id: String) {
    if let Some(flag) = core.jobs.lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub fn default_output_path(input: String, extension: String) -> String {
    let path = PathBuf::from(&input);
    path.with_extension(extension.trim_start_matches('.'))
        .display()
        .to_string()
}
