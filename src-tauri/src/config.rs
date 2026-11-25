//! Persistent configuration: global settings, profiles and transcribers.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Model used by the profile created on first run.
pub const DEFAULT_MODEL: &str = "ggml-large-v3-turbo-q5_0.bin";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Unload a model after this many minutes without use. 0 = never.
    pub model_idle_minutes: u32,
    pub use_gpu: bool,
    pub flash_attn: bool,
    /// CPU threads for inference. 0 = automatic.
    pub threads: u32,
    /// Empty = `%LOCALAPPDATA%\<app>\models`.
    pub models_dir: String,
    /// Empty = look for ffmpeg on PATH.
    pub ffmpeg_path: String,
    pub start_minimized: bool,
    pub show_overlay: bool,
    pub restore_clipboard: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model_idle_minutes: 15,
            use_gpu: true,
            flash_attn: true,
            threads: 0,
            models_dir: String::new(),
            ffmpeg_path: String::new(),
            start_minimized: false,
            show_overlay: true,
            restore_clipboard: true,
        }
    }
}

/// A model plus the decoding settings to run it with.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Model file name inside the models folder, e.g. `ggml-base.en.bin`.
    pub model: String,
    /// ISO 639-1 code, or `auto`.
    pub language: String,
    /// Translate the speech to English instead of transcribing it.
    pub translate: bool,
    /// Text that primes the decoder: vocabulary, names, punctuation style.
    pub initial_prompt: String,
    /// 1 = greedy decoding, >1 = beam search with this width.
    pub beam_size: u32,
    pub temperature: f32,
    /// Don't feed earlier text back in as a prompt. Prevents repetition loops on long audio.
    pub no_context: bool,
    pub suppress_non_speech: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "New profile".into(),
            model: DEFAULT_MODEL.into(),
            language: "auto".into(),
            translate: false,
            initial_prompt: String::new(),
            beam_size: 5,
            temperature: 0.0,
            no_context: true,
            suppress_non_speech: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyMode {
    /// Press once to start, press again to stop.
    #[default]
    Toggle,
    /// Record while the key is held down.
    Hold,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum PasteMethod {
    /// Put the text on the clipboard and send Ctrl+V.
    #[default]
    Clipboard,
    /// Type the text as simulated key presses.
    Type,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Microphone {
    pub id: String,
    pub name: String,
}

/// A profile bound to a hotkey, a microphone and an output.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Transcriber {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub profile_id: String,
    /// Accelerator string understood by `global-hotkey`, e.g. `Ctrl+Super+KeyE`.
    pub hotkey: String,
    pub mode: HotkeyMode,
    /// `None` = the Windows default input device.
    pub microphone: Option<Microphone>,
    pub paste: bool,
    pub paste_method: PasteMethod,
    pub append_space: bool,
    pub file_output: bool,
    pub file_path: String,
    pub file_timestamps: bool,
}

impl Default for Transcriber {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "New transcriber".into(),
            enabled: true,
            profile_id: String::new(),
            hotkey: String::new(),
            mode: HotkeyMode::Toggle,
            microphone: None,
            paste: true,
            paste_method: PasteMethod::Clipboard,
            append_space: true,
            file_output: false,
            file_path: String::new(),
            file_timestamps: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub settings: Settings,
    pub profiles: Vec<Profile>,
    pub transcribers: Vec<Transcriber>,
    /// Models loaded at startup and never unloaded for being idle.
    pub keep_loaded: Vec<String>,
}

impl Config {
    fn first_run() -> Self {
        let profile = Profile {
            id: "default".into(),
            name: "Default".into(),
            ..Profile::default()
        };
        let transcriber = Transcriber {
            id: "dictate".into(),
            name: "Dictate".into(),
            profile_id: profile.id.clone(),
            hotkey: "Ctrl+Super+KeyE".into(),
            ..Transcriber::default()
        };
        Self {
            settings: Settings::default(),
            profiles: vec![profile],
            transcribers: vec![transcriber],
            keep_loaded: Vec::new(),
        }
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn transcriber(&self, id: &str) -> Option<&Transcriber> {
        self.transcribers.iter().find(|t| t.id == id)
    }
}

pub struct ConfigStore {
    path: PathBuf,
    data: Mutex<Config>,
}

impl ConfigStore {
    pub fn load(path: PathBuf) -> Self {
        let config = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(config) => config,
                Err(err) => {
                    // Keep the unreadable file so the user's setup isn't silently lost.
                    let backup = path.with_extension("json.broken");
                    log::error!(
                        "config is invalid ({err}); moved it to {}",
                        backup.display()
                    );
                    let _ = fs::rename(&path, &backup);
                    Config::first_run()
                }
            },
            Err(_) => Config::first_run(),
        };
        if let Err(err) = write_atomic(&path, &config) {
            log::error!("could not write config: {err:#}");
        }
        Self {
            path,
            data: Mutex::new(config),
        }
    }

    pub fn get(&self) -> Config {
        self.data.lock().unwrap().clone()
    }

    /// Change the config and save it to disk.
    pub fn update<R>(&self, f: impl FnOnce(&mut Config) -> Result<R>) -> Result<R> {
        let mut data = self.data.lock().unwrap();
        let mut next = data.clone();
        let result = f(&mut next)?;
        write_atomic(&self.path, &next)?;
        *data = next;
        Ok(result)
    }
}

fn write_atomic(path: &Path, config: &Config) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(config)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}
