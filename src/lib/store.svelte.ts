import { listen } from "@tauri-apps/api/event";
import { api, type Snapshot } from "./api";

class Store {
  snap = $state<Snapshot | null>(null);
  /** Live download progress, updated faster than full snapshots. */
  progress = $state<Record<string, { received: number; total: number }>>({});
  toasts = $state<{ id: number; text: string; kind: "error" | "info" }[]>([]);
  private nextToast = 1;
  private pending = false;

  async init() {
    await this.refresh();
    await listen("state-changed", () => this.refresh());
    await listen<{ file: string; received: number; total: number }>(
      "download-progress",
      ({ payload }) => {
        this.progress[payload.file] = payload;
      },
    );
    await listen<string>("app-error", ({ payload }) => this.toast(payload, "error"));
  }

  async refresh() {
    // Coalesce bursts of events into one request.
    if (this.pending) return;
    this.pending = true;
    try {
      this.snap = await api.snapshot();
    } finally {
      this.pending = false;
    }
  }

  toast(text: string, kind: "error" | "info" = "info") {
    const id = this.nextToast++;
    this.toasts.push({ id, text, kind });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), kind === "error" ? 9000 : 3500);
  }

  /** Run a backend call, show errors as toasts, and refresh. */
  async run<T>(call: Promise<T>): Promise<T | undefined> {
    try {
      const result = await call;
      await this.refresh();
      return result;
    } catch (err) {
      this.toast(String(err), "error");
      await this.refresh();
      return undefined;
    }
  }
}

export const store = new Store();
