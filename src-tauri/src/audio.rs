//! Microphone capture (cpal/WASAPI), resampling and file decoding (ffmpeg).

use crate::{config::Microphone, transcribe::SAMPLE_RATE};
use anyhow::{Context, Result, anyhow, bail};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::Serialize;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_mics() -> Vec<MicInfo> {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|d| d.id().ok())
        .map(|id| id.to_string());
    let Ok(devices) = host.input_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| {
            let id = device.id().ok()?.to_string();
            Some(MicInfo {
                is_default: default_id.as_deref() == Some(id.as_str()),
                name: device.to_string(),
                id,
            })
        })
        .collect()
}

/// Find the configured mic by id, then by name. Falls back to the default device.
fn find_device(host: &cpal::Host, mic: Option<&Microphone>) -> (Option<cpal::Device>, bool) {
    let Some(mic) = mic else {
        return (host.default_input_device(), false);
    };
    if let Ok(id) = mic.id.parse::<cpal::DeviceId>()
        && let Some(device) = host.device_by_id(&id)
    {
        return (Some(device), false);
    }
    if let Ok(mut devices) = host.input_devices()
        && let Some(device) = devices.find(|d| d.to_string() == mic.name)
    {
        return (Some(device), false);
    }
    (host.default_input_device(), true)
}

pub struct Recording {
    stop_tx: mpsc::Sender<()>,
    handle: Option<JoinHandle<Result<Vec<f32>>>>,
    /// Peak level of the latest audio block, as `f32` bits.
    level: Arc<AtomicU32>,
    pub device_name: String,
    /// The configured mic wasn't found and the default one is used instead.
    pub fell_back: bool,
}

impl Recording {
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Stop recording and return the audio as 16 kHz mono.
    pub fn stop(mut self) -> Result<Vec<f32>> {
        let _ = self.stop_tx.send(());
        self.handle
            .take()
            .expect("recording already stopped")
            .join()
            .map_err(|_| anyhow!("audio capture thread panicked"))?
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        // Dropping without stop() discards the audio; still release the device.
        let _ = self.stop_tx.send(());
    }
}

pub fn start_recording(mic: Option<&Microphone>) -> Result<Recording> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(String, bool), String>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let level = Arc::new(AtomicU32::new(0));
    let mic = mic.cloned();
    let thread_level = level.clone();

    let handle =
        thread::Builder::new()
            .name("mic-capture".into())
            .spawn(move || -> Result<Vec<f32>> {
                let fail = |tx: &mpsc::Sender<Result<(String, bool), String>>,
                            err: anyhow::Error| {
                    let _ = tx.send(Err(format!("{err:#}")));
                    err
                };
                let host = cpal::default_host();
                let (device, fell_back) = find_device(&host, mic.as_ref());
                let Some(device) = device else {
                    return Err(fail(&ready_tx, anyhow!("No microphone found")));
                };
                let name = device.to_string();
                let supported = device
                    .default_input_config()
                    .with_context(|| format!("opening {name}"))
                    .map_err(|e| fail(&ready_tx, e))?;
                let rate = supported.sample_rate();
                let channels = supported.channels() as usize;
                let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(rate as usize * 30)));
                let stream = build_stream(
                    &device,
                    supported.config(),
                    supported.sample_format(),
                    channels,
                    buffer.clone(),
                    thread_level,
                )
                .with_context(|| format!("opening {name}"))
                .map_err(|e| fail(&ready_tx, e))?;
                stream
                    .play()
                    .with_context(|| format!("starting {name}"))
                    .map_err(|e| fail(&ready_tx, e))?;
                let _ = ready_tx.send(Ok((name, fell_back)));

                // Wait for stop() (or for the Recording to be dropped).
                let _ = stop_rx.recv();
                drop(stream);
                let samples = std::mem::take(&mut *buffer.lock().unwrap());
                Ok(resample(&samples, rate, SAMPLE_RATE as u32))
            })?;

    match ready_rx.recv() {
        Ok(Ok((device_name, fell_back))) => Ok(Recording {
            stop_tx,
            handle: Some(handle),
            level,
            device_name,
            fell_back,
        }),
        Ok(Err(message)) => {
            let _ = handle.join();
            Err(anyhow!(message))
        }
        Err(_) => Err(anyhow!("audio capture thread exited unexpectedly")),
    }
}

