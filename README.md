# whisper-app
Windows rust based application for transcribing using Whisper. Built for personal use, but maybe someone else will also find it useful. Especially since paid apps using whisper has been popping up everywhere, and whisper is a free model, perfectly capable of running locally even on a low end laptop.

Everything runs on your PC. No account, no cloud, no subscription. Audio never leaves your computer; the only
network traffic is downloading the models you choose.

## What it does

- **Dictate anywhere.** Press a global hotkey, speak, and the text is pasted into the window you were using
  (or typed key by key, or appended to a text file).
- **A floating bubble** shows when it is recording, how loud your mic is, and when it is transcribing, so you
  always know what the app is doing.
- **Transcribers:** a hotkey + a profile + a microphone + an output. For example, `Ctrl+Win+D` dictates English
  with your headset mic, and `Ctrl+[` dictates Polish with your webcam mic.
- **Profiles:** a model plus language and decoding settings.
- **Transcribe audio and video files** to plain text, SRT subtitles, or both.
- **Models are downloaded in the app** and kept in memory only while needed: they load on first use and unload after a
  configurable idle time, or stay loaded if you pin them.
- **Lives in the tray.** Closing the window hides it; hotkeys keep working. Quit from the tray menu to unload everything.
- Optional **Start with Windows**.

## Which installer do I need?

Download one installer from the [Releases](https://github.com/samuelbsource/whisper-app/releases) page. They are
the same app. They differ only in how the speech model is run.

| Your computer | Installer | Notes |
|---|---|---|
| **NVIDIA graphics card**: GeForce GTX 16-series, RTX 20, 30, 40 or 50 | **CUDA** | Fastest option, but a big download (about 500 MB) because it includes NVIDIA's own libraries. |
| **AMD Radeon**, **Intel Arc / Iris**, or an **older NVIDIA card** (GTX 10-series and older) | **Vulkan** | Uses your GPU through the standard Vulkan driver. Small download (under 10 MB). |
| **No dedicated graphics card**, an old PC, or you're **not sure** | **CPU** | Works on every Windows 10/11 PC. Small download (under 10 MB). Slower, so pair it with a small model (base or small). |

How to check your graphics card: press `Ctrl+Shift+Esc`, open **Performance**, and look at **GPU** in the
left-hand list.

If an installer doesn't start or the GPU isn't being used, install the **Vulkan** one; if that fails, use the **CPU**
one. All three read the same settings and models, so switching is just installing another one. **Settings > About**
shows which backend is running.

Which model? The `large-v3-turbo-q5_0` model (574 MB download) is a good default on a GPU with 8 GB of memory (it is
the one used in development, on an RTX 2070). On CPU, start with `base` or `small`.

> **Windows SmartScreen:** the installers are not code-signed yet, so Windows may warn that the publisher is
> unknown. Choose *More info > Run anyway* if you trust the download. Each release includes `SHA256SUMS.txt` so you can
> check the file you got.

## First run

1. Open **Models** and download a model.
2. A default **Profile** and a **Transcriber** named *Dictate* exist already. Open **Transcribers**, click the
   shortcut box, press the combination you want, and **Save transcriber**.
3. Click into any text field in any app, press your hotkey, speak, and press it again (or release it, in hold mode).

Tips and limits:
- **Win-key shortcuts are partly reserved by Windows** (for example `Win+E`), and other programs may already own a
  combination. If a shortcut shows a warning, pick another; `Ctrl+Win+letter` or `Ctrl+[` style shortcuts are usually free.
- **Pasting into a window that runs as administrator** won't work unless Win Whisper also runs as administrator
  (a Windows security rule).
- **Paste vs. type:** the default pastes with `Ctrl+V` and then restores your clipboard. Choose *type it key by key*
  for apps that block paste.
- **Transcribing files** needs [ffmpeg](https://ffmpeg.org/) to read audio and video: `winget install Gyan.FFmpeg`,
  or point to `ffmpeg.exe` in Settings.

## Where things are stored

| What | Where |
|---|---|
| Settings, profiles, transcribers | `%APPDATA%\com.samuelboczek.whisperapp\config.json` |
| Downloaded models | `%LOCALAPPDATA%\com.samuelboczek.whisperapp\models` (changeable in Settings) |

## Build from source

Requirements: Windows 10/11, [Rust](https://rustup.rs/) 1.88 or newer with the MSVC toolchain, Node.js 22 or newer,
CMake, and LLVM (libclang), plus the CUDA Toolkit (for the CUDA build) or the Vulkan SDK (for the Vulkan build).

```powershell
winget install Kitware.CMake LLVM.LLVM
.\scripts\dev.ps1                       # run in dev mode with hot reload
.\scripts\release.ps1                   # build the installer
```

The scripts find CMake, LLVM and CUDA for you. The first build compiles whisper.cpp and takes a while.

## Licence and credits

Win Whisper is released under the [MIT licence](LICENSE).

It is built on [whisper.cpp](https://github.com/ggml-org/whisper.cpp) (via
[whisper-rs](https://codeberg.org/tazz4843/whisper-rs)), OpenAI's [Whisper](https://github.com/openai/whisper) models,
[Tauri](https://tauri.app/) and [Svelte](https://svelte.dev/). Full third-party licence notices are in
[THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt) and in the app under **Settings > About**.
