<script lang="ts">
  import { onDestroy } from "svelte";

  const {
    value = "#ffffff",
    label = "",
    oncommit
  }: {
    value?: string | null;
    label?: string;
    oncommit: (value: string) => void | Promise<void>;
  } = $props();

  let draft = $state("#ffffff");
  let lastCommitted = "#ffffff";
  let commitTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    const nextValue = value || "#ffffff";
    draft = nextValue;
    lastCommitted = nextValue;
  });

  function toHex(input: string | null | undefined) {
    const value = (input || "").trim();
    const shortHex = /^#([0-9a-f]{3})$/i.exec(value);
    if (shortHex) {
      return `#${shortHex[1]
        .split("")
        .map((part) => part + part)
        .join("")}`;
    }
    const hex = /^#([0-9a-f]{6})/i.exec(value);
    if (hex) return `#${hex[1]}`;
    const rgb = /rgba?\(\s*(\d+(?:\.\d+)?)\s*,\s*(\d+(?:\.\d+)?)\s*,\s*(\d+(?:\.\d+)?)/i.exec(value);
    if (!rgb) return "#ffffff";
    return `#${[rgb[1], rgb[2], rgb[3]]
      .map((part) => Math.max(0, Math.min(255, Math.round(Number(part))))
        .toString(16)
        .padStart(2, "0"))
      .join("")}`;
  }

  function clearCommitTimer() {
    if (!commitTimer) return;
    clearTimeout(commitTimer);
    commitTimer = null;
  }

  function commit(nextValue = draft) {
    clearCommitTimer();
    draft = nextValue.trim() || "#ffffff";
    if (draft === lastCommitted) return;
    lastCommitted = draft;
    void oncommit(draft);
  }

  function scheduleColorCommit(nextValue: string) {
    draft = nextValue;
    clearCommitTimer();
    commitTimer = setTimeout(() => commit(nextValue), 220);
  }

  onDestroy(clearCommitTimer);
</script>

<label class="color-field">
  {#if label}
    <span>{label}</span>
  {/if}
  <span class="color-row">
    <input
      type="color"
      value={toHex(draft)}
      oninput={(event) => scheduleColorCommit(event.currentTarget.value)}
      onchange={(event) => scheduleColorCommit(event.currentTarget.value)}
      aria-label={label || "Color"}
    />
    <input
      value={draft}
      placeholder="#ffffff or rgb(255, 255, 255)"
      oninput={(event) => (draft = event.currentTarget.value)}
      onblur={() => commit()}
    />
  </span>
</label>

<style>
  .color-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .color-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
  }

  input {
    box-sizing: border-box;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-dark) 38%, transparent);
    color: var(--text-primary);
    font: inherit;
    font-size: 13px;
  }

  input[type="color"] {
    flex: none;
    width: 40px;
    min-width: 40px;
    height: 34px;
    padding: 2px;
  }

  input:not([type="color"]) {
    width: 100%;
    min-width: 0;
    padding: 8px 10px;
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 24%, transparent);
  }
</style>
