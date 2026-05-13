<script lang="ts">
  import type { Snippet } from "svelte";
  import EditorTemplate from "$lib/EditorTemplate.svelte";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
  import CommandPalette, { type EditorCommand } from "$lib/editor/CommandPalette.svelte";
  import {
    hitTestItemEntries,
    itemStyle as editorItemStyle,
    moveItemByZIndex,
    moveItemToStackEdge,
    resizeItemFromDelta,
    type EditorCanvasModel,
    type EditorItemEntry,
    type EditorItemModel,
    type EditorLayerModel,
    type PlacementPoint
  } from "$lib/editor/model";
  import { emptySnapGuides, snapGuideStyle, snapItemPosition, type SnapGuides } from "$lib/editor/snapping";
  import { packageRuntime } from "$lib/packageRuntime.svelte";

  type WorkspaceItem = EditorItemModel & {
    kind?: "visual" | "text" | "image" | "shape";
    package_id?: string | null;
    export_name?: string | null;
    settings: Record<string, unknown>;
  };

  type WorkspaceLayer = EditorLayerModel<WorkspaceItem> & { kind: "normal" | "event" };
  type WorkspaceCanvas = EditorCanvasModel & {
    id: string;
    name: string;
    layers: WorkspaceLayer[];
    items?: WorkspaceItem[];
  };
  type RendererMode = "runtime" | "editor" | "page";
  type RendererSource = "overlay" | "page" | "configuration";
  type MockEvent = { id: number; event: unknown } | null;
  type DragState = {
    mode: "move" | "resize";
    itemId: string;
    startPointer: PlacementPoint;
    startItem: Pick<WorkspaceItem, "x" | "y" | "width" | "height">;
    clickThroughCandidateIds: string[];
    changed: boolean;
  };
  type PanState = {
    startPointer: { x: number; y: number };
    startPan: { x: number; y: number };
  };
  type ContextMenuState = {
    itemId: string;
    x: number;
    y: number;
  };

  let {
    title,
    message = "",
    canvas,
    layers,
    centerKey = canvas.id,
    canvasAriaLabel,
    layoutId = canvas.id,
    rendererSource = "overlay",
    rendererMode = "editor",
    editable = true,
    mockEvent = null,
    layoutRevision = 0,
    commands = [],
    commandOpen = $bindable(false),
    selectedItemId = $bindable(""),
    activeLayerId = $bindable(""),
    snapEnabled = $bindable(true),
    gridSize = $bindable(20),
    zoom = $bindable(0.48),
    panX = $bindable(0),
    panY = $bindable(0),
    stage = $bindable<HTMLElement | undefined>(),
    stageWrap = $bindable<HTMLElement | undefined>(),
    onSave,
    onRefreshPreview,
    onDuplicateSelected,
    onDeleteSelected,
    onClose,
    onAddVisualAt,
    leftPanel,
    rightPanel,
    overlays
  }: {
    title: string;
    message?: string;
    canvas: WorkspaceCanvas;
    layers: WorkspaceLayer[];
    centerKey?: string;
    canvasAriaLabel: string;
    layoutId?: string | null;
    rendererSource?: RendererSource;
    rendererMode?: RendererMode;
    editable?: boolean;
    mockEvent?: MockEvent;
    layoutRevision?: number;
    commands?: EditorCommand[];
    commandOpen?: boolean;
    selectedItemId?: string;
    activeLayerId?: string;
    snapEnabled?: boolean;
    gridSize?: number;
    zoom?: number;
    panX?: number;
    panY?: number;
    stage?: HTMLElement;
    stageWrap?: HTMLElement;
    onSave: () => void | Promise<void>;
    onRefreshPreview: () => void;
    onDuplicateSelected: () => void;
    onDeleteSelected: () => void;
    onClose: () => void | Promise<void>;
    onAddVisualAt?: (ref: string, placement: PlacementPoint) => void;
    leftPanel: Snippet;
    rightPanel: Snippet;
    overlays?: Snippet;
  } = $props();

  let dragState = $state<DragState | null>(null);
  let panState = $state<PanState | null>(null);
  let contextMenu = $state<ContextMenuState | null>(null);
  let snapGuides = $state<SnapGuides>(emptySnapGuides());
  let eventLayerActive = $state(false);

  const selectedEntry = $derived(findItem(selectedItemId));

  function allItems(): EditorItemEntry<WorkspaceItem, WorkspaceLayer>[] {
    return layers.flatMap((layer) => layer.items.map((item) => ({ layer, item })));
  }

  function eventLayerMasksNormalLayers() {
    return rendererMode !== "page" && eventLayerActive;
  }

  function frameItems() {
    const eventLayerMask = eventLayerMasksNormalLayers();
    return allItems().filter((entry) => !eventLayerMask || entry.layer.kind === "event");
  }

  function findItem(itemId: string) {
    if (!itemId) return null;
    for (const layer of layers) {
      const item = layer.items.find((entry) => entry.id === itemId);
      if (item) return { layer, item };
    }
    return null;
  }

  function selectItemEntry(entry: EditorItemEntry<WorkspaceItem, WorkspaceLayer>) {
    activeLayerId = entry.layer.id;
    selectedItemId = entry.item.id;
  }

  function pointForClient(clientX: number, clientY: number): PlacementPoint {
    if (!stage) return { x: 0, y: 0 };
    const rect = stage.getBoundingClientRect();
    return {
      x: ((clientX - rect.left) / rect.width) * canvas.width,
      y: ((clientY - rect.top) / rect.height) * canvas.height
    };
  }

  function pointForEvent(event: PointerEvent | DragEvent) {
    return pointForClient(event.clientX, event.clientY);
  }

  function pointerTargetForMove(event: PointerEvent, fallbackItem: WorkspaceItem) {
    const fallbackEntry = findItem(fallbackItem.id);
    const candidates = hitTestItemEntries(allItems(), pointForEvent(event));
    const selectedIndex = candidates.findIndex((entry) => entry.item.id === selectedItemId);
    if (selectedIndex !== -1) {
      const nextCandidates = [...candidates.slice(selectedIndex + 1), ...candidates.slice(0, selectedIndex)];
      return {
        entry: candidates[selectedIndex],
        clickThroughCandidateIds: nextCandidates.map((entry) => entry.item.id)
      };
    }
    return {
      entry: candidates[0] ?? fallbackEntry,
      clickThroughCandidateIds: []
    };
  }

  function selectNextClickThroughCandidate(candidateIds: string[]) {
    const nextEntry = candidateIds.map((itemId) => findItem(itemId)).find((entry) => entry !== null);
    if (!nextEntry) return false;
    selectItemEntry(nextEntry);
    return true;
  }

  function clampItem(item: WorkspaceItem) {
    item.width = Math.round(Math.max(24, Math.min(item.width, canvas.width)));
    item.height = Math.round(Math.max(18, Math.min(item.height, canvas.height)));
    item.x = Math.round(Math.max(0, Math.min(item.x, canvas.width - item.width)));
    item.y = Math.round(Math.max(0, Math.min(item.y, canvas.height - item.height)));
  }

  function itemStyle(item: WorkspaceItem) {
    return editorItemStyle(item, canvas);
  }

  function guideStyle(axis: "x" | "y", value: number | null) {
    return snapGuideStyle(axis, value, canvas);
  }

  function refreshPreview() {
    onRefreshPreview();
  }

  function save() {
    void onSave();
  }

  function startPan(event: PointerEvent, detail?: { spacePressed: boolean }) {
    if (event.button !== 1 && !event.altKey && !detail?.spacePressed) return;
    event.preventDefault();
    panState = {
      startPointer: { x: event.clientX, y: event.clientY },
      startPan: { x: panX, y: panY }
    };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function startDrag(event: PointerEvent, item: WorkspaceItem, mode: "move" | "resize") {
    if (!editable || event.button !== 0) return;
    event.stopPropagation();
    event.preventDefault();
    contextMenu = null;
    const target =
      mode === "move"
        ? pointerTargetForMove(event, item)
        : { entry: findItem(item.id), clickThroughCandidateIds: [] };
    const entry = target.entry;
    if (!entry) return;
    selectItemEntry(entry);
    if (entry.item.locked || entry.layer.locked) {
      selectNextClickThroughCandidate(target.clickThroughCandidateIds);
      return;
    }
    dragState = {
      mode,
      itemId: entry.item.id,
      startPointer: pointForEvent(event),
      startItem: {
        x: entry.item.x,
        y: entry.item.y,
        width: entry.item.width,
        height: entry.item.height
      },
      clickThroughCandidateIds: target.clickThroughCandidateIds,
      changed: false
    };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function pointerMove(event: PointerEvent) {
    if (panState) {
      panX = panState.startPan.x + event.clientX - panState.startPointer.x;
      panY = panState.startPan.y + event.clientY - panState.startPointer.y;
      return;
    }
    if (!dragState) return;
    const entry = findItem(dragState.itemId);
    if (!entry) return;
    const pointer = pointForEvent(event);
    const dx = pointer.x - dragState.startPointer.x;
    const dy = pointer.y - dragState.startPointer.y;
    if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) dragState.changed = true;
    if (dragState.mode === "move") {
      entry.item.x = dragState.startItem.x + dx;
      entry.item.y = dragState.startItem.y + dy;
      snapGuides = snapItemPosition(entry.item, canvas, allItems().map((entry) => entry.item), {
        enabled: snapEnabled,
        gridSize
      });
    } else {
      resizeItemFromDelta(entry.item, dragState.startItem, dx, dy, event.shiftKey, {
        minWidth: 24,
        minHeight: 18,
        maxWidth: canvas.width,
        maxHeight: canvas.height
      });
    }
    clampItem(entry.item);
    refreshPreview();
  }

  function pointerUp() {
    if (panState) {
      panState = null;
      return;
    }
    if (!dragState) return;
    const shouldSave = dragState.changed;
    const clickThroughCandidateIds = dragState.clickThroughCandidateIds;
    dragState = null;
    snapGuides = emptySnapGuides();
    if (!shouldSave) {
      selectNextClickThroughCandidate(clickThroughCandidateIds);
      return;
    }
    save();
  }

  function openContextMenu(event: MouseEvent, item: WorkspaceItem) {
    if (!editable) return;
    event.preventDefault();
    event.stopPropagation();
    const target = pointerTargetForMove(event as PointerEvent, item).entry ?? findItem(item.id);
    if (!target) return;
    selectItemEntry(target);
    contextMenu = { itemId: target.item.id, x: event.clientX, y: event.clientY };
  }

  function contextMenuStyle() {
    if (!contextMenu) return "";
    return `left:${contextMenu.x}px;top:${contextMenu.y}px;`;
  }

  function runContextAction(action: () => void) {
    contextMenu = null;
    action();
  }

  function toggleSelectedLock() {
    if (!selectedEntry) return;
    selectedEntry.item.locked = !selectedEntry.item.locked;
    save();
  }

  function toggleSelectedVisible() {
    if (!selectedEntry) return;
    selectedEntry.item.visible = !selectedEntry.item.visible;
    refreshPreview();
    save();
  }

  function moveSelectedInStack(direction: -1 | 1) {
    if (!selectedEntry) return;
    if (moveItemByZIndex(selectedEntry.item, selectedEntry.layer.items, direction)) {
      refreshPreview();
      save();
    }
  }

  function stackSelected(edge: "front" | "back") {
    if (!selectedEntry) return;
    moveItemToStackEdge(selectedEntry.item, selectedEntry.layer.items, edge);
    refreshPreview();
    save();
  }

  function dropVisual(event: DragEvent) {
    const ref =
      event.dataTransfer?.getData("application/x-bakingrl-visual") ||
      event.dataTransfer?.getData("text/plain");
    if (!ref) return;
    onAddVisualAt?.(ref, pointForEvent(event));
  }

  function handleEditorKeyDown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      commandOpen = true;
      return;
    }
    const target = event.target;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement ||
      (target instanceof HTMLElement && target.isContentEditable)
    ) {
      return;
    }
    if (event.shiftKey && event.key.toLowerCase() === "i") {
      event.preventDefault();
      commandOpen = true;
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && selectedEntry) {
      event.preventDefault();
      onDeleteSelected();
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "d" && selectedEntry) {
      event.preventDefault();
      onDuplicateSelected();
      return;
    }
    if ((event.key === "[" || event.key === "]") && selectedEntry) {
      event.preventDefault();
      moveSelectedInStack(event.key === "]" ? 1 : -1);
      return;
    }
    if (!selectedEntry || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
    event.preventDefault();
    const step = event.shiftKey ? 10 : 1;
    if (event.key === "ArrowLeft") selectedEntry.item.x -= step;
    if (event.key === "ArrowRight") selectedEntry.item.x += step;
    if (event.key === "ArrowUp") selectedEntry.item.y -= step;
    if (event.key === "ArrowDown") selectedEntry.item.y += step;
    clampItem(selectedEntry.item);
    refreshPreview();
    save();
  }
