import { listen } from "@tauri-apps/api/event";
import "./overlay.css";

type Kind = "recording" | "transcribing" | "done" | "info" | "error";
interface OverlayState {
  kind: Kind;
  title: string;
  detail: string;
}

const root = document.getElementById("overlay")!;
root.innerHTML = `
  <div class="bubble" data-kind="info">
    <div class="icon"><span class="dot"></span><span class="spinner"></span><span class="glyph"></span></div>
    <div class="text">
      <div class="title"></div>
      <div class="detail"></div>
      <div class="meter"><div class="bar"></div></div>
    </div>
  </div>`;

const bubble = root.querySelector<HTMLElement>(".bubble")!;
const title = root.querySelector<HTMLElement>(".title")!;
const detail = root.querySelector<HTMLElement>(".detail")!;
const glyph = root.querySelector<HTMLElement>(".glyph")!;
const bar = root.querySelector<HTMLElement>(".bar")!;

const glyphs: Record<Kind, string> = {
  recording: "",
  transcribing: "",
  done: "✓",
  info: "i",
  error: "!",
};

let smoothed = 0;

listen<OverlayState>("overlay", ({ payload }) => {
  bubble.dataset.kind = payload.kind;
  title.textContent =
    payload.kind === "recording" ? `Recording · ${payload.title}` : payload.title;
  detail.textContent = payload.detail;
  glyph.textContent = glyphs[payload.kind];
  if (payload.kind !== "recording") bar.style.width = "0%";
});

listen<number>("overlay-level", ({ payload }) => {
  // Speech peaks are small; a square root makes the meter respond to quiet voices.
  const target = Math.min(1, Math.sqrt(payload) * 1.4);
  smoothed = Math.max(target, smoothed * 0.8);
  bar.style.width = `${Math.round(smoothed * 100)}%`;
});
