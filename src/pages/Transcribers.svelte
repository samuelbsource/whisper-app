<script lang="ts">
  import { onMount } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import { api, type MicInfo, type Transcriber } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import HotkeyInput from "../lib/HotkeyInput.svelte";

  const transcribers = $derived(store.snap?.config.transcribers ?? []);
  const profiles = $derived(store.snap?.config.profiles ?? []);
  const errors = $derived(store.snap?.hotkeyErrors ?? {});
  const activity = $derived(store.snap?.activity ?? {});

  let mics = $state<MicInfo[]>([]);
  let selected = $state("");
  let draft = $state<Transcriber | null>(null);

  const refreshMics = async () => (mics = await api.listMics());
  onMount(refreshMics);

  $effect(() => {
    if (!draft && transcribers.length) select(transcribers[0].id);
  });

  function select(id: string) {
    selected = id;
    const t = transcribers.find((t) => t.id === id);
    draft = t ? structuredClone($state.snapshot(t)) : null;
  }

  function create() {
    selected = "";
    draft = {
      id: "", name: "New transcriber", enabled: true, profileId: profiles[0]?.id ?? "",
      hotkey: "", mode: "toggle", microphone: null, paste: true, pasteMethod: "clipboard",
      appendSpace: true, fileOutput: false, filePath: "", fileTimestamps: true,
    };
  }

  async function persist() {
    if (!draft) return;
    const saved = await store.run(api.saveTranscriber(draft));
    if (saved) {
      selected = saved.id;
      draft = structuredClone(saved);
      store.toast("Transcriber saved");
    }
  }

  async function remove() {
    if (!draft?.id || !confirm(`Delete transcriber “${draft.name}”?`)) return;
    await store.run(api.deleteTranscriber(draft.id));
    draft = null;
    selected = "";
  }

  async function toggleEnabled(t: Transcriber) {
    await store.run(api.saveTranscriber({ ...$state.snapshot(t), enabled: !t.enabled }));
    if (draft?.id === t.id) draft.enabled = !t.enabled;
  }

  async function pickFile() {
    const path = await save({
      title: "Append transcriptions to…",
      defaultPath: draft?.filePath || "transcriptions.txt",
      filters: [{ name: "Text", extensions: ["txt"] }],
    });
    if (path && draft) draft.filePath = path;
  }

  // Registration errors belong to the saved hotkey, not to an unsaved edit.
  const savedHotkey = $derived(transcribers.find((t) => t.id === draft?.id)?.hotkey);
  const hotkeyChanged = $derived(!!draft && draft.hotkey !== (savedHotkey ?? ""));
  const hotkeyError = $derived(draft?.id && !hotkeyChanged ? (errors[draft.id] ?? "") : "");

  // The microphone select works with ids; a saved mic that is unplugged stays selectable.
  const micValue = $derived(draft?.microphone?.id ?? "");
  function setMic(id: string) {
    if (!draft) return;
    if (!id) return void (draft.microphone = null);
    const info = mics.find((m) => m.id === id);
    draft.microphone = { id, name: info?.name ?? draft.microphone?.name ?? "" };
  }
  const micMissing = $derived(!!draft?.microphone && !mics.some((m) => m.id === draft!.microphone!.id));
</script>

<h1>Transcribers</h1>
<p class="sub">
  A transcriber ties a profile to a hotkey, a microphone and a destination. Press the hotkey anywhere in Windows to dictate.
</p>

