//! Keeps whisper models in memory, loads them on demand and unloads idle ones.

use crate::Core;
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use whisper_rs::{WhisperContext, WhisperContextParameters};

struct Loaded {
    ctx: Arc<WhisperContext>,
    last_used: Instant,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelRuntime {
    /// `unloaded`, `loading` or `loaded`.
    pub state: &'static str,
    pub idle_secs: u64,
}

#[derive(Default)]
pub struct Engine {
    loaded: Mutex<HashMap<String, Loaded>>,
    loading: Mutex<HashSet<String>>,
    /// Serialises loads so the same model is never read twice at once.
    load_lock: Mutex<()>,
}

impl Engine {
    /// Return the model, loading it first if needed. Blocks while loading.
    pub fn get(&self, core: &Core, file: &str) -> Result<Arc<WhisperContext>> {
        if let Some(ctx) = self.touch(file) {
            return Ok(ctx);
        }
        let _guard = self.load_lock.lock().unwrap();
        if let Some(ctx) = self.touch(file) {
            return Ok(ctx);
        }

        let path = core.models_dir().join(file);
        if !path.is_file() {
            bail!("Model {file} is not downloaded. Get it on the Models page.");
        }
        let settings = core.config.get().settings;
        let mut params = WhisperContextParameters::default();
        params
            .use_gpu(settings.use_gpu)
            .flash_attn(settings.flash_attn);

        self.loading.lock().unwrap().insert(file.to_string());
        core.notify();
        let started = Instant::now();
        let result = WhisperContext::new_with_params(&path, params)
            .with_context(|| format!("loading {file}"));
        self.loading.lock().unwrap().remove(file);

        let ctx = match result {
            Ok(ctx) => Arc::new(ctx),
            Err(err) => {
                core.notify();
                return Err(err);
            }
        };
        log::info!("loaded {file} in {:.1}s", started.elapsed().as_secs_f32());
        self.loaded.lock().unwrap().insert(
            file.to_string(),
            Loaded {
                ctx: ctx.clone(),
                last_used: Instant::now(),
            },
        );
        core.notify();
        Ok(ctx)
    }

    /// Mark a loaded model as just used and return it.
    pub fn touch(&self, file: &str) -> Option<Arc<WhisperContext>> {
        let mut loaded = self.loaded.lock().unwrap();
        loaded.get_mut(file).map(|entry| {
            entry.last_used = Instant::now();
            entry.ctx.clone()
        })
    }

    /// Drop a model. Transcriptions still using it finish first, since they hold their own `Arc`.
    pub fn unload(&self, file: &str) -> bool {
        self.loaded.lock().unwrap().remove(file).is_some()
    }

    pub fn unload_all(&self) {
        self.loaded.lock().unwrap().clear();
    }

    pub fn has_loaded(&self) -> bool {
        !self.loaded.lock().unwrap().is_empty()
    }

    pub fn runtime(&self, file: &str) -> ModelRuntime {
        if self.loading.lock().unwrap().contains(file) {
            return ModelRuntime {
                state: "loading",
                idle_secs: 0,
            };
        }
        match self.loaded.lock().unwrap().get(file) {
            Some(entry) => ModelRuntime {
                state: "loaded",
                idle_secs: entry.last_used.elapsed().as_secs(),
            },
            None => ModelRuntime {
                state: "unloaded",
                idle_secs: 0,
            },
        }
    }

    /// Unload models idle for longer than `idle`, except pinned and in-use ones.
    fn sweep(&self, idle: Duration, keep: &[String]) -> Vec<String> {
        let mut loaded = self.loaded.lock().unwrap();
        let expired: Vec<String> = loaded
            .iter()
            .filter(|(file, entry)| {
                !keep.contains(file)
                    && entry.last_used.elapsed() >= idle
                    && Arc::strong_count(&entry.ctx) == 1
            })
            .map(|(file, _)| file.clone())
            .collect();
        for file in &expired {
            loaded.remove(file);
        }
        expired
    }
}

pub fn spawn_idle_sweeper(core: Arc<Core>) {
    thread::Builder::new()
        .name("model-sweeper".into())
        .spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(10));
                let config = core.config.get();
                let minutes = config.settings.model_idle_minutes;
                if minutes == 0 {
                    continue;
                }
                let expired = core.engine.sweep(
                    Duration::from_secs(minutes as u64 * 60),
                    &config.keep_loaded,
                );
                if !expired.is_empty() {
                    log::info!("unloaded idle models: {}", expired.join(", "));
                    core.notify();
                    crate::tray::refresh(&core);
                }
            }
        })
        .expect("spawn model sweeper");
}
