<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type Settings } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import About from "../lib/About.svelte";

  let draft = $state<Settings | null>(null);
  let dirty = $state(false);

  $effect(() => {
    if (store.snap && !draft) draft = { ...store.snap.config.settings };
  });

  const touch = () => (dirty = true);

  async function persist() {
    if (!draft) return;
    await store.run(api.saveSettings($state.snapshot(draft)));
    dirty = false;
    store.toast("Settings saved");
  }

  async function pickFolder() {
    const dir = await open({ directory: true, title: "Models folder" });
    if (typeof dir === "string" && draft) {
      draft.modelsDir = dir;
      touch();
    }
  }

  async function pickFfmpeg() {
    const file = await open({ title: "Locate ffmpeg.exe", filters: [{ name: "ffmpeg", extensions: ["exe"] }] });
    if (typeof file === "string" && draft) {
      draft.ffmpegPath = file;
      touch();
    }
  }
</script>

<h1>Settings</h1>
<p class="sub">Global settings for the whole app.</p>

{#if draft && store.snap}
  <div class="card form">
    <h2>Memory</h2>
    <div class="field">
      <label for="s-idle">Unload models after this many idle minutes (0 = never)</label>
      <input id="s-idle" type="number" min="0" max="1440" bind:value={draft.modelIdleMinutes} oninput={touch} />
      <div class="hint">Models pinned with “Keep loaded” on the Models page are never unloaded.</div>
    </div>

    <h2>Performance</h2>
    <label class="check"><input type="checkbox" bind:checked={draft.useGpu} onchange={touch} /> Use the GPU ({store.snap.gpuBackend} build)</label>
    {#if store.snap.gpuBackend === "CPU only"}
      <div class="hint" style="margin: -4px 0 8px 24px">This build has no GPU backend, so it always runs on the CPU.</div>
    {/if}
    <label class="check"><input type="checkbox" bind:checked={draft.flashAttn} onchange={touch} /> Flash attention (faster on most GPUs)</label>
    <div class="field" style="max-width: 280px">
      <label for="s-threads">CPU threads (0 = automatic)</label>
      <input id="s-threads" type="number" min="0" max="64" bind:value={draft.threads} oninput={touch} />
    </div>
    <div class="hint" style="margin-bottom: 12px">Changing GPU options unloads the loaded models so they reload with the new settings.</div>

    <h2>Interface</h2>
    <label class="check"><input type="checkbox" bind:checked={draft.showOverlay} onchange={touch} /> Show the floating status bubble while recording and transcribing</label>
    <label class="check"><input type="checkbox" bind:checked={draft.restoreClipboard} onchange={touch} /> Restore the previous clipboard after pasting</label>
    <label class="check"><input type="checkbox" bind:checked={draft.startMinimized} onchange={touch} /> Start hidden in the tray</label>
    <label class="check">
      <input type="checkbox" checked={store.snap.autostart} onchange={(e) => store.run(api.setAutostart(e.currentTarget.checked))} />
      Start with Windows
    </label>

    <h2 style="margin-top: 16px">Files</h2>
    <div class="field">
      <label for="s-models">Models folder</label>
      <div class="row">
        <input id="s-models" type="text" class="grow" bind:value={draft.modelsDir} oninput={touch} placeholder={store.snap.modelsDir} />
        <button onclick={pickFolder}>Browse…</button>
      </div>
      <div class="hint">Leave empty for the default: {store.snap.modelsDir}</div>
    </div>
    <div class="field">
      <label for="s-ffmpeg">ffmpeg.exe</label>
      <div class="row">
        <input id="s-ffmpeg" type="text" class="grow" bind:value={draft.ffmpegPath} oninput={touch} placeholder={store.snap.ffmpeg ?? "Not found on PATH"} />
        <button onclick={pickFfmpeg}>Browse…</button>
      </div>
      <div class="hint">{store.snap.ffmpeg ? `Using ${store.snap.ffmpeg}` : "Needed only for the Transcribe audio page."}</div>
    </div>

    <div class="row" style="margin-top: 14px">
      <button class="primary" onclick={persist} disabled={!dirty}>Save settings</button>
    </div>
  </div>

  <About />
{/if}

<style>
  .form { max-width: 720px; }
</style>
