//! Delivering text: paste into the focused window, type it, or append to a file.

use crate::config::PasteMethod;
use anyhow::{Context, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant},
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU,
    VK_LSHIFT, VK_LWIN, VK_RCONTROL, VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN,
};

const VK_V: VIRTUAL_KEY = VIRTUAL_KEY(0x56);

pub fn deliver_to_window(text: &str, method: PasteMethod, restore_clipboard: bool) -> Result<()> {
    // The user may still be holding the hotkey's modifiers. Sending Ctrl+V while Win is
    // down would trigger a Windows shortcut instead of pasting.
    wait_for_modifiers_released(Duration::from_secs(5));
    match method {
        PasteMethod::Clipboard => paste_via_clipboard(text, restore_clipboard),
        PasteMethod::Type => {
            type_text(text);
            Ok(())
        }
    }
}

fn modifiers_down() -> bool {
    [
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ]
    .iter()
    .any(|vk| unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0)
}

fn wait_for_modifiers_released(timeout: Duration) {
    let started = Instant::now();
    while modifiers_down() && started.elapsed() < timeout {
        thread::sleep(Duration::from_millis(20));
    }
}

fn paste_via_clipboard(text: &str, restore: bool) -> Result<()> {
    use arboard::SetExtWindows;
    let mut clipboard = arboard::Clipboard::new().context("opening the clipboard")?;
    let previous = if restore {
        clipboard.get_text().ok()
    } else {
        None
    };
    clipboard
        .set()
        .exclude_from_history()
        .exclude_from_cloud()
        .text(text)
        .context("writing to the clipboard")?;
    thread::sleep(Duration::from_millis(30));
    send_keys(&[
        key(VK_CONTROL, 0, KEYBD_EVENT_FLAGS(0)),
        key(VK_V, 0, KEYBD_EVENT_FLAGS(0)),
        key(VK_V, 0, KEYEVENTF_KEYUP),
        key(VK_CONTROL, 0, KEYEVENTF_KEYUP),
    ]);
    if let Some(previous) = previous {
        // Give the target app time to read the clipboard before putting the old text back.
        thread::sleep(Duration::from_millis(500));
        let _ = clipboard.set().exclude_from_history().text(previous);
    }
    Ok(())
}

fn type_text(text: &str) {
    let mut inputs = Vec::new();
    for ch in text.chars() {
        match ch {
            '\r' => {}
            '\n' => {
                inputs.push(key(VK_RETURN, 0, KEYBD_EVENT_FLAGS(0)));
                inputs.push(key(VK_RETURN, 0, KEYEVENTF_KEYUP));
            }
            _ => {
                let mut units = [0u16; 2];
                for unit in ch.encode_utf16(&mut units) {
                    inputs.push(key(VIRTUAL_KEY(0), *unit, KEYEVENTF_UNICODE));
                    inputs.push(key(
                        VIRTUAL_KEY(0),
                        *unit,
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                    ));
                }
            }
        }
    }
    // Some apps drop characters that arrive in one huge burst.
    for chunk in inputs.chunks(64) {
        send_keys(chunk);
        thread::sleep(Duration::from_millis(5));
    }
}

fn key(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send_keys(inputs: &[INPUT]) {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if (sent as usize) < inputs.len() {
        // Happens when the target window runs as administrator (UIPI) or input is blocked.
        log::warn!("SendInput delivered {sent} of {} key events", inputs.len());
    }
}

pub fn append_to_file(path: &str, text: &str, timestamps: bool) -> Result<()> {
    let path = Path::new(path.trim());
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let line = if timestamps {
        format!(
            "[{}] {text}\r\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        )
    } else {
        format!("{text}\r\n")
    };
    file.write_all(line.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}
