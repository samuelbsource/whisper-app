//! Running whisper on 16 kHz mono samples, and formatting the result.

use crate::config::Profile;
use anyhow::{Result, bail};
use serde::Serialize;
use std::{
    fmt::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext};

pub const SAMPLE_RATE: usize = 16_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Default)]
pub struct RunOptions {
    /// 0 = automatic.
    pub threads: u32,
    /// Maximum segment length in characters, 0 = whisper's default segmentation.
    pub max_len: u32,
    pub timestamps: bool,
    pub progress: Option<Box<dyn FnMut(i32) + 'static>>,
    pub cancel: Option<Arc<AtomicBool>>,
}

pub fn auto_threads() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4)
        .clamp(1, 8)
}

pub fn run(
    ctx: &WhisperContext,
    profile: &Profile,
    samples: &[f32],
    opts: RunOptions,
) -> Result<Vec<Segment>> {
    let mut state = ctx.create_state()?;

    let strategy = if profile.beam_size > 1 {
        SamplingStrategy::BeamSearch {
            beam_size: profile.beam_size.min(16) as i32,
            patience: -1.0,
        }
    } else {
        SamplingStrategy::Greedy { best_of: 5 }
    };
    let mut params = FullParams::new(strategy);

    let language = match profile.language.trim() {
        "" => "auto",
        lang => lang,
    };
    params.set_language(Some(language));
    params.set_translate(profile.translate);
    params.set_no_context(profile.no_context);
    params.set_suppress_nst(profile.suppress_non_speech);
    params.set_temperature(profile.temperature.clamp(0.0, 1.0));
    let prompt = profile.initial_prompt.trim();
    if !prompt.is_empty() {
        params.set_initial_prompt(prompt);
    }
    let threads = if opts.threads == 0 {
        auto_threads()
    } else {
        opts.threads
    };
    params.set_n_threads(threads as i32);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_no_timestamps(!opts.timestamps);
    if opts.max_len > 0 {
        params.set_token_timestamps(true);
        params.set_max_len(opts.max_len as i32);
        params.set_split_on_word(true);
    }
    if let Some(progress) = opts.progress {
        params.set_progress_callback_safe(progress);
    }
    if let Some(cancel) = opts.cancel.clone() {
        params.set_abort_callback_safe(move || cancel.load(Ordering::Relaxed));
    }

    // whisper.cpp refuses very short inputs, so pad them with silence.
    let min_len = SAMPLE_RATE + SAMPLE_RATE / 10;
    let padded;
    let data = if samples.len() < min_len {
        padded = {
            let mut v = samples.to_vec();
            v.resize(min_len, 0.0);
            v
        };
        &padded[..]
    } else {
        samples
    };

    let result = state.full(params, data);
    if opts
        .cancel
        .as_ref()
        .is_some_and(|c| c.load(Ordering::Relaxed))
    {
        bail!("Cancelled");
    }
    result?;

    let mut segments = Vec::new();
    for segment in state.as_iter() {
        let text = segment.to_str_lossy()?.trim().to_string();
        if text.is_empty() || is_annotation(&text) {
            continue;
        }
        segments.push(Segment {
            start_ms: segment.start_timestamp() * 10,
            end_ms: segment.end_timestamp() * 10,
            text,
        });
    }
    Ok(segments)
}

/// Whisper marks silence and noise with things like `[BLANK_AUDIO]` or `(music)`.
fn is_annotation(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with('[') && t.ends_with(']'))
        || (t.starts_with('(') && t.ends_with(')'))
        || (t.starts_with('*') && t.ends_with('*'))
}

pub fn to_plain_text(segments: &[Segment]) -> String {
    segments
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Text for dictation: one line, single spaces.
pub fn to_dictation(segments: &[Segment]) -> String {
    segments
        .iter()
        .flat_map(|s| s.text.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn to_srt(segments: &[Segment]) -> String {
    let mut out = String::new();
    for (i, s) in segments.iter().enumerate() {
        let end = s.end_ms.max(s.start_ms + 1);
        let _ = write!(
            out,
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            srt_time(s.start_ms),
            srt_time(end),
            s.text
        );
    }
    out
}

fn srt_time(ms: i64) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: i64, end_ms: i64, text: &str) -> Segment {
        Segment {
            start_ms,
            end_ms,
            text: text.into(),
        }
    }

    #[test]
    fn srt_formats_times_and_indices() {
        let srt = to_srt(&[seg(0, 1500, "Hello"), seg(3_723_004, 3_725_000, "there")]);
        assert_eq!(
            srt,
            "1\n00:00:00,000 --> 00:00:01,500\nHello\n\n2\n01:02:03,004 --> 01:02:05,000\nthere\n\n"
        );
    }

    #[test]
    fn dictation_collapses_whitespace() {
        let text = to_dictation(&[seg(0, 1, " Hello  world."), seg(1, 2, "Second  line ")]);
        assert_eq!(text, "Hello world. Second line");
    }

    #[test]
    fn annotations_are_detected() {
        assert!(is_annotation("[BLANK_AUDIO]"));
        assert!(is_annotation("(upbeat music)"));
        assert!(!is_annotation("Hello (world)"));
    }
}
