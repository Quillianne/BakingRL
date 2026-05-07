<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import ColorField from "$lib/ColorField.svelte";
  import EditorTemplate from "$lib/EditorTemplate.svelte";
  import InstanceSettingsForm from "$lib/InstanceSettingsForm.svelte";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
  import { emptySnapGuides, snapGuideStyle, snapItemPosition, type SnapGuides } from "$lib/editor/snapping";
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
  };

  type PagesFile = {
    pages: PageLayout[];
  };

  type DragState = {
    mode: "move" | "resize";
    itemId: string;
    startPointer: { x: number; y: number };
    startItem: Pick<PageItem, "x" | "y" | "width" | "height">;
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

  let packages = $state<PackageDescriptor[]>([]);
  let pages = $state<PagesFile | null>(null);
  let selectedItemId = $state("");
  let activeLayerId = $state("");
  let visualSearch = $state("");
  let previewMode = $state(false);
  let message = $state("");
  let stageWrap = $state<HTMLElement>();
  let stage = $state<HTMLElement>();
  let dragState = $state<DragState | null>(null);
  let panState = $state<PanState | null>(null);
  let contextMenu = $state<ContextMenuState | null>(null);
  let snapEnabled = $state(true);
  let gridSize = $state(20);
  let snapGuides = $state<SnapGuides>(emptySnapGuides());
  let layoutRevision = $state(0);
  let zoom = $state(0.58);
  let panX = $state(0);
  let panY = $state(0);
  let showApp = $state(true);
  let showAdd = $state(true);
  let showBackground = $state(false);
  let showLayers = $state(true);
  let showProperties = $state(true);

  const page = $derived(pages?.pages.find((entry) => entry.id === data.pageId) ?? null);
  const layers = $derived(page ? sortedLayers(page) : []);
  const activeLayer = $derived(layers.find((layer) => layer.id === activeLayerId) ?? layers[0] ?? null);
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
    if (!activeLayerId && layers.length) activeLayerId = layers[0].id;
  });

  async function refresh() {
    packages = await invoke<PackageDescriptor[]>("list_packages");
    pages = await invoke<PagesFile>("get_pages");
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

  function addVisual(ref: string) {
    if (!page || !activeLayer || !ref) return;
    const selected = visualExports.find((entry) => entry.ref === ref);
    if (!selected) return;
    const item: PageItem = {
      id: `item-${Date.now()}`,
      kind: "visual",
      package_id: selected.package.id,
      export_name: selected.visual.name,
      name: selected.visual.name,
      x: Math.round(page.width / 2 - selected.visual.default_width / 2),
      y: Math.round(page.height / 2 - selected.visual.default_height / 2),
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
      z_index: activeLayer.items.length + 10,
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
    selectedEntry.layer.items = selectedEntry.layer.items.filter((item) => item.id !== selectedEntry.item.id);
    selectedItemId = "";
    void save();
  }

  function pointForEvent(event: PointerEvent) {
    if (!stage) return { x: 0, y: 0 };
    const rect = stage.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / rect.width) * (page?.width ?? 1),
      y: ((event.clientY - rect.top) / rect.height) * (page?.height ?? 1)
    };
  }

  function clampItem(item: PageItem) {
    if (!page) return;
    item.width = Math.max(24, Math.min(item.width, page.width));
    item.height = Math.max(18, Math.min(item.height, page.height));
    item.x = Math.max(0, Math.min(item.x, page.width - item.width));
    item.y = Math.max(0, Math.min(item.y, page.height - item.height));
    normalizeItem(item);
  }

  function refreshPreview() {
    layoutRevision += 1;
    pages = pages;
  }

  function allItems() {
    if (!page) return [];
    return page.layers.flatMap((layer) => layer.items.map((item) => ({ layer, item })));
  }

  function itemStyle(item: PageItem) {
    if (!page) return "";
    return `
      left:${(item.x / page.width) * 100}%;
      top:${(item.y / page.height) * 100}%;
      width:${(item.width / page.width) * 100}%;
      height:${(item.height / page.height) * 100}%;
      z-index:${item.z_index};
    `;
  }

  function guideStyle(axis: "x" | "y", value: number | null) {
    return snapGuideStyle(axis, value, page);
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

  function startDrag(event: PointerEvent, item: PageItem, mode: "move" | "resize") {
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
    if (!dragState || !page) return;
    const entry = findItem(dragState.itemId);
    if (!entry) return;
    const pointer = pointForEvent(event);
    const dx = pointer.x - dragState.startPointer.x;
    const dy = pointer.y - dragState.startPointer.y;
    if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) dragState.changed = true;
    if (dragState.mode === "move") {
      entry.item.x = dragState.startItem.x + dx;
      entry.item.y = dragState.startItem.y + dy;
      snapGuides = snapItemPosition(entry.item, page, allItems().map((entry) => entry.item), {
        enabled: snapEnabled,
        gridSize
      });
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

  function itemSetting(key: string, fallback: string | number | boolean) {
    if (!selectedEntry) return fallback;
    const value = selectedEntry.item.settings?.[key];
    return value === undefined || value === null ? fallback : value;
  }

  function updateItemSetting(key: string, value: string | number | boolean) {
    if (!selectedEntry) return;
    selectedEntry.item.settings = { ...(selectedEntry.item.settings ?? {}), [key]: value };
    void save();
  }

  function openContextMenu(event: MouseEvent, item: PageItem) {
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
    if (!selectedEntry || !page) return;
    selectedEntry.item.z_index = Math.max(0, ...page.layers.flatMap((layer) => layer.items.map((item) => item.z_index))) + 1;
    contextMenu = null;
    void save();
  }

  function sendSelectedBackward() {
    if (!selectedEntry || !page) return;
    selectedEntry.item.z_index = Math.min(0, ...page.layers.flatMap((layer) => layer.items.map((item) => item.z_index))) - 1;
    contextMenu = null;
    void save();
  }

  function openPage() {
    if (!page) return;
    const returnState = captureRouteReturnState();
    storePendingRouteReturn(returnState);
    window.location.href = `/page/${encodeURIComponent(page.id)}${returnStateQuery(returnState)}`;
  }

  async function closeEditor() {
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
    <EditorTemplate
      title={page.name}
      {message}
      canvas={page}
      centerKey={page.id}
      canvasAriaLabel="Page editor canvas"
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
        <OverlayRenderer
          source="page"
          layoutId={page.id}
          layoutOverride={page}
          {layoutRevision}
          mode={previewMode ? "page" : "editor"}
        />
        {#if !previewMode}
          <div class="frames">
            {#each page.layers.flatMap((layer) => layer.items.map((item) => ({ layer, item }))) as entry}
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
                  <span>{entry.item.name}</span>
                  <small>{entry.layer.name}</small>
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
        {/if}
      {/snippet}

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
            <div class="accordion-header accordion-header-with-actions">
              <button class="accordion-toggle" onclick={() => (showLayers = !showLayers)} aria-expanded={showLayers}>
                <svg class:rotated={showLayers} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"></polyline></svg>
                <h2>Layers</h2>
              </button>
              <div class="header-actions">
                <button class="icon-btn add-layer-btn" onclick={addLayer} title="Add layer">
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
                </button>
              </div>
            </div>
            {#if showLayers}
              <div class="accordion-body">
                <div class="list layer-list">
                  {#each layers as layer}
                    <div class="layer-row" class:active={activeLayerId === layer.id}>
                      <button class="layer-select" onclick={() => (activeLayerId = layer.id)} title="Select layer">
                        <span class="layer-indicator"></span>
                      </button>
                      <input
                        class="layer-name-input"
                        bind:value={layer.name}
                        onclick={() => (activeLayerId = layer.id)}
                        onfocus={() => (activeLayerId = layer.id)}
                        onblur={() => save()}
                      />
                      <button class="icon-btn" onclick={() => { layer.visible = !layer.visible; void save(); }} title="Toggle visibility">
                        {layer.visible ? "On" : "Off"}
                      </button>
                      <button class="icon-btn danger" onclick={() => deleteLayer(layer)} disabled={page.layers.length <= 1} title="Delete layer">Del</button>
                    </div>
                  {/each}
                </div>
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
              <div class="accordion-body">
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
                      oncommit={() => save()}
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

  .frames {
    position: absolute;
    inset: 0;
    z-index: 100;
    pointer-events: none;
  }

  .frame-box {
    position: absolute;
    border: 1px solid color-mix(in srgb, var(--accent) 58%, transparent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    box-sizing: border-box;
    cursor: move;
    pointer-events: auto;
    touch-action: none;
  }

  .frame-box.selected {
    border: 2px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .frame-box.hidden {
    opacity: 0.35;
    border-style: dashed;
  }

  .frame-box.locked {
    border-color: var(--warn);
  }

  .frame-label {
    position: absolute;
    left: -1px;
    top: -24px;
    display: flex;
    gap: 8px;
    max-width: 260px;
    padding: 3px 8px;
    border-radius: 4px 4px 0 0;
    overflow: hidden;
    background: var(--accent);
    color: var(--text-primary);
    font-size: 11px;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
  }

  .frame-label small {
    opacity: 0.72;
  }

  .frame-box:hover .frame-label,
  .frame-box.selected .frame-label {
    opacity: 1;
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

  .btn-secondary,
  .icon-btn {
    background: var(--bg-panel-hover);
  }

  .btn-danger,
  .danger {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    border-color: color-mix(in srgb, var(--danger) 40%, transparent);
    color: var(--danger);
  }

  .icon-btn {
    min-width: 30px;
    min-height: 30px;
    display: grid;
    place-items: center;
    padding: 0 7px;
  }

  .layer-row {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto auto;
    gap: 8px;
    align-items: center;
  }

  .layer-row.active {
    color: var(--accent);
  }

  .layer-select {
    background: transparent;
    border-color: transparent;
  }

  .layer-indicator {
    width: 9px;
    height: 9px;
    border: 2px solid var(--text-muted);
    border-radius: 999px;
  }

  .layer-row.active .layer-indicator {
    border-color: var(--accent);
    background: var(--accent);
  }

  .layer-name-input {
    background: transparent;
    border-color: transparent;
    color: var(--text-primary);
    font-weight: 650;
  }

  .layer-name-input:focus {
    border-color: var(--border-color);
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
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

  .accordion-header:hover,
  .accordion-toggle:hover {
    background: var(--editor-bg-panel-hover);
  }

  .accordion-header-with-actions {
    padding: 0;
    cursor: default;
  }

  .accordion-toggle {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    border: 0;
    background: transparent;
    color: var(--text-primary);
    text-align: left;
  }

  .accordion-header svg,
  .accordion-toggle svg {
    color: var(--text-secondary);
    transition: transform 0.2s ease;
  }

  .accordion-header svg.rotated,
  .accordion-toggle svg.rotated {
    transform: rotate(90deg);
  }

  .accordion-header h2,
  .accordion-toggle h2 {
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

</style>
