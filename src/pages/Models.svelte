<script lang="ts">
  import { api, formatBytes, type ModelInfo } from "../lib/api";
  import { store } from "../lib/store.svelte";

  let showAll = $state(false);
  let refreshing = $state(false);

  const models = $derived(store.snap?.models ?? []);
  const visible = $derived(showAll ? models : models.filter((m) => m.downloaded || m.download || !m.variant.includes("tdrz")));

  function pct(m: ModelInfo): number {
    const live = store.progress[m.file] ?? m.download;
    if (!live || !live.total) return 0;
    return Math.min(100, (live.received / live.total) * 100);
  }

  async function refresh() {
    refreshing = true;
    await store.run(api.refreshCatalog());
    refreshing = false;
  }

  async function remove(m: ModelInfo) {
    if (!confirm(`Delete ${m.file} (${formatBytes(m.size)}) from disk?`)) return;
    await store.run(api.deleteModel(m.file));
  }

  function idle(secs: number): string {
    return secs < 60 ? `${secs}s idle` : `${Math.floor(secs / 60)}m idle`;
  }
</script>

<h1>Models</h1>
<p class="sub">
  Whisper models are loaded on demand and unloaded after the idle time set in Settings.
  Pin a model to keep it in memory permanently.
</p>

<div class="row" style="margin-bottom: 14px">
  <button onclick={refresh} disabled={refreshing}>{refreshing ? "Refreshing…" : "Refresh list"}</button>
  <button onclick={() => api.openModelsDir()}>Open models folder</button>
  <label class="check" style="margin: 0 0 0 auto"><input type="checkbox" bind:checked={showAll} /> Show diarization variants</label>
</div>

<div class="list">
  {#each visible as m (m.file)}
    <div class="card item">
      <div class="grow">
        <div class="row wrap" style="gap: 8px">
          <strong>{m.family}{m.englishOnly ? " (English)" : ""}</strong>
          {#if m.variant}<span class="badge">{m.variant}</span>{/if}
          {#if m.recommended}<span class="badge accent">recommended</span>{/if}
          {#if !m.inCatalog}<span class="badge">local file</span>{/if}
          {#if m.runtime.state === "loaded"}<span class="badge ok">loaded · {idle(m.runtime.idleSecs)}</span>{/if}
          {#if m.runtime.state === "loading"}<span class="badge warn">loading…</span>{/if}
        </div>
        <div class="muted small mono">{m.file} · {formatBytes(m.size)}</div>
        {#if m.download}
          <div class="progress" style="margin-top: 8px"><div style="width: {pct(m)}%"></div></div>
          <div class="muted small">{pct(m).toFixed(0)}%</div>
        {/if}
      </div>

      <div class="row">
        {#if m.download}
          <button onclick={() => api.cancelDownload(m.file)}>Cancel</button>
        {:else if !m.downloaded}
          <button class="primary" onclick={() => store.run(api.downloadModel(m.file))}>Download</button>
        {:else}
          <label class="check" style="margin: 0" title="Load at startup and never unload">
            <input type="checkbox" checked={m.keepLoaded} onchange={(e) => store.run(api.setKeepLoaded(m.file, e.currentTarget.checked))} />
            Keep loaded
          </label>
          {#if m.runtime.state === "loaded"}
            <button onclick={() => store.run(api.unloadModel(m.file))}>Unload</button>
          {:else}
            <button disabled={m.runtime.state === "loading"} onclick={() => api.loadModel(m.file)}>Load</button>
          {/if}
          <button class="danger" onclick={() => remove(m)}>Delete</button>
        {/if}
      </div>
    </div>
  {:else}
    <div class="empty">No models listed yet. Check your internet connection and press “Refresh list”.</div>
  {/each}
</div>

<style>
  .list { display: flex; flex-direction: column; gap: 8px; }
  .item { display: flex; align-items: center; gap: 16px; padding: 10px 14px; }
</style>