<div class="layout">
  <div class="list">
    {#each transcribers as t (t.id)}
      <div class="pick" class:active={selected === t.id}>
        <button class="pickbtn" onclick={() => select(t.id)}>
          <div class="row" style="gap: 6px">
            <strong>{t.name}</strong>
            {#if activity[t.id] === "recording"}<span class="badge danger">● rec</span>{/if}
            {#if activity[t.id] === "transcribing"}<span class="badge warn">…</span>{/if}
          </div>
          <span class="muted small">{t.hotkey ? t.hotkey.replace(/Super/g, "Win").replace(/Key/g, "").replaceAll("+", " + ") : "no hotkey"}</span>
          {#if errors[t.id]}<span class="error-text">⚠ hotkey problem</span>{/if}
        </button>
        <input type="checkbox" checked={t.enabled} title="Enabled" onchange={() => toggleEnabled(t)} />
      </div>
    {/each}
    <button onclick={create} disabled={!profiles.length}>+ New transcriber</button>
    {#if !profiles.length}<div class="hint">Create a profile first.</div>{/if}
  </div>

  {#if draft}
    <div class="card form">
      <div class="grid2">
        <div class="field">
          <label for="t-name">Name</label>
          <input id="t-name" type="text" bind:value={draft.name} />
        </div>
        <div class="field">
          <label for="t-profile">Profile</label>
          <select id="t-profile" bind:value={draft.profileId}>
            {#each profiles as p}<option value={p.id}>{p.name}</option>{/each}
          </select>
        </div>
      </div>

      <h2>Hotkey</h2>
      <div class="grid2">
        <div class="field">
          <span class="label">Shortcut</span>
          <HotkeyInput bind:value={draft.hotkey} error={hotkeyError} />
          {#if hotkeyChanged}<div class="hint">New shortcut. Press “Save transcriber” to activate it.</div>{/if}
          <div class="hint">Some Win+ combinations are reserved by Windows (e.g. Win+E). Ctrl+Win+letter is usually free.</div>
        </div>
        <div class="field">
          <label for="t-mode">Behaviour</label>
          <select id="t-mode" bind:value={draft.mode}>
            <option value="toggle">Toggle: press to start, press again to stop</option>
            <option value="hold">Hold: record while the keys are held</option>
          </select>
        </div>
      </div>

      <h2>Microphone</h2>
      <div class="field">
        <div class="row">
          <select class="grow" value={micValue} onchange={(e) => setMic(e.currentTarget.value)}>
            <option value="">Windows default input device</option>
            {#if micMissing && draft.microphone}<option value={draft.microphone.id}>{draft.microphone.name} (not connected)</option>{/if}
            {#each mics as m}<option value={m.id}>{m.name}{m.isDefault ? " (default)" : ""}</option>{/each}
          </select>
          <button onclick={refreshMics}>Rescan</button>
        </div>
        {#if micMissing}<div class="hint">This microphone isn't connected. The default one is used until it returns.</div>{/if}
      </div>

      <h2>Output</h2>
      <label class="check"><input type="checkbox" bind:checked={draft.paste} /> Insert the text into the current window</label>
      {#if draft.paste}
        <div class="indent">
          <div class="field">
            <label for="t-method">Method</label>
            <select id="t-method" bind:value={draft.pasteMethod}>
              <option value="clipboard">Paste with Ctrl+V (fast, restores your clipboard)</option>
              <option value="type">Type it key by key (for apps that block paste)</option>
            </select>
          </div>
          <label class="check"><input type="checkbox" bind:checked={draft.appendSpace} /> Add a space after the text</label>
        </div>
      {/if}
      <label class="check"><input type="checkbox" bind:checked={draft.fileOutput} /> Append to a text file</label>
      {#if draft.fileOutput}
        <div class="indent">
          <div class="row">
            <input type="text" class="grow" bind:value={draft.filePath} placeholder="C:\Users\you\Documents\notes.txt" />
            <button onclick={pickFile}>Browse…</button>
          </div>
          <label class="check" style="margin-top: 8px"><input type="checkbox" bind:checked={draft.fileTimestamps} /> Prefix each entry with the date and time</label>
        </div>
      {/if}
      {#if !draft.paste && !draft.fileOutput}<div class="error-text">Choose at least one output.</div>{/if}

      <div class="row" style="margin-top: 14px">
        <button class="primary" onclick={persist}>Save transcriber</button>
        {#if draft.id && activity[draft.id] === "recording"}
          <button onclick={() => api.cancelRecording(draft!.id)}>Cancel recording</button>
        {/if}
        {#if draft.id}<button class="danger" onclick={remove}>Delete</button>{/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .layout { display: grid; grid-template-columns: 250px 1fr; gap: 18px; align-items: start; }
  .list { display: flex; flex-direction: column; gap: 6px; }
  .pick { display: flex; align-items: center; gap: 8px; border: 1px solid var(--border); background: var(--card); border-radius: 6px; padding-right: 10px; }
  .pick.active { border-color: var(--accent); background: var(--hover); }
  .pickbtn { flex: 1; display: flex; flex-direction: column; align-items: flex-start; text-align: left; border: none; background: transparent; }
  .form { max-width: 760px; }
  .indent { margin: 0 0 10px 24px; }
</style>