fn build_stream(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    format: cpal::SampleFormat,
    channels: usize,
    buffer: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream> {
    use cpal::SampleFormat as F;
    match format {
        F::F32 => typed_stream::<f32>(device, config, channels, buffer, level),
        F::F64 => typed_stream::<f64>(device, config, channels, buffer, level),
        F::I16 => typed_stream::<i16>(device, config, channels, buffer, level),
        F::I32 => typed_stream::<i32>(device, config, channels, buffer, level),
        F::I8 => typed_stream::<i8>(device, config, channels, buffer, level),
        F::U8 => typed_stream::<u8>(device, config, channels, buffer, level),
        F::U16 => typed_stream::<u16>(device, config, channels, buffer, level),
        F::U32 => typed_stream::<u32>(device, config, channels, buffer, level),
        other => bail!("unsupported sample format {other:?}"),
    }
}

fn typed_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    buffer: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = channels.max(1);
    let stream = device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| {
            let mut peak = 0f32;
            let mut buf = buffer.lock().unwrap();
            for frame in data.chunks(channels) {
                let sum: f32 = frame
                    .iter()
                    .map(|&s| <f32 as cpal::Sample>::from_sample(s))
                    .sum();
                let mono = sum / frame.len() as f32;
                peak = peak.max(mono.abs());
                buf.push(mono);
            }
            level.store(peak.to_bits(), Ordering::Relaxed);
        },
        |err| log::warn!("audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}

/// Windowed-sinc resampler for a whole buffer. Good enough for speech and has no dependencies.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = (input.len() as f64 * ratio).floor() as usize;
    // Low-pass just under the lower of the two Nyquist frequencies.
    let cutoff = ratio.min(1.0) * 0.94;
    let half_width = (12.0 / cutoff).ceil() as isize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let center = i as f64 / ratio;
        let first = center.floor() as isize - half_width + 1;
        let mut acc = 0.0f64;
        let mut weight = 0.0f64;
        for j in first..first + 2 * half_width {
            if j < 0 || j as usize >= input.len() {
                continue;
            }
            let x = center - j as f64;
            let sinc = if x.abs() < 1e-9 {
                1.0
            } else {
                let a = std::f64::consts::PI * x * cutoff;
                a.sin() / a
            };
            // Hann window over the kernel.
            let w = 0.5 + 0.5 * (std::f64::consts::PI * x / half_width as f64).cos();
            let k = sinc * w;
            acc += input[j as usize] as f64 * k;
            weight += k;
        }
        out.push(if weight.abs() > 1e-9 {
            (acc / weight) as f32
        } else {
            0.0
        });
    }
    out
}

/// Locate ffmpeg: the configured path, next to our exe, on PATH, then the winget links folder.
pub fn find_ffmpeg(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return path.is_file().then_some(path);
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("ffmpeg.exe"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&path).map(|dir| dir.join("ffmpeg.exe")));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join(r"Microsoft\WinGet\Links\ffmpeg.exe"));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Decode any audio or video file to 16 kHz mono f32 samples with ffmpeg.
pub fn decode_file(ffmpeg: &Path, input: &Path, cancel: &AtomicBool) -> Result<Vec<f32>> {
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut command = Command::new(ffmpeg);
    command
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(input)
        .args(["-vn", "-sn", "-dn", "-ac", "1", "-ar"])
        .arg(SAMPLE_RATE.to_string())
        .args(["-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .with_context(|| format!("starting {}", ffmpeg.display()))?;

    let mut stderr = child.stderr.take().expect("piped stderr");
    let stderr_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut bytes = Vec::new();
    let mut chunk = vec![0u8; 1 << 16];
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Cancelled");
        }
        let n = stdout.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    let status = child.wait()?;
    let errors = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        let detail = errors.lines().last().unwrap_or("unknown error").trim();
        bail!("ffmpeg could not decode the file: {detail}");
    }
    let samples: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    if samples.is_empty() {
        bail!("The file has no audio track");
    }
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_keeps_length_ratio_and_tone() {
        // 1 s of a 440 Hz tone at 48 kHz.
        let input: Vec<f32> = (0..48_000)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin() * 0.5)
            .collect();
        let out = resample(&input, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        let peak = out[1000..15_000].iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.05, "peak {peak}");
    }

    #[test]
    fn resample_same_rate_is_identity() {
        let input = vec![0.1, -0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }
}
