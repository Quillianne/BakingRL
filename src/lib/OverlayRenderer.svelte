<script lang="ts">
  import { onMount } from "svelte";
  import { adapter } from "$lib/adapter/index";

  type RendererMode = "runtime" | "editor" | "page";
  type LayoutType = "ingame" | "stream";
  type LayoutSource = "overlay" | "page" | "configuration";

  const {
    layoutType = "ingame",
    layoutId = null,
    layoutOverride = null,
    layoutRevision = 0,
    packageRevision = 0,
    mode = "runtime",
    source = "overlay",
    preview = false,
    onEditorActionsChange = undefined,
    onEventLayerActiveChange = undefined
  }: {
    layoutType?: LayoutType;
    layoutId?: string | null;
    layoutOverride?: LayoutModel | null;
    layoutRevision?: number;
    packageRevision?: number;
    mode?: RendererMode;
    source?: LayoutSource;
    preview?: boolean;
    onEditorActionsChange?: (actions: VisualEditorActionHandle[]) => void;
    onEventLayerActiveChange?: (active: boolean) => void;
  } = $props();

  type VisualExportDescriptor = {
    name: string;
    entry: string;
    default_width: number;
    default_height: number;
    settings: string | null;
  };

  type PackageDescriptor = {
    id: string;
    name: string;
    enabled: boolean;
    effective_permissions?: {
      bus?: {
        read?: string[];
      };
      registry?: {
        read?: string[];
      };
    };
    exports: {
      visuals: VisualExportDescriptor[];
      configuration?: {
        visuals: VisualExportDescriptor[];
      } | null;
    };
  };

  type OverlayItem = {
    id: string;
    kind?: "visual" | "text" | "image" | "shape";
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
  };

  type PageBackground = {
    kind: "color" | "image";
    color: string;
    image?: string | null;
    fit: "cover" | "contain" | "stretch";
  };

  type LayoutModel = OverlayLayout & {
    background?: PageBackground;
  };

  type OverlayLayoutCatalog = {
    active_layout_id: string;
    stream_layout_id: string;
    layouts: OverlayLayout[];
  };

  type PagesFile = {
    pages: LayoutModel[];
  };

  type AppSettings = {
    obs: {
      host: string;
      port: number;
      access_token: string;
    };
  };

  type ComponentExportSource = {
    package_id: string;
    export_name: string;
    entry: string;
    source: string;
    props_schema: any | null;
  };

  type Diagnostics = {
    log(message: string, data?: unknown): void;
    warn(message: string, data?: unknown): void;
    error(message: string, data?: unknown): void;
  };

  type ComponentContext = {
    root: HTMLElement;
    providerPackageId: string;
    exportName: string;
    assets: {
      url(ref: string): string;
    };
    diagnostics: Diagnostics;
  };

  type ComponentHandle = {
    mount(root: HTMLElement, props?: Record<string, unknown>): Promise<void | (() => void)>;
  };

  type ComponentExport = {
    mount(context: ComponentContext, props: Record<string, unknown>): void | (() => void) | Promise<void | (() => void)>;
  };

  type ComponentModule = {
    default?: ComponentExport;
    mount?: ComponentExport["mount"];
  };

  type VisualContext = {
    root: HTMLElement;
    package: PackageDescriptor;
    exportName: string;
    item: OverlayItem;
    settings: Record<string, unknown>;
    mode: "runtime" | "editor";
    editor?: {
      emit(eventName: string, payload?: unknown): void;
    };
    setActive(active: boolean): void;
    bus: {
      subscribe(eventName: string, callback: (event: unknown) => void): () => void;
    };
    registry: {
      get(key: string): Promise<unknown>;
    };
    components: {
      load(ref: string): Promise<ComponentHandle>;
    };
    services: {
      call(ref: string, method: string, input?: unknown): Promise<unknown>;
    };
    assets: {
      url(ref: string): string;
    };
    diagnostics: Diagnostics;
  };

  type VisualEditorAction = {
    id: string;
    label: string;
    disabled?: boolean;
    run(context: VisualContext): void | Promise<void>;
  };

  type VisualExport = {
    mount(context: VisualContext): void | (() => void) | Promise<void | (() => void)>;
    update?(context: VisualContext): void | Promise<void>;
    unmount?(): void | Promise<void>;
    editor?: {
      mount?(context: VisualContext): void | (() => void) | Promise<void | (() => void)>;
      actions?(context: VisualContext): VisualEditorAction[] | Promise<VisualEditorAction[]>;
    };
  };

  type VisualModule = {
    default?: VisualExport;
    mount?: VisualExport["mount"];
    update?: VisualExport["update"];
    unmount?: VisualExport["unmount"];
    editor?: VisualExport["editor"];
  };

  type VisualEditorActionHandle = {
    itemId: string;
    actionId: string;
    label: string;
    disabled?: boolean;
    run(): Promise<void>;
  };

  type MountedItem = {
    cleanup: () => void;
    update?: (layer: OverlayLayer, item: OverlayItem, layout: LayoutModel) => void | Promise<void>;
  };

  let host: HTMLElement;
  let latestEvent: unknown = null;
  let packages = $state<PackageDescriptor[]>([]);
  let overlayLayouts = $state<OverlayLayoutCatalog | null>(null);
  let pages = $state<PagesFile | null>(null);
  let eventActiveItems = $state(new Set<string>());
  let mountedItems = new Map<string, MountedItem>();
  let telemetrySubscribers = new Set<(event: unknown) => void>();
  let componentSourceCache = new Map<string, ComponentExportSource>();
  let settingsCache = new Map<string, Record<string, unknown>>();
  let editorActionHandles = new Map<string, VisualEditorActionHandle[]>();
  let moduleVersion = 0;
  let observedPackageRevision: number | null = null;
  let previewScale = $state(1);

  const persistedActiveOverlay = $derived.by(() => selectedOverlayLayout(overlayLayouts));
  const persistedActivePage = $derived.by(() => selectedPageLayout(pages));
  const activeLayout = $derived.by((): LayoutModel | null => {
    return layoutOverride ?? (source === "page" ? persistedActivePage : persistedActiveOverlay);
  });
  const activeLayoutSyncKey = $derived.by(() => layoutSyncKey(activeLayout));

  const eventLayerActive = $derived(eventActiveItems.size > 0);
  const visualContextMode = $derived.by((): "runtime" | "editor" => (mode === "editor" ? "editor" : "runtime"));

  $effect(() => {
    onEventLayerActiveChange?.(eventLayerActive);
  });

  const hostStyle = $derived.by(() => {
    let style = "";
    if (preview && activeLayout) {
      style += `position:absolute;top:50%;left:50%;width:${activeLayout.width}px;height:${activeLayout.height}px;transform:translate(-50%, -50%) scale(${previewScale});transform-origin:center;`;
    }

    if (mode !== "page") return style;

    const background = activeLayout?.background;
    if (!background) return style + "background:var(--editor-bg-dark);";
    if (background.kind === "image" && background.image) {
      const size = background.fit === "stretch" ? "100% 100%" : background.fit;
      return style + `background-color:${background.color || "var(--editor-bg-dark)"};background-image:url("${cssUrl(background.image)}");background-size:${size};background-position:center;background-repeat:no-repeat;`;
    }
    return style + `background:${background.color || "var(--editor-bg-dark)"};`;
  });

  $effect(() => {
    if (preview && host && activeLayout) {
      const parent = host.parentElement;
      if (!parent) return;
      const observer = new ResizeObserver((entries) => {
        for (const entry of entries) {
          const width = entry.contentRect.width || entry.target.clientWidth;
          const height = entry.contentRect.height || entry.target.clientHeight;
          if (width > 0 && height > 0 && activeLayout.width > 0 && activeLayout.height > 0) {
            previewScale = Math.min(width / activeLayout.width, height / activeLayout.height);
          }
        }
      });
      observer.observe(parent);
      return () => observer.disconnect();
    }
  });

  $effect(() => {
    layoutRevision;
    activeLayoutSyncKey;
    visualContextMode;
    if (visualContextMode === "editor") latestEvent = null;
    if (activeLayout) {
      void syncMountedItems(activeLayout);
    }
  });

  $effect(() => {
    if (observedPackageRevision === null) {
      observedPackageRevision = packageRevision;
      return;
    }
    if (packageRevision === observedPackageRevision) return;
    observedPackageRevision = packageRevision;
    invalidateMountedModules();
  });

  $effect(() => {
    applyLayerVisibility();
  });

  function layoutLayers(layout: LayoutModel) {
    const layers = layout.layers?.length
      ? layout.layers
      : [
          {
            id: "legacy-main",
            name: "Main",
            kind: "normal" as const,
            visible: true,
            locked: false,
            order: 0,
            items: layout.items ?? []
          }
        ];
    return [...layers].sort((a, b) => {
      if (a.kind === "event" && b.kind !== "event") return 1;
      if (a.kind !== "event" && b.kind === "event") return -1;
      return a.order - b.order;
    });
  }

  function layoutItems(layout: LayoutModel) {
    return layoutLayers(layout).flatMap((layer) => layer.items.map((item) => ({ layer, item })));
  }

  function selectedOverlayLayout(catalog: OverlayLayoutCatalog | null) {
    if (!catalog) return null;
    const selectedLayoutId = layoutId ?? (layoutType === "ingame" ? catalog.active_layout_id : catalog.stream_layout_id);
    return catalog.layouts.find((layout) => layout.id === selectedLayoutId) ?? null;
  }

  function selectedPageLayout(file: PagesFile | null) {
    if (!file || !layoutId) return null;
    return file.pages.find((page) => page.id === layoutId) ?? null;
  }

  function layoutSyncKey(layout: LayoutModel | null) {
    if (!layout) return "";
    return JSON.stringify({
      id: layout.id,
      width: layout.width,
      height: layout.height,
      background: layout.background ?? null,
      layers: layoutLayers(layout).map((layer) => ({
        id: layer.id,
        kind: layer.kind,
        visible: layer.visible,
        locked: layer.locked,
        order: layer.order,
        items: layer.items.map((item) => ({
          id: item.id,
          kind: itemKind(item),
          package_id: item.package_id ?? null,
          export_name: item.export_name ?? null,
          name: item.name,
          x: item.x,
          y: item.y,
          width: item.width,
          height: item.height,
          z_index: item.z_index,
          visible: item.visible,
          locked: item.locked,
          opacity: item.opacity,
          settings: settingsObject(item)
        }))
      }))
    });
  }

  function subscribe(callback: (event: unknown) => void) {
    telemetrySubscribers.add(callback);
    if (latestEvent) notifyTelemetrySubscriber(callback, latestEvent);
    return () => telemetrySubscribers.delete(callback);
  }

  function notifyTelemetrySubscriber(callback: (event: unknown) => void, event: unknown) {
    try {
      callback(event);
    } catch (error) {
      console.warn("Plugin telemetry subscriber failed.", error);
    }
  }

  function publishTelemetry(event: unknown) {
    latestEvent = event;
    for (const callback of telemetrySubscribers) notifyTelemetrySubscriber(callback, event);
  }

  function publishEditorTelemetry(eventName: string, payload: unknown) {
    if (visualContextMode !== "editor") return;
    const name = String(eventName ?? "").trim();
    if (!name) return;
    publishTelemetry({ Event: name, Data: payload ?? null });
  }

  function notifyEditorActionsChanged() {
    onEditorActionsChange?.([...editorActionHandles.values()].flat());
  }

  function setEditorActions(itemId: string, actions: VisualEditorActionHandle[]) {
    if (actions.length) {
      editorActionHandles.set(itemId, actions);
    } else {
      editorActionHandles.delete(itemId);
    }
    notifyEditorActionsChanged();
  }

  function clearEditorActions(itemId: string) {
    if (!editorActionHandles.delete(itemId)) return;
    notifyEditorActionsChanged();
  }

  function matchesPattern(patterns: string[] | undefined, value: string) {
    return Boolean(
      patterns?.some((pattern) => {
        if (pattern === "*" || pattern === value) return true;
        return pattern.endsWith(".*") && value.startsWith(pattern.slice(0, -1));
      })
    );
  }

  function eventNameFor(event: any) {
    return String(event?.Event ?? event?.event ?? "");
  }

  function itemVisible(layer: OverlayLayer, item: OverlayItem) {
    if (item.visible === false || layer.visible === false) return false;
    if (mode !== "page" && eventLayerActive && layer.kind !== "event") return false;
    return true;
  }

  function applyItemStyle(root: HTMLElement, layer: OverlayLayer, item: OverlayItem, layout: LayoutModel) {
    root.style.position = "absolute";
    root.style.left = `${(item.x / layout.width) * 100}%`;
    root.style.top = `${(item.y / layout.height) * 100}%`;
    root.style.width = `${(item.width / layout.width) * 100}%`;
    root.style.height = `${(item.height / layout.height) * 100}%`;
    root.style.zIndex = String(layer.kind === "event" ? 100_000 + item.z_index : item.z_index);
    root.style.opacity = String(item.opacity ?? 1);
    root.style.display = itemVisible(layer, item) ? "block" : "none";
    root.style.overflow = "hidden";
    root.style.pointerEvents = mode === "editor" ? "none" : "auto";
    root.style.setProperty("--bakingrl-item-width", `${item.width}px`);
    root.style.setProperty("--bakingrl-item-height", `${item.height}px`);
    root.style.setProperty("--bakingrl-layout-width", `${layout.width}px`);
    root.style.setProperty("--bakingrl-layout-height", `${layout.height}px`);
  }

  function itemRoot(itemId: string) {
    if (!host) return null;
    for (const element of host.querySelectorAll<HTMLElement>("[data-item-id]")) {
      if (element.dataset.itemId === itemId) return element;
    }
    return null;
  }

  function applyLayerVisibility() {
    if (!activeLayout || !host) return;
    for (const { layer, item } of layoutItems(activeLayout)) {
      const root = itemRoot(item.id);
      if (root) applyItemStyle(root, layer, item, activeLayout);
    }
  }

  function packageForItem(item: OverlayItem) {
    const packageId = item.package_id;
    if (!packageId) return null;
    return packages.find((pkg) => pkg.id === packageId && pkg.enabled) ?? null;
  }

  function visualForItem(item: OverlayItem) {
    const exportName = item.export_name;
    if (!exportName || itemKind(item) !== "visual") return null;
    const pkg = packageForItem(item);
    if (!pkg) return null;
    const visuals = source === "configuration" ? (pkg.exports.configuration?.visuals ?? []) : pkg.exports.visuals;
    return visuals.find((visual) => visual.name === exportName) ?? null;
  }

  function itemKind(item: OverlayItem) {
    return item.kind ?? "visual";
  }

  function cssUrl(value: string) {
    return value.replace(/"/g, "%22");
  }

  function moduleUrl(packageId: string, entry: string) {
    return adapter.packageModuleUrl(packageId, entry, `${packageRevision}.${moduleVersion}`);
  }

  function packageAssetUrl(packageId: string, ref: string) {
    const value = String(ref ?? "");
    if (/^(https?:|data:|blob:|\/)/.test(value)) return value;
    return adapter.packageFileUrl(packageId, value);
  }

  function settingsObject(item: OverlayItem) {
    return item.settings && typeof item.settings === "object" && !Array.isArray(item.settings) ? item.settings : {};
  }

  function validateProps(schema: any | null, props: Record<string, unknown>) {
    if (!schema || typeof schema !== "object") return;
    const required = Array.isArray(schema.required) ? schema.required : [];
    for (const key of required) {
      if (!(key in props)) throw new Error(`Missing required component prop '${key}'.`);
    }
    const properties = schema.properties && typeof schema.properties === "object" ? schema.properties : {};
    for (const [key, value] of Object.entries(props)) {
      const expected = properties[key]?.type;
      if (!expected || value == null) continue;
      if (expected === "number" && typeof value !== "number") throw new Error(`Component prop '${key}' must be a number.`);
      if (expected === "string" && typeof value !== "string") throw new Error(`Component prop '${key}' must be a string.`);
      if (expected === "boolean" && typeof value !== "boolean") throw new Error(`Component prop '${key}' must be a boolean.`);
    }
  }

  function visualMountSignature(item: OverlayItem) {
    return JSON.stringify({
      package_id: item.package_id ?? null,
      export_name: item.export_name ?? null,
      settings: settingsObject(item)
    });
  }

  function mountedContextMode() {
    return visualContextMode;
  }

  async function getPackageSettings(packageId: string) {
    const cached = settingsCache.get(packageId);
    if (cached) return cached;
    const settings = await adapter.invoke<Record<string, unknown>>("get_package_settings", { packageId });
    settingsCache.set(packageId, settings);
    return settings;
  }

  async function getComponentSource(callerPackageId: string, componentRef: string) {
    const key = `${callerPackageId}->${componentRef}`;
    const cached = componentSourceCache.get(key);
    if (cached) return cached;
    const source = await adapter.invoke<ComponentExportSource>("read_component_export_source", {
      callerPackageId,
      componentRef
    });
    componentSourceCache.set(key, source);
    return source;
  }

  function invalidateMountedModules() {
    moduleVersion += 1;
    componentSourceCache.clear();
    settingsCache.clear();
    for (const mounted of mountedItems.values()) mounted.cleanup();
    mountedItems.clear();
    if (activeLayout) void syncMountedItems(activeLayout);
  }

  function renderNativeItem(root: HTMLElement, item: OverlayItem) {
    const settings = settingsObject(item);
    const kind = itemKind(item);
    root.classList.add("native-page-block");
    root.replaceChildren();

    if (kind === "text") {
      const text = document.createElement("div");
      text.className = "native-page-text";
      text.textContent = String(settings.text ?? item.name ?? "Text");
      text.style.width = "100%";
      text.style.height = "100%";
      text.style.display = "flex";
      text.style.alignItems = String(settings.verticalAlign ?? "center");
      text.style.justifyContent = String(settings.align ?? "center");
      text.style.color = String(settings.color ?? "var(--text-primary)");
      text.style.fontSize = `${Number(settings.fontSize ?? 24)}px`;
      text.style.fontWeight = String(settings.fontWeight ?? 700);
      text.style.textAlign = String(settings.textAlign ?? "center") as CanvasTextAlign;
      text.style.whiteSpace = "pre-wrap";
      text.style.overflow = "hidden";
      root.appendChild(text);
      return;
    }

    if (kind === "image") {
      const src = typeof settings.src === "string" ? settings.src : "";
      if (src) {
        const image = document.createElement("img");
        image.src = src;
        image.alt = String(settings.alt ?? item.name ?? "");
        image.style.width = "100%";
        image.style.height = "100%";
        image.style.objectFit = String(settings.fit ?? "cover") as "cover";
        image.style.display = "block";
        root.appendChild(image);
      }
      return;
    }

    const shape = document.createElement("div");
    shape.className = "native-page-shape";
    shape.style.width = "100%";
    shape.style.height = "100%";
    shape.style.background = String(settings.fill ?? "color-mix(in srgb, var(--accent) 18%, transparent)");
    shape.style.border = `${Number(settings.borderWidth ?? 1)}px solid ${String(settings.borderColor ?? "color-mix(in srgb, var(--text-primary) 22%, transparent)")}`;
    shape.style.borderRadius = `${Number(settings.borderRadius ?? 8)}px`;
    root.appendChild(shape);
  }

  function mountNativeItem(layer: OverlayLayer, item: OverlayItem, layout: LayoutModel) {
    if (!host || mountedItems.has(item.id)) return;
    const root = document.createElement("div");
    root.className = "visual-export native-export";
    root.dataset.itemId = item.id;
    root.dataset.layerId = layer.id;
    root.dataset.layerKind = layer.kind;
    root.dataset.nativeKind = itemKind(item);
    applyItemStyle(root, layer, item, layout);
    renderNativeItem(root, item);
    host.appendChild(root);
    mountedItems.set(item.id, { cleanup: () => root.remove() });
  }

  async function mountItem(layer: OverlayLayer, item: OverlayItem, layout: LayoutModel) {
    if (!host || mountedItems.has(item.id)) return;

    if (itemKind(item) !== "visual") {
      mountNativeItem(layer, item, layout);
      return;
    }

    const pkg = packageForItem(item);
    const visual = visualForItem(item);
    if (!pkg || !visual || !item.package_id || !item.export_name) return;
    const mountedPackage = pkg;
    const mountedVisual = visual;

    const root = document.createElement("div");
    root.className = "visual-export";
    root.dataset.itemId = item.id;
    root.dataset.packageId = item.package_id;
    root.dataset.exportName = item.export_name;
    root.dataset.layerId = layer.id;
    root.dataset.layerKind = layer.kind;
    root.dataset.mountSignature = visualMountSignature(item);
    root.dataset.contextMode = mountedContextMode();
    applyItemStyle(root, layer, item, layout);
    host.appendChild(root);

    let disposed = false;
    let visualCleanup: void | (() => void);
    let editorCleanup: void | (() => void);
    let visualModule: VisualExport | undefined;
    const busCleanups = new Set<() => void>();
    const cleanupMountedVisual = () => {
      disposed = true;
      clearEditorActions(item.id);
      if (typeof editorCleanup === "function") editorCleanup();
      if (typeof visualCleanup === "function") visualCleanup();
      void visualModule?.unmount?.();
      for (const cleanup of busCleanups) cleanup();
      busCleanups.clear();
      const nextActiveItems = new Set(eventActiveItems);
      nextActiveItems.delete(item.id);
      eventActiveItems = nextActiveItems;
      root.remove();
    };
    mountedItems.set(item.id, { cleanup: cleanupMountedVisual });

    try {
      const packageSettings = await getPackageSettings(item.package_id);
      const settings = { ...packageSettings, ...settingsObject(item) };
      const visualMod = (await import(/* @vite-ignore */ moduleUrl(mountedPackage.id, mountedVisual.entry))) as VisualModule;
      visualModule = visualMod.default ?? (visualMod.mount ? {
        mount: visualMod.mount,
        update: visualMod.update,
        unmount: visualMod.unmount,
        editor: visualMod.editor
      } : undefined);
      const loadedVisual = visualModule;
      const mountVisual = loadedVisual?.mount;
      if (!loadedVisual || !mountVisual) {
        throw new Error(`Visual export '${mountedPackage.id}/${item.export_name}' does not export mount().`);
      }

      const diagnostics: Diagnostics = {
        log(message, data) {
          data === undefined ? console.log(message) : console.log(message, data);
        },
        warn(message, data) {
          data === undefined ? console.warn(message) : console.warn(message, data);
        },
        error(message, data) {
          data === undefined ? console.error(message) : console.error(message, data);
        }
      };

      const createContext = (nextLayer: OverlayLayer, nextItem: OverlayItem, nextSettings: Record<string, unknown>): VisualContext => ({
        root,
        package: mountedPackage,
        exportName: nextItem.export_name ?? mountedVisual.name,
        item: nextItem,
        settings: nextSettings,
        mode: mountedContextMode(),
        editor: mountedContextMode() === "editor"
          ? {
              emit(eventName, payload) {
                publishEditorTelemetry(eventName, payload);
              }
            }
          : undefined,
        setActive(active) {
          if (nextLayer.kind !== "event") return;
          const nextActiveItems = new Set(eventActiveItems);
          if (active) {
            nextActiveItems.add(nextItem.id);
          } else {
            nextActiveItems.delete(nextItem.id);
          }
          eventActiveItems = nextActiveItems;
        },
        bus: {
          subscribe(eventName, callback) {
            const requestedEventName = String(eventName ?? "");
            const allowedReads = mountedPackage.effective_permissions?.bus?.read;
            if (!matchesPattern(allowedReads, requestedEventName)) {
              throw new Error(`Package '${mountedPackage.id}' cannot subscribe to '${requestedEventName}'.`);
            }
            const cleanup = subscribe((event: any) => {
              const actualEventName = eventNameFor(event);
              if (
                matchesPattern(allowedReads, actualEventName) &&
                (requestedEventName === "*" || actualEventName === requestedEventName)
              ) {
                callback(event);
              }
            });
            busCleanups.add(cleanup);
            return () => {
              cleanup();
              busCleanups.delete(cleanup);
            };
          }
        },
        registry: {
          get(key) {
            return adapter.invoke("plugin_registry_get", { packageId: mountedPackage.id, key: String(key) });
          }
        },
        components: {
          async load(ref) {
            const componentRef = String(ref ?? "");
            const component = await getComponentSource(mountedPackage.id, componentRef);
            const componentMod = (await import(/* @vite-ignore */ moduleUrl(component.package_id, component.entry))) as ComponentModule;
            const componentModule = componentMod.default ?? componentMod;
            const mountComponent = componentModule.mount;
            if (!mountComponent) {
              throw new Error(`Component export '${componentRef}' does not export mount().`);
            }
            return {
              async mount(componentRoot, props = {}) {
                validateProps(component.props_schema, props);
                return mountComponent(
                  {
                    root: componentRoot,
                    providerPackageId: component.package_id,
                    exportName: component.export_name,
                    assets: {
                      url(assetRef) {
                        return packageAssetUrl(component.package_id, assetRef);
                      }
                    },
                    diagnostics
                  },
                  props
                );
              }
            };
          }
        },
        services: {
          call(ref, method, input) {
            return adapter.invoke("call_service_export", {
              callerPackageId: mountedPackage.id,
              serviceRef: String(ref ?? ""),
              method: String(method ?? ""),
              input: input ?? null
            });
          }
        },
        assets: {
          url(ref) {
            return packageAssetUrl(mountedPackage.id, ref);
          }
        },
        diagnostics
      });

      const mountContext = createContext(layer, item, settings);
      const mountResult = await mountVisual(mountContext);

      if (disposed) {
        if (typeof mountResult === "function") mountResult();
        return;
      }
      visualCleanup = mountResult;
      if (mountedContextMode() === "editor") {
        const editorMountResult = await loadedVisual.editor?.mount?.(mountContext);
        if (disposed) {
          if (typeof editorMountResult === "function") editorMountResult();
          return;
        }
        editorCleanup = editorMountResult;
        const actions = await loadedVisual.editor?.actions?.(mountContext);
        if (disposed) return;
        setEditorActions(
          item.id,
          (actions ?? []).map((action) => ({
            itemId: item.id,
            actionId: String(action.id),
            label: String(action.label),
            disabled: action.disabled,
            async run() {
              await action.run(mountContext);
            }
          }))
        );
      } else {
        clearEditorActions(item.id);
      }
      mountedItems.set(item.id, {
        cleanup: cleanupMountedVisual,
        async update(nextLayer, nextItem, nextLayout) {
          if (!loadedVisual.update) return;
          const nextPackageSettings = await getPackageSettings(mountedPackage.id);
          const nextSettings = { ...nextPackageSettings, ...settingsObject(nextItem) };
          const nextContext = createContext(nextLayer, nextItem, nextSettings);
          root.dataset.mountSignature = visualMountSignature(nextItem);
          applyItemStyle(root, nextLayer, nextItem, nextLayout);
          await loadedVisual.update(nextContext);
          if (mountedContextMode() === "editor") {
            const actions = await loadedVisual.editor?.actions?.(nextContext);
            setEditorActions(
              nextItem.id,
              (actions ?? []).map((action) => ({
                itemId: nextItem.id,
                actionId: String(action.id),
                label: String(action.label),
                disabled: action.disabled,
                async run() {
                  await action.run(nextContext);
                }
              }))
            );
          }
        }
      });
    } catch (error) {
      if (disposed) return;
      const message = error instanceof Error ? error.message : String(error);
      root.classList.add("visual-export-error");
      root.textContent = message;
    }
  }

  async function syncMountedItems(layout: LayoutModel) {
    const itemEntries = layoutItems(layout);
    const activeIds = new Set(
      itemEntries
        .filter(({ item }) => itemKind(item) !== "visual" || Boolean(visualForItem(item)))
        .map(({ item }) => item.id)
    );

    for (const [itemId, cleanup] of mountedItems) {
      if (!activeIds.has(itemId)) {
        cleanup.cleanup();
        mountedItems.delete(itemId);
      }
    }

    for (const { layer, item } of itemEntries) {
      const root = itemRoot(item.id);
      if (root) {
        if (itemKind(item) === "visual" && root.dataset.contextMode !== mountedContextMode()) {
          const mounted = mountedItems.get(item.id);
          mounted?.cleanup();
          mountedItems.delete(item.id);
          await mountItem(layer, item, layout);
          continue;
        }
        if (itemKind(item) === "visual" && root.dataset.mountSignature !== visualMountSignature(item)) {
          const mounted = mountedItems.get(item.id);
          if (mounted?.update) {
            await mounted.update(layer, item, layout);
            continue;
          }
          mounted?.cleanup();
          mountedItems.delete(item.id);
          await mountItem(layer, item, layout);
          continue;
        }
        applyItemStyle(root, layer, item, layout);
        if (itemKind(item) !== "visual") renderNativeItem(root, item);
      } else {
        await mountItem(layer, item, layout);
      }
    }
  }

  async function refreshState() {
    if (adapter.isTauri) {
      const settings = await adapter.invoke<AppSettings>("get_app_settings");
      adapter.configureGateway(settings.obs.host, settings.obs.port, settings.obs.access_token);
    }
    packages = await adapter.invoke<PackageDescriptor[]>("list_packages");
    let nextLayout: LayoutModel | null = layoutOverride;
    if (source === "page") {
      const nextPages = await adapter.invoke<PagesFile>("get_pages");
      pages = nextPages;
      nextLayout = nextLayout ?? selectedPageLayout(nextPages);
    } else {
      const nextCatalog = await adapter.invoke<OverlayLayoutCatalog>("get_overlay_layouts");
      overlayLayouts = nextCatalog;
      nextLayout = nextLayout ?? (source === "overlay" ? selectedOverlayLayout(nextCatalog) : activeLayout);
    }
    if (nextLayout) await syncMountedItems(nextLayout);
  }

  onMount(() => {
    void refreshState();

    let unlistenTelemetry: (() => void) | undefined;
    let unlistenPackages: (() => void) | undefined;
    let unlistenOverlays: (() => void) | undefined;
    let unlistenPages: (() => void) | undefined;
    let unlistenSettings: (() => void) | undefined;

    void adapter.listen("bakingrl-telemetry", (event) => {
      if (visualContextMode !== "editor") publishTelemetry(event);
    }).then((unlisten) => {
      unlistenTelemetry = unlisten;
    });
    void adapter.listen<PackageDescriptor[]>("bakingrl-packages-changed", (event) => {
      packages = event;
      invalidateMountedModules();
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    void adapter.listen<OverlayLayoutCatalog>("bakingrl-overlay-layouts-changed", (event) => {
      overlayLayouts = event;
      if (source === "overlay" && !layoutOverride) {
        const nextLayout = selectedOverlayLayout(event);
        if (nextLayout) void syncMountedItems(nextLayout);
      }
    }).then((unlisten) => {
      unlistenOverlays = unlisten;
    });
    void adapter.listen<PagesFile>("bakingrl-pages-changed", (event) => {
      pages = event;
      if (source === "page" && !layoutOverride) {
        const nextLayout = selectedPageLayout(event);
        if (nextLayout) void syncMountedItems(nextLayout);
      }
    }).then((unlisten) => {
      unlistenPages = unlisten;
    });
    void adapter.listen<string>("bakingrl-package-settings-changed", (packageId) => {
      settingsCache.delete(packageId);
      for (const [itemId, cleanup] of mountedItems) {
        const root = itemRoot(itemId);
        if (root?.dataset.packageId === packageId) {
          cleanup.cleanup();
          mountedItems.delete(itemId);
        }
      }
      if (activeLayout) void syncMountedItems(activeLayout);
    }).then((unlisten) => {
      unlistenSettings = unlisten;
    });

    const pollInterval = !adapter.isTauri ? setInterval(() => void refreshState(), 2000) : undefined;

    return () => {
      unlistenTelemetry?.();
      unlistenPackages?.();
      unlistenOverlays?.();
      unlistenPages?.();
      unlistenSettings?.();
      if (pollInterval) clearInterval(pollInterval);
      for (const mounted of mountedItems.values()) mounted.cleanup();
      mountedItems.clear();
      editorActionHandles.clear();
      onEditorActionsChange?.([]);
      telemetrySubscribers.clear();
      componentSourceCache.clear();
      settingsCache.clear();
    };
  });
</script>

<main
  class="overlay-renderer-host"
  class:editor={mode === "editor"}
  class:page={mode === "page"}
  class:embedded={layoutOverride !== null}
  style={hostStyle}
  bind:this={host}
  aria-label="Visual export host"
></main>

<style>
  .overlay-renderer-host.embedded {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 0;
    width: 100%;
    height: 100%;
  }
</style>
