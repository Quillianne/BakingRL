<script lang="ts">
  import { sortItemsForDisplay, type EditorItemModel, type EditorLayerModel, type ItemDropPosition } from "$lib/editor/model";

  type Layer = EditorLayerModel<EditorItemModel>;
  type ItemDropTarget = {
    layerId: string;
    itemId: string | null;
    position: ItemDropPosition;
  };
  type PointerDragState = {
    layerId: string;
    itemId: string;
    pointerId: number;
    startX: number;
    startY: number;
    currentX: number;
    currentY: number;
    dragging: boolean;
  };
  type ItemContextMenuState = {
    layerId: string;
    itemId: string;
    x: number;
    y: number;
  };

  let {
    layers,
    activeLayerId = "",
    selectedItemId = "",
    title = "Layers",
    addLayerLabel = "Add layer",
    addEventLayerLabel = "Add event layer",
    allowEventLayer = false,
    canDeleteLayer = () => true,
    canMoveLayer = () => true,
    onaddlayer,
    onaddeventlayer,
    onselectlayer,
    onselectitem,
    onrenamelayer,
    oncommitlayer,
    ontogglelayervisible,
    ontogglelayerlock,
    onmovelayer,
    ondeletelayer,
    ontoggleitemvisible,
    ontoggleitemlock,
    onreorderitem,
    onduplicateitem,
    ondeleteitem,
    externalDropTarget = null
  }: {
    layers: Layer[];
    activeLayerId?: string;
    selectedItemId?: string;
    title?: string;
    addLayerLabel?: string;
    addEventLayerLabel?: string;
    allowEventLayer?: boolean;
    canDeleteLayer?: (layer: any) => boolean;
    canMoveLayer?: (layer: any) => boolean;
    onaddlayer?: () => void;
    onaddeventlayer?: () => void;
    onselectlayer: (layer: any) => void;
    onselectitem: (layer: any, item: any) => void;
    onrenamelayer: (layer: any, name: string) => void;
    oncommitlayer?: (layer: any) => void;
    ontogglelayervisible: (layer: any) => void;
    ontogglelayerlock: (layer: any) => void;
    onmovelayer: (layer: any, direction: -1 | 1) => void;
    ondeletelayer: (layer: any) => void;
    ontoggleitemvisible: (layer: any, item: any) => void;
    ontoggleitemlock: (layer: any, item: any) => void;
    onreorderitem: (
      sourceLayer: any,
      item: any,
      targetLayer: any,
      targetItem: any | null,
      position: ItemDropPosition
    ) => void;
    onduplicateitem?: (layer: any, item: any) => void;
    ondeleteitem?: (layer: any, item: any) => void;
    externalDropTarget?: ItemDropTarget | null;
  } = $props();

  let pointerDrag = $state<PointerDragState | null>(null);
  let dropTarget = $state<ItemDropTarget | null>(null);
  let itemContextMenu = $state<ItemContextMenuState | null>(null);
  let panel: HTMLElement | undefined = $state();
  const activeDropTarget = $derived(externalDropTarget ?? dropTarget);
  const dragPreview = $derived.by(() => {
    if (!pointerDrag?.dragging) return null;
    const layer = layerById(pointerDrag.layerId);
    const item = layer ? itemById(layer, pointerDrag.itemId) : null;
    if (!layer || !item) return null;
    return { layer, item, x: pointerDrag.currentX, y: pointerDrag.currentY };
  });

  function layerById(layerId: string) {
    return layers.find((layer) => layer.id === layerId) ?? null;
  }

  function itemById(layer: Layer, itemId: string) {
    return layer.items.find((item) => item.id === itemId) ?? null;
  }

  function canDragItem(layer: Layer, item: EditorItemModel) {
    return layer.locked !== true && item.locked !== true;
  }

  function closeItemContextMenu() {
    itemContextMenu = null;
  }

  function clearPointerDrag(event?: PointerEvent) {
    if (event?.currentTarget instanceof HTMLElement && event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    pointerDrag = null;
    dropTarget = null;
  }

  function openItemContextMenu(event: MouseEvent, layer: Layer, item: EditorItemModel) {
    event.preventDefault();
    event.stopPropagation();
    closeItemContextMenu();
    onselectitem(layer, item);
    itemContextMenu = {
      layerId: layer.id,
      itemId: item.id,
      x: event.clientX,
      y: event.clientY
    };
  }

  function itemContextMenuStyle() {
    if (!itemContextMenu) return "";
    return `left:${itemContextMenu.x}px;top:${itemContextMenu.y}px;`;
  }

  function contextMenuEntry() {
    if (!itemContextMenu) return null;
    const layer = layerById(itemContextMenu.layerId);
    const item = layer ? itemById(layer, itemContextMenu.itemId) : null;
    return layer && item ? { layer, item } : null;
  }

  function runItemContextAction(action: (layer: Layer, item: EditorItemModel) => void) {
    const entry = contextMenuEntry();
    closeItemContextMenu();
    if (!entry) return;
    action(entry.layer, entry.item);
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") closeItemContextMenu();
  }

  function layerRowAtPoint(clientX: number, clientY: number) {
    if (!panel) return null;
    const panelRect = panel.getBoundingClientRect();
    if (
      clientX < panelRect.left - 48 ||
      clientX > panelRect.right + 48 ||
      clientY < panelRect.top ||
      clientY > panelRect.bottom
    ) {
      return null;
    }

    const rows = Array.from(panel.querySelectorAll<HTMLElement>(".layer-row[data-editor-layer-id]"));
    for (const row of rows) {
      const rect = row.getBoundingClientRect();
      if (clientY <= rect.bottom) return row;
    }
    return rows.at(-1) ?? null;
  }

  function dropTargetForLayerAtY(layer: Layer, layerRow: HTMLElement, clientY: number): ItemDropTarget | null {
    const itemRows = Array.from(layerRow.querySelectorAll<HTMLElement>(".item-row[data-editor-item-id]"))
      .filter((row) => {
        const itemId = row.dataset.editorItemId;
        return itemId && (layer.id !== pointerDrag?.layerId || itemId !== pointerDrag.itemId);
      });

    if (!itemRows.length) {
      return layer.id === pointerDrag?.layerId ? null : { layerId: layer.id, itemId: null, position: "end" };
    }

    for (const row of itemRows) {
      const itemId = row.dataset.editorItemId;
      const item = itemId ? itemById(layer, itemId) : null;
      if (!item) continue;
      const rect = row.getBoundingClientRect();
      if (clientY < rect.top + rect.height / 2) {
        return { layerId: layer.id, itemId: item.id, position: "before" };
      }
    }

    const lastRow = itemRows.at(-1);
    const lastItemId = lastRow?.dataset.editorItemId;
    const lastItem = lastItemId ? itemById(layer, lastItemId) : null;
    return lastItem
      ? { layerId: layer.id, itemId: lastItem.id, position: "after" }
      : { layerId: layer.id, itemId: null, position: "end" };
  }

  function updatePointerDropTarget(event: PointerEvent) {
    if (!pointerDrag?.dragging) return;

    const layerRow = layerRowAtPoint(event.clientX, event.clientY);
    const layerId = layerRow?.dataset.editorLayerId;
    const targetLayer = layerId ? layerById(layerId) : null;
    if (targetLayer && layerRow && !targetLayer.locked) {
      dropTarget = dropTargetForLayerAtY(targetLayer, layerRow, event.clientY);
      return;
    }

    dropTarget = null;
  }

  function startItemPointerDrag(event: PointerEvent, layer: Layer, item: EditorItemModel) {
    if (event.button !== 0 || !canDragItem(layer, item)) return;
    event.preventDefault();
    event.stopPropagation();
    closeItemContextMenu();
    if (event.currentTarget instanceof HTMLElement) {
      event.currentTarget.setPointerCapture(event.pointerId);
    }
    pointerDrag = {
      layerId: layer.id,
      itemId: item.id,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      currentX: event.clientX,
      currentY: event.clientY,
      dragging: false
    };
    dropTarget = null;
    onselectitem(layer, item);
  }

  function moveItemPointerDrag(event: PointerEvent) {
    if (!pointerDrag || pointerDrag.pointerId !== event.pointerId) return;
    if (event.buttons === 0) {
      clearPointerDrag(event);
      return;
    }
    event.preventDefault();
    event.stopPropagation();

    const distance = Math.hypot(event.clientX - pointerDrag.startX, event.clientY - pointerDrag.startY);
    if (!pointerDrag.dragging && distance < 4) {
      pointerDrag = { ...pointerDrag, currentX: event.clientX, currentY: event.clientY };
      return;
    }
    pointerDrag = { ...pointerDrag, currentX: event.clientX, currentY: event.clientY, dragging: true };
    updatePointerDropTarget(event);
  }

  function endItemPointerDrag(event: PointerEvent) {
    if (!pointerDrag || pointerDrag.pointerId !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();

    const state = pointerDrag;
    const target = dropTarget;
    clearPointerDrag(event);
    if (!state.dragging || !target) return;

    const sourceLayer = layerById(state.layerId);
    const item = sourceLayer ? itemById(sourceLayer, state.itemId) : null;
    const targetLayer = layerById(target.layerId);
    const targetItem = targetLayer && target.itemId ? itemById(targetLayer, target.itemId) : null;
    if (!sourceLayer || !item || !targetLayer || (target.itemId && !targetItem)) return;
    onreorderitem(sourceLayer, item, targetLayer, targetItem, target.position);
  }

  function dropClass(layer: Layer, item: EditorItemModel, position: ItemDropPosition) {
    return activeDropTarget?.layerId === layer.id && activeDropTarget.itemId === item.id && activeDropTarget.position === position;
  }

  function layerDropActive(layer: Layer) {
    return activeDropTarget?.layerId === layer.id && activeDropTarget.itemId === null;
  }

  function itemDragging(layer: Layer, item: EditorItemModel) {
    return pointerDrag?.dragging === true && pointerDrag.layerId === layer.id && pointerDrag.itemId === item.id;
  }

  function dragPreviewStyle(x: number, y: number) {
    return `left:${x}px;top:${y}px;`;
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<section bind:this={panel} class="layer-panel" class:dragging={pointerDrag?.dragging === true} aria-label={title}>
  <header class="panel-head">
    <h2>{title}</h2>
    <div class="panel-actions">
      {#if allowEventLayer && onaddeventlayer}
        <button class="icon-btn" type="button" onclick={onaddeventlayer} title={addEventLayerLabel} aria-label={addEventLayerLabel}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M13 2 3 14h9l-1 8 10-12h-9z"></path></svg>
        </button>
      {/if}
      {#if onaddlayer}
        <button class="icon-btn" type="button" onclick={onaddlayer} title={addLayerLabel} aria-label={addLayerLabel}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 5v14M5 12h14"></path></svg>
        </button>
      {/if}
    </div>
  </header>

  <div class="layer-list" role="list">
    {#each layers as layer (layer.id)}
      <article
        class="layer-row"
        class:active={activeLayerId === layer.id}
        class:event={layer.kind === "event"}
        class:dropEnd={layerDropActive(layer)}
        data-editor-layer-id={layer.id}
        role="listitem"
      >
        <div class="layer-main">
          <button class="grab-zone" type="button" onclick={() => onselectlayer(layer)} title="Select layer" aria-label={`Select ${layer.name}`}>
            <span></span>
          </button>
          <input
            class="layer-name"
            value={layer.name}
            readonly={layer.kind === "event"}
            oninput={(event) => onrenamelayer(layer, event.currentTarget.value)}
            onblur={() => oncommitlayer?.(layer)}
            onfocus={() => onselectlayer(layer)}
            onclick={() => onselectlayer(layer)}
            aria-label={`Layer name ${layer.name}`}
          />
          <div class="row-actions">
            <button class="icon-btn" type="button" onclick={() => ontogglelayervisible(layer)} title={layer.visible ? "Hide" : "Show"} aria-label={layer.visible ? "Hide layer" : "Show layer"}>
              {#if layer.visible}
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"></path><circle cx="12" cy="12" r="3"></circle></svg>
              {:else}
                <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m3 3 18 18M10.6 10.6A3 3 0 0 0 12 15a3 3 0 0 0 2.1-.9M9.9 5.5A10.8 10.8 0 0 1 12 5c6 0 10 7 10 7a13.9 13.9 0 0 1-2.3 3.2M6.4 6.7C3.7 8.5 2 12 2 12s4 7 10 7c1.2 0 2.3-.3 3.3-.7"></path></svg>
              {/if}
            </button>
            <button class="icon-btn" type="button" onclick={() => ontogglelayerlock(layer)} title={layer.locked ? "Unlock" : "Lock"} aria-label={layer.locked ? "Unlock layer" : "Lock layer"}>
              {#if layer.locked}
                <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="11" width="16" height="9" rx="2"></rect><path d="M8 11V8a4 4 0 0 1 8 0v3"></path></svg>
              {:else}
                <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="11" width="16" height="9" rx="2"></rect><path d="M8 11V8a4 4 0 0 1 7.5-2"></path></svg>
              {/if}
            </button>
            <button class="icon-btn" type="button" onclick={() => onmovelayer(layer, -1)} disabled={!canMoveLayer(layer)} title="Move up" aria-label="Move layer up">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 15 6-6 6 6"></path></svg>
            </button>
            <button class="icon-btn" type="button" onclick={() => onmovelayer(layer, 1)} disabled={!canMoveLayer(layer)} title="Move down" aria-label="Move layer down">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6"></path></svg>
            </button>
            <button class="icon-btn danger" type="button" onclick={() => ondeletelayer(layer)} disabled={!canDeleteLayer(layer)} title="Delete" aria-label="Delete layer">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"></path></svg>
            </button>
          </div>
        </div>

        {#if layer.items.length}
          <div class="item-list" role="list" aria-label={`${layer.name} items`}>
            {#each sortItemsForDisplay(layer.items) as item (item.id)}
              <div
                class="item-row"
                class:selected={selectedItemId === item.id}
                class:hidden={item.visible === false || layer.visible === false}
                class:locked={item.locked || layer.locked}
                class:canDrag={canDragItem(layer, item)}
                class:dragging={itemDragging(layer, item)}
                class:dropBefore={dropClass(layer, item, "before")}
                class:dropAfter={dropClass(layer, item, "after")}
                data-editor-layer-id={layer.id}
                data-editor-item-id={item.id}
                role="listitem"
                oncontextmenu={(event) => openItemContextMenu(event, layer, item)}
              >
                <button
                  class="item-grip"
                  type="button"
                  disabled={!canDragItem(layer, item)}
                  aria-label={`Move ${item.name}`}
                  title="Drag to reorder"
                  onpointerdown={(event) => startItemPointerDrag(event, layer, item)}
                  onpointermove={moveItemPointerDrag}
                  onpointerup={endItemPointerDrag}
                  onpointercancel={clearPointerDrag}
                ></button>
                <button class="item-pick" type="button" onclick={() => { closeItemContextMenu(); onselectitem(layer, item); }} title={item.name} aria-label={`Select ${item.name}`}>
                  <span class="item-name">{item.name}</span>
                  <span class="item-z">z{item.z_index}</span>
                </button>
                <div class="row-actions item-actions">
                  <button class="icon-btn" type="button" onclick={() => ontoggleitemvisible(layer, item)} title={item.visible ? "Hide" : "Show"} aria-label={item.visible ? "Hide item" : "Show item"}>
                    {#if item.visible}
                      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z"></path><circle cx="12" cy="12" r="3"></circle></svg>
                    {:else}
                      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m3 3 18 18M10.6 10.6A3 3 0 0 0 12 15a3 3 0 0 0 2.1-.9M9.9 5.5A10.8 10.8 0 0 1 12 5c6 0 10 7 10 7a13.9 13.9 0 0 1-2.3 3.2M6.4 6.7C3.7 8.5 2 12 2 12s4 7 10 7c1.2 0 2.3-.3 3.3-.7"></path></svg>
                    {/if}
                  </button>
                  <button class="icon-btn" type="button" onclick={() => ontoggleitemlock(layer, item)} title={item.locked ? "Unlock" : "Lock"} aria-label={item.locked ? "Unlock item" : "Lock item"}>
                    {#if item.locked}
                      <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="11" width="16" height="9" rx="2"></rect><path d="M8 11V8a4 4 0 0 1 8 0v3"></path></svg>
                    {:else}
                      <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="11" width="16" height="9" rx="2"></rect><path d="M8 11V8a4 4 0 0 1 7.5-2"></path></svg>
                    {/if}
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="empty-layer">Empty</div>
        {/if}
      </article>
    {/each}
  </div>
</section>

{#if dragPreview}
  <div class="drag-preview" style={dragPreviewStyle(dragPreview.x, dragPreview.y)} aria-hidden="true">
    <span class="drag-preview-grip"></span>
    <span class="drag-preview-main">
      <span class="drag-preview-name">{dragPreview.item.name}</span>
      <span class="drag-preview-layer">{dragPreview.layer.name}</span>
    </span>
    <span class="drag-preview-z">z{dragPreview.item.z_index}</span>
  </div>
{/if}

{#if itemContextMenu && contextMenuEntry()}
  <div class="layer-context-menu" style={itemContextMenuStyle()} role="menu">
    {#if onduplicateitem}
      <button role="menuitem" type="button" onclick={() => runItemContextAction(onduplicateitem)}>Duplicate</button>
    {/if}
    <button role="menuitem" type="button" onclick={() => runItemContextAction(ontoggleitemvisible)}>
      {contextMenuEntry()?.item.visible ? "Hide" : "Show"}
    </button>
    <button role="menuitem" type="button" onclick={() => runItemContextAction(ontoggleitemlock)}>
      {contextMenuEntry()?.item.locked ? "Unlock" : "Lock"}
    </button>
    {#if ondeleteitem}
      <button role="menuitem" type="button" class="danger" onclick={() => runItemContextAction(ondeleteitem)}>Delete</button>
    {/if}
  </div>
{/if}

<style>
  .layer-panel {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 8px;
  }

  .layer-panel.dragging {
    user-select: none;
  }

  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .panel-head h2 {
    margin: 0;
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .panel-actions,
  .row-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .layer-list {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 6px;
  }

  .layer-row {
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-panel-hover) 48%, transparent);
  }

  .layer-row.active {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 24%, transparent);
  }

  .layer-row.dropEnd {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, var(--bg-panel-hover));
  }

  .layer-row.event {
    border-style: dashed;
  }

  .layer-main,
  .item-row {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 6px;
    padding: 5px;
  }

  .grab-zone {
    display: grid;
    width: 20px;
    height: 24px;
    flex: none;
    place-items: center;
    border: 0;
    border-radius: 4px;
    background: transparent;
    cursor: pointer;
  }

  .grab-zone span {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    border: 2px solid var(--text-muted);
  }

  .layer-row.active .grab-zone span {
    border-color: var(--accent);
    background: var(--accent);
  }

  .layer-name {
    min-width: 0;
    flex: 1;
    height: 26px;
    padding: 0 6px;
    border: 1px solid transparent;
    border-radius: 4px;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: 12px;
    font-weight: 650;
  }

  .layer-name:focus {
    border-color: var(--border-color-focus);
    outline: none;
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
  }

  .item-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 5px 5px 31px;
  }

  .item-row {
    position: relative;
    padding: 2px;
    border-radius: 5px;
    color: var(--text-secondary);
  }

  .item-row.dropBefore::before,
  .item-row.dropAfter::after {
    content: "";
    position: absolute;
    right: 4px;
    left: 4px;
    height: 2px;
    border-radius: 999px;
    background: var(--accent);
    box-shadow: 0 0 10px color-mix(in srgb, var(--accent) 70%, transparent);
    pointer-events: none;
  }

  .item-row.dropBefore::before {
    top: -2px;
  }

  .item-row.dropAfter::after {
    bottom: -2px;
  }

  .item-row.selected {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--text-primary);
  }

  .item-row.hidden {
    opacity: 0.55;
  }

  .item-row.dragging {
    opacity: 0.45;
  }

  .drag-preview {
    position: fixed;
    z-index: 100000;
    display: flex;
    width: min(260px, calc(100vw - 32px));
    min-height: 34px;
    align-items: center;
    gap: 8px;
    padding: 6px 9px;
    border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--border-color));
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg-panel) 94%, var(--accent));
    box-shadow: 0 12px 28px rgb(0 0 0 / 0.34);
    color: var(--text-primary);
    pointer-events: none;
    transform: translate(12px, 10px) rotate(1deg);
  }

  .drag-preview-grip {
    width: 10px;
    height: 14px;
    flex: none;
    background:
      linear-gradient(var(--text-muted), var(--text-muted)) 0 2px / 10px 1px no-repeat,
      linear-gradient(var(--text-muted), var(--text-muted)) 0 7px / 10px 1px no-repeat,
      linear-gradient(var(--text-muted), var(--text-muted)) 0 12px / 10px 1px no-repeat;
  }

  .drag-preview-main {
    display: grid;
    min-width: 0;
    flex: 1;
    gap: 1px;
  }

  .drag-preview-name,
  .drag-preview-layer {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .drag-preview-name {
    font-size: 12px;
    font-weight: 650;
  }

  .drag-preview-layer {
    color: var(--text-muted);
    font-size: 10px;
  }

  .drag-preview-z {
    flex: none;
    color: var(--text-muted);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }

  .layer-context-menu {
    position: fixed;
    z-index: 100001;
    display: grid;
    min-width: 150px;
    overflow: hidden;
    padding: 4px;
    border: 1px solid var(--border-color);
    border-radius: 6px;
    background: var(--editor-bg-panel);
    box-shadow: 0 14px 34px rgb(0 0 0 / 0.32);
    pointer-events: auto;
  }

  .layer-context-menu button {
    display: flex;
    align-items: center;
    width: 100%;
    height: 28px;
    padding: 0 9px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-primary);
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }

  .layer-context-menu button:hover,
  .layer-context-menu button:focus-visible {
    outline: none;
    background: var(--editor-bg-panel-hover);
  }

  .layer-context-menu button.danger {
    color: var(--danger);
  }

  .item-row.locked .item-name::after {
    content: " locked";
    color: var(--warn);
    font-size: 10px;
    font-weight: 500;
  }

  .item-grip {
    display: grid;
    width: 14px;
    height: 24px;
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

  .item-grip:active {
    cursor: grabbing;
  }

  .item-grip:disabled {
    cursor: default;
  }

  .item-grip::before {
    content: "";
    width: 9px;
    height: 10px;
    background:
      linear-gradient(var(--text-muted), var(--text-muted)) 0 1px / 9px 1px no-repeat,
      linear-gradient(var(--text-muted), var(--text-muted)) 0 5px / 9px 1px no-repeat,
      linear-gradient(var(--text-muted), var(--text-muted)) 0 9px / 9px 1px no-repeat;
  }

  .item-row.canDrag:hover .item-grip,
  .item-row.canDrag:focus-within .item-grip,
  .item-row.canDrag.selected .item-grip {
    opacity: 1;
  }

  .item-pick {
    display: flex;
    min-width: 0;
    flex: 1;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    height: 24px;
    padding: 0 6px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }

  .item-pick:hover {
    background: color-mix(in srgb, var(--editor-bg-panel-hover) 80%, transparent);
  }

  .item-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-z {
    flex: none;
    color: var(--text-muted);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }

  .item-actions {
    opacity: 0;
    transition: opacity 0.16s;
  }

  .item-row:hover .item-actions,
  .item-row:focus-within .item-actions,
  .item-row.selected .item-actions {
    opacity: 1;
  }

  .icon-btn {
    display: grid;
    width: 24px;
    height: 24px;
    flex: none;
    place-items: center;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .icon-btn:hover:not(:disabled),
  .icon-btn:focus-visible {
    outline: none;
    background: var(--editor-bg-panel-hover);
    color: var(--text-primary);
  }

  .icon-btn:disabled {
    cursor: default;
    opacity: 0.35;
  }

  .icon-btn.danger:hover:not(:disabled) {
    color: var(--danger);
  }

  .icon-btn svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .empty-layer {
    padding: 0 8px 8px 36px;
    color: var(--text-muted);
    font-size: 11px;
  }
</style>
