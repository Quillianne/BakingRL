<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";

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

  let packages = $state<PackageDescriptor[]>([]);
  let pages = $state<PagesFile | null>(null);
  let selectedItemId = $state("");
  let activeLayerId = $state("");
  let visualSearch = $state("");
  let selectedVisualRef = $state("");
  let previewMode = $state(false);
  let message = $state("");
  let stage = $state<HTMLElement>();
  let dragState = $state<DragState | null>(null);
  let layoutRevision = $state(0);

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
  const filteredVisualExports = $derived(
    visualExports.filter((entry) => {
      const search = visualSearch.trim().toLowerCase();
      if (!search) return true;
      return `${entry.package.name} ${entry.package.id} ${entry.visual.name}`.toLowerCase().includes(search);
    })
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

  function addVisual() {
    if (!page || !activeLayer || !selectedVisualRef) return;
    const selected = visualExports.find((entry) => entry.ref === selectedVisualRef);
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
      text: { width: 360, height: 120, settings: { text: "New text", color: "#f8fafc", fontSize: 28 } },
      image: { width: 420, height: 240, settings: { src: "", fit: "cover" } },
      shape: { width: 320, height: 180, settings: { fill: "rgba(59, 130, 246, 0.18)", borderColor: "rgba(255,255,255,0.24)", borderRadius: 8 } }
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
  }

  function refreshPreview() {
    layoutRevision += 1;
    pages = pages;
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

  function startDrag(event: PointerEvent, item: PageItem, mode: "move" | "resize") {
    const entry = findItem(item.id);
    event.stopPropagation();
    event.preventDefault();
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
    } else {
      entry.item.width = dragState.startItem.width + dx;
      entry.item.height = dragState.startItem.height + dy;
    }
    clampItem(entry.item);
    refreshPreview();
  }

  function pointerUp() {
    if (!dragState) return;
    const shouldSave = dragState.changed;
    dragState = null;
    if (shouldSave) void save();
  }

  function setItemSettings(raw: string) {
    if (!selectedEntry) return;
    try {
      const parsed = raw.trim() ? JSON.parse(raw) : {};
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("Settings must be a JSON object.");
      }
      selectedEntry.item.settings = parsed;
      void save();
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }
  }

  function openPage() {
    if (!page) return;
    window.location.href = `/page/${encodeURIComponent(page.id)}`;
  }

  function backToDashboard() {
    window.location.href = "/";
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

<main onpointermove={pointerMove} onpointerup={pointerUp} class:dragging={dragState !== null}>
  {#if page}
    <section class="stage-wrap">
      <div class="stage" bind:this={stage}>
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
        {/if}
      </div>
    </section>

    <aside class="editor-panel">
      <header>
        <div>
          <strong>{page.name}</strong>
          {#if message}<span>{message}</span>{/if}
        </div>
        <button class="icon-btn" onclick={backToDashboard} title="Back to dashboard">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="19" y1="12" x2="5" y2="12"></line><polyline points="12 19 5 12 12 5"></polyline></svg>
        </button>
      </header>

      <div class="panel-content">
        <section class="panel-section">
          <div class="section-title">
            <h2>Page</h2>
            <label class="toggle">
              <input type="checkbox" bind:checked={previewMode} />
              Preview
            </label>
          </div>
          <label>Name<input bind:value={page.name} onblur={() => save()} /></label>
          <div class="split">
            <label>Width<input type="number" bind:value={page.width} onblur={() => save()} /></label>
            <label>Height<input type="number" bind:value={page.height} onblur={() => save()} /></label>
          </div>
          <label>Open target
            <select bind:value={page.settings.open_target} onchange={() => save()}>
              <option value="app">In app</option>
              <option value="window">New app window</option>
            </select>
          </label>
          <div class="actions">
            <button class="btn-primary" onclick={openPage}>Open</button>
            <button class="btn-secondary" onclick={() => void save()}>Save</button>
          </div>
        </section>

        <section class="panel-section">
          <h2>Background</h2>
          <label>Type
            <select bind:value={page.background.kind} onchange={() => save()}>
              <option value="color">Color</option>
              <option value="image">Image URL</option>
            </select>
          </label>
          <label>Color<input bind:value={page.background.color} onblur={() => save()} /></label>
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
        </section>

        <section class="panel-section">
          <div class="section-title">
            <h2>Add</h2>
          </div>
          <div class="native-buttons">
            <button class="btn-secondary" onclick={() => addNative("text")}>Text</button>
            <button class="btn-secondary" onclick={() => addNative("image")}>Image</button>
            <button class="btn-secondary" onclick={() => addNative("shape")}>Shape</button>
          </div>
          <div class="search-box">
            <input bind:value={visualSearch} placeholder="Search plugin visuals" />
          </div>
          <select bind:value={selectedVisualRef}>
            <option value="">Select a visual...</option>
            {#each filteredVisualExports as entry}
              <option value={entry.ref}>{entry.package.name} / {entry.visual.name}</option>
            {/each}
          </select>
          <button class="btn-primary" onclick={addVisual} disabled={!selectedVisualRef}>Add Visual</button>
        </section>

        <section class="panel-section">
          <div class="section-title">
            <h2>Layers</h2>
            <button class="icon-btn" onclick={addLayer} title="Add layer">
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
            </button>
          </div>
          {#each layers as layer}
            <div class="layer-row" class:active={activeLayerId === layer.id}>
              <button onclick={() => (activeLayerId = layer.id)}>{layer.name}</button>
              <input bind:value={layer.name} onblur={() => save()} />
              <button class="icon-btn" onclick={() => { layer.visible = !layer.visible; void save(); }} title="Toggle visibility">
                {layer.visible ? "On" : "Off"}
              </button>
              <button class="icon-btn danger" onclick={() => deleteLayer(layer)} disabled={page.layers.length <= 1} title="Delete layer">Del</button>
            </div>
          {/each}
        </section>

        <section class="panel-section">
          <h2>Properties</h2>
          {#if selectedEntry}
            <label>Name<input bind:value={selectedEntry.item.name} onblur={() => save()} /></label>
            <div class="split">
              <label>X<input type="number" bind:value={selectedEntry.item.x} onblur={() => save()} /></label>
              <label>Y<input type="number" bind:value={selectedEntry.item.y} onblur={() => save()} /></label>
            </div>
            <div class="split">
              <label>Width<input type="number" bind:value={selectedEntry.item.width} onblur={() => save()} /></label>
              <label>Height<input type="number" bind:value={selectedEntry.item.height} onblur={() => save()} /></label>
            </div>
            <label>Opacity<input type="range" min="0" max="1" step="0.05" bind:value={selectedEntry.item.opacity} onchange={() => save()} /></label>
            <div class="native-buttons">
              <label class="toggle"><input type="checkbox" bind:checked={selectedEntry.item.visible} onchange={() => save()} /> Visible</label>
              <label class="toggle"><input type="checkbox" bind:checked={selectedEntry.item.locked} onchange={() => save()} /> Locked</label>
            </div>
            <label>Settings JSON
              <textarea value={JSON.stringify(selectedEntry.item.settings ?? {}, null, 2)} onblur={(event) => setItemSettings(event.currentTarget.value)}></textarea>
            </label>
            <div class="actions">
              <button class="btn-secondary" onclick={duplicateSelected}>Duplicate</button>
              <button class="btn-danger" onclick={deleteSelected}>Delete</button>
            </div>
          {:else}
            <p class="empty">Select an item on the page.</p>
          {/if}
        </section>
      </div>
    </aside>
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
    height: 100vh;
    overflow: hidden;
    color: var(--text-primary);
    background: var(--editor-bg-dark);
    font-family: var(--font-family);
  }

  main.dragging {
    user-select: none;
    cursor: grabbing;
  }

  .stage-wrap {
    position: absolute;
    inset: 0 360px 0 0;
    display: grid;
    place-items: stretch;
    background: rgba(4, 7, 12, 0.48);
  }

  .stage {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .frames {
    position: absolute;
    inset: 0;
    z-index: 100;
    pointer-events: none;
  }

  .frame-box {
    position: absolute;
    border: 1px solid rgba(59, 130, 246, 0.58);
    background: rgba(59, 130, 246, 0.08);
    box-sizing: border-box;
    cursor: move;
    pointer-events: auto;
    touch-action: none;
  }

  .frame-box.selected {
    border: 2px solid var(--accent);
    background: rgba(59, 130, 246, 0.16);
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
    color: white;
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
    background: white;
    cursor: nwse-resize;
  }

  .editor-panel {
    position: absolute;
    inset: 0 0 0 auto;
    width: 360px;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--border-color);
    background: var(--editor-bg-panel);
    backdrop-filter: blur(18px);
  }

  .editor-panel header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    min-height: 54px;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-color);
  }

  .editor-panel header div {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .editor-panel header strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
  }

  .editor-panel header span {
    color: var(--success);
    font-size: 11px;
  }

  .panel-content {
    min-height: 0;
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .panel-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.18);
  }

  .section-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
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
    background: rgba(0, 0, 0, 0.28);
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
    background: rgba(255, 255, 255, 0.04);
  }

  .btn-danger,
  .danger {
    background: rgba(239, 68, 68, 0.12);
    border-color: rgba(239, 68, 68, 0.4);
    color: #fecaca;
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
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    gap: 6px;
    align-items: center;
  }

  .layer-row.active {
    color: var(--accent);
  }

  .layer-row > button:first-child {
    max-width: 92px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding: 7px;
    background: rgba(255, 255, 255, 0.04);
  }

  .search-box input {
    background: rgba(0, 0, 0, 0.28);
  }

  .empty,
  .loading {
    color: var(--text-secondary);
  }

  .loading {
    height: 100%;
    display: grid;
    place-items: center;
  }
</style>
