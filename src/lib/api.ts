import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  modelIdleMinutes: number;
  useGpu: boolean;
  flashAttn: boolean;
  threads: number;
  modelsDir: string;
  ffmpegPath: string;
  startMinimized: boolean;
  showOverlay: boolean;
  restoreClipboard: boolean;
}

export interface Profile {
  id: string;
  name: string;
  model: string;
  language: string;
  translate: boolean;
  initialPrompt: string;
  beamSize: number;
  temperature: number;
  noContext: boolean;
  suppressNonSpeech: boolean;
}

export interface Microphone {
  id: string;
  name: string;
}

export interface Transcriber {
  id: string;
  name: string;
  enabled: boolean;
  profileId: string;
  hotkey: string;
  mode: "toggle" | "hold";
  microphone: Microphone | null;
  paste: boolean;
  pasteMethod: "clipboard" | "type";
  appendSpace: boolean;
  fileOutput: boolean;
  filePath: string;
  fileTimestamps: boolean;
}

export interface Config {
  settings: Settings;
  profiles: Profile[];
  transcribers: Transcriber[];
  keepLoaded: string[];
}

export interface ModelInfo {
  file: string;
  family: string;
  variant: string;
  englishOnly: boolean;
  size: number;
  downloaded: boolean;
  inCatalog: boolean;
  recommended: boolean;
  keepLoaded: boolean;
  download: { file: string; received: number; total: number } | null;
  runtime: { state: "unloaded" | "loading" | "loaded"; idleSecs: number };
}

export interface Snapshot {
  config: Config;
  models: ModelInfo[];
  hotkeyErrors: Record<string, string>;
  activity: Record<string, "recording" | "transcribing">;
  modelsDir: string;
  ffmpeg: string | null;
  gpuBackend: string;
  whisperVersion: string;
  appVersion: string;
  autostart: boolean;
}

export interface MicInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface FileJobEvent {
  id: string;
  stage: "decoding" | "loading" | "transcribing" | "done" | "cancelled" | "error";
  progress: number;
  message: string;
  text: string | null;
  srt: string | null;
  durationSecs: number;
}

export const api = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  listMics: () => invoke<MicInfo[]>("list_mics"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  setAutostart: (enabled: boolean) => invoke<void>("set_autostart", { enabled }),
  saveProfile: (profile: Profile) => invoke<Profile>("save_profile", { profile }),
  deleteProfile: (id: string) => invoke<void>("delete_profile", { id }),
  saveTranscriber: (transcriber: Transcriber) =>
    invoke<Transcriber>("save_transcriber", { transcriber }),
  deleteTranscriber: (id: string) => invoke<void>("delete_transcriber", { id }),
  setHotkeysPaused: (paused: boolean) => invoke<void>("set_hotkeys_paused", { paused }),
  cancelRecording: (id: string) => invoke<void>("cancel_recording", { id }),
  refreshCatalog: () => invoke<void>("refresh_catalog"),
  downloadModel: (file: string) => invoke<void>("download_model", { file }),
  cancelDownload: (file: string) => invoke<void>("cancel_download", { file }),
  deleteModel: (file: string) => invoke<void>("delete_model", { file }),
  loadModel: (file: string) => invoke<void>("load_model", { file }),
  unloadModel: (file: string) => invoke<void>("unload_model", { file }),
  setKeepLoaded: (file: string, keep: boolean) =>
    invoke<void>("set_keep_loaded", { file, keep }),
  openModelsDir: () => invoke<void>("open_models_dir"),
  writeTextFile: (path: string, contents: string) =>
    invoke<void>("write_text_file", { path, contents }),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  transcribeFile: (path: string, profileId: string, maxLen: number) =>
    invoke<string>("transcribe_file", { path, profileId, maxLen }),
  cancelFileJob: (id: string) => invoke<void>("cancel_file_job", { id }),
  defaultOutputPath: (input: string, extension: string) =>
    invoke<string>("default_output_path", { input, extension }),
  getNotices: () => invoke<Notices>("get_notices"),
};

export interface Notices {
  appLicense: string;
  components: { name: string; license: string; note: string }[];
  /** Notice that only applies to this build flavour, e.g. the NVIDIA libraries in the CUDA build. */
  extra: string | null;
  thirdParty: string;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${Math.round(bytes / 1e6)} MB`;
  return `${Math.round(bytes / 1e3)} KB`;
}

export const LANGUAGES: [string, string][] = [
  ["auto", "Auto-detect"], ["en", "English"], ["pl", "Polish"], ["de", "German"],
  ["fr", "French"], ["es", "Spanish"], ["it", "Italian"], ["pt", "Portuguese"],
  ["nl", "Dutch"], ["cs", "Czech"], ["sk", "Slovak"], ["uk", "Ukrainian"],
  ["ru", "Russian"], ["sv", "Swedish"], ["no", "Norwegian"], ["da", "Danish"],
  ["fi", "Finnish"], ["tr", "Turkish"], ["ar", "Arabic"], ["he", "Hebrew"],
  ["hi", "Hindi"], ["zh", "Chinese"], ["ja", "Japanese"], ["ko", "Korean"],
];
