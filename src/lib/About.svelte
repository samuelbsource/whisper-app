<script lang="ts">
  import { api, type Notices } from "./api";
  import { store } from "./store.svelte";

  let notices = $state<Notices | null>(null);
  let open = $state<"" | "app" | "third">("");
  let loading = $state(false);

  async function show(which: "app" | "third") {
    if (open === which) {
      open = "";
      return;
    }
    if (!notices) {
      loading = true;
      try {
        notices = await api.getNotices();
      } catch (err) {
        store.toast(String(err), "error");
        return;
      } finally {
        loading = false;
      }
    }
    open = which;
  }
</script>

{#if store.snap}
  <div class="card about">
    <h2>About</h2>
    <p class="muted" style="margin: 0 0 4px">
      Win Whisper {store.snap.appVersion} · running on <strong>{store.snap.gpuBackend}</strong> ·
      whisper.cpp {store.snap.whisperVersion}
    </p>
    <p class="muted small" style="margin: 0 0 12px">
      Free software under the MIT licence. Closing the window keeps the app in the tray; use “Quit” in the tray menu to
      unload all models and exit.
    </p>

    <div class="row">
      <button class="small" onclick={() => show("app")} disabled={loading}>{open === "app" ? "Hide" : "Show"} licence and components</button>
      <button class="small" onclick={() => show("third")} disabled={loading}>{open === "third" ? "Hide" : "Show"} all third-party notices</button>
    </div>

    {#if notices && open === "app"}
      <div class="panel">
        <pre class="text">{notices.appLicense}</pre>
        <h2 style="margin-top: 14px">Main components</h2>
        <ul>
          {#each notices.components as c}
            <li><strong>{c.name}</strong> ({c.license}): <span class="muted">{c.note}</span></li>
          {/each}
        </ul>
        {#if notices.extra}<pre class="text">{notices.extra}</pre>{/if}
      </div>
    {:else if notices && open === "third"}
      <div class="panel"><pre class="text">{notices.thirdParty}</pre></div>
    {/if}
  </div>
{/if}

<style>
  .about { max-width: 720px; margin-top: 18px; }
  .panel {
    margin-top: 12px;
    max-height: 340px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--input);
    padding: 10px 12px;
  }
  .text {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    font: 12px/1.45 "Cascadia Mono", Consolas, monospace;
  }
  ul { margin: 6px 0 0; padding-left: 20px; }
  li { margin-bottom: 4px; }
</style>
