<script lang="ts">
  import { api } from "./api";

  let { value = $bindable(""), error = "" }: { value: string; error?: string } = $props();
  let capturing = $state(false);

  const modifierCodes = ["Control", "Shift", "Alt", "Meta", "AltGraph"];

  async function start() {
    capturing = true;
    // Unregister every hotkey so the combination reaches this window.
    await api.setHotkeysPaused(true);
  }

  async function stop() {
    capturing = false;
    await api.setHotkeysPaused(false);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return void stop();
    if (e.key === "Backspace" || e.key === "Delete") {
      value = "";
      return void stop();
    }
    if (modifierCodes.includes(e.key)) return;
    const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean) as string[];
    if (!mods.length && !/^F\d+$/.test(e.code)) return; // a bare letter would break normal typing
    value = [...mods, e.code].join("+");
    stop();
  }

  const pretty = (v: string) =>
    v.replace(/\bSuper\b/g, "Win").replace(/Key([A-Z])\b/g, "$1").replace(/Digit(\d)/g, "$1").replaceAll("+", " + ");
</script>

<div class="wrap">
  <button type="button" class="box" class:capturing onclick={() => (capturing ? stop() : start())} {onkeydown} onblur={() => capturing && stop()}>
    {#if capturing}Press the shortcut… (Esc cancels, Backspace clears){:else if value}{pretty(value)}{:else}Not set. Click to record{/if}
  </button>
  {#if error}<div class="error-text">{error}</div>{/if}
</div>

<style>
  .wrap { display: flex; flex-direction: column; gap: 4px; }
  .box { text-align: left; font-weight: 600; min-height: 34px; }
  .box.capturing { border-color: var(--accent); color: var(--accent); }
</style>
