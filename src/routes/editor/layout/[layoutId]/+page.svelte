<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import EditorTemplate from "$lib/EditorTemplate.svelte";
  import InstanceSettingsForm from "$lib/InstanceSettingsForm.svelte";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
  import { adapter } from "$lib/adapter/index";
  import { emptySnapGuides, snapGuideStyle, snapItemPosition, type SnapGuides } from "$lib/editor/snapping";
  import { createLayoutThumbnail } from "$lib/layoutThumbnail";
  import { packageRuntime } from "$lib/packageRuntime.svelte";
  import VisualLibrary from "$lib/editor/VisualLibrary.svelte";
  import { routeReturnFromParams, storeRouteScrollRestore } from "$lib/returnState";
  import { RL_TELEMETRY_EVENT_NAMES, telemetryFrameTemplate, type RlTelemetryEventName } from "$lib/rlTelemetry";

  const { data } = $props();

  type VisualExportDescriptor = {
    name: string;
    entry: string;
    default_width: number;
    default_height: number;
  };

  type PackageDescriptor = {
    id: string;
    name: string;
    enabled: boolean;
    exports: {
      visuals: VisualExportDescriptor[];
    };
  };

  type OverlayItem = {
    id: string;
    package_id: string;
    export_name: string;
    name: string;
    x: number;
    y: number;
    width: number;
    height: number;
    z_index: number;
    visible: boolean;
    locked: boolean;
    opacity: number;
    settings: Record<string, unknown>;
  };

  type OverlayLayer = {
    id: string;
    name: string;
    kind: "normal" | "event";
    visible: boolean;
    locked: boolean;
    order: number;
    items: OverlayItem[];
  };

  type OverlayLayout = {
    id: string;
    name: string;
    width: number;
    height: number;
    layers: OverlayLayer[];
    items?: OverlayItem[];
    thumbnail?: string | null;
  };

  type OverlayLayoutCatalog = {
    active_layout_id: string;
    stream_layout_id: string;
    layouts: OverlayLayout[];
  };

  type DragState = {
    mode: "move" | "resize";
    itemId: string;
    startPointer: { x: number; y: number };
    startItem: Pick<OverlayItem, "x" | "y" | "width" | "height">;
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

  type ConfirmRequest = {
    title: string;
    message: string;
    confirmLabel: string;
    run: () => void | Promise<void>;
  };

  let packages = $state<PackageDescriptor[]>([]);
  let overlayLayouts = $state<OverlayLayoutCatalog | null>(null);
  let selectedItemId = $state("");
  let activeLayerId = $state("");
  let loaded = $state(false);
  let loadError = $state("");
  let snapEnabled = $state(true);
  let gridSize = $state(20);
  let visualSearch = $state("");
  let message = $state("");
  let mockEvent = $state<{ id: number; event: unknown } | null>(null);
  let stageWrap = $state<HTMLElement>();
  let stage = $state<HTMLElement>();
  let dragState = $state<DragState | null>(null);
  let panState = $state<PanState | null>(null);
  let contextMenu = $state<ContextMenuState | null>(null);
  let snapGuides = $state<SnapGuides>(emptySnapGuides());
  let confirmRequest = $state<ConfirmRequest | null>(null);
  let layoutRevision = $state(0);
  let zoom = $state(0.48);
  let panX = $state(0);
  let panY = $state(0);
  let dragLayerId = $state("");

  // Accordion state
  let showOverlay = $state(true);
  let showVisuals = $state(false);
  let showLayers = $state(true);
  let showMocks = $state(false);
  let showSettings = $state(true);

  const layout = $derived(overlayLayouts?.layouts.find((entry) => entry.id === data.layoutId) ?? null);
  const layers = $derived(layout ? sortedLayers(layout) : []);
  const activeLayer = $derived(
    layers.find((layer) => layer.id === activeLayerId) ??
      layers.find((layer) => layer.kind === "normal") ??
      layers[0] ??
      null
  );
  const selectedEntry = $derived(findItem(selectedItemId));
  const visualExports = $derived(
    packages.filter((pkg) => pkg.enabled).flatMap((pkg) =>
      pkg.exports.visuals.map((visual) => ({
        package: pkg,
        visual,
        ref: `${pkg.id}/${visual.name}`
      }))
    )
  );

  $effect(() => {
    if (!activeLayerId && layers.length) {
      activeLayerId = layers.find((layer) => layer.kind === "normal")?.id ?? layers[0].id;
    }
  });

  async function refresh() {
    loadError = "";
    try {
      [packages, overlayLayouts] = await Promise.all([
        adapter.invoke<PackageDescriptor[]>("list_packages"),
        adapter.invoke<OverlayLayoutCatalog>("get_overlay_layouts")
      ]);
    } catch (error) {
      loadError = error instanceof Error ? error.message : String(error);
    } finally {
      loaded = true;
    }
  }

  async function save(layoutToSave = layout) {
    if (!layoutToSave) return;
    reindexLayers(layoutToSave);
    normalizeLayout(layoutToSave);
    try {
      overlayLayouts = await adapter.invoke<OverlayLayoutCatalog>("save_overlay_layout", { layout: layoutToSave });
      message = "Saved.";
      setTimeout(() => (message = ""), 2000);
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }
  }

  async function saveThumbnail() {
    if (!layout) return;
    layout.thumbnail = createLayoutThumbnail(layout, { kind: "overlay" });
    await save(layout);
  }

  function sortedLayers(layout: OverlayLayout) {
    return [...layout.layers].sort((a, b) => {
      if (a.kind === "event" && b.kind !== "event") return -1;
      if (a.kind !== "event" && b.kind === "event") return 1;
      return a.order - b.order;
    });
  }

  function reindexLayers(layout: OverlayLayout) {
    const orderedLayers = sortedLayers(layout);
    orderedLayers.forEach((layer, index) => {
      layer.order = index;
    });
    layout.layers = orderedLayers;
    layout.items = [];
  }

  function asWholeNumber(value: unknown, fallback = 0) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? Math.round(parsed) : fallback;
  }

  function normalizeLayout(layoutToNormalize: OverlayLayout) {
    layoutToNormalize.width = Math.max(320, asWholeNumber(layoutToNormalize.width, 1920));
    layoutToNormalize.height = Math.max(240, asWholeNumber(layoutToNormalize.height, 1080));
    normalizeLayoutItems(layoutToNormalize);
  }

  function findItem(itemId: string) {
    if (!layout || !itemId) return null;
    for (const layer of layout.layers) {
      const item = layer.items.find((entry) => entry.id === itemId);
      if (item) return { layer, item };
    }
    return null;
  }

  function addLayer(kind: "normal" | "event" = "normal") {
    if (!layout) return;
    if (kind === "event" && layout.layers.some((layer) => layer.kind === "event")) return;
    const layer: OverlayLayer = {
      id: `layer-${Date.now()}`,
      name: kind === "event" ? "Events Layer" : `Layer ${layout.layers.length + 1}`,
      kind,
      visible: true,
      locked: false,
      order: layout.layers.length,
      items: []
    };
    layout.layers = [...layout.layers, layer];
    activeLayerId = layer.id;
    void save();
  }

  function askConfirmation(request: ConfirmRequest) {
    confirmRequest = request;
  }

  function cancelConfirmation() {
    confirmRequest = null;
  }

  async function confirmAction() {
    const request = confirmRequest;
    confirmRequest = null;
    await request?.run();
  }

  function deleteLayer(layer: OverlayLayer) {
    if (!layout || layer.kind === "event") return;
    const normalLayers = layout.layers.filter((entry) => entry.kind === "normal");
    if (normalLayers.length <= 1) return;
    askConfirmation({
      title: "Delete layer",
      message: `Delete "${layer.name}" and all its items? This cannot be undone.`,
      confirmLabel: "Delete",
      run: () => deleteLayerConfirmed(layer.id)
    });
  }

  function deleteLayerConfirmed(layerId: string) {
    if (!layout) return;
    const layer = layout.layers.find((entry) => entry.id === layerId);
    if (!layer || layer.kind === "event") return;
    layout.layers = layout.layers.filter((entry) => entry.id !== layer.id);
    if (activeLayerId === layer.id) activeLayerId = layout.layers.find((entry) => entry.kind === "normal")?.id ?? "";
    void save();
  }

  function moveLayer(layer: OverlayLayer, direction: -1 | 1) {
    if (!layout || layer.kind === "event") return;
    const normalLayers = sortedLayers(layout).filter((entry) => entry.kind === "normal");
    const index = normalLayers.findIndex((entry) => entry.id === layer.id);
    const target = normalLayers[index + direction];
    if (!target) return;
    const order = layer.order;
    layer.order = target.order;
    target.order = order;
    void save();
  }

  function startLayerDrag(layer: OverlayLayer) {
    if (layer.kind === "event") return;
    dragLayerId = layer.id;
  }

  function dropLayer(target: OverlayLayer) {
    if (!layout || !dragLayerId || target.kind === "event" || dragLayerId === target.id) return;
    const normals = sortedLayers(layout).filter((layer) => layer.kind === "normal");
    const draggedIndex = normals.findIndex((layer) => layer.id === dragLayerId);
    const targetIndex = normals.findIndex((layer) => layer.id === target.id);
    if (draggedIndex < 0 || targetIndex < 0) return;
    const [dragged] = normals.splice(draggedIndex, 1);
    normals.splice(targetIndex, 0, dragged);
    normals.forEach((layer, index) => {
      layer.order = index;
    });
    dragLayerId = "";
    void save();
  }

  function addVisual(ref: string) {
    if (!layout || !activeLayer || !ref) return;
    const selected = visualExports.find((entry) => entry.ref === ref);
    if (!selected) return;
    const item: OverlayItem = {
      id: `item-${Date.now()}`,
      package_id: selected.package.id,
      export_name: selected.visual.name,
      name: selected.visual.name,
      x: Math.round(layout.width / 2 - selected.visual.default_width / 2),
      y: Math.round(layout.height / 2 - selected.visual.default_height / 2),
      width: selected.visual.default_width,
      height: selected.visual.default_height,
      z_index: activeLayer.items.length + 10,
      visible: true,
      locked: false,
      opacity: 1,
      settings: {}
    };
    activeLayer.items = [...activeLayer.items, item];
    selectedItemId = item.id;
    void save();
  }

  function duplicateSelected() {
    if (!selectedEntry) return;
    const item = structuredClone(selectedEntry.item);
    item.id = `item-${Date.now()}`;
    item.name = `${item.name} Copy`;
    item.x += 24;
    item.y += 24;
    selectedEntry.layer.items = [...selectedEntry.layer.items, item];
    selectedItemId = item.id;
    void save();
  }

  function deleteSelected() {
    if (!selectedEntry) return;
    const itemName = selectedEntry.item.name;
    askConfirmation({
      title: "Delete item",
      message: `Delete "${itemName}" from "${selectedEntry.layer.name}"?`,
      confirmLabel: "Delete",
      run: () => deleteSelectedConfirmed(selectedEntry.item.id)
    });
  }

  function deleteSelectedConfirmed(itemId: string) {
    if (!layout) return;
    const entry = findItem(itemId);
    if (!entry) return;
    entry.layer.items = entry.layer.items.filter((item) => item.id !== itemId);
    selectedItemId = "";
    void save();
  }

  async function closeEditor() {
    await saveThumbnail();
    try {
      await adapter.invoke("close_overlay_editor");
    } catch (error) {
      message = String(error);
    }
    const returnState = routeReturnFromParams(data.returnTo, data.scrollY, "/overlays");
    storeRouteScrollRestore(returnState);
    await goto(returnState.returnTo);
  }

  function pointForEvent(event: PointerEvent) {
    if (!stage) return { x: 0, y: 0 };
    const rect = stage.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / rect.width) * (layout?.width ?? 1),
      y: ((event.clientY - rect.top) / rect.height) * (layout?.height ?? 1)
    };
  }

  function clampItem(item: OverlayItem) {
    if (!layout) return;
    item.width = Math.round(Math.max(24, Math.min(item.width, layout.width)));
    item.height = Math.round(Math.max(18, Math.min(item.height, layout.height)));
    item.x = Math.round(Math.max(0, Math.min(item.x, layout.width - item.width)));
    item.y = Math.round(Math.max(0, Math.min(item.y, layout.height - item.height)));
  }

  function normalizeLayoutItems(layoutToNormalize: OverlayLayout) {
    for (const layer of layoutToNormalize.layers) {
      for (const item of layer.items) {
        item.width = Math.round(Math.max(24, Math.min(item.width, layoutToNormalize.width)));
        item.height = Math.round(Math.max(18, Math.min(item.height, layoutToNormalize.height)));
        item.x = Math.round(Math.max(0, Math.min(item.x, layoutToNormalize.width - item.width)));
        item.y = Math.round(Math.max(0, Math.min(item.y, layoutToNormalize.height - item.height)));
      }
    }
  }

  function snapItem(item: OverlayItem) {
    if (!layout) return;
    snapGuides = snapItemPosition(item, layout, allItems().map((entry) => entry.item), {
      enabled: snapEnabled,
      gridSize
    });
  }

  function allItems() {
    if (!layout) return [];
    return layout.layers.flatMap((layer) => layer.items.map((item) => ({ layer, item })));
  }

  function refreshPreview() {
    layoutRevision += 1;
    overlayLayouts = overlayLayouts;
  }

  function itemStyle(item: OverlayItem) {
    if (!layout) return "";
    return `
      left:${(item.x / layout.width) * 100}%;
      top:${(item.y / layout.height) * 100}%;
      width:${(item.width / layout.width) * 100}%;
      height:${(item.height / layout.height) * 100}%;
      z-index:${item.z_index};
    `;
  }

  function guideStyle(axis: "x" | "y", value: number | null) {
    return snapGuideStyle(axis, value, layout);
  }

  function startPan(event: PointerEvent) {
    if (event.button !== 1 && !event.altKey) return;
    event.preventDefault();
    panState = {
      startPointer: { x: event.clientX, y: event.clientY },
      startPan: { x: panX, y: panY }
    };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function startDrag(event: PointerEvent, item: OverlayItem, mode: "move" | "resize") {
    const entry = findItem(item.id);
    event.stopPropagation();
    event.preventDefault();
    contextMenu = null;
    if (!entry) return;
    selectedItemId = item.id;
    activeLayerId = entry.layer.id;
    if (entry.item.locked || entry.layer.locked) return;
    dragState = {
      mode,
      itemId: item.id,
      startPointer: pointForEvent(event),
      startItem: {
        x: item.x,
        y: item.y,
        width: item.width,
        height: item.height
      },
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
    if (!dragState || !layout) return;
    const entry = findItem(dragState.itemId);
    if (!entry) return;
    const pointer = pointForEvent(event);
    const dx = pointer.x - dragState.startPointer.x;
    const dy = pointer.y - dragState.startPointer.y;
    if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
      dragState.changed = true;
    }
    if (dragState.mode === "move") {
      entry.item.x = dragState.startItem.x + dx;
      entry.item.y = dragState.startItem.y + dy;
      snapItem(entry.item);
    } else {
      entry.item.width = dragState.startItem.width + dx;
      entry.item.height = dragState.startItem.height + dy;
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
    dragState = null;
    snapGuides = emptySnapGuides();
    if (shouldSave) void save();
  }

  function openContextMenu(event: MouseEvent, item: OverlayItem) {
    event.preventDefault();
    event.stopPropagation();
    selectedItemId = item.id;
    contextMenu = { itemId: item.id, x: event.clientX, y: event.clientY };
  }

  function contextMenuStyle() {
    if (!contextMenu) return "";
    return `left:${contextMenu.x}px;top:${contextMenu.y}px;`;
  }

  function toggleSelectedLock() {
    if (!selectedEntry) return;
    selectedEntry.item.locked = !selectedEntry.item.locked;
    contextMenu = null;
    void save();
  }

  function toggleSelectedVisible() {
    if (!selectedEntry) return;
    selectedEntry.item.visible = !selectedEntry.item.visible;
    contextMenu = null;
    void save();
  }

  function bringSelectedForward() {
    if (!selectedEntry) return;
    selectedEntry.item.z_index = Math.max(0, ...allItems().map((entry) => entry.item.z_index)) + 1;
    contextMenu = null;
    void save();
  }

  function sendSelectedBackward() {
    if (!selectedEntry) return;
    selectedEntry.item.z_index = Math.min(0, ...allItems().map((entry) => entry.item.z_index)) - 1;
    contextMenu = null;
    void save();
  }

  function fireMock(eventName: RlTelemetryEventName) {
    mockEvent = { id: Date.now(), event: telemetryFrameTemplate(eventName) };
  }

  onMount(() => {
    void refresh();
    let unlistenOverlays: (() => void) | undefined;
    let unlistenPackages: (() => void) | undefined;
    void adapter.listen<OverlayLayoutCatalog>("bakingrl-overlay-layouts-changed", (payload) => {
      overlayLayouts = payload;
      loaded = true;
    }).then((unlisten) => {
      unlistenOverlays = unlisten;
    });
    void adapter.listen<PackageDescriptor[]>("bakingrl-packages-changed", (payload) => {
      packages = payload;
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    return () => {
      unlistenOverlays?.();
      unlistenPackages?.();
    };
  });
</script>

<main class="layout-editor-page">
  <ConfirmDialog
    open={confirmRequest !== null}
    title={confirmRequest?.title}
    message={confirmRequest?.message}
    confirmLabel={confirmRequest?.confirmLabel}
    danger
    onconfirm={confirmAction}
    oncancel={cancelConfirmation}
  />

  {#if layout}
    <EditorTemplate
      title={layout.name}
      {message}
      canvas={layout}
      centerKey={layout.id}
      canvasAriaLabel="Layout editor canvas"
      dragging={dragState !== null}
      bind:zoom
      bind:panX
      bind:panY
      bind:stage
      bind:stageWrap
      onPointerMove={pointerMove}
      onPointerUp={pointerUp}
      onStagePointerDown={startPan}
      onClose={closeEditor}
    >
      {#snippet stageContent()}
        <OverlayRenderer layoutId={layout.id} layoutOverride={layout} {layoutRevision} packageRevision={packageRuntime.revision} mode="editor" {mockEvent} />
        <div class="frames">
          {#each allItems() as entry}
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
        {#if snapGuides.x !== null}
          <div class="snap-guide vertical" style={guideStyle("x", snapGuides.x)}></div>
        {/if}
        {#if snapGuides.y !== null}
          <div class="snap-guide horizontal" style={guideStyle("y", snapGuides.y)}></div>
        {/if}
      {/snippet}

      {#snippet leftPanel()}
        <section class="accordion">
          <button class="accordion-header" onclick={() => showOverlay = !showOverlay}>
            <svg class:rotated={showOverlay} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
            <h2>Overlay</h2>
          </button>
          {#if showOverlay}
            <div class="accordion-body properties-panel">
              <div class="prop-group">
                <label for="layout-name">Name</label>
                <input id="layout-name" bind:value={layout.name} onblur={() => save()} />
              </div>
              <div class="prop-row">
                <div class="prop-group">
                  <label for="layout-width">Width</label>
                  <input id="layout-width" type="number" min="320" step="1" bind:value={layout.width} onblur={() => save()} />
                </div>
                <div class="prop-group">
                  <label for="layout-height">Height</label>
                  <input id="layout-height" type="number" min="240" step="1" bind:value={layout.height} onblur={() => save()} />
                </div>
              </div>
            </div>
          {/if}
        </section>

        <!-- Visuals Library -->
        <section class="accordion">
          <button class="accordion-header" onclick={() => showVisuals = !showVisuals}>
            <svg class:rotated={showVisuals} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
            <h2>Add Visual</h2>
          </button>
          {#if showVisuals}
            <div class="accordion-body">
              <VisualLibrary entries={visualExports} bind:search={visualSearch} onadd={addVisual} />
            </div>
          {/if}
        </section>

        <!-- Layers -->
        <section class="accordion">
          <div class="accordion-header accordion-header-with-actions">
            <button class="accordion-toggle" onclick={() => showLayers = !showLayers} aria-expanded={showLayers}>
              <svg class:rotated={showLayers} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
              <h2>Layers</h2>
            </button>
            <div class="header-actions">
              <button class="icon-btn add-layer-btn event-add" onclick={() => addLayer("event")} title="Add Event Layer" disabled={layout.layers.some((layer) => layer.kind === "event")}>
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg>
              </button>
              <button class="icon-btn add-layer-btn" onclick={() => addLayer("normal")} title="Add Layer">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
              </button>
            </div>
          </div>
          {#if showLayers}
            <div class="accordion-body">
              <div class="list layer-list">
                {#each layers as layer}
                  <div
                    class:active={activeLayerId === layer.id}
                    class="layer-row {layer.kind}"
                    role="listitem"
                    draggable={layer.kind !== "event"}
                    ondragstart={() => startLayerDrag(layer)}
                    ondragover={(event) => event.preventDefault()}
                    ondrop={() => dropLayer(layer)}
                  >
                    <button class="layer-select" onclick={() => (activeLayerId = layer.id)} title="Select Layer">
                      <div class="layer-indicator"></div>
                    </button>

                    <div class="layer-name-input">
                      {#if layer.kind === "event"}
                        <svg class="event-icon" xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg>
                      {/if}
                      <input bind:value={layer.name} onblur={() => save()} />
                    </div>

                    <div class="layer-actions">
                      <button class="icon-btn toggle {layer.visible ? '' : 'off'}" onclick={() => { layer.visible = !layer.visible; void save(); }} title={layer.visible ? "Hide" : "Show"}>
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          {#if layer.visible}
                            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path><circle cx="12" cy="12" r="3"></circle>
                          {:else}
                            <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"></path><line x1="1" y1="1" x2="23" y2="23"></line>
                          {/if}
                        </svg>
                      </button>
                      <button class="icon-btn toggle {layer.locked ? 'on' : ''}" onclick={() => { layer.locked = !layer.locked; void save(); }} title={layer.locked ? "Unlock" : "Lock"}>
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                          {#if layer.locked}
                            <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path>
                          {:else}
                            <rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 9.9-1"></path>
                          {/if}
                        </svg>
                      </button>
                      <button class="icon-btn" onclick={() => moveLayer(layer, -1)} disabled={layer.kind === "event"} title="Move Up">
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="18 15 12 9 6 15"></polyline></svg>
                      </button>
                      <button class="icon-btn" onclick={() => moveLayer(layer, 1)} disabled={layer.kind === "event"} title="Move Down">
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"></polyline></svg>
                      </button>
                      <button class="icon-btn danger" onclick={() => deleteLayer(layer)} disabled={layer.kind === "event"} title="Delete Layer">
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </section>

        <!-- Mock Events -->
        <section class="accordion mock-events-panel">
          <button class="accordion-header" onclick={() => showMocks = !showMocks}>
            <svg class:rotated={showMocks} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
            <h2>Mock Events</h2>
          </button>
          {#if showMocks}
            <div class="accordion-body">
              <div class="mock-grid">
                {#each RL_TELEMETRY_EVENT_NAMES as eventName}
                  <button class="btn-outline small" onclick={() => fireMock(eventName)}>{eventName}</button>
                {/each}
              </div>
            </div>
          {/if}
        </section>
      {/snippet}

      {#snippet rightPanel()}
        <!-- Selected Item Properties -->
        <section class="accordion">
          <button class="accordion-header" onclick={() => showSettings = !showSettings}>
            <svg class:rotated={showSettings} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
            <h2>Properties</h2>
          </button>
          {#if showSettings}
            <div class="accordion-body">
              {#if selectedEntry}
                <div class="properties-panel">
                  <div class="prop-group">
                    <label for="selected-item-name">Name</label>
                    <input id="selected-item-name" bind:value={selectedEntry.item.name} onblur={() => save()} />
                  </div>

                  <div class="prop-row">
                    <div class="prop-group">
                      <label for="selected-item-x">X</label>
                      <input id="selected-item-x" type="number" bind:value={selectedEntry.item.x} onblur={() => save()} />
                    </div>
                    <div class="prop-group">
                      <label for="selected-item-y">Y</label>
                      <input id="selected-item-y" type="number" bind:value={selectedEntry.item.y} onblur={() => save()} />
                    </div>
                  </div>
                  <div class="prop-row">
                    <div class="prop-group">
                      <label for="selected-item-width">Width</label>
                      <input id="selected-item-width" type="number" bind:value={selectedEntry.item.width} onblur={() => save()} />
                    </div>
                    <div class="prop-group">
                      <label for="selected-item-height">Height</label>
                      <input id="selected-item-height" type="number" bind:value={selectedEntry.item.height} onblur={() => save()} />
                    </div>
                  </div>

                  <div class="prop-group">
                    <label for="selected-item-opacity">Opacity ({Math.round(selectedEntry.item.opacity * 100)}%)</label>
                    <input id="selected-item-opacity" type="range" min="0" max="1" step="0.05" bind:value={selectedEntry.item.opacity} onchange={() => save()} />
                  </div>

                  <div class="toggles-row">
                    <label class="checkbox-label">
                      <input type="checkbox" bind:checked={selectedEntry.item.visible} onchange={() => save()} />
                      <span class="checkmark"></span> Visible
                    </label>
                    <label class="checkbox-label">
                      <input type="checkbox" bind:checked={selectedEntry.item.locked} onchange={() => save()} />
                      <span class="checkmark"></span> Locked
                    </label>
                  </div>

                  <InstanceSettingsForm
                    item={selectedEntry.item}
                    packageId={selectedEntry.item.package_id}
                    exportName={selectedEntry.item.export_name}
                    oncommit={() => save()}
                  />

                  <div class="toolbar bottom-actions">
                    <button class="btn-secondary flex-1" onclick={duplicateSelected}>
                      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
                      Duplicate
                    </button>
                    <button class="btn-danger flex-1" onclick={deleteSelected}>
                      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                      Delete
                    </button>
                  </div>
                </div>
              {:else}
                <div class="empty-state small">
                  <p>Select an item to edit its properties.</p>
                </div>
              {/if}

              <!-- Workspace Settings always visible at bottom of properties -->
              <div class="divider"></div>
              <div class="workspace-settings">
                <label class="checkbox-label">
                  <input type="checkbox" bind:checked={snapEnabled} />
                  <span class="checkmark"></span> Enable Snapping
                </label>
                <div class="prop-group row-align">
                  <label for="editor-grid-size">Grid Size</label>
                  <input id="editor-grid-size" type="number" min="1" max="200" bind:value={gridSize} disabled={!snapEnabled} class="small-input" />
                </div>
              </div>
            </div>
          {/if}
        </section>
      {/snippet}

      {#snippet overlays()}
        {#if contextMenu && selectedEntry}
          <div class="context-menu" style={contextMenuStyle()} role="menu">
            <button role="menuitem" onclick={duplicateSelected}>Duplicate</button>
            <button role="menuitem" onclick={toggleSelectedLock}>{selectedEntry.item.locked ? "Unlock" : "Lock"}</button>
            <button role="menuitem" onclick={toggleSelectedVisible}>{selectedEntry.item.visible ? "Hide" : "Show"}</button>
            <button role="menuitem" onclick={bringSelectedForward}>Bring forward</button>
            <button role="menuitem" onclick={sendSelectedBackward}>Send backward</button>
            <button role="menuitem" class="danger" onclick={deleteSelected}>Delete</button>
          </div>
        {/if}
      {/snippet}
    </EditorTemplate>
  {:else if loadError}
    <div class="loading-state error-state">
      <p>Unable to load layout.</p>
      <pre>{loadError}</pre>
      <button class="btn-secondary" onclick={() => void refresh()}>Retry</button>
    </div>
  {:else if loaded}
    <div class="loading-state error-state">
      <p>Layout not found: {data.layoutId}</p>
      {#if overlayLayouts?.layouts.length}
        <pre>Available layouts: {overlayLayouts.layouts.map((entry) => entry.id).join(", ")}</pre>
      {/if}
      <button class="btn-secondary" onclick={() => void refresh()}>Retry</button>
    </div>
  {:else}
    <div class="loading-state">
      <div class="spinner"></div>
      <p>Loading layout...</p>
    </div>
  {/if}
</main>

<style>
  .layout-editor-page {
    width: 100vw;
    min-width: 0;
    height: var(--app-content-height, 100vh);
    min-height: 0;
    overflow: hidden;
    position: relative;
    background: var(--editor-bg-dark);
  }

  .layout-editor-page.dragging {
    user-select: none;
    cursor: grabbing !important;
  }

  /* Stage Area */
  .stage-wrap {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 0;
    background: color-mix(in srgb, var(--editor-bg-dark) 82%, transparent);
  }

  .stage {
    position: relative;
    width: 100%;
    height: 100%;
    box-shadow:
      0 0 0 1px var(--border-color),
      0 24px 64px color-mix(in srgb, var(--bg-dark) 74%, transparent);
    background-color: color-mix(in srgb, var(--bg-dark) 12%, transparent);
    overflow: visible;
  }

  .stage.checkerboard::before {
    content: '';
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(45deg, color-mix(in srgb, var(--text-muted) 12%, transparent) 25%, transparent 25%),
      linear-gradient(-45deg, color-mix(in srgb, var(--text-muted) 12%, transparent) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, color-mix(in srgb, var(--text-muted) 12%, transparent) 75%),
      linear-gradient(-45deg, transparent 75%, color-mix(in srgb, var(--text-muted) 12%, transparent) 75%);
    background-size: 20px 20px;
    background-position: 0 0, 0 10px, 10px -10px, -10px 0px;
    pointer-events: none;
    z-index: 0;
  }

  /* Frames */
  .frames {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 100;
  }

  .frame-box {
    position: absolute;
    padding: 0;
    border: 1px solid color-mix(in srgb, var(--accent) 42%, transparent);
    background: color-mix(in srgb, var(--accent) 7%, transparent);
    color: var(--text-primary);
    cursor: move;
    pointer-events: auto;
    transition: border-color 0.1s, background-color 0.1s;
    box-sizing: border-box;
    touch-action: none;
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
    opacity: 0.3;
  }

  .frame-box.locked {
    border-color: color-mix(in srgb, var(--warn) 50%, transparent);
    background: color-mix(in srgb, var(--warn) 6%, transparent);
  }
  .frame-box.locked.selected { border-color: var(--warn); }

  .frame-label {
    position: absolute;
    top: -24px;
    left: -1px;
    background: var(--accent);
    padding: 2px 8px;
    border-radius: 4px 4px 0 0;
    font-size: 11px;
    display: flex;
    gap: 8px;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.1s;
    white-space: nowrap;
  }
  .frame-box.locked .frame-label { background: var(--warn); }
  .frame-box:hover .frame-label, .frame-box.selected .frame-label { opacity: 1; }

  .frame-label .name { font-weight: 600; }
  .frame-label .layer-name { opacity: 0.7; font-weight: normal; }

  .resize-handle {
    position: absolute;
    right: -7px;
    bottom: -7px;
    width: 14px;
    height: 14px;
    padding: 0;
    background: var(--text-primary);
    border: 2px solid var(--accent);
    border-radius: 50%;
    cursor: nwse-resize;
    opacity: 0;
    transition: opacity 0.1s;
    touch-action: none;
  }
  .frame-box.locked .resize-handle { display: none; }
  .frame-box:hover .resize-handle, .frame-box.selected .resize-handle { opacity: 1; }

  /* Editor Panel */
  .editor-panel {
    position: absolute;
    z-index: 500;
    width: 320px;
    max-height: calc(var(--app-content-height, 100vh) - 48px);
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: var(--editor-bg-panel);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    box-shadow:
      0 12px 40px color-mix(in srgb, var(--bg-dark) 68%, transparent),
      0 0 0 1px color-mix(in srgb, var(--border-color) 72%, transparent) inset;
  }

  .drag-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 16px;
    background: var(--editor-bg-panel-hover);
    border-bottom: 1px solid var(--border-color);
    cursor: grab;
    border-radius: var(--radius-md) var(--radius-md) 0 0;
  }
  .drag-header:active { cursor: grabbing; }

  .header-actions { display: flex; align-items: center; gap: 8px; }

  .status-msg {
    font-size: 11px;
    color: var(--success);
    background: var(--success-bg);
    padding: 2px 6px;
    border-radius: 4px;
    animation: fadeIn 0.2s ease;
  }

  .panel-content {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  /* Scrollbar for panel */
  .panel-content::-webkit-scrollbar { width: 6px; }
  .panel-content::-webkit-scrollbar-track { background: transparent; }
  .panel-content::-webkit-scrollbar-thumb { background: var(--border-color-focus); border-radius: 10px; }
  .panel-content::-webkit-scrollbar-thumb:hover { background: var(--accent); }

  /* Accordions */
  .accordion {
    background: var(--editor-bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .accordion-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
  }
  .accordion-header:hover { background: var(--editor-bg-panel-hover); }

  .accordion-header-with-actions {
    padding: 0;
    cursor: default;
  }
  .accordion-header-with-actions:hover { background: transparent; }

  .accordion-toggle {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    background: transparent;
    border: none;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
  }
  .accordion-toggle:hover { background: var(--editor-bg-panel-hover); }
  .accordion-header-with-actions .header-actions { padding-right: 12px; }

  .accordion-header svg, .accordion-toggle svg {
    color: var(--text-secondary);
    transition: transform 0.2s ease;
  }
  .accordion-header svg.rotated, .accordion-toggle svg.rotated { transform: rotate(90deg); }

  .accordion-header h2, .accordion-toggle h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    flex: 1;
  }

  .accordion-body {
    padding: 0 12px 12px 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* Shared Form UI */
  .list { display: flex; flex-direction: column; gap: 4px; }

  /* Layer List */
  .layer-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    background: color-mix(in srgb, var(--bg-panel-hover) 46%, transparent);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    transition: border-color 0.2s;
  }
  .layer-row.active { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); }

  .layer-select {
    background: none; border: none; padding: 4px; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
  }
  .layer-indicator { width: 8px; height: 8px; border-radius: 50%; border: 2px solid var(--text-secondary); }
  .layer-row.active .layer-indicator { border-color: var(--accent); background: var(--accent); }

  .layer-name-input {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    background: transparent;
  }
  .layer-name-input input {
    width: 100%;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-primary);
    font-size: 13px;
    padding: 4px;
    border-radius: 4px;
  }
  .layer-name-input input:focus { border-color: var(--border-color); background: color-mix(in srgb, var(--bg-dark) 35%, transparent); outline: none; }
  .event-icon { color: var(--warn); }

  .layer-actions { display: flex; gap: 2px; }

  /* Properties */
  .properties-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .prop-group { display: flex; flex-direction: column; gap: 4px; flex: 1; }
  .prop-group.row-align { flex-direction: row; align-items: center; justify-content: space-between; }
  .prop-row { display: flex; gap: 8px; }

  .prop-group label { font-size: 11px; color: var(--text-secondary); font-weight: 500; text-transform: uppercase; }

  .properties-panel input:not([type="range"]):not([type="checkbox"]) {
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 6px 8px;
    border-radius: 4px;
    font-size: 13px;
    width: 100%;
    box-sizing: border-box;
    font-family: inherit;
  }
  .properties-panel input:focus {
    outline: none; border-color: var(--accent);
  }

  .small-input { width: 60px !important; }

  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }

  .toggles-row { display: flex; gap: 16px; margin: 4px 0; }

  /* Buttons & Checkboxes */
  .icon-btn {
    background: transparent; border: none; color: var(--text-secondary);
    width: 24px; height: 24px; border-radius: 4px;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; transition: var(--transition);
  }
  .icon-btn:hover:not(:disabled) { background: var(--editor-bg-panel-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.3; cursor: not-allowed; }
  .icon-btn.danger:hover:not(:disabled) { background: color-mix(in srgb, var(--danger) 15%, transparent); color: var(--danger); }
  .icon-btn.small { width: 20px; height: 20px; }
  .icon-btn.small svg { width: 12px; height: 12px; }
  .icon-btn.add-layer-btn {
    width: 28px;
    height: 28px;
    color: var(--text-primary);
  }
  .icon-btn.add-layer-btn svg {
    width: 16px;
    height: 16px;
  }

  .icon-btn.toggle.on { color: var(--accent); }
  .icon-btn.toggle.off { color: var(--text-muted); opacity: 0.5; }

  .btn-outline {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    font-size: 13px;
    cursor: pointer;
    transition: var(--transition);
  }
  .btn-outline:hover { background: var(--editor-bg-panel-hover); }
  .btn-outline.small { padding: 4px 8px; font-size: 12px; }

  .btn-secondary {
    background: var(--bg-panel-hover); border: 1px solid var(--border-color);
    color: var(--text-primary); padding: 8px; border-radius: var(--radius-sm);
    font-size: 13px; cursor: pointer; display: flex; align-items: center; justify-content: center; gap: 6px;
  }
  .btn-secondary:hover { background: var(--editor-bg-panel-hover); }

  .btn-danger {
    background: color-mix(in srgb, var(--danger) 10%, transparent); border: 1px solid color-mix(in srgb, var(--danger) 22%, transparent);
    color: var(--danger); padding: 8px; border-radius: var(--radius-sm);
    font-size: 13px; cursor: pointer; display: flex; align-items: center; justify-content: center; gap: 6px;
  }
  .btn-danger:hover { background: color-mix(in srgb, var(--danger) 20%, transparent); border-color: color-mix(in srgb, var(--danger) 40%, transparent); }

  .flex-1 { flex: 1; }

  /* Checkbox styling */
  .checkbox-label {
    display: flex; align-items: center; gap: 8px;
    cursor: pointer; font-size: 13px; color: var(--text-primary); user-select: none;
  }
  .checkbox-label input { position: absolute; opacity: 0; cursor: pointer; height: 0; width: 0; }
  .checkmark {
    height: 16px; width: 16px; background-color: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    border: 1px solid var(--border-color); border-radius: 4px; position: relative; transition: var(--transition);
  }
  .checkbox-label:hover input ~ .checkmark { border-color: var(--text-secondary); }
  .checkbox-label input:checked ~ .checkmark { background-color: var(--accent); border-color: var(--accent); }
  .checkmark:after {
    content: ""; position: absolute; display: none;
    left: 4px; top: 1px; width: 4px; height: 8px;
    border: solid var(--text-primary); border-width: 0 2px 2px 0; transform: rotate(45deg);
  }
  .checkbox-label input:checked ~ .checkmark:after { display: block; }

  /* Utility */
  .divider { height: 1px; background: var(--border-color); margin: 4px 0; }
  .bottom-actions { display: flex; gap: 8px; margin-top: 8px; }

  .mock-grid { display: flex; flex-wrap: wrap; gap: 6px; }

  .empty-state { padding: 24px; text-align: center; color: var(--text-muted); font-size: 13px; }
  .empty-state.small { padding: 12px; }

  .loading-state {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    height: 100%;
    min-height: 520px;
    gap: 16px;
    color: var(--text-muted);
  }
  .spinner {
    width: 24px; height: 24px; border: 2px solid var(--border-color);
    border-top-color: var(--accent); border-radius: 50%; animation: spin 1s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes fadeIn { from { opacity: 0; } to { opacity: 1; } }

  /* Studio editor shell */
  .stage-wrap {
    position: absolute;
    inset: 48px 0 0 0;
    display: grid;
    place-items: center;
    overflow: hidden;
    background:
      radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--text-muted) 28%, transparent) 1px, transparent 0),
      var(--editor-bg-dark);
    background-size: 24px 24px;
    cursor: default;
    padding: 24px 348px 24px 292px; /* fallback padding */
  }

  .stage {
    width: auto;
    height: auto;
    flex: none;
    transform-origin: center center;
    background-color: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    outline: 1px solid var(--border-color-focus);
    box-shadow: 0 24px 80px color-mix(in srgb, var(--bg-dark) 70%, transparent);
  }

  .stage.checkerboard::before {
    background-image:
      linear-gradient(color-mix(in srgb, var(--text-muted) 18%, transparent) 1px, transparent 1px),
      linear-gradient(90deg, color-mix(in srgb, var(--text-muted) 18%, transparent) 1px, transparent 1px);
    background-size: 40px 40px;
    background-position: 0 0;
  }

  .editor-panel {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    max-height: none;
    border: 0;
    border-radius: 0;
    background: transparent;
    box-shadow: none;
    backdrop-filter: none;
    pointer-events: none;
  }

  .drag-header {
    position: absolute;
    inset: 0 0 auto 0;
    z-index: 700;
    height: 48px;
    padding: 0 12px;
    border-radius: 0;
    background: var(--editor-bg-panel);
    cursor: default;
    pointer-events: auto;
  }

  .panel-content {
    position: absolute;
    inset: 48px 0 0 0;
    display: block;
    padding: 12px;
    box-sizing: border-box;
    overflow: hidden;
    pointer-events: none;
    z-index: 600;
  }

  .side-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 280px;
    height: auto;
    min-height: 0;
    max-height: calc(100% - 24px);
    overflow: hidden;
    pointer-events: none;
  }

  .side-panel.left {
    position: absolute;
    top: 12px;
    left: 12px;
    overflow-y: auto;
    overscroll-behavior: contain;
    pointer-events: auto;
  }

  .side-panel.right {
    width: 336px;
  }

  .side-panel.properties-side {
    position: absolute;
    right: 12px;
    bottom: 12px;
    max-height: calc(100% - 24px);
    overflow: visible;
    pointer-events: none;
  }

  .accordion {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-height: 0;
    max-height: 100%;
    overflow: hidden;
    border-color: var(--border-color);
    background: var(--editor-bg-panel);
    pointer-events: auto;
  }

  .accordion-body {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .layer-row.event {
    border-color: color-mix(in srgb, var(--warn) 48%, transparent);
    background: color-mix(in srgb, var(--warn) 10%, transparent);
  }

  .layer-row[draggable="true"] {
    cursor: grab;
  }

  .event-add {
    color: var(--warn);
  }

  .zoom-readout {
    min-width: 54px;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    color: var(--text-secondary);
    font-size: 12px;
  }

  .context-menu {
    position: fixed;
    z-index: 900;
    display: flex;
    min-width: 180px;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--editor-bg-panel);
    box-shadow: 0 18px 48px color-mix(in srgb, var(--bg-dark) 72%, transparent);
  }

  .context-menu button {
    justify-content: flex-start;
    padding: 8px 10px;
    border: 0;
    background: transparent;
    color: var(--text-secondary);
    font-size: 13px;
    text-align: left;
  }

  .context-menu button:hover {
    background: var(--editor-bg-panel-hover);
    color: var(--text-primary);
  }

  .context-menu button.danger {
    color: var(--danger);
  }

  @media (max-width: 980px) {
    .stage-wrap {
      inset: 48px 0 0 0;
      padding: 24px;
    }

    .panel-content {
      inset: 48px 0 0 0;
      height: auto;
      padding: 10px;
      background: transparent;
      pointer-events: none;
    }

    .side-panel {
      width: min(280px, calc(50vw - 16px)) !important;
      flex-basis: auto !important;
      height: auto;
      max-height: calc(100% - 20px);
    }

    .side-panel.left {
      top: 10px;
      left: 10px;
    }

    .side-panel.properties-side {
      right: 10px;
      bottom: 10px;
      width: min(336px, calc(50vw - 16px)) !important;
    }

    .accordion {
      max-height: 100%;
    }
  }
</style>
