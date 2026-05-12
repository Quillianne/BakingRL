<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import InstanceSettingsForm from "$lib/InstanceSettingsForm.svelte";
  import { adapter } from "$lib/adapter/index";
  import type { EditorCommand } from "$lib/editor/CommandPalette.svelte";
  import EditorWorkspace from "$lib/editor/EditorWorkspace.svelte";
  import LayerItemsPanel from "$lib/editor/LayerItemsPanel.svelte";
  import {
    insertionPoint,
    moveItemToLayer,
    moveItemToStackEdge,
    nextZIndex,
    type ItemDropPosition,
    type PlacementPoint
  } from "$lib/editor/model";
  import { createLayoutThumbnail } from "$lib/layoutThumbnail";
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
  let stage = $state<HTMLElement>();
  let confirmRequest = $state<ConfirmRequest | null>(null);
  let layoutRevision = $state(0);
  let zoom = $state(0.48);
  let panX = $state(0);
  let panY = $state(0);
  let dragLayerId = $state("");
  let pluginInteractionMode = $state(false);
  let commandOpen = $state(false);

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
  const editorCommands = $derived<EditorCommand[]>([
    ...visualExports.map((entry) => ({
      id: `visual:${entry.ref}`,
      label: entry.visual.name,
      detail: entry.package.name,
      keywords: `${entry.package.id} visual add insert`,
      run: () => addVisual(entry.ref)
    })),
    {
      id: "toggle-interact",
      label: pluginInteractionMode ? "Edit mode" : "Interact mode",
      detail: "Canvas",
      keywords: "plugin preview test",
      run: () => (pluginInteractionMode = !pluginInteractionMode)
    },
    {
      id: "duplicate-selected",
      label: "Duplicate",
      detail: selectedEntry?.item.name,
      disabled: !selectedEntry,
      keywords: "copy item",
      run: duplicateSelected
    },
    {
      id: "delete-selected",
      label: "Delete",
      detail: selectedEntry?.item.name,
      disabled: !selectedEntry,
      keywords: "remove item",
      run: deleteSelected
    },
    {
      id: "front-selected",
      label: "Front",
      detail: selectedEntry?.item.name,
      disabled: !selectedEntry,
      keywords: "above forward top",
      run: bringSelectedForward
    },
    {
      id: "back-selected",
      label: "Back",
      detail: selectedEntry?.item.name,
      disabled: !selectedEntry,
      keywords: "below backward bottom",
      run: sendSelectedBackward
    }
  ]);

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
      refreshPreview();
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

  function addVisual(ref: string, placement?: PlacementPoint | null) {
    if (!layout || !activeLayer || !ref) return;
    const selected = visualExports.find((entry) => entry.ref === ref);
    if (!selected) return;
    const width = Math.round(Math.min(Math.max(24, Number(selected.visual.default_width) || 320), layout.width));
    const height = Math.round(Math.min(Math.max(18, Number(selected.visual.default_height) || 180), layout.height));
    const point = insertionPoint(layout, width, height, placement);
    const item: OverlayItem = {
      id: `item-${Date.now()}`,
      package_id: selected.package.id,
      export_name: selected.visual.name,
      name: selected.visual.name,
      x: point.x,
      y: point.y,
      width,
      height,
      z_index: nextZIndex(activeLayer.items),
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

  function pointForClient(clientX: number, clientY: number) {
    if (!stage) return { x: 0, y: 0 };
    const rect = stage.getBoundingClientRect();
    return {
      x: ((clientX - rect.left) / rect.width) * (layout?.width ?? 1),
      y: ((clientY - rect.top) / rect.height) * (layout?.height ?? 1)
    };
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

  function selectItemEntry(entry: { layer: OverlayLayer; item: OverlayItem }) {
    activeLayerId = entry.layer.id;
    selectedItemId = entry.item.id;
  }

  function refreshPreview() {
    layoutRevision += 1;
    overlayLayouts = overlayLayouts;
  }

  function bringSelectedForward() {
    if (!selectedEntry) return;
    moveItemToStackEdge(selectedEntry.item, selectedEntry.layer.items, "front");
    void save();
  }

  function sendSelectedBackward() {
    if (!selectedEntry) return;
    moveItemToStackEdge(selectedEntry.item, selectedEntry.layer.items, "back");
    void save();
  }

  function selectLayer(layer: OverlayLayer) {
    activeLayerId = layer.id;
  }

  function selectLayerItem(layer: OverlayLayer, item: OverlayItem) {
    selectItemEntry({ layer, item });
  }

  function renameLayer(layer: OverlayLayer, name: string) {
    if (layer.kind === "event") return;
    layer.name = name;
  }

  function toggleLayerVisible(layer: OverlayLayer) {
    layer.visible = !layer.visible;
    void save();
  }

  function toggleLayerLocked(layer: OverlayLayer) {
    layer.locked = !layer.locked;
    void save();
  }

  function toggleItemVisible(layer: OverlayLayer, item: OverlayItem) {
    item.visible = !item.visible;
    selectLayerItem(layer, item);
    void save();
  }

  function toggleItemLocked(layer: OverlayLayer, item: OverlayItem) {
    item.locked = !item.locked;
    selectLayerItem(layer, item);
    void save();
  }

  function reorderLayerItem(
    sourceLayer: OverlayLayer,
    item: OverlayItem,
    targetLayer: OverlayLayer,
    targetItem: OverlayItem | null,
    position: ItemDropPosition
  ) {
    if (sourceLayer.locked || targetLayer.locked || item.locked) return;
    if (moveItemToLayer(sourceLayer, item, targetLayer, targetItem, position)) {
      selectLayerItem(targetLayer, item);
      refreshPreview();
      void save();
    }
  }

  function placeVisual(ref: string, event: PointerEvent) {
    if (!stage || !ref) return;
    const rect = stage.getBoundingClientRect();
    if (
      event.clientX < rect.left ||
      event.clientX > rect.right ||
      event.clientY < rect.top ||
      event.clientY > rect.bottom
    ) {
      return;
    }
    addVisual(ref, pointForClient(event.clientX, event.clientY));
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
    <EditorWorkspace
      title={layout.name}
      {message}
      canvas={layout}
      {layers}
      centerKey={layout.id}
      canvasAriaLabel="Layout editor canvas"
      rendererMode={pluginInteractionMode ? "page" : "editor"}
      editable={!pluginInteractionMode}
      {mockEvent}
      {layoutRevision}
      commands={editorCommands}
      bind:commandOpen
      bind:selectedItemId
      bind:activeLayerId
      bind:snapEnabled
      bind:gridSize
      bind:zoom
      bind:panX
      bind:panY
      bind:stage
      onSave={() => save()}
      onRefreshPreview={refreshPreview}
      onDuplicateSelected={duplicateSelected}
      onDeleteSelected={deleteSelected}
      onAddVisualAt={addVisual}
      onClose={closeEditor}
    >
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
              <VisualLibrary entries={visualExports} bind:search={visualSearch} onadd={addVisual} onplace={placeVisual} />
            </div>
          {/if}
        </section>

        <!-- Layers -->
        <section class="accordion">
          <button class="accordion-header" onclick={() => showLayers = !showLayers} aria-expanded={showLayers}>
            <svg class:rotated={showLayers} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
            <h2>Layers</h2>
          </button>
          {#if showLayers}
            <div class="accordion-body">
              <LayerItemsPanel
                {layers}
                {activeLayerId}
                {selectedItemId}
                allowEventLayer
                onaddlayer={() => addLayer("normal")}
                onaddeventlayer={() => addLayer("event")}
                canDeleteLayer={(layer) => layer.kind !== "event"}
                canMoveLayer={(layer) => layer.kind !== "event"}
                onselectlayer={selectLayer}
                onselectitem={selectLayerItem}
                onrenamelayer={renameLayer}
                oncommitlayer={() => save()}
                ontogglelayervisible={toggleLayerVisible}
                ontogglelayerlock={toggleLayerLocked}
                onmovelayer={moveLayer}
                ondeletelayer={deleteLayer}
                ontoggleitemvisible={toggleItemVisible}
                ontoggleitemlock={toggleItemLocked}
                onreorderitem={reorderLayerItem}
              />
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
                    oncommit={() => {
                      refreshPreview();
                      return save();
                    }}
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
                  <input type="checkbox" bind:checked={pluginInteractionMode} />
                  <span class="checkmark"></span> Interact
                </label>
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

    </EditorWorkspace>
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

  .accordion-header svg {
    color: var(--text-secondary);
    transition: transform 0.2s ease;
  }
  .accordion-header svg.rotated { transform: rotate(90deg); }

  .accordion-header h2 {
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

  .toggles-row { display: flex; flex-wrap: wrap; gap: 12px 16px; margin: 4px 0; }

  .workspace-settings {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px 12px;
    align-items: center;
  }

  .workspace-settings .checkbox-label {
    min-width: 0;
    white-space: nowrap;
  }

  .workspace-settings .prop-group.row-align {
    grid-column: 1 / -1;
    gap: 12px;
  }

  /* Buttons & Checkboxes */
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
    flex: none;
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

</style>
