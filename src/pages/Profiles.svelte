<script lang="ts">
  import { api, LANGUAGES, formatBytes, type Profile } from "../lib/api";
  import { store } from "../lib/store.svelte";

  const blank = (): Profile => ({
    id: "", name: "New profile", model: "", language: "auto", translate: false,
    initialPrompt: "", beamSize: 5, temperature: 0, noContext: true, suppressNonSpeech: true,
  });

  const profiles = $derived(store.snap?.config.profiles ?? []);
  const models = $derived((store.snap?.models ?? []).filter((m) => m.downloaded));

  let selected = $state<string>("");
  let draft = $state<Profile | null>(null);

  $effect(() => {
    if (!draft && profiles.length) select(profiles[0].id);
  });

  function select(id: string) {
    selected = id;
    const p = profiles.find((p) => p.id === id);
    draft = p ? { ...p } : null;
  }

  function create() {
    const p = blank();
    p.model = models[0]?.file ?? "";
    selected = "";
    draft = p;
  }

  async function save() {
    if (!draft) return;
    const saved = await store.run(api.saveProfile(draft));
    if (saved) {
      selected = saved.id;
      draft = { ...saved };
      store.toast("Profile saved");
    }
  }

  async function remove() {
    if (!draft?.id || !confirm(`Delete profile “${draft.name}”?`)) return;
    const id = draft.id;
    await store.run(api.deleteProfile(id));
    // A profile still used by a transcriber is refused by the backend and stays.
    if (!store.snap?.config.profiles.some((p) => p.id === id)) {
      draft = null;
      selected = "";
    }
  }

  const missingModel = $derived(draft && draft.model && !models.some((m) => m.file === draft!.model));
</script>

<h1>Profiles</h1>
<p class="sub">A profile is a model plus a language and decoding settings. Transcribers use profiles.</p>

<div class="layout">
  <div class="list">
    {#each profiles as p (p.id)}
      <button class="pick" class:active={selected === p.id} onclick={() => select(p.id)}>
        <strong>{p.name}</strong>
        <span class="muted small">{p.model.replace(/^ggml-|\.bin$/g, "")} · {p.language}</span>
      </button>
    {/each}
    <button onclick={create}>+ New profile</button>
  </div>

  {#if draft}
    <div class="card form">
      <div class="field">
        <label for="p-name">Name</label>
        <input id="p-name" type="text" bind:value={draft.name} />
      </div>
      <div class="grid2">
        <div class="field">
          <label for="p-model">Model</label>
          <select id="p-model" bind:value={draft.model}>
            {#if missingModel || !draft.model}<option value={draft.model}>{draft.model || "Select a model…"}</option>{/if}
            {#each models as m}
              <option value={m.file}>{m.family}{m.englishOnly ? " (en)" : ""} {m.variant} · {formatBytes(m.size)}</option>
            {/each}
          </select>
          {#if missingModel || !models.length}<div class="error-text">Download this model on the Models page first.</div>{/if}
        </div>
        <div class="field">
          <label for="p-lang">Language</label>
          <select id="p-lang" bind:value={draft.language}>
            {#each LANGUAGES as [code, name]}<option value={code}>{name}</option>{/each}
          </select>
          <div class="hint">English-only models ignore this. Fixing the language is faster and more accurate than auto-detect.</div>
        </div>
      </div>

      <label class="check"><input type="checkbox" bind:checked={draft.translate} /> Translate speech to English</label>

      <div class="field">
        <label for="p-prompt">Initial prompt (optional)</label>
        <textarea id="p-prompt" bind:value={draft.initialPrompt} placeholder="Names, jargon or the punctuation style you want, e.g. “Hello, I'm Samuel. We use Rust and Tauri.”"></textarea>
      </div>

      <h2>Advanced</h2>
      <div class="grid2">
        <div class="field">
          <label for="p-beam">Beam size (1 = fastest, 5 = more accurate)</label>
          <input id="p-beam" type="number" min="1" max="16" bind:value={draft.beamSize} />
        </div>
        <div class="field">
          <label for="p-temp">Temperature (0 = deterministic)</label>
          <input id="p-temp" type="number" min="0" max="1" step="0.1" bind:value={draft.temperature} />
        </div>
      </div>
      <label class="check"><input type="checkbox" bind:checked={draft.noContext} /> Don't carry text between segments (prevents repetition loops on long audio)</label>
      <label class="check"><input type="checkbox" bind:checked={draft.suppressNonSpeech} /> Suppress non-speech tokens</label>

      <div class="row" style="margin-top: 14px">
        <button class="primary" onclick={save} disabled={!draft.model}>Save profile</button>
        {#if draft.id}<button class="danger" onclick={remove}>Delete</button>{/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .layout { display: grid; grid-template-columns: 240px 1fr; gap: 18px; align-items: start; }
  .list { display: flex; flex-direction: column; gap: 6px; }
  .pick { display: flex; flex-direction: column; align-items: flex-start; text-align: left; }
  .pick.active { border-color: var(--accent); background: var(--hover); }
  .form { max-width: 720px; }
</style>
