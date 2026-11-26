mod audio;
mod commands;
mod config;
mod engine;
mod hotkeys;
mod models;
mod output;
mod overlay;
mod runtime;
mod transcribe;
mod tray;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
    thread,
};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

/// Shared application state.
pub struct Core {
    pub app: AppHandle,
    pub config: config::ConfigStore,
    pub models: models::ModelStore,
    pub engine: engine::Engine,
    pub runtime: runtime::Runtime,
    pub hotkeys: hotkeys::Hotkeys,
    /// Running file transcriptions → cancel flag.
    pub jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub last_text: Mutex<Option<String>>,
    data_dir: PathBuf,
}

impl Core {
    pub fn models_dir(&self) -> PathBuf {
        let configured = self.config.get().settings.models_dir;
        match configured.trim() {
            "" => self.data_dir.join("models"),
            dir => PathBuf::from(dir),
        }
    }

    /// Tell the manager window that backend state changed.
    pub fn notify(&self) {
        let _ = self.app.emit("state-changed", ());
    }

    pub fn report_error(&self, context: &str, err: &anyhow::Error) {
        log::error!("{context}: {err:#}");
        let _ = self.app.emit("app-error", format!("{context}: {err:#}"));
    }

    /// Release microphones and free all model memory.
    pub fn shutdown(&self) {
        self.runtime.cancel_all();
        self.engine.unload_all();
    }
}

pub(crate) fn core(app: &AppHandle) -> Arc<Core> {
    app.state::<Arc<Core>>().inner().clone()
}

pub(crate) fn try_core(app: &AppHandle) -> Option<Arc<Core>> {
    app.try_state::<Arc<Core>>().map(|s| s.inner().clone())
}

fn setup(app: &mut tauri::App) -> anyhow::Result<()> {
    whisper_rs::install_logging_hooks();
    let handle = app.handle().clone();
    let config_dir = app.path().app_config_dir()?;
    let data_dir = app.path().app_local_data_dir()?;
    std::fs::create_dir_all(&data_dir)?;

    let core = Arc::new(Core {
        app: handle.clone(),
        config: config::ConfigStore::load(config_dir.join("config.json")),
        models: models::ModelStore::new(data_dir.join("catalog.json")),
        engine: engine::Engine::default(),
        runtime: runtime::Runtime::default(),
        hotkeys: hotkeys::Hotkeys::default(),
        jobs: Mutex::new(HashMap::new()),
        last_text: Mutex::new(None),
        data_dir,
    });
    app.manage(core.clone());

    runtime::start_worker(core.clone());
    tray::create(&handle)?;
    overlay::create(&handle)?;
    core.hotkeys.apply(&core);
    engine::spawn_idle_sweeper(core.clone());

    let preload = core.clone();
    thread::spawn(move || {
        for file in preload.config.get().keep_loaded {
            if let Err(err) = preload.engine.get(&preload, &file) {
                preload.report_error(&format!("Loading {file}"), &err);
            }
        }
        tray::refresh(&preload);
    });

    let catalog = core.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = models::refresh_catalog(&catalog).await {
            log::warn!("could not refresh the model list: {err:#}");
        }
    });

    let minimized =
        std::env::args().any(|a| a == "--minimized") || core.config.get().settings.start_minimized;
    if !minimized {
        tray::show_main(&handle);
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("whisper_rs", log::LevelFilter::Warn)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(hotkeys::on_event)
                .build(),
        )
        .setup(|app| setup(app).map_err(|err| -> Box<dyn std::error::Error> { err.into() }))
        .on_window_event(|window, event| {
            // Closing the manager only hides it; the tray keeps the app running.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.label() == "main"
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_notices,
            commands::list_mics,
            commands::save_settings,
            commands::set_autostart,
            commands::save_profile,
            commands::delete_profile,
            commands::save_transcriber,
            commands::delete_transcriber,
            commands::set_hotkeys_paused,
            commands::cancel_recording,
            commands::refresh_catalog,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::load_model,
            commands::unload_model,
            commands::set_keep_loaded,
            commands::open_models_dir,
            commands::write_text_file,
            commands::copy_text,
            commands::transcribe_file,
            commands::cancel_file_job,
            commands::default_output_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building the app");

    app.run(|app, event| match event {
        // Only "Quit" in the tray menu ends the app.
        RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        RunEvent::Exit => {
            if let Some(core) = try_core(app) {
                core.shutdown();
            }
        }
        _ => {}
    });
}
