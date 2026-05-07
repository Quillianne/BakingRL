<script lang="ts">
  export type VisualLibraryEntry = {
    ref: string;
    package: {
      id: string;
      name: string;
    };
    visual: {
      name: string;
    };
  };

  let {
    entries,
    search = $bindable(""),
    searchPlaceholder = "Search visuals...",
    emptyLabel = "No visuals available.",
    onadd
  }: {
    entries: VisualLibraryEntry[];
    search?: string;
    searchPlaceholder?: string;
    emptyLabel?: string;
    onadd: (ref: string) => void;
  } = $props();

  const filteredEntries = $derived(
    entries.filter((entry) => {
      const needle = search.trim().toLowerCase();
      if (!needle) return true;
      return `${entry.package.name} ${entry.package.id} ${entry.visual.name}`.toLowerCase().includes(needle);
    })
  );
</script>

<div class="visual-library">
  <div class="visual-search">
    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
      <circle cx="11" cy="11" r="8"></circle>
      <line x1="21" y1="21" x2="16.65" y2="16.65"></line>
    </svg>
    <input bind:value={search} placeholder={searchPlaceholder} />
  </div>

  {#if filteredEntries.length}
    <div class="visual-list">
      {#each filteredEntries as entry (entry.ref)}
        <button class="visual-list-item" onclick={() => onadd(entry.ref)}>
          <span class="item-icon">
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect>
            </svg>
          </span>
          <span class="item-text">
            <span class="title">{entry.visual.name}</span>
            <span class="sub">{entry.package.name}</span>
          </span>
          <svg class="add-icon" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="12" y1="5" x2="12" y2="19"></line>
            <line x1="5" y1="12" x2="19" y2="12"></line>
          </svg>
        </button>
      {/each}
    </div>
  {:else}
    <p class="visual-empty">{emptyLabel}</p>
  {/if}
</div>

<style>
  .visual-library {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 10px;
  }

  .visual-search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
  }

  .visual-search svg {
    flex: none;
    color: var(--text-secondary);
  }

  .visual-search input {
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: 13px;
  }

  .visual-list {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 4px;
  }

  .visual-list-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-panel-hover) 70%, transparent);
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
    transition: var(--transition);
  }

  .visual-list-item:hover {
    border-color: var(--border-color-focus);
    background: var(--editor-bg-panel-hover);
  }

  .item-icon {
    display: flex;
    flex: none;
    color: var(--accent);
  }

  .item-text {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
  }

  .title {
    overflow: hidden;
    font-size: 13px;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub {
    overflow: hidden;
    color: var(--text-muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .add-icon {
    flex: none;
    color: var(--text-secondary);
    opacity: 0;
    transition: opacity 0.2s;
  }

  .visual-list-item:hover .add-icon {
    opacity: 1;
  }

  .visual-empty {
    margin: 0;
    padding: 12px;
    color: var(--text-muted);
    font-size: 12px;
    text-align: center;
  }
</style>
