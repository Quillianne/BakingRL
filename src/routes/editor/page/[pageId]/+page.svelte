<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ColorField from "$lib/ColorField.svelte";
  import InstanceSettingsForm from "$lib/InstanceSettingsForm.svelte";
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
  import { preserveScroll } from "$lib/editor/preserveScroll";
  import { createLayoutThumbnail } from "$lib/layoutThumbnail";
  import VisualLibrary from "$lib/editor/VisualLibrary.svelte";
  import {
    captureRouteReturnState,
    returnStateQuery,
    routeReturnFromParams,
    storePendingRouteReturn,
    storeRouteScrollRestore
  } from "$lib/returnState";

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

  type PageItem = {
    id: string;
    kind: "visual" | "text" | "image" | "shape";
    package_id?: string | null;
    export_name?: string | null;
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

  type PageLayer = {
    id: string;
    name: string;
    kind: "normal";
    visible: boolean;
    locked: boolean;
    order: number;
    items: PageItem[];
  };

  type PageLayout = {
    id: string;
    name: string;
    favorite: boolean;
    width: number;
    height: number;
    background: {
      kind: "color" | "image";
      color: string;
      image?: string | null;
      fit: "cover" | "contain" | "stretch";
    };
    settings: {
      open_target: "app" | "window";
    };
    layers: PageLayer[];
    template_source?: string | null;
    thumbnail?: string | null;
  };

  type PagesFile = {
    pages: PageLayout[];
  };

  type LayerDropPreview = {
    layerId: string;
    itemId: string | null;
    position: ItemDropPosition;
  };

  let packages = $state<PackageDescriptor[]>([]);
  let pages = $state<PagesFile | null>(null);
  let selectedItemId = $state("");
  let activeLayerId = $state("");
  let visualSearch = $state("");
  let previewMode = $state(false);
  let message = $state("");
  let visualLayerDropTarget = $state<LayerDropPreview | null>(null);
  let stage = $state<HTMLElement>();
  let snapEnabled = $state(true);
  let gridSize = $state(20);
  let layoutRevision = $state(0);
  let zoom = $state(0.58);
  let panX = $state(0);
  let panY = $state(0);
  let commandOpen = $state(false);
  let showApp = $state(true);
  let showAdd = $state(true);
  let showBackground = $state(false);
  let showLayers = $state(true);
  let showProperties = $state(true);

  const page = $derived(pages?.pages.find((entry) => entry.id === data.pageId) ?? null);
  const layers = $derived(page ? sortedLayers(page) : []);
  const activeLayer = $derived(layers.find((layer) => layer.id === activeLayerId) ?? layers[0] ?? null);
  const selectedEntry = $derived(findItem(selectedItemId));
  const propertiesScrollKey = $derived(`page:${page?.id ?? data.pageId}:properties:${selectedItemId || "none"}`);
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
      id: "add-text",
      label: "Text",
      detail: "Native",
      keywords: "insert add",
      run: () => addNative("text")
    },
    {
      id: "add-image",
      label: "Image",
      detail: "Native",
      keywords: "insert add",
      run: () => addNative("image")
    },
    {
      id: "add-shape",
      label: "Shape",
      detail: "Native",
      keywords: "insert add",
      run: () => addNative("shape")
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
    if (!activeLayerId && layers.length) activeLayerId = layers[0].id;
  });

  async function refresh() {
    const [nextPackages, nextPages] = await Promise.all([
      invoke<PackageDescriptor[]>("list_packages"),
      invoke<PagesFile>("get_pages")
    ]);
    packages = nextPackages;
    pages = nextPages;
  }

  async function save(pageToSave = page) {
    if (!pageToSave) return;
    normalizePage(pageToSave);
    reindexLayers(pageToSave);
    pages = await invoke<PagesFile>("save_page", { page: pageToSave });
    layoutRevision += 1;
    message = "Saved.";
    setTimeout(() => (message = ""), 1500);
  }

  async function saveThumbnail() {
    if (!page) return;
    page.thumbnail = createLayoutThumbnail(page, { kind: "page" });
    await save(page);
  }

  function sortedLayers(page: PageLayout) {
    return [...page.layers].sort((a, b) => a.order - b.order);
  }

  function reindexLayers(page: PageLayout) {
    const orderedLayers = sortedLayers(page);
    orderedLayers.forEach((layer, index) => {
      layer.order = index;
      layer.kind = "normal";
    });
    page.layers = orderedLayers;
  }

  function asWholeNumber(value: unknown, fallback = 0) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? Math.round(parsed) : fallback;
  }

  function normalizePage(page: PageLayout) {
    page.width = Math.max(320, asWholeNumber(page.width, 1440));
    page.height = Math.max(240, asWholeNumber(page.height, 900));
    for (const layer of page.layers) {
      for (const item of layer.items) normalizeItem(item);
    }
  }

  function normalizeItem(item: PageItem) {
    item.x = asWholeNumber(item.x);
    item.y = asWholeNumber(item.y);
    item.width = Math.max(24, asWholeNumber(item.width, 320));
    item.height = Math.max(18, asWholeNumber(item.height, 120));
    item.z_index = asWholeNumber(item.z_index);
  }

  function findItem(itemId: string) {
    if (!page || !itemId) return null;
    for (const layer of page.layers) {
      const item = layer.items.find((entry) => entry.id === itemId);
      if (item) return { layer, item };
    }
    return null;
  }

  function addLayer() {
    if (!page) return;
    const layer: PageLayer = {
      id: `layer-${Date.now()}`,
      name: `Layer ${page.layers.length + 1}`,
      kind: "normal",
      visible: true,
      locked: false,
      order: page.layers.length,
      items: []
    };
    page.layers = [...page.layers, layer];
    activeLayerId = layer.id;
    void save();
  }

  function deleteLayer(layer: PageLayer) {
    if (!page || page.layers.length <= 1) return;
    page.layers = page.layers.filter((entry) => entry.id !== layer.id);
    if (activeLayerId === layer.id) activeLayerId = page.layers[0]?.id ?? "";
    if (selectedEntry?.layer.id === layer.id) selectedItemId = "";
    void save();
  }

  function layerDropTargetForClient(clientX: number, clientY: number) {
    const element = document.elementFromPoint(clientX, clientY) as HTMLElement | null;
    const itemRow = element?.closest<HTMLElement>(".item-row[data-editor-layer-id][data-editor-item-id]");
    if (itemRow) {
      const layer = page?.layers.find((entry) => entry.id === itemRow.dataset.editorLayerId);
      const item = layer?.items.find((entry) => entry.id === itemRow.dataset.editorItemId) ?? null;
      if (!layer || layer.locked || !item) return null;
      const rect = itemRow.getBoundingClientRect();
      const position: ItemDropPosition = clientY < rect.top + rect.height / 2 ? "before" : "after";
      return { layer, targetItem: item, position };
    }

    const layerRow = element?.closest<HTMLElement>(".layer-row[data-editor-layer-id]");
    const layer = layerRow ? page?.layers.find((entry) => entry.id === layerRow.dataset.editorLayerId) : null;
    if (!layer || layer.locked) return null;
    return { layer, targetItem: null, position: "end" as ItemDropPosition };
  }

  function createVisualItem(ref: string, targetLayer: PageLayer, placement?: PlacementPoint | null) {
    if (!page || !ref) return null;
    const selected = visualExports.find((entry) => entry.ref === ref);
    if (!selected) return null;
    const width = Math.round(Math.min(Math.max(24, Number(selected.visual.default_width) || 320), page.width));
    const height = Math.round(Math.min(Math.max(18, Number(selected.visual.default_height) || 180), page.height));
    const point = insertionPoint(page, width, height, placement);
    return {
      id: `item-${Date.now()}`,
      kind: "visual",
      package_id: selected.package.id,
      export_name: selected.visual.name,
      name: selected.visual.name,
      x: point.x,
      y: point.y,
      width,
      height,
      z_index: nextZIndex(targetLayer.items),
      visible: true,
      locked: false,
      opacity: 1,
      settings: {}
    } satisfies PageItem;
  }

  function addVisualToLayer(
    ref: string,
    targetLayer: PageLayer,
    placement?: PlacementPoint | null,
    targetItem: PageItem | null = null,
    position: ItemDropPosition = "end"
  ) {
    if (!page || !ref || targetLayer.locked) return;
    const item = createVisualItem(ref, targetLayer, placement);
    if (!item) return;
    visualLayerDropTarget = null;
    item.z_index = nextZIndex(targetLayer.items);
    targetLayer.items = [...targetLayer.items, item];
    if (targetItem && position !== "end") {
      moveItemToLayer(targetLayer, item, targetLayer, targetItem, position);
    }
    selectedItemId = item.id;
    activeLayerId = targetLayer.id;
    void save();
  }

  function addVisual(ref: string, placement?: PlacementPoint | null) {
    if (!activeLayer) return;
    addVisualToLayer(ref, activeLayer, placement);
  }

  function updateVisualLayerDropTarget(_ref: string, event: PointerEvent) {
    const target = layerDropTargetForClient(event.clientX, event.clientY);
    visualLayerDropTarget = target
      ? {
          layerId: target.layer.id,
          itemId: target.targetItem?.id ?? null,
          position: target.position
        }
      : null;
  }

  function clearVisualLayerDropTarget() {
    visualLayerDropTarget = null;
  }

  function addNative(kind: "text" | "image" | "shape") {
    if (!page || !activeLayer) return;
    const defaults = {
      text: { width: 360, height: 120, settings: { text: "New text", color: "var(--text-primary)", fontSize: 28 } },
      image: { width: 420, height: 240, settings: { src: "", fit: "cover" } },
      shape: {
        width: 320,
        height: 180,
        settings: {
          fill: "color-mix(in srgb, var(--accent) 18%, transparent)",
          borderColor: "color-mix(in srgb, var(--text-primary) 24%, transparent)",
          borderRadius: 8
        }
      }
    }[kind];
    const item: PageItem = {
      id: `item-${Date.now()}`,
      kind,
      name: kind === "text" ? "Text" : kind === "image" ? "Image" : "Shape",
      x: Math.round(page.width / 2 - defaults.width / 2),
      y: Math.round(page.height / 2 - defaults.height / 2),
      width: defaults.width,
      height: defaults.height,
      z_index: nextZIndex(activeLayer.items),
      visible: true,
      locked: false,
      opacity: 1,
      settings: defaults.settings
    };
    activeLayer.items = [...activeLayer.items, item];
    selectedItemId = item.id;
    void save();
  }

  function duplicateSelected() {
    if (!selectedEntry) return;
    duplicateItem(selectedEntry.layer, selectedEntry.item);
  }

  function duplicateItem(layer: PageLayer, sourceItem: PageItem) {
    const item = JSON.parse(JSON.stringify(sourceItem)) as PageItem;
    item.id = `item-${Date.now()}`;
    item.name = `${item.name} Copy`;
    item.x += 24;
    item.y += 24;
    item.z_index = nextZIndex(layer.items);
    layer.items = [...layer.items, item];
    selectedItemId = item.id;
    activeLayerId = layer.id;
    void save();
  }

  function deleteSelected() {
    if (!selectedEntry) return;
    deleteItem(selectedEntry.layer, selectedEntry.item);
  }

  function deleteItem(layer: PageLayer, itemToDelete: PageItem) {
    layer.items = layer.items.filter((item) => item.id !== itemToDelete.id);
    selectedItemId = "";
    void save();
  }

  function pointForClient(clientX: number, clientY: number) {
    if (!stage) return { x: 0, y: 0 };
    const rect = stage.getBoundingClientRect();
    return {
      x: ((clientX - rect.left) / rect.width) * (page?.width ?? 1),
      y: ((clientY - rect.top) / rect.height) * (page?.height ?? 1)
    };
  }

  function refreshPreview() {
    layoutRevision += 1;
    pages = pages;
  }

  function selectItemEntry(entry: { layer: PageLayer; item: PageItem }) {
    activeLayerId = entry.layer.id;
    selectedItemId = entry.item.id;
  }

  function itemSetting(key: string, fallback: string | number | boolean) {
    if (!selectedEntry) return fallback;
    const value = selectedEntry.item.settings?.[key];
    return value === undefined || value === null ? fallback : value;
  }

  function updateItemSetting(key: string, value: string | number | boolean) {
    if (!selectedEntry) return;
    selectedEntry.item.settings = { ...(selectedEntry.item.settings ?? {}), [key]: value };
    refreshPreview();
    void save();
  }

  function bringSelectedForward() {
    if (!selectedEntry || !page) return;
    moveItemToStackEdge(selectedEntry.item, selectedEntry.layer.items, "front");
    void save();
  }

  function sendSelectedBackward() {
    if (!selectedEntry || !page) return;
    moveItemToStackEdge(selectedEntry.item, selectedEntry.layer.items, "back");
    void save();
  }

  function selectLayer(layer: PageLayer) {
    activeLayerId = layer.id;
  }

  function selectLayerItem(layer: PageLayer, item: PageItem) {
    selectItemEntry({ layer, item });
  }

  function renameLayer(layer: PageLayer, name: string) {
    layer.name = name;
  }

  function moveLayer(layer: PageLayer, direction: -1 | 1) {
    if (!page) return;
    const ordered = sortedLayers(page);
    const index = ordered.findIndex((entry) => entry.id === layer.id);
    const target = ordered[index + direction];
    if (!target) return;
    const order = layer.order;
    layer.order = target.order;
    target.order = order;
    void save();
  }

  function toggleLayerVisible(layer: PageLayer) {
    layer.visible = !layer.visible;
    void save();
  }

  function toggleLayerLocked(layer: PageLayer) {
    layer.locked = !layer.locked;
    void save();
  }

  function toggleItemVisible(layer: PageLayer, item: PageItem) {
    item.visible = !item.visible;
    selectLayerItem(layer, item);
    void save();
  }

  function toggleItemLocked(layer: PageLayer, item: PageItem) {
    item.locked = !item.locked;
    selectLayerItem(layer, item);
    void save();
  }

  function reorderLayerItem(
    sourceLayer: PageLayer,
    item: PageItem,
    targetLayer: PageLayer,
    targetItem: PageItem | null,
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
    const layerTarget = layerDropTargetForClient(event.clientX, event.clientY);
    if (layerTarget) {
      addVisualToLayer(ref, layerTarget.layer, null, layerTarget.targetItem, layerTarget.position);
      return;
    }

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

  async function openPage() {
    if (!page) return;
    await saveThumbnail();
    const returnState = captureRouteReturnState();
    storePendingRouteReturn(returnState);
    window.location.href = `/page/${encodeURIComponent(page.id)}${returnStateQuery(returnState)}`;
  }

  async function closeEditor() {
    await saveThumbnail();
    const returnState = routeReturnFromParams(data.returnTo, data.scrollY, "/pages");
    storeRouteScrollRestore(returnState);
    await goto(returnState.returnTo);
  }

  onMount(() => {
    void refresh().catch((error) => {
      message = String(error);
    });
    let unlistenPages: (() => void) | undefined;
    let unlistenPackages: (() => void) | undefined;
    void listen<PagesFile>("bakingrl-pages-changed", (event) => {
      pages = event.payload;
    }).then((unlisten) => {
      unlistenPages = unlisten;
    });
    void listen<PackageDescriptor[]>("bakingrl-packages-changed", (event) => {
      packages = event.payload;
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    return () => {
      unlistenPages?.();
      unlistenPackages?.();
    };
  });
</script>

<main class="page-editor-page">
  {#if page}
    <EditorWorkspace
      title={page.name}
      {message}
      canvas={page}
      {layers}
      centerKey={page.id}
      canvasAriaLabel="Page editor canvas"
      rendererSource="page"
      rendererMode={previewMode ? "page" : "editor"}
      editable={!previewMode}
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
            <button class="accordion-header" onclick={() => (showApp = !showApp)}>
              <svg class:rotated={showApp} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"></polyline></svg>
              <h2>App</h2>
            </button>
            {#if showApp}
              <div class="accordion-body">
                <label>Name<input bind:value={page.name} onblur={() => save()} /></label>
                <div class="split">
                  <label>Width<input type="number" step="1" bind:value={page.width} onblur={() => save()} /></label>
                  <label>Height<input type="number" step="1" bind:value={page.height} onblur={() => save()} /></label>
                </div>
                <label>Open target
                  <select bind:value={page.settings.open_target} onchange={() => save()}>
                    <option value="app">In app</option>
                    <option value="window">New app window</option>
                  </select>
                </label>
                <label class="toggle">
                  <input type="checkbox" bind:checked={previewMode} />
                  Preview
                </label>
                <div class="actions">
                  <button class="btn-primary" onclick={openPage}>Open</button>
                </div>
              </div>
            {/if}
          </section>

          <section class="accordion">
            <button class="accordion-header" onclick={() => (showAdd = !showAdd)}>
              <svg class:rotated={showAdd} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"></polyline></svg>
              <h2>Add</h2>
            </button>
            {#if showAdd}
              <div class="accordion-body">
                <div class="native-buttons">
                  <button class="btn-secondary" onclick={() => addNative("text")}>Text</button>
                  <button class="btn-secondary" onclick={() => addNative("image")}>Image</button>
                  <button class="btn-secondary" onclick={() => addNative("shape")}>Shape</button>
                </div>
                <VisualLibrary
                  entries={visualExports}
                  bind:search={visualSearch}
                  searchPlaceholder="Search plugin visuals..."
                  onadd={addVisual}
                  onplace={placeVisual}
                  ondragmove={updateVisualLayerDropTarget}
                  ondragend={clearVisualLayerDropTarget}
                />
              </div>
            {/if}
          </section>

          <section class="accordion">
            <button class="accordion-header" onclick={() => (showBackground = !showBackground)}>
              <svg class:rotated={showBackground} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"></polyline></svg>
              <h2>Background</h2>
            </button>
            {#if showBackground}
              <div class="accordion-body">
                <label>Type
                  <select bind:value={page.background.kind} onchange={() => save()}>
                    <option value="color">Color</option>
                    <option value="image">Image URL</option>
                  </select>
                </label>
                <ColorField label="Color" value={page.background.color} oncommit={(value) => { page.background.color = value; void save(); }} />
                {#if page.background.kind === "image"}
                  <label>Image URL<input bind:value={page.background.image} onblur={() => save()} /></label>
                  <label>Fit
                    <select bind:value={page.background.fit} onchange={() => save()}>
                      <option value="cover">Cover</option>
                      <option value="contain">Contain</option>
                      <option value="stretch">Stretch</option>
                    </select>
                  </label>
                {/if}
              </div>
            {/if}
          </section>

          <section class="accordion">
            <button class="accordion-header" onclick={() => (showLayers = !showLayers)} aria-expanded={showLayers}>
              <svg class:rotated={showLayers} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"></polyline></svg>
              <h2>Layers</h2>
            </button>
            {#if showLayers}
              <div class="accordion-body">
                <LayerItemsPanel
                  {layers}
                  {activeLayerId}
                  {selectedItemId}
                  onaddlayer={addLayer}
                  canDeleteLayer={() => page.layers.length > 1}
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
                  onduplicateitem={duplicateItem}
                  ondeleteitem={deleteItem}
                  externalDropTarget={visualLayerDropTarget}
                />
              </div>
            {/if}
          </section>

      {/snippet}

      {#snippet rightPanel()}
          <section class="accordion">
            <button class="accordion-header" onclick={() => (showProperties = !showProperties)}>
              <svg class:rotated={showProperties} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"></polyline></svg>
              <h2>Properties</h2>
            </button>
            {#if showProperties}
              <div class="accordion-body" use:preserveScroll={propertiesScrollKey}>
                {#if selectedEntry}
                  <label>Name<input bind:value={selectedEntry.item.name} onblur={() => save()} /></label>
                  <div class="split">
                    <label>X<input type="number" step="1" bind:value={selectedEntry.item.x} onblur={() => save()} /></label>
                    <label>Y<input type="number" step="1" bind:value={selectedEntry.item.y} onblur={() => save()} /></label>
                  </div>
                  <div class="split">
                    <label>Width<input type="number" step="1" bind:value={selectedEntry.item.width} onblur={() => save()} /></label>
                    <label>Height<input type="number" step="1" bind:value={selectedEntry.item.height} onblur={() => save()} /></label>
                  </div>
                  <label>Opacity<input type="range" min="0" max="1" step="0.05" bind:value={selectedEntry.item.opacity} onchange={() => save()} /></label>
                  <div class="native-buttons">
                    <label class="toggle"><input type="checkbox" bind:checked={selectedEntry.item.visible} onchange={() => save()} /> Visible</label>
                    <label class="toggle"><input type="checkbox" bind:checked={selectedEntry.item.locked} onchange={() => save()} /> Locked</label>
                  </div>
                  {#if selectedEntry.item.kind === "visual" && selectedEntry.item.package_id && selectedEntry.item.export_name}
                  <InstanceSettingsForm
                    item={selectedEntry.item}
                    packageId={selectedEntry.item.package_id}
                    exportName={selectedEntry.item.export_name}
                    onpreview={refreshPreview}
                    oncommit={() => {
                      refreshPreview();
                      return save();
                      }}
                    />
                  {:else if selectedEntry.item.kind === "text"}
                    <label>Text<textarea value={String(itemSetting("text", ""))} onblur={(event) => updateItemSetting("text", event.currentTarget.value)}></textarea></label>
                    <ColorField label="Color" value={String(itemSetting("color", "var(--text-primary)"))} oncommit={(value) => updateItemSetting("color", value)} />
                    <label>Font size<input type="number" step="1" value={String(itemSetting("fontSize", 28))} onblur={(event) => updateItemSetting("fontSize", asWholeNumber(event.currentTarget.value, 28))} /></label>
                  {:else if selectedEntry.item.kind === "image"}
                    <label>Source URL<input value={String(itemSetting("src", ""))} onblur={(event) => updateItemSetting("src", event.currentTarget.value)} /></label>
                    <label>Fit
                      <select value={String(itemSetting("fit", "cover"))} onchange={(event) => updateItemSetting("fit", event.currentTarget.value)}>
                        <option value="cover">Cover</option>
                        <option value="contain">Contain</option>
                        <option value="stretch">Stretch</option>
                      </select>
                    </label>
                  {:else if selectedEntry.item.kind === "shape"}
                    <ColorField label="Fill" value={String(itemSetting("fill", "color-mix(in srgb, var(--accent) 18%, transparent)"))} oncommit={(value) => updateItemSetting("fill", value)} />
                    <ColorField label="Border color" value={String(itemSetting("borderColor", "color-mix(in srgb, var(--text-primary) 24%, transparent)"))} oncommit={(value) => updateItemSetting("borderColor", value)} />
                    <label>Border width<input type="number" step="1" min="0" value={String(itemSetting("borderWidth", 1))} onblur={(event) => updateItemSetting("borderWidth", asWholeNumber(event.currentTarget.value, 1))} /></label>
                    <label>Border radius<input type="number" step="1" min="0" value={String(itemSetting("borderRadius", 8))} onblur={(event) => updateItemSetting("borderRadius", asWholeNumber(event.currentTarget.value, 8))} /></label>
                  {/if}
                  <div class="actions">
                    <button class="btn-secondary" onclick={duplicateSelected}>Duplicate</button>
                    <button class="btn-danger" onclick={deleteSelected}>Delete</button>
                  </div>
                {:else}
                  <p class="empty">Select an item on the page.</p>
                {/if}

                <div class="divider"></div>
                <div class="workspace-settings">
                  <label class="toggle">
                    <input type="checkbox" bind:checked={snapEnabled} />
                    Enable Snapping
                  </label>
                  <label class="grid-size-control">
                    Grid Size
                    <input type="number" min="1" max="200" bind:value={gridSize} disabled={!snapEnabled} />
                  </label>
                </div>
              </div>
            {/if}
          </section>
      {/snippet}

    </EditorWorkspace>
  {:else}
    <section class="loading">{message || "Loading page..."}</section>
  {/if}
</main>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: var(--editor-bg-dark);
  }

  main {
    position: relative;
    width: 100vw;
    height: var(--app-content-height, 100vh);
    overflow: hidden;
    color: var(--text-primary);
    background: var(--editor-bg-dark);
    font-family: var(--font-family);
  }

  h2 {
    margin: 0;
    font-size: 13px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  input,
  select,
  textarea {
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--border-color);
    border-radius: 5px;
    padding: 7px 8px;
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    color: var(--text-primary);
    font: inherit;
    font-size: 13px;
  }

  textarea {
    min-height: 120px;
    resize: vertical;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  }

  .split {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 8px;
  }

  .actions,
  .native-buttons {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .workspace-settings {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .grid-size-control {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 72px;
    align-items: center;
    gap: 8px;
  }

  .toggle {
    flex-direction: row;
    align-items: center;
    text-transform: none;
    color: var(--text-primary);
    font-size: 12px;
  }

  .toggle input {
    width: auto;
  }

  button {
    border: 1px solid var(--border-color);
    border-radius: 6px;
    color: var(--text-primary);
    cursor: pointer;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .btn-primary,
  .btn-secondary,
  .btn-danger {
    padding: 8px 10px;
    font-weight: 700;
  }

  .btn-primary {
    background: var(--accent);
  }

  .btn-secondary {
    background: var(--bg-panel-hover);
  }

  .btn-danger {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    border-color: color-mix(in srgb, var(--danger) 40%, transparent);
    color: var(--danger);
  }

  .empty,
  .loading {
    color: var(--text-secondary);
  }

  .divider {
    height: 1px;
    background: var(--border-color);
  }

  .loading {
    height: 100%;
    display: grid;
    place-items: center;
  }

  .accordion {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    min-height: 0;
    max-height: 100%;
    overflow: hidden;
    gap: 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--editor-bg-panel);
    pointer-events: auto;
  }

  .accordion-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    text-align: left;
  }

  .accordion-header:hover {
    background: var(--editor-bg-panel-hover);
  }

  .accordion-header svg {
    color: var(--text-secondary);
    transition: transform 0.2s ease;
  }

  .accordion-header svg.rotated {
    transform: rotate(90deg);
  }

  .accordion-header h2 {
    margin: 0;
    flex: 1;
    font-size: 13px;
    font-weight: 700;
  }

  .accordion-body {
    display: flex;
    min-height: 0;
    flex-direction: column;
    gap: 12px;
    padding: 0 12px 12px 12px;
    overflow-y: auto;
    overscroll-behavior: contain;
  }

</style>
