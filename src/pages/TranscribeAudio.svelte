<script lang="ts">
  import { onMount } from "svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { listen } from "@tauri-apps/api/event";
  import { api, type FileJobEvent } from "../lib/api";
  import { store } from "../lib/store.svelte";

  const profiles = $derived(store.snap?.config.profiles ?? []);

  let file = $state("");
  let profileId = $state("");
  let maxLen = $state(0);
  let jobId = $state("");
  let job = $state<FileJobEvent | null>(null);
  let result = $state<{ text: string; srt: string } | null>(null);
  let view = $state<"text" | "srt">("text");

  const running = $derived(!!jobId);

  $effect(() => {
    if (!profileId && profiles.length) profileId = profiles[0].id;
  });

  onMount(() => {
    const unlisten = listen<FileJobEvent>("file-job", ({ payload }) => {
      // Events can arrive before the start command returns its id; adopt the first one.
      if (jobId === "pending") jobId = payload.id;
      if (payload.id !== jobId) return;
      job = payload;
      if (payload.stage === "done") {
        result = { text: payload.text ?? "", srt: payload.srt ?? "" };
        jobId = "";
      } else if (payload.stage === "error") {
        store.toast(payload.message, "error");
        jobId = "";
      } else if (payload.stage === "cancelled") {
        jobId = "";
      }
    });
    return () => void unlisten.then((f) => f());
  });

  async function browse() {
    const picked = await open({
      title: "Choose an audio or video file",
      multiple: false,
      filters: [
        { name: "Audio and video", extensions: ["mp3", "wav", "m4a", "flac", "ogg", "opus", "aac", "wma", "mp4", "mkv", "mov", "avi", "webm", "m4v", "ts"] },
        { name: "All files", extensions: ["*"] },
      ],
    });
    if (typeof picked === "string") {
      file = picked;
      result = null;
      job = null;
    }
  }

  async function start() {
    result = null;
    job = null;
    // Listeners compare against jobId, so reserve it before events can arrive.
    jobId = "pending";
    try {
      const id = await api.transcribeFile(file, profileId, maxLen);
      // A fast failure may already have finished the job; don't resurrect it.
      if (jobId === "pending") jobId = id;
    } catch (err) {
      jobId = "";
      store.toast(String(err), "error");
    }
  }

  async function saveAs(kind: "txt" | "srt" | "both") {
    if (!result) return;
    const ext = kind === "both" ? "txt" : kind;
    const path = await save({
      title: kind === "both" ? "Save text and subtitles (pick the .txt name)" : `Save ${ext.toUpperCase()}`,
      defaultPath: await api.defaultOutputPath(file, ext),
      filters: [{ name: ext === "srt" ? "SubRip subtitles" : "Text", extensions: [ext] }],
    });
    if (!path) return;
    if (kind === "txt") await store.run(api.writeTextFile(path, result.text));
    else if (kind === "srt") await store.run(api.writeTextFile(path, result.srt));
    else {
      await store.run(api.writeTextFile(path, result.text));
      await store.run(api.writeTextFile(await api.defaultOutputPath(path, "srt"), result.srt));
    }
    store.toast("Saved");
  }

  const stageText = $derived(
    !job ? "" :
    job.stage === "decoding" ? "Reading audio…" :
    job.stage === "loading" ? job.message :
    job.stage === "transcribing" ? `Transcribing… ${job.progress}%` :
    job.stage === "done" ? `Done · ${Math.round(job.durationSecs)} s of audio` :
    job.stage === "cancelled" ? "Cancelled" : "",
  );
</script>

<h1>Transcribe audio</h1>
<p class="sub">Transcribe a recording or video and save the result as text, SRT subtitles, or both.</p>

{#if !store.snap?.ffmpeg}
  <div class="card warnbox">
    ffmpeg was not found, and it is needed to read audio and video files.
    Install it with <span class="mono">winget install Gyan.FFmpeg</span> or set its path in Settings.
  </div>
{/if}

<div class="card form">
  <div class="field">
    <label for="a-file">File</label>
    <div class="row">
      <input id="a-file" type="text" class="grow" bind:value={file} placeholder="Choose a file…" />
      <button onclick={browse} disabled={running}>Browse…</button>
    </div>
  </div>
  <div class="grid2">
    <div class="field">
      <label for="a-profile">Profile</label>
      <select id="a-profile" bind:value={profileId}>
        {#each profiles as p}<option value={p.id}>{p.name}</option>{/each}
      </select>
    </div>
    <div class="field">
      <label for="a-len">Subtitle line length (characters, 0 = automatic)</label>
      <input id="a-len" type="number" min="0" max="200" bind:value={maxLen} />
    </div>
  </div>

  <div class="row">
    <button class="primary" onclick={start} disabled={running || !file.trim() || !profileId}>Transcribe</button>
    {#if running && jobId !== "pending"}<button onclick={() => api.cancelFileJob(jobId)}>Cancel</button>{/if}
    <span class="muted">{stageText}</span>
  </div>
  {#if job?.stage === "transcribing"}
    <div class="progress" style="margin-top: 10px"><div style="width: {job.progress}%"></div></div>
  {/if}
</div>

{#if result}
  <div class="card form" style="margin-top: 16px">
    <div class="row" style="margin-bottom: 10px">
      <div class="row" style="gap: 4px">
        <button class:primary={view === "text"} onclick={() => (view = "text")}>Text</button>
        <button class:primary={view === "srt"} onclick={() => (view = "srt")}>SRT</button>
      </div>
      <span class="grow"></span>
      <button onclick={() => api.copyText(view === "text" ? result!.text : result!.srt).then(() => store.toast("Copied"))}>Copy</button>
      <button onclick={() => saveAs("txt")}>Save .txt</button>
      <button onclick={() => saveAs("srt")}>Save .srt</button>
      <button class="primary" onclick={() => saveAs("both")}>Save both</button>
    </div>
    <textarea readonly rows="16" class="mono out" value={view === "text" ? result.text : result.srt}></textarea>
  </div>
{/if}

<style>
  .form { max-width: 860px; }
  .warnbox { max-width: 860px; margin-bottom: 14px; border-color: var(--warn); }
  .out { min-height: 280px; font-size: 13px; }
</style>
