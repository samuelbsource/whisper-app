//! Small always-on-top status pill shown while recording and transcribing.
//!
//! The window must never take focus, or the transcript would be pasted into it.
//! It is shown and hidden with raw Win32 calls so Windows never activates it.

use crate::Core;
use serde::Serialize;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow},
    UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, HWND_TOPMOST, SW_HIDE,
        SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowLongPtrW,
        SetWindowPos, ShowWindowAsync, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    },
};

const LABEL: &str = "overlay";

/// Bumped on every show/hide so a delayed hide doesn't hide a newer message.
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize)]
pub struct OverlayState {
    /// `recording`, `transcribing`, `done`, `info` or `error`.
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
}

impl OverlayState {
    pub fn new(kind: &'static str, title: &str, detail: &str) -> Self {
        Self {
            kind,
            title: title.to_string(),
            detail: detail.to_string(),
        }
    }
    pub fn info(title: &str, detail: &str) -> Self {
        Self::new("info", title, detail)
    }
    pub fn error(title: &str, detail: &str) -> Self {
        Self::new("error", title, detail)
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    // Created visible but off-screen so the webview initialises as visible,
    // then hidden straight away with Win32 (not Tauri) so it stays "visible" to WebView2.
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
        .title("Win Whisper status")
        .inner_size(360.0, 72.0)
        .position(-32000.0, -32000.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(true)
        .build()?;
    let _ = window.set_ignore_cursor_events(true);
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let extra = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT).0 as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | extra);
            let _ = ShowWindowAsync(hwnd, SW_HIDE);
        }
    }
    Ok(())
}

pub fn show(core: &Core, state: OverlayState) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = core.app.emit_to(LABEL, "overlay", &state);
    if !core.config.get().settings.show_overlay {
        return;
    }
    if let Some(hwnd) = hwnd(&core.app) {
        show_without_focus(hwnd);
    }
    let linger_ms = match state.kind {
        "done" => Some(1600),
        "info" => Some(2500),
        "error" => Some(5000),
        _ => None,
    };
    if let Some(ms) = linger_ms {
        let app = core.app.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(ms));
            if GENERATION.load(Ordering::SeqCst) == generation
                && let Some(hwnd) = hwnd(&app)
            {
                unsafe {
                    let _ = ShowWindowAsync(hwnd, SW_HIDE);
                }
            }
        });
    }
}

pub fn hide(core: &Core) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Some(hwnd) = hwnd(&core.app) {
        unsafe {
            let _ = ShowWindowAsync(hwnd, SW_HIDE);
        }
    }
}

pub fn level(core: &Core, level: f32) {
    let _ = core.app.emit_to(LABEL, "overlay-level", level);
}

fn hwnd(app: &AppHandle) -> Option<HWND> {
    app.get_webview_window(LABEL)?.hwnd().ok()
}

/// Bottom centre of the monitor the user is working on, above the taskbar.
fn show_without_focus(hwnd: HWND) {
    unsafe {
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
        let work = info.rcWork;
        let x = work.left + (work.right - work.left - w) / 2;
        let y = work.bottom - h - (work.bottom - work.top) / 18;
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
        );
    }
}
