<script lang="ts">
  import { tick } from "svelte";

  export type EditorCommand = {
    id: string;
    label: string;
    detail?: string;
    keywords?: string;
    disabled?: boolean;
    run: () => void;
  };

  let {
    open = $bindable(false),
    commands
  }: {
    open?: boolean;
    commands: EditorCommand[];
  } = $props();

  let query = $state("");
  let input: HTMLInputElement | undefined = $state();

  const filteredCommands = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    return commands
      .filter((command) => {
        if (command.disabled) return false;
        if (!needle) return true;
        return `${command.label} ${command.detail ?? ""} ${command.keywords ?? ""}`.toLowerCase().includes(needle);
      })
      .slice(0, 12);
  });

  $effect(() => {
    if (!open) {
      query = "";
      return;
    }
    void tick().then(() => input?.focus());
  });

  function close() {
    open = false;
  }

  function run(command: EditorCommand | undefined) {
    if (!command || command.disabled) return;
    command.run();
    close();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      run(filteredCommands[0]);
    }
  }
</script>

{#if open}
  <div class="command-backdrop" role="presentation" onclick={close}>
    <!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role, a11y_click_events_have_key_events -->
    <section class="command-palette" role="dialog" aria-modal="true" aria-label="Commands" onclick={(event) => event.stopPropagation()}>
      <input
        bind:this={input}
        bind:value={query}
        onkeydown={handleKeydown}
        placeholder="Search command or visual"
        aria-label="Search command or visual"
      />
      <div class="command-list" role="listbox">
        {#each filteredCommands as command (command.id)}
          <button type="button" role="option" aria-selected="false" onclick={() => run(command)}>
            <span>{command.label}</span>
            {#if command.detail}
              <small>{command.detail}</small>
            {/if}
          </button>
        {:else}
          <p>No result</p>
        {/each}
      </div>
    </section>
  </div>
{/if}

<style>
  .command-backdrop {
    position: fixed;
    inset: 0;
    z-index: 1200;
    display: grid;
    align-items: start;
    justify-items: center;
    padding-top: 74px;
    background: color-mix(in srgb, var(--bg-dark) 42%, transparent);
  }

  .command-palette {
    display: flex;
    width: min(520px, calc(100vw - 32px));
    max-height: min(520px, calc(100vh - 110px));
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--editor-bg-panel);
    box-shadow: 0 24px 80px color-mix(in srgb, var(--bg-dark) 78%, transparent);
  }

  input {
    width: 100%;
    min-width: 0;
    height: 42px;
    box-sizing: border-box;
    padding: 0 14px;
    border: 0;
    border-bottom: 1px solid var(--border-color);
    outline: none;
    background: color-mix(in srgb, var(--bg-dark) 28%, transparent);
    color: var(--text-primary);
    font: inherit;
    font-size: 14px;
  }

  .command-list {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 3px;
    overflow-y: auto;
    padding: 6px;
  }

  button {
    display: grid;
    min-width: 0;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    min-height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
  }

  button:hover,
  button:focus-visible {
    outline: none;
    background: var(--editor-bg-panel-hover);
  }

  span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  small {
    min-width: 0;
    overflow: hidden;
    color: var(--text-muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  p {
    margin: 0;
    padding: 14px;
    color: var(--text-muted);
    font-size: 12px;
    text-align: center;
  }
</style>
