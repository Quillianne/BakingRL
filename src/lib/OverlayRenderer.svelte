<script lang="ts">
  import { onMount } from "svelte";
  import { adapter } from "$lib/adapter/index";

  type RendererMode = "runtime" | "editor" | "page";
  type LayoutType = "ingame" | "stream";
  type LayoutSource = "overlay" | "page";
  type MockEvent = { id: number; event: unknown } | null;

  const {
    layoutType = "ingame",
    layoutId = null,
    layoutOverride = null,
    layoutRevision = 0,
    mode = "runtime",
    source = "overlay",
    mockEvent = null,
    preview = false
  }: {
    layoutType?: LayoutType;
    layoutId?: string | null;
    layoutOverride?: LayoutModel | null;
    layoutRevision?: number;
    mode?: RendererMode;
    source?: LayoutSource;
    mockEvent?: MockEvent;
    preview?: boolean;
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
    };
  };

  type VisualModule = {
    default?: VisualExport;
    mount?: VisualExport["mount"];
  };

  type ComponentModule = {
    default?: ComponentExport;
    mount?: ComponentExport["mount"];
  };

  type VisualExport = {
    mount(context: VisualContext): void | (() => void) | Promise<void | (() => void)>;
  };

  type ComponentExport = {
    mount(
      context: ComponentContext,
      props: Record<string, unknown>
    ): void | (() => void) | Promise<void | (() => void)>;
  };

  type ComponentContext = {
    root: HTMLElement;
    providerPackageId: string;
    exportName: string;
    assets: {
      url(ref: string): string;
    };
    diagnostics: VisualContext["diagnostics"];
  };

  type ComponentHandle = {
    mount(root: HTMLElement, props?: Record<string, unknown>): Promise<void | (() => void)>;
  };

  type ComponentExportSource = {
    package_id: string;
    export_name: string;
    entry: string;
    source: string;
    props_schema: any | null;
  };

  type VisualContext = {
    root: HTMLElement;
    package: PackageDescriptor;
    exportName: string;
    item: OverlayItem;
    settings: Record<string, unknown>;
    setActive(active: boolean): void;
    bus: {
      subscribe(eventName: string, callback: (event: any) => void): () => void;
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
    diagnostics: {
      log(message: string, data?: unknown): void;
      warn(message: string, data?: unknown): void;
      error(message: string, data?: unknown): void;
    };
  };

  let host: HTMLElement;
  let latestEvent: unknown = null;
  let lastMockEventId = 0;
  let packages = $state<PackageDescriptor[]>([]);
  let overlayLayouts = $state<OverlayLayoutCatalog | null>(null);
  let pages = $state<PagesFile | null>(null);
  let eventActiveItems = $state(new Set<string>());
  let mountedItems = new Map<string, () => void>();
  let telemetrySubscribers = new Set<(event: unknown) => void>();
  let componentSourceCache = new Map<string, ComponentExportSource>();
  let settingsCache = new Map<string, Record<string, unknown>>();
  let moduleVersion = 0;
  let previewScale = $state(1);

  const persistedActiveOverlay = $derived.by(() => {
    const selectedLayoutId = layoutId ?? (layoutType === "ingame" ? overlayLayouts?.active_layout_id : overlayLayouts?.stream_layout_id);
    return overlayLayouts?.layouts.find((layout) => layout.id === selectedLayoutId) ?? null;
  });
  const persistedActivePage = $derived.by(() => {
    if (!layoutId) return null;
    return pages?.pages.find((page) => page.id === layoutId) ?? null;
  });
  const activeLayout = $derived.by((): LayoutModel | null => {
    return layoutOverride ?? (source === "page" ? persistedActivePage : persistedActiveOverlay);
  });

  const eventLayerActive = $derived(eventActiveItems.size > 0);
  const hostStyle = $derived.by(() => {
    let style = "";
    if (preview && activeLayout) {
      style += `position:absolute;top:50%;left:50%;width:${activeLayout.width}px;height:${activeLayout.height}px;transform:translate(-50%, -50%) scale(${previewScale});transform-origin:center;`;
    }

    if (mode !== "page") return style;

    const background = activeLayout?.background;
    if (!background) return style + "background:#0f172a;";
    if (background.kind === "image" && background.image) {
      const size = background.fit === "stretch" ? "100% 100%" : background.fit;
      return style + `background-color:${background.color || "#0f172a"};background-image:url("${cssUrl(background.image)}");background-size:${size};background-position:center;background-repeat:no-repeat;`;
    }
    return style + `background:${background.color || "#0f172a"};`;
  });

  $effect(() => {
    if (preview && host && activeLayout) {
      const parent = host.parentElement;
      if (!parent) return;
      const observer = new ResizeObserver((entries) => {
        for (const entry of entries) {
          const width = entry.contentRect.width || entry.target.clientWidth;
          if (width > 0 && activeLayout.width > 0) {
            previewScale = width / activeLayout.width;
          }
        }
      });
      observer.observe(parent);
      return () => observer.disconnect();
    }
  });

  $effect(() => {
    layoutRevision;
    if (activeLayout) {
      void syncMountedItems(activeLayout);
    }
  });

  $effect(() => {
    if (mockEvent && mockEvent.id !== lastMockEventId) {
      lastMockEventId = mockEvent.id;
      publishTelemetry(mockEvent.event);
    }
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

  function subscribe(callback: (event: unknown) => void) {
    telemetrySubscribers.add(callback);
    if (latestEvent) callback(latestEvent);
    return () => telemetrySubscribers.delete(callback);
  }

  function publishTelemetry(event: unknown) {
    latestEvent = event;
    for (const callback of telemetrySubscribers) callback(event);
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
    if (mode === "runtime" && eventLayerActive && layer.kind !== "event") return false;
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
    root.style.overflow = "visible";
    root.style.pointerEvents = mode === "editor" ? "none" : "auto";
  }

  function applyLayerVisibility() {
    if (!activeLayout || !host) return;
    for (const { layer, item } of layoutItems(activeLayout)) {
      const root = host.querySelector<HTMLElement>(`[data-item-id="${item.id}"]`);
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
    return packageForItem(item)?.exports.visuals.find((visual) => visual.name === exportName) ?? null;
  }

  function itemKind(item: OverlayItem) {
    return item.kind ?? "visual";
  }

  function cssUrl(value: string) {
    return value.replace(/"/g, "%22");
  }

  function settingsObject(item: OverlayItem) {
    return item.settings && typeof item.settings === "object" && !Array.isArray(item.settings) ? item.settings : {};
  }

  function visualMountSignature(item: OverlayItem) {
    return JSON.stringify({
      package_id: item.package_id ?? null,
      export_name: item.export_name ?? null,
      settings: settingsObject(item)
    });
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

  function packageAssetUrl(packageId: string, ref: string) {
    if (/^(https?:|data:|blob:|\/)/.test(ref)) return ref;
    return adapter.packageFileUrl(packageId, ref);
  }

  function moduleUrl(packageId: string, entry: string) {
    return `${adapter.packageFileUrl(packageId, entry)}?v=${moduleVersion}`;
  }

  function validateProps(schema: any, props: Record<string, unknown>) {
    if (!schema || typeof schema !== "object") return;
    const required = Array.isArray(schema.required) ? schema.required : [];
    for (const key of required) {
      if (!(key in props)) throw new Error(`Missing required component prop '${key}'.`);
    }
    const properties = schema.properties && typeof schema.properties === "object" ? schema.properties : {};
    for (const [key, value] of Object.entries(props)) {
      const expected = properties[key]?.type;
      if (!expected || value == null) continue;
      if (expected === "number" && typeof value !== "number") {
        throw new Error(`Component prop '${key}' must be a number.`);
      }
      if (expected === "string" && typeof value !== "string") {
        throw new Error(`Component prop '${key}' must be a string.`);
      }
      if (expected === "boolean" && typeof value !== "boolean") {
        throw new Error(`Component prop '${key}' must be a boolean.`);
      }
    }
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
      text.style.color = String(settings.color ?? "#f8fafc");
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
    shape.style.background = String(settings.fill ?? "rgba(59, 130, 246, 0.18)");
    shape.style.border = `${Number(settings.borderWidth ?? 1)}px solid ${String(settings.borderColor ?? "rgba(255, 255, 255, 0.2)")}`;
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
    mountedItems.set(item.id, () => root.remove());
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

    const root = document.createElement("div");
    root.className = "visual-export";
    root.dataset.itemId = item.id;
    root.dataset.packageId = item.package_id;
    root.dataset.exportName = item.export_name;
    root.dataset.layerId = layer.id;
    root.dataset.layerKind = layer.kind;
    root.dataset.mountSignature = visualMountSignature(item);
    applyItemStyle(root, layer, item, layout);
    host.appendChild(root);

    try {
      const packageSettings = await getPackageSettings(item.package_id);
      const settings = { ...packageSettings, ...(item.settings ?? {}) };
      const mod = (await import(/* @vite-ignore */ moduleUrl(item.package_id, visual.entry))) as VisualModule;
      const visualModule = mod.default ?? mod;
      if (!visualModule.mount) {
        throw new Error(`Visual export '${item.package_id}/${item.export_name}' does not export mount().`);
      }
      const cleanup = await visualModule.mount({
        root,
        package: pkg,
        exportName: item.export_name,
        item,
        settings,
        setActive(active: boolean) {
          if (layer.kind !== "event") return;
          const nextActiveItems = new Set(eventActiveItems);
          if (active) {
            nextActiveItems.add(item.id);
          } else {
            nextActiveItems.delete(item.id);
          }
          eventActiveItems = nextActiveItems;
        },
        bus: {
          subscribe(eventName, callback) {
            const allowedReads = pkg.effective_permissions?.bus?.read;
            if (!matchesPattern(allowedReads, eventName)) {
              throw new Error(`Package '${pkg.id}' cannot subscribe to '${eventName}'.`);
            }
            return subscribe((event: any) => {
              const actualEventName = eventNameFor(event);
              if (
                matchesPattern(allowedReads, actualEventName) &&
                (eventName === "*" || actualEventName === eventName)
              ) {
                callback(event);
              }
            });
          }
        },
        registry: {
          get(key: string) {
            return adapter.invoke("plugin_registry_get", { packageId: pkg.id, key });
          }
        },
        components: {
          async load(ref: string) {
            const component = await getComponentSource(pkg.id, ref);
            const mod = (await import(
              /* @vite-ignore */ moduleUrl(component.package_id, component.entry)
            )) as ComponentModule;
            const componentModule = mod.default ?? mod;
            if (!componentModule.mount) {
              throw new Error(`Component export '${ref}' does not export mount().`);
            }
            const mountComponent = componentModule.mount;
            return {
              async mount(root: HTMLElement, props: Record<string, unknown> = {}) {
                validateProps(component.props_schema, props);
                return mountComponent(
                  {
                    root,
                    providerPackageId: component.package_id,
                    exportName: component.export_name,
                    assets: {
                      url(assetRef: string) {
                        return packageAssetUrl(component.package_id, assetRef);
                      }
                    },
                    diagnostics: console
                  },
                  props
                );
              }
            };
          }
        },
        services: {
          async call(ref: string, method: string, input?: unknown) {
            return adapter.invoke("call_service_export", {
              callerPackageId: pkg.id,
              serviceRef: ref,
              method,
              input: input ?? null
            });
          }
        },
        assets: {
          url(ref: string) {
            return packageAssetUrl(pkg.id, ref);
          }
        },
        diagnostics: console
      });
      mountedItems.set(item.id, () => {
        if (cleanup) cleanup();
        const nextActiveItems = new Set(eventActiveItems);
        nextActiveItems.delete(item.id);
        eventActiveItems = nextActiveItems;
        root.remove();
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      root.classList.add("visual-export-error");
      root.textContent = message;
      mountedItems.set(item.id, () => root.remove());
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
        cleanup();
        mountedItems.delete(itemId);
      }
    }

    for (const { layer, item } of itemEntries) {
      const root = host?.querySelector<HTMLElement>(`[data-item-id="${item.id}"]`);
      if (root) {
        if (itemKind(item) === "visual" && root.dataset.mountSignature !== visualMountSignature(item)) {
          mountedItems.get(item.id)?.();
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
      adapter.configureGateway(settings.obs.host, settings.obs.port);
    }
    packages = await adapter.invoke<PackageDescriptor[]>("list_packages");
    if (source === "page") {
      pages = await adapter.invoke<PagesFile>("get_pages");
    } else {
      overlayLayouts = await adapter.invoke<OverlayLayoutCatalog>("get_overlay_layouts");
    }
    if (activeLayout) await syncMountedItems(activeLayout);
  }

  onMount(() => {
    void refreshState();

    let unlistenTelemetry: (() => void) | undefined;
    let unlistenPackages: (() => void) | undefined;
    let unlistenOverlays: (() => void) | undefined;
    let unlistenPages: (() => void) | undefined;
    let unlistenSettings: (() => void) | undefined;

    void adapter.listen("bakingrl-telemetry", publishTelemetry).then((unlisten) => {
      unlistenTelemetry = unlisten;
    });
    void adapter.listen<PackageDescriptor[]>("bakingrl-packages-changed", (event) => {
      packages = event;
      moduleVersion += 1;
      componentSourceCache.clear();
      for (const cleanup of mountedItems.values()) cleanup();
      mountedItems.clear();
      if (activeLayout) void syncMountedItems(activeLayout);
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    void adapter.listen<OverlayLayoutCatalog>("bakingrl-overlay-layouts-changed", (event) => {
      overlayLayouts = event;
    }).then((unlisten) => {
      unlistenOverlays = unlisten;
    });
    void adapter.listen<PagesFile>("bakingrl-pages-changed", (event) => {
      pages = event;
    }).then((unlisten) => {
      unlistenPages = unlisten;
    });
    void adapter.listen<string>("bakingrl-package-settings-changed", (packageId) => {
      settingsCache.delete(packageId);
      for (const [itemId, cleanup] of mountedItems) {
        const root = host?.querySelector<HTMLElement>(`[data-item-id="${itemId}"]`);
        if (root?.dataset.packageId === packageId) {
          cleanup();
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
      for (const cleanup of mountedItems.values()) cleanup();
      mountedItems.clear();
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
  style={hostStyle}
  bind:this={host}
  aria-label="Visual export host"
></main>
