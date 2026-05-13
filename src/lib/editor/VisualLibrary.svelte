<script lang="ts">
  export type VisualLibraryEntry = {
    ref: string;
    package: {
      id: string;
      name: string;
    };
    visual: {
      name: string;
      default_width?: number;
      default_height?: number;
      settings?: string | null;
    };
  };
  type PointerVisualDragState = {
    ref: string;
    name: string;
    packageName: string;
    pointerId: number;
    startX: number;
    startY: number;
    currentX: number;
    currentY: number;
    dragging: boolean;
  };

  let {
    entries,
    search = $bindable(""),
    searchPlaceholder = "Search visuals...",
    emptyLabel = "No visuals available.",
    onadd,
    onplace,
    ondragmove,
    ondragend
  }: {
    entries: VisualLibraryEntry[];
    search?: string;
    searchPlaceholder?: string;
    emptyLabel?: string;
    onadd: (ref: string) => void;
    onplace?: (ref: string, event: PointerEvent) => void;
    ondragmove?: (ref: string, event: PointerEvent) => void;
    ondragend?: () => void;
  } = $props();

  let pointerDrag = $state<PointerVisualDragState | null>(null);
  let suppressClick = $state(false);

  const filteredEntries = $derived.by(() => {
    const needle = search.trim().toLowerCase();
    return entries.filter((entry) => {
      if (!needle) return true;
      return `${entry.package.name} ${entry.package.id} ${entry.visual.name}`.toLowerCase().includes(needle);
    });
  });

  const groupedEntries = $derived.by(() => {
    const groups = new Map<string, { package: VisualLibraryEntry["package"]; entries: VisualLibraryEntry[] }>();
    for (const entry of filteredEntries) {
      const group = groups.get(entry.package.id);
      if (group) {
        group.entries.push(entry);
      } else {
        groups.set(entry.package.id, { package: entry.package, entries: [entry] });
      }
    }
    return [...groups.values()].sort((a, b) => a.package.name.localeCompare(b.package.name));
  });
  const dragPreview = $derived(pointerDrag?.dragging === true ? pointerDrag : null);

  function visualMeta(entry: VisualLibraryEntry) {
    const width = Math.round(Number(entry.visual.default_width) || 0);
    const height = Math.round(Number(entry.visual.default_height) || 0);
    return width > 0 && height > 0 ? `${width}x${height}` : entry.package.id;
  }

  function clearPointerDrag(event?: PointerEvent) {
    if (event?.currentTarget instanceof HTMLElement && event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    if (pointerDrag?.dragging) ondragend?.();
    pointerDrag = null;
  }

  function startVisualPointerDrag(event: PointerEvent, entry: VisualLibraryEntry) {
    if (event.button !== 0) return;
    event.stopPropagation();
    if (event.currentTarget instanceof HTMLElement) {
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    pointerDrag = {
      ref: entry.ref,
      name: entry.visual.name,
      packageName: entry.package.name,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      currentX: event.clientX,
      currentY: event.clientY,
      dragging: false
    };
  }

  function moveVisualPointerDrag(event: PointerEvent) {
    if (!pointerDrag || pointerDrag.pointerId !== event.pointerId) return;
    if (event.buttons === 0) {
      clearPointerDrag(event);
      return;
    }
    const distance = Math.hypot(event.clientX - pointerDrag.startX, event.clientY - pointerDrag.startY);
    if (!pointerDrag.dragging && distance < 4) {
      pointerDrag = { ...pointerDrag, currentX: event.clientX, currentY: event.clientY };
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const nextDrag = { ...pointerDrag, currentX: event.clientX, currentY: event.clientY, dragging: true };
    pointerDrag = nextDrag;
    ondragmove?.(nextDrag.ref, event);
  }

  function endVisualPointerDrag(event: PointerEvent) {
    if (!pointerDrag || pointerDrag.pointerId !== event.pointerId) return;
    const state = pointerDrag;
    clearPointerDrag(event);
    if (!state.dragging) return;
    event.preventDefault();
    event.stopPropagation();
    suppressClick = true;
    onplace?.(state.ref, event);
    setTimeout(() => {
      suppressClick = false;
    }, 0);
  }

  function clickVisual(event: MouseEvent, entry: VisualLibraryEntry) {
    if (suppressClick) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    onadd(entry.ref);
  }

  function dragPreviewStyle(x: number, y: number) {
    return `left:${x}px;top:${y}px;`;
  }
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
      {#each groupedEntries as group (group.package.id)}
        <section class="visual-group" aria-label={group.package.name}>
          <header class="group-title">
            <span>{group.package.name}</span>
            <small>{group.entries.length}</small>
          </header>
          {#each group.entries as entry (entry.ref)}
            <div class="visual-list-item" class:dragging={dragPreview?.ref === entry.ref}>
              <button
                class="visual-grip"
                type="button"
                aria-label={`Drag ${entry.visual.name}`}
                title="Drag to place"
                onpointerdown={(event) => startVisualPointerDrag(event, entry)}
                onpointermove={moveVisualPointerDrag}
                onpointerup={endVisualPointerDrag}
                onpointercancel={clearPointerDrag}
              ></button>
              <button
                class="visual-pick"
                type="button"
                onclick={(event) => clickVisual(event, entry)}
                title={`${entry.visual.name} - ${entry.package.name}`}
              >
                <span class="item-text">
                  <span class="title">{entry.visual.name}</span>
                  <span class="sub">{visualMeta(entry)}</span>
                </span>
                <svg class="add-icon" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="12" y1="5" x2="12" y2="19"></line>
                  <line x1="5" y1="12" x2="19" y2="12"></line>
                </svg>
              </button>
            </div>
          {/each}
        </section>
      {/each}
    </div>
  {:else}
    <p class="visual-empty">{emptyLabel}</p>
  {/if}
</div>

{#if dragPreview}
  <div class="visual-drag-preview" style={dragPreviewStyle(dragPreview.currentX, dragPreview.currentY)} aria-hidden="true">
    <span class="visual-drag-preview-grip"></span>
    <span class="item-text">
      <span class="title">{dragPreview.name}</span>
      <span class="sub">{dragPreview.packageName}</span>
    </span>
  </div>
{/if}

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
    gap: 10px;
  }

  .visual-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .group-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .group-title span {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .group-title small {
    flex: none;
    font: inherit;
    opacity: 0.7;
  }

  .visual-list-item {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 6px;
    padding: 5px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-panel-hover) 70%, transparent);
    color: var(--text-primary);
    transition: var(--transition);
  }

  .visual-list-item:hover {
    border-color: var(--border-color-focus);
    background: var(--editor-bg-panel-hover);
  }

  .visual-list-item.dragging {
    opacity: 0.5;
  }

  .visual-grip {
    display: grid;
    width: 14px;
    height: 28px;
    flex: none;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    cursor: grab;
    opacity: 0;
    touch-action: none;
    transition: opacity 0.16s;
  }

  .visual-grip:active {
    cursor: grabbing;
  }

  .visual-grip::before,
  .visual-drag-preview-grip {
    content: "";
    width: 9px;
    height: 10px;
    background:
      linear-gradient(var(--text-muted), var(--text-muted)) 0 1px / 9px 1px no-repeat,
      linear-gradient(var(--text-muted), var(--text-muted)) 0 5px / 9px 1px no-repeat,
      linear-gradient(var(--text-muted), var(--text-muted)) 0 9px / 9px 1px no-repeat;
  }

  .visual-list-item:hover .visual-grip,
  .visual-list-item:focus-within .visual-grip,
  .visual-list-item.dragging .visual-grip {
    opacity: 1;
  }

  .visual-pick {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    gap: 8px;
    min-height: 28px;
    padding: 3px 5px 3px 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .visual-pick:focus-visible {
    outline: none;
    background: color-mix(in srgb, var(--editor-bg-panel-hover) 80%, transparent);
  }

  .visual-drag-preview {
    position: fixed;
    z-index: 100000;
    display: flex;
    width: min(240px, calc(100vw - 32px));
    min-height: 38px;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--border-color));
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg-panel) 94%, var(--accent));
    box-shadow: 0 12px 28px rgb(0 0 0 / 0.34);
    color: var(--text-primary);
    pointer-events: none;
    transform: translate(12px, 10px) rotate(1deg);
  }

  .visual-drag-preview-grip {
    flex: none;
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

  .visual-list-item:hover .add-icon,
  .visual-list-item:focus-within .add-icon {
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
