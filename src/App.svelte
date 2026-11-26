<script lang="ts">
  import { onMount } from "svelte";
  import { store } from "./lib/store.svelte";
  import Models from "./pages/Models.svelte";
  import Profiles from "./pages/Profiles.svelte";
  import Transcribers from "./pages/Transcribers.svelte";
  import TranscribeAudio from "./pages/TranscribeAudio.svelte";
  import Settings from "./pages/Settings.svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const tabs = [
    { id: "models", label: "Models", icon: "M12 3v12m0 0l-4-4m4 4l4-4M4 17v3h16v-3" },
    { id: "profiles", label: "Profiles", icon: "M12 12a4 4 0 100-8 4 4 0 000 8zm-8 9a8 8 0 0116 0" },
    { id: "transcribers", label: "Transcribers", icon: "M12 3a3 3 0 00-3 3v6a3 3 0 006 0V6a3 3 0 00-3-3zm-7 9a7 7 0 0014 0m-7 7v3" },
    { id: "audio", label: "Transcribe audio", icon: "M4 12h2m3-6v12m3-9v6m3-10v14m3-8h2" },
    { id: "settings", label: "Settings", icon: "M12 15a3 3 0 100-6 3 3 0 000 6zm7.4-3a7.4 7.4 0 00-.1-1.2l2-1.6-2-3.4-2.4 1a7 7 0 00-2-1.2L14.5 3h-4l-.4 2.6a7 7 0 00-2 1.2l-2.4-1-2 3.4 2 1.6a7.400 7.400 0 000 2.400l-2 1.600 2 3.400 2.400-1a7 7 0 002 1.200l.4 2.600h4l.4-2.600a7 7 0 002-1.200l2.400 1 2-3.400-2-1.600c.1-.4.1-.8.1-1.200z" },
  ] as const;

  type TabId = (typeof tabs)[number]["id"];
  let tab = $state<TabId>("models");

  const recording = $derived(Object.values(store.snap?.activity ?? {}).includes("recording"));
  const loaded = $derived((store.snap?.models ?? []).filter((m) => m.runtime.state === "loaded").length);

  onMount(() => {
    store.init().then(() => getCurrentWindow().show());
  });
</script>

{#if store.snap}
  <div class="shell">
    <nav>
      <div class="brand">
        <img src="/icon.png" alt="" width="26" height="26" />
        <span>Win Whisper</span>
      </div>
      {#each tabs as t}
        <button class="tab" class:active={tab === t.id} onclick={() => (tab = t.id)}>
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d={t.icon} /></svg>
          {t.label}
        </button>
      {/each}
      <div class="foot">
        {#if recording}<span class="badge danger">● Recording</span>{/if}
        <div class="muted small">
          {loaded} model{loaded === 1 ? "" : "s"} loaded · {store.snap.gpuBackend}
        </div>
        <div class="muted small">v{store.snap.appVersion}</div>
      </div>
    </nav>
    <main>
      {#if tab === "models"}<Models />
      {:else if tab === "profiles"}<Profiles />
      {:else if tab === "transcribers"}<Transcribers />
      {:else if tab === "audio"}<TranscribeAudio />
      {:else}<Settings />{/if}
    </main>
  </div>
{/if}

<div class="toasts">
  {#each store.toasts as t (t.id)}
    <div class="toast" class:err={t.kind === "error"}>{t.text}</div>
  {/each}
</div>

<style>
  .shell { display: flex; height: 100%; }
  nav {
    width: 220px;
    flex: none;
    background: var(--side);
    border-right: 1px solid var(--border);
    padding: 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .brand { display: flex; align-items: center; gap: 10px; font-weight: 650; font-size: 15px; padding: 4px 10px 16px; }
  .brand img { border-radius: 6px; }
  .tab {
    display: flex; align-items: center; gap: 12px;
    border: none; background: transparent;
    text-align: left; padding: 9px 12px; border-radius: 8px;
  }
  .tab.active { background: var(--hover); font-weight: 600; box-shadow: inset 3px 0 0 var(--accent); }
  .foot { margin-top: auto; padding: 8px 10px; display: flex; flex-direction: column; gap: 4px; }
  main { flex: 1; overflow-y: auto; padding: 26px 32px 40px; }
  .toasts { position: fixed; right: 18px; bottom: 18px; display: flex; flex-direction: column; gap: 8px; z-index: 50; max-width: 420px; }
  .toast {
    background: #26262e; color: #f2f2f6; padding: 10px 14px; border-radius: 8px;
    border-left: 4px solid var(--accent); box-shadow: 0 6px 20px rgba(0,0,0,.35); font-size: 13px;
    white-space: pre-wrap; word-break: break-word;
  }
  .toast.err { border-left-color: var(--danger); }
</style>