</script>

<CommandPalette bind:open={commandOpen} {commands} />

<EditorTemplate
  {title}
  {message}
  {canvas}
  {centerKey}
  {canvasAriaLabel}
  dragging={dragState !== null}
  bind:zoom
  bind:panX
  bind:panY
  bind:stage
  bind:stageWrap
  onPointerMove={pointerMove}
  onPointerUp={pointerUp}
  onStagePointerDown={startPan}
  onKeyDown={handleEditorKeyDown}
  onStageDrop={dropVisual}
  {onClose}
>
  {#snippet stageContent()}
    <OverlayRenderer
      source={rendererSource}
      {layoutId}
      layoutOverride={canvas}
      {layoutRevision}
      packageRevision={packageRuntime.revision}
      mode={rendererMode}
      {mockEvent}
      onEventLayerActiveChange={(active) => (eventLayerActive = active)}
    />
    {#if editable}
      <div class="frames">
        {#each frameItems() as entry}
          <div
            class="frame-box"
            class:selected={selectedItemId === entry.item.id}
            class:hidden={entry.item.visible === false || entry.layer.visible === false}
            class:locked={entry.item.locked || entry.layer.locked}
            role="button"
            tabindex="0"
            style={itemStyle(entry.item)}
            onpointerdown={(event) => startDrag(event, entry.item, "move")}
            oncontextmenu={(event) => openContextMenu(event, entry.item)}
            title={entry.item.name}
          >
            <div class="frame-label">
              <span class="name">{entry.item.name}</span>
              <span class="layer-name">{entry.layer.name}</span>
            </div>
            <button
              type="button"
              class="resize-handle"
              aria-label={`Resize ${entry.item.name}`}
              onpointerdown={(event) => startDrag(event, entry.item, "resize")}
            ></button>
          </div>
        {/each}
      </div>
    {/if}
    {#if snapGuides.x !== null}
      <div class="snap-guide vertical" style={guideStyle("x", snapGuides.x)}></div>
    {/if}
    {#if snapGuides.y !== null}
      <div class="snap-guide horizontal" style={guideStyle("y", snapGuides.y)}></div>
    {/if}
  {/snippet}

  {#snippet leftPanel()}
    {@render leftPanel()}
  {/snippet}

  {#snippet rightPanel()}
    {@render rightPanel()}
  {/snippet}

  {#snippet overlays()}
    {#if contextMenu && selectedEntry}
      <div class="context-menu" style={contextMenuStyle()} role="menu">
        <button role="menuitem" onclick={() => runContextAction(onDuplicateSelected)}>Duplicate</button>
        <button role="menuitem" onclick={() => runContextAction(toggleSelectedLock)}>{selectedEntry.item.locked ? "Unlock" : "Lock"}</button>
        <button role="menuitem" onclick={() => runContextAction(toggleSelectedVisible)}>{selectedEntry.item.visible ? "Hide" : "Show"}</button>
        <button role="menuitem" onclick={() => runContextAction(() => moveSelectedInStack(1))}>Above</button>
        <button role="menuitem" onclick={() => runContextAction(() => moveSelectedInStack(-1))}>Below</button>
        <button role="menuitem" onclick={() => runContextAction(() => stackSelected("front"))}>Front</button>
        <button role="menuitem" onclick={() => runContextAction(() => stackSelected("back"))}>Back</button>
        <button role="menuitem" class="danger" onclick={() => runContextAction(onDeleteSelected)}>Delete</button>
      </div>
    {/if}
    {#if overlays}
      {@render overlays()}
    {/if}
  {/snippet}
</EditorTemplate>

<style>
  .frames {
    position: absolute;
    inset: 0;
    z-index: 100;
    pointer-events: none;
  }

  .frame-box {
    position: absolute;
    box-sizing: border-box;
    padding: 0;
    border: 1px solid color-mix(in srgb, var(--accent) 50%, transparent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    color: var(--text-primary);
    cursor: move;
    pointer-events: auto;
    touch-action: none;
    transition: border-color 0.1s, background-color 0.1s;
  }

  .frame-box:hover {
    border-color: color-mix(in srgb, var(--accent) 78%, transparent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }

  .frame-box.selected {
    border: 2px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .frame-box.hidden {
    border-style: dashed;
    opacity: 0.34;
  }

  .frame-box.locked {
    border-color: color-mix(in srgb, var(--warn) 58%, transparent);
    background: color-mix(in srgb, var(--warn) 6%, transparent);
  }

  .frame-box.locked.selected {
    border-color: var(--warn);
  }

  .frame-label {
    position: absolute;
    top: -24px;
    left: -1px;
    display: flex;
    max-width: 260px;
    gap: 8px;
    overflow: hidden;
    padding: 3px 8px;
    border-radius: 4px 4px 0 0;
    background: var(--accent);
    color: var(--text-primary);
    font-size: 11px;
    opacity: 0;
    pointer-events: none;
    white-space: nowrap;
    transition: opacity 0.1s;
  }

  .frame-box.locked .frame-label {
    background: var(--warn);
  }

  .frame-box:hover .frame-label,
  .frame-box.selected .frame-label {
    opacity: 1;
  }

  .frame-label .name {
    font-weight: 600;
  }

  .frame-label .layer-name {
    opacity: 0.72;
    font-weight: normal;
  }

  .resize-handle {
    position: absolute;
    right: -7px;
    bottom: -7px;
    width: 14px;
    height: 14px;
    padding: 0;
    border: 2px solid var(--accent);
    border-radius: 50%;
    background: var(--text-primary);
    cursor: nwse-resize;
    opacity: 0;
    touch-action: none;
    transition: opacity 0.1s;
  }

  .frame-box.locked .resize-handle {
    display: none;
  }

  .frame-box:hover .resize-handle,
  .frame-box.selected .resize-handle {
    opacity: 1;
  }

  .context-menu {
    position: fixed;
    z-index: 1000;
    display: flex;
    min-width: 160px;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--editor-bg-panel);
    box-shadow: var(--shadow-lg);
  }

  .context-menu button {
    padding: 8px 12px;
    border: 0;
    border-bottom: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-primary);
    text-align: left;
    cursor: pointer;
  }

  .context-menu button:last-child {
    border-bottom: 0;
  }

  .context-menu button:hover {
    background: var(--editor-bg-panel-hover);
  }

  .context-menu button.danger {
    color: var(--danger);
  }
</style>
