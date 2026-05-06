<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import {
    RL_TELEMETRY_EVENT_NAMES,
    telemetryFrameTemplateJson,
    type GameEventFrame,
    type RlTelemetryEventName
  } from "$lib/rlTelemetry";
  import {
    getInitialLocale,
    localeOptions,
    storeLocale,
    translations,
    type Locale,
    type TranslationKey
  } from "$lib/i18n";
  import { applyTheme, DEFAULT_THEME, getStoredTheme, THEMES, type ThemeId } from "$lib/themes";

  type VisualExportDescriptor = {
    name: string;
    entry: string;
    default_width: number;
    default_height: number;
    settings: string | null;
  };

  type NamedExportDescriptor = {
    name: string;
  };

  type ServiceExportDescriptor = {
    name: string;
    methods: string[];
  };

  type PageExportDescriptor = {
    name: string;
    path: string;
    title: string | null;
    description: string | null;
  };

  type LayoutTemplateExportDescriptor = {
    name: string;
    path: string;
    title: string | null;
    description: string | null;
  };

  type PermissionShape = {
    bus?: {
      read?: string[];
      publish?: string[];
    };
    registry?: {
      read?: string[];
      write?: string[];
    };
    network?: {
      http?: string[];
      websocket?: string[];
    };
    storage?: string[] | {
      read?: string[];
      write?: string[];
    };
  };

  type PermissionSection = {
    title: string;
    rows: {
      label: string;
      values: string[];
      emptyLabel: string;
    }[];
  };

  type PackageDescriptor = {
    id: string;
    name: string;
    version: string;
    author: string | null;
    enabled: boolean;
    path: string;
    exports: {
      visuals: VisualExportDescriptor[];
      components: NamedExportDescriptor[];
      services: ServiceExportDescriptor[];
      connectors: NamedExportDescriptor[];
      assets: NamedExportDescriptor[];
      schemas: NamedExportDescriptor[];
      pages: PageExportDescriptor[];
      layouts: LayoutTemplateExportDescriptor[];
    };
    imports: {
      components: string[];
      services: string[];
    };
    effective_permissions: PermissionShape;
    settings: string | null;
    error: string | null;
  };

  type ManifestExports = {
    visuals?: Record<string, unknown>;
    components?: Record<string, unknown>;
    services?: Record<string, unknown>;
    connectors?: Record<string, unknown>;
    assets?: Record<string, unknown>;
    schemas?: Record<string, unknown>;
    pages?: Record<string, unknown>;
    layouts?: Record<string, unknown>;
  };

  type BundleInspection = {
    manifest: {
      id: string;
      name: string;
      version: string;
      author: string | null;
      exports: ManifestExports;
      imports?: PackageDescriptor["imports"];
      permissions?: PermissionShape;
    };
    hashes_present: boolean;
    signature_present: boolean;
    signature_verified: boolean;
    signature_public_key: string | null;
    file_count: number;
    uncompressed_size: number;
    sha256: string;
  };

  type PreparedPackageInstall = {
    path: string;
    source: string;
    inspection: BundleInspection;
  };

  type PendingInstall = PreparedPackageInstall & {
    kind: "file" | "url" | "git";
    label: string;
  };

  type ConfirmRequest = {
    title: string;
    message: string;
    confirmLabel: string;
    run: () => void | Promise<void>;
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
    template_source?: string | null;
  };

  type OverlayLayoutCatalog = {
    active_layout_id: string;
    stream_layout_id: string;
    layouts: OverlayLayout[];
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
    created_at_ms: number;
    updated_at_ms: number;
    template_source?: string | null;
  };

  type PagesFile = {
    pages: PageLayout[];
  };

  type AppSettings = {
    behavior: {
      launch_at_startup: boolean;
      close_will_hide: boolean;
      start_minimized: boolean;
    };
    obs: {
      host: string;
      port: number;
    };
    overlay: {
      hide_when_game_unfocused: boolean;
      update_rate_fps: number;
      use_monitor_size: boolean;
      monitor_id?: string | null;
      screen_width: number;
      screen_height: number;
    };
    telemetry: {
      rocket_league_host: string;
      rocket_league_port: number;
    };
  };

  type TelemetryConnectionStatus = {
    state: "connecting" | "connected" | "disconnected" | string;
    message: string | null;
    host: string;
    port: number;
    updated_at_ms: number;
  };

  type OverlayMonitor = {
    id: string;
    name: string;
    x: number;
    y: number;
    width: number;
    height: number;
    scaleFactor: number;
    primary: boolean;
    current: boolean;
  };

  type RegistryEntry = {
    key: string;
    value: unknown;
  };

  type DeveloperTelemetryEntry = {
    id: string;
    receivedAt: string;
    receivedAtMs: number;
    eventName: string;
    frame: GameEventFrame;
  };

  type DeveloperTelemetryGroup = {
    eventName: string;
    latest: DeveloperTelemetryEntry;
    count: number;
    lastReceivedAt: number;
  };

  type DeveloperTelemetrySort = "recent" | "alpha";

  type DeveloperFrameTemplate = RlTelemetryEventName;
  const TELEMETRY_HELP_DISMISSED_KEY = "bakingrl.telemetryHelp.dismissed";

  let activeTab = $state<"home" | "pages" | "overlays" | "packages" | "developer" | "settings">("home");
  let settingsSection = $state<"appearance" | "telemetry" | "overlay">("appearance");
  let locale = $state<Locale>("fr");
  let packages = $state<PackageDescriptor[]>([]);
  let overlayLayouts = $state<OverlayLayoutCatalog | null>(null);
  let pages = $state<PagesFile | null>(null);
  let appSettings = $state<AppSettings | null>(null);
  let bundlePath = $state("");
  let bundleUrl = $state("");
  let gitRepo = $state("");
  let gitRev = $state("");
  let expandedLayoutId = $state("");
  let newLayoutName = $state("");
  let newPageName = $state("");
  let expandedPageId = $state("");
  let message = $state("");
  let busy = $state(false);
  let pendingInstall = $state<PendingInstall | null>(null);
  let currentTheme = $state<ThemeId>(DEFAULT_THEME);
  let confirmRequest = $state<ConfirmRequest | null>(null);
  let telemetryStatus = $state<TelemetryConnectionStatus | null>(null);
  let telemetryHelpOpen = $state(false);
  let telemetryHelpDontShow = $state(false);
  let overlayMonitors = $state<OverlayMonitor[]>([]);
  let registryEntries = $state<RegistryEntry[]>([]);
  let developerTelemetry = $state<DeveloperTelemetryEntry[]>([]);
  let developerTelemetryGroups = $state<DeveloperTelemetryGroup[]>([]);
  let developerTelemetrySort = $state<DeveloperTelemetrySort>("recent");
  let developerFrameTemplate = $state<DeveloperFrameTemplate>("UpdateState");
  let developerFrameJson = $state(telemetryFrameTemplateJson("UpdateState"));

  const obsBaseUrl = $derived(
    appSettings ? `http://${appSettings.obs.host}:${appSettings.obs.port}` : ""
  );
  const telemetryConnected = $derived(telemetryStatus?.state === "connected");
  const telemetryStatusLabel = $derived.by(() => {
    if (!telemetryStatus) return t("common.loading");
    if (telemetryStatus.state === "connected") return t("common.connected");
    if (telemetryStatus.state === "connecting") return t("common.connecting");
    return t("common.disconnected");
  });
  const sortedDeveloperTelemetryGroups = $derived.by(() => {
    return [...developerTelemetryGroups].sort((a, b) => {
      if (developerTelemetrySort === "alpha") {
        return a.eventName.localeCompare(b.eventName);
      }
      return b.lastReceivedAt - a.lastReceivedAt || a.eventName.localeCompare(b.eventName);
    });
  });

  const enabledPackageCount = $derived(packages.filter((pkg) => pkg.enabled).length);
  const packageErrorCount = $derived(packages.filter((pkg) => pkg.error).length);
  const overlayLayoutCount = $derived(overlayLayouts?.layouts.length ?? 0);
  const pageCount = $derived(pages?.pages.length ?? 0);
  const homeInGameLayout = $derived(
    overlayLayouts?.layouts.find((layout) => layout.id === overlayLayouts?.active_layout_id) ?? null
  );
  const homeStreamLayout = $derived(
    overlayLayouts?.layouts.find((layout) => layout.id === overlayLayouts?.stream_layout_id) ?? null
  );
  const favoritePages = $derived(pages?.pages.filter((page) => page.favorite) ?? []);
  const homeMainLayouts = $derived(
    [
      { kind: "ingame", label: t("home.ingameLayout"), layout: homeInGameLayout },
      { kind: "stream", label: t("home.streamLayout"), layout: homeStreamLayout }
    ] as const
  );

  function t(key: TranslationKey) {
    return translations[locale][key];
  }

  function tx(key: TranslationKey, values: Record<string, string | number>) {
    return Object.entries(values).reduce(
      (text, [name, value]) => text.replaceAll(`{${name}}`, String(value)),
      t(key)
    );
  }

  function setLocale(nextLocale: Locale) {
    locale = nextLocale;
    storeLocale(nextLocale);
  }

  async function refresh() {
    packages = await invoke<PackageDescriptor[]>("list_packages");
    overlayLayouts = await invoke<OverlayLayoutCatalog>("get_overlay_layouts");
    pages = await invoke<PagesFile>("get_pages");
    appSettings = await invoke<AppSettings>("get_app_settings");
    telemetryStatus = await invoke<TelemetryConnectionStatus>("get_telemetry_status");
    overlayMonitors = await invoke<OverlayMonitor[]>("list_overlay_monitors");
    registryEntries = await invoke<RegistryEntry[]>("registry_entries");
    if (expandedLayoutId && !overlayLayouts.layouts.some((layout) => layout.id === expandedLayoutId)) {
      expandedLayoutId = "";
    }
    if (expandedPageId && !pages.pages.some((page) => page.id === expandedPageId)) {
      expandedPageId = "";
    }
  }

  async function reloadPackages() {
    busy = true;
    message = "";
    try {
      packages = await invoke<PackageDescriptor[]>("reload_packages");
      message = t("msg.packagesReloaded");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function togglePackage(pkg: PackageDescriptor) {
    busy = true;
    message = "";
    try {
      packages = await invoke<PackageDescriptor[]>("set_package_enabled", {
        packageId: pkg.id,
        enabled: !pkg.enabled
      });
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
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

  function removePackage(pkg: PackageDescriptor) {
    askConfirmation({
      title: t("confirm.removePackageTitle"),
      message: tx("confirm.removePackageMessage", { name: pkg.name }),
      confirmLabel: t("common.remove"),
      run: () => removePackageConfirmed(pkg)
    });
  }

  async function removePackageConfirmed(pkg: PackageDescriptor) {
    busy = true;
    message = "";
    try {
      packages = await invoke<PackageDescriptor[]>("remove_package", {
        packageId: pkg.id
      });
      message = t("msg.packageRemoved");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function inspectInstallFile() {
    if (!bundlePath.trim()) return;
    busy = true;
    message = "";
    try {
      const path = bundlePath.trim();
      const inspection = await invoke<BundleInspection>("inspect_package_bundle", { path });
      await setPendingInstall({ kind: "file", label: path, path, source: `file:${path}`, inspection });
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function confirmPendingInstall() {
    if (!pendingInstall) return;
    busy = true;
    message = "";
    try {
      const sourceKind = pendingInstall.kind;
      await invoke("install_prepared_package", {
        path: pendingInstall.path,
        source: pendingInstall.source
      });
      pendingInstall = null;
      if (sourceKind === "file") {
        bundlePath = "";
      } else if (sourceKind === "url") {
        bundleUrl = "";
      } else {
        gitRepo = "";
        gitRev = "";
      }
      await refresh();
      message =
        sourceKind === "file"
          ? t("msg.installedFile")
          : sourceKind === "url"
            ? t("msg.installedUrl")
            : t("msg.installedGit");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function installFromUrl() {
    if (!bundleUrl.trim()) return;
    busy = true;
    message = "";
    try {
      const url = bundleUrl.trim();
      const prepared = await invoke<PreparedPackageInstall>("prepare_package_from_url", { url });
      await setPendingInstall({ kind: "url", label: url, ...prepared });
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function installFromGit() {
    if (!gitRepo.trim()) return;
    busy = true;
    message = "";
    try {
      const repo = gitRepo.trim();
      const rev = gitRev.trim() || null;
      const prepared = await invoke<PreparedPackageInstall>("prepare_package_from_git", { repo, rev });
      await setPendingInstall({
        kind: "git",
        label: rev ? `${repo}#${rev}` : repo,
        ...prepared
      });
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function prepareDeepLinkInstall(deepLink: string) {
    busy = true;
    activeTab = "packages";
    message = "";
    try {
      const prepared = await invoke<PreparedPackageInstall>("prepare_package_from_deep_link", {
        deepLink
      });
      const label = prepared.source.startsWith("deeplink:")
        ? prepared.source.slice("deeplink:".length)
        : deepLink;
      await setPendingInstall({ kind: "url", label, ...prepared });
    } catch (error) {
      message = `${t("msg.deepLinkRejected")}: ${String(error)}`;
    } finally {
      busy = false;
    }
  }

  async function handleDeepLinkUrls(urls: string[] | null) {
    if (!urls?.length) return;
    await prepareDeepLinkInstall(urls[0]);
  }

  async function setPendingInstall(next: PendingInstall) {
    const previous = pendingInstall;
    pendingInstall = next;
    if (previous && previous.kind !== "file" && previous.path !== next.path) {
      try {
        await invoke("discard_prepared_package", { path: previous.path });
      } catch {
        // A failed cleanup should not block replacing the prompt.
      }
    }
  }

  async function cancelPendingInstall() {
    const install = pendingInstall;
    pendingInstall = null;
    if (!install || install.kind === "file") return;
    try {
      await invoke("discard_prepared_package", { path: install.path });
    } catch {
      // A failed cleanup should not block the user from canceling the prompt.
    }
  }

  async function createOverlayLayout() {
    busy = true;
    message = "";
    try {
      overlayLayouts = await invoke<OverlayLayoutCatalog>("create_overlay_layout", {
        name: newLayoutName.trim() || t("overlays.untitled")
      });
      expandedLayoutId = overlayLayouts.active_layout_id;
      newLayoutName = "";
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function saveLayout(layout: OverlayLayout) {
    busy = true;
    message = "";
    try {
      overlayLayouts = await invoke<OverlayLayoutCatalog>("save_overlay_layout", { layout });
      message = t("msg.overlaySaved");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  function toggleLayoutDetails(layoutId: string) {
    expandedLayoutId = expandedLayoutId === layoutId ? "" : layoutId;
  }

  function layoutItemCount(layout: OverlayLayout) {
    return (layout.layers ?? []).reduce((total, layer) => total + layer.items.length, 0);
  }

  function layoutLayerCount(layout: OverlayLayout) {
    return layout.layers?.length ?? 0;
  }

  function isInGameLayout(layout: OverlayLayout) {
    return overlayLayouts?.active_layout_id === layout.id;
  }

  function isStreamLayout(layout: OverlayLayout) {
    return overlayLayouts?.stream_layout_id === layout.id;
  }

  async function renameLayout(layout: OverlayLayout, name: string) {
    const trimmed = name.trim();
    if (!trimmed || trimmed === layout.name) return;
    await saveLayout({ ...layout, name: trimmed });
  }

  function handleLayoutNameBlur(layout: OverlayLayout, event: FocusEvent) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.value.trim()) {
      input.value = layout.name;
      return;
    }
    void renameLayout(layout, input.value);
  }

  function handleLayoutNameKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      (event.currentTarget as HTMLInputElement).blur();
    }
  }

  function deleteLayout(layout: OverlayLayout) {
    if ((overlayLayouts?.layouts.length ?? 0) <= 1) {
      message = t("msg.overlayRequired");
      return;
    }
    askConfirmation({
      title: t("confirm.deleteLayoutTitle"),
      message: tx("confirm.deleteLayoutMessage", { name: layout.name }),
      confirmLabel: t("common.delete"),
      run: () => deleteLayoutConfirmed(layout.id)
    });
  }

  async function deleteLayoutConfirmed(layoutId: string) {
    busy = true;
    message = "";
    try {
      overlayLayouts = await invoke<OverlayLayoutCatalog>("delete_overlay_layout", { layoutId });
      if (expandedLayoutId === layoutId) {
        expandedLayoutId = "";
      }
      message = t("msg.overlayDeleted");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function routeOverlayLayout(layoutId: string, stream = false) {
    busy = true;
    message = "";
    try {
      overlayLayouts = await invoke<OverlayLayoutCatalog>(
        stream ? "set_stream_overlay_layout" : "set_active_overlay_layout",
        { layoutId }
      );
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  function removeItem(layout: OverlayLayout, itemId: string) {
    const itemName = layout.layers.flatMap((layer) => layer.items).find((item) => item.id === itemId)?.name ?? t("common.items");
    askConfirmation({
      title: t("confirm.removeItemTitle"),
      message: tx("confirm.removeItemMessage", { item: itemName, layout: layout.name }),
      confirmLabel: t("common.remove"),
      run: () => removeItemConfirmed(layout.id, itemId)
    });
  }

  function removeItemConfirmed(layoutId: string, itemId: string) {
    const layout = overlayLayouts?.layouts.find((candidate) => candidate.id === layoutId);
    if (!layout) return;
    for (const layer of layout.layers) {
      layer.items = layer.items.filter((item) => item.id !== itemId);
    }
    void saveLayout(layout);
  }

  function sortedLayers(layout: OverlayLayout) {
    return [...(layout.layers ?? [])].sort((a, b) => {
      if (a.kind === "event" && b.kind !== "event") return 1;
      if (a.kind !== "event" && b.kind === "event") return -1;
      return a.order - b.order;
    });
  }

  function layoutUrl(layoutId: string) {
    return `${obsBaseUrl}/overlay/layout/${encodeURIComponent(layoutId)}`;
  }

  function streamUrl() {
    return `${obsBaseUrl}/overlay/stream`;
  }

  async function copyText(value: string, label: string) {
    await navigator.clipboard.writeText(value);
    message = `${label} ${t("msg.copied")}`;
  }

  function openPreview(value: string) {
    window.open(value, "_blank", "noopener,noreferrer");
  }

  async function openLayoutEditor(layoutId: string) {
    try {
      await invoke("open_overlay_layout_editor", { layoutId });
    } catch (error) {
      message = String(error);
      window.location.href = `/editor/layout/${layoutId}`;
    }
  }

  async function importPackageLayout(packageId: string, exportName: string) {
    busy = true;
    message = "";
    try {
      overlayLayouts = await invoke<OverlayLayoutCatalog>("import_package_layout", {
        packageId,
        exportName
      });
      activeTab = "overlays";
      expandedLayoutId = overlayLayouts.active_layout_id;
      message = t("msg.overlayImported");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function createPage() {
    busy = true;
    message = "";
    try {
      pages = await invoke<PagesFile>("create_page", {
        name: newPageName.trim() || t("pages.untitled")
      });
      expandedPageId = pages.pages[0]?.id ?? "";
      newPageName = "";
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function savePage(page: PageLayout) {
    busy = true;
    message = "";
    try {
      pages = await invoke<PagesFile>("save_page", { page });
      message = t("msg.pageSaved");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  function togglePageDetails(pageId: string) {
    expandedPageId = expandedPageId === pageId ? "" : pageId;
  }

  function pageItemCount(page: PageLayout) {
    return page.layers.reduce((total, layer) => total + layer.items.length, 0);
  }

  function pagePluginCount(page: PageLayout) {
    return page.layers.reduce(
      (total, layer) => total + layer.items.filter((item) => item.kind === "visual").length,
      0
    );
  }

  async function renamePage(page: PageLayout, name: string) {
    const trimmed = name.trim();
    if (!trimmed || trimmed === page.name) return;
    await savePage({ ...page, name: trimmed });
  }

  function handlePageNameBlur(page: PageLayout, event: FocusEvent) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.value.trim()) {
      input.value = page.name;
      return;
    }
    void renamePage(page, input.value);
  }

  async function updatePageOpenTarget(page: PageLayout, openTarget: "app" | "window") {
    await savePage({ ...page, settings: { ...page.settings, open_target: openTarget } });
  }

  async function togglePageFavorite(page: PageLayout) {
    await savePage({ ...page, favorite: !page.favorite });
  }

  async function duplicatePage(pageId: string) {
    busy = true;
    message = "";
    try {
      pages = await invoke<PagesFile>("duplicate_page", { pageId });
      expandedPageId = pages.pages[0]?.id ?? "";
      message = t("msg.pageDuplicated");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  function deletePage(page: PageLayout) {
    askConfirmation({
      title: t("confirm.deletePageTitle"),
      message: tx("confirm.deletePageMessage", { name: page.name }),
      confirmLabel: t("common.delete"),
      run: () => deletePageConfirmed(page.id)
    });
  }

  async function deletePageConfirmed(pageId: string) {
    busy = true;
    message = "";
    try {
      pages = await invoke<PagesFile>("delete_page", { pageId });
      if (expandedPageId === pageId) expandedPageId = "";
      message = t("msg.pageDeleted");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function importPackagePage(packageId: string, exportName: string) {
    busy = true;
    message = "";
    try {
      pages = await invoke<PagesFile>("import_package_page", {
        packageId,
        exportName
      });
      activeTab = "pages";
      expandedPageId = pages.pages[0]?.id ?? "";
      message = t("msg.pageImported");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  async function openPage(pageId: string) {
    try {
      await invoke("open_page", { pageId });
    } catch (error) {
      message = String(error);
      window.location.href = `/page/${pageId}`;
    }
  }

  function openPageEditor(pageId: string) {
    window.location.href = `/editor/page/${pageId}`;
  }

  const pageTemplates = $derived(
    packages.filter((pkg) => pkg.enabled).flatMap((pkg) =>
      pkg.exports.pages.map((page) => ({
        package: pkg,
        page
      }))
    )
  );

  const layoutTemplates = $derived(
    packages.filter((pkg) => pkg.enabled).flatMap((pkg) =>
      pkg.exports.layouts.map((layoutTemplate) => ({
        package: pkg,
        layoutTemplate
      }))
    )
  );

  async function saveAppSettings() {
    if (!appSettings) return;
    busy = true;
    message = "";
    try {
      appSettings.overlay.monitor_id = appSettings.overlay.monitor_id || null;
      appSettings = await invoke<AppSettings>("save_app_settings", { settings: appSettings });
      overlayMonitors = await invoke<OverlayMonitor[]>("list_overlay_monitors");
      message = t("msg.settingsSaved");
    } catch (error) {
      message = String(error);
    } finally {
      busy = false;
    }
  }

  function exportCount(pkg: PackageDescriptor) {
    return (
      pkg.exports.visuals.length +
      pkg.exports.components.length +
      pkg.exports.services.length +
      pkg.exports.connectors.length +
      pkg.exports.assets.length +
      pkg.exports.schemas.length +
      pkg.exports.pages.length +
      pkg.exports.layouts.length
    );
  }

  function formatJson(value: unknown) {
    return JSON.stringify(value, null, 2);
  }

  function inspectionExportCount(inspection: BundleInspection) {
    const exports = inspection.manifest.exports;
    return (
      Object.keys(exports.visuals ?? {}).length +
      Object.keys(exports.components ?? {}).length +
      Object.keys(exports.services ?? {}).length +
      Object.keys(exports.connectors ?? {}).length +
      Object.keys(exports.assets ?? {}).length +
      Object.keys(exports.schemas ?? {}).length +
      Object.keys(exports.pages ?? {}).length +
      Object.keys(exports.layouts ?? {}).length
    );
  }

  function signatureStatus(inspection: BundleInspection) {
    if (inspection.signature_verified) return "verified";
    if (inspection.signature_present) return "invalid";
    return "missing";
  }

  function permissionValueList(value: unknown): string[] {
    return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === "string") : [];
  }

  function permissionSections(permissions: PermissionShape | null | undefined): PermissionSection[] {
    const bus = permissions?.bus ?? {};
    const registry = permissions?.registry ?? {};
    const network = permissions?.network ?? {};
    const storage = permissions?.storage;
    const storageRead = Array.isArray(storage) ? storage : storage?.read;
    const storageWrite = Array.isArray(storage) ? storage : storage?.write;

    return [
      {
        title: t("permissions.telemetryBus"),
        rows: [
          { label: t("permissions.readEvents"), values: permissionValueList(bus.read), emptyLabel: t("permissions.noReadEvents") },
          { label: t("permissions.publishEvents"), values: permissionValueList(bus.publish), emptyLabel: t("permissions.noPublishEvents") }
        ]
      },
      {
        title: t("permissions.registry"),
        rows: [
          { label: t("permissions.readKeys"), values: permissionValueList(registry.read), emptyLabel: t("permissions.noReadKeys") },
          { label: t("permissions.writeKeys"), values: permissionValueList(registry.write), emptyLabel: t("permissions.noWriteKeys") }
        ]
      },
      {
        title: t("permissions.network"),
        rows: [
          { label: t("permissions.httpHosts"), values: permissionValueList(network.http), emptyLabel: t("permissions.noHttp") },
          { label: t("permissions.websocketHosts"), values: permissionValueList(network.websocket), emptyLabel: t("permissions.noWebsocket") }
        ]
      },
      {
        title: t("permissions.storage"),
        rows: [
          { label: t("permissions.readStorage"), values: permissionValueList(storageRead), emptyLabel: t("permissions.noReadStorage") },
          { label: t("permissions.writeStorage"), values: permissionValueList(storageWrite), emptyLabel: t("permissions.noWriteStorage") }
        ]
      }
    ];
  }

  function permissionTotal(permissions: PermissionShape | null | undefined) {
    return permissionSections(permissions).reduce(
      (total, section) => total + section.rows.reduce((rowTotal, row) => rowTotal + row.values.length, 0),
      0
    );
  }

  function loadDeveloperFrameTemplate() {
    developerFrameJson = telemetryFrameTemplateJson(developerFrameTemplate);
  }

  function recordTelemetryFrame(frame: GameEventFrame) {
    const receivedAtMs = Date.now();
    const entry: DeveloperTelemetryEntry = {
      id: `${receivedAtMs}-${Math.random().toString(36).slice(2)}`,
      receivedAt: new Date().toLocaleTimeString(),
      receivedAtMs,
      eventName: frame.Event,
      frame
    };
    developerTelemetry = [entry, ...developerTelemetry].slice(0, 80);
    const existingGroup = developerTelemetryGroups.find((group) => group.eventName === entry.eventName);
    if (!existingGroup) {
      developerTelemetryGroups = [
        ...developerTelemetryGroups,
        {
          eventName: entry.eventName,
          latest: entry,
          count: 1,
          lastReceivedAt: entry.receivedAtMs
        }
      ];
      return;
    }
    developerTelemetryGroups = developerTelemetryGroups.map((group) =>
      group.eventName === entry.eventName
        ? {
            ...group,
            latest: entry,
            count: group.count + 1,
            lastReceivedAt: entry.receivedAtMs
          }
        : group
    );
  }

  async function refreshRegistryEntries() {
    registryEntries = await invoke<RegistryEntry[]>("registry_entries");
  }

  async function sendDeveloperFrame() {
    message = "";
    try {
      const parsed = JSON.parse(developerFrameJson) as Partial<GameEventFrame>;
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("Frame must be a JSON object.");
      }
      if (typeof parsed.Event !== "string" || !parsed.Event.trim()) {
        throw new Error('Frame must include a non-empty "Event" string.');
      }
      const frame: GameEventFrame = {
        Event: parsed.Event.trim(),
        Data: parsed.Data ?? {}
      };
      await invoke("emit_developer_telemetry", { frame });
      message = `${t("msg.developerFrameSent")} (${frame.Event})`;
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }
  }

  function selectTheme(themeId: ThemeId) {
    currentTheme = applyTheme(themeId);
  }

  function telemetryHelpDismissed() {
    try {
      return localStorage.getItem(TELEMETRY_HELP_DISMISSED_KEY) === "true";
    } catch {
      return false;
    }
  }

  function openTelemetryHelp(useStoredChoice = true) {
    telemetryHelpDontShow = useStoredChoice ? telemetryHelpDismissed() : false;
    telemetryHelpOpen = true;
  }

  function closeTelemetryHelp() {
    try {
      if (telemetryHelpDontShow) {
        localStorage.setItem(TELEMETRY_HELP_DISMISSED_KEY, "true");
      } else {
        localStorage.removeItem(TELEMETRY_HELP_DISMISSED_KEY);
      }
    } catch {
      // Ignore storage failures; the help can still be closed for this session.
    }
    telemetryHelpOpen = false;
  }

  function isTauriRuntime() {
    return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);
  }

  onMount(() => {
    locale = getInitialLocale();
    currentTheme = applyTheme(getStoredTheme());
    if (!telemetryHelpDismissed()) {
      openTelemetryHelp(false);
    }
    void refresh();
    let unlistenPackages: (() => void) | undefined;
    let unlistenOverlays: (() => void) | undefined;
    let unlistenPages: (() => void) | undefined;
    let unlistenDeepLinks: (() => void) | undefined;
    let unlistenTelemetryStatus: (() => void) | undefined;
    let unlistenTelemetry: (() => void) | undefined;
    if (isTauriRuntime() && getCurrentWindow().label === "main") {
      void getCurrent()
        .then(handleDeepLinkUrls)
        .catch((error) => {
          message = `${t("msg.deepLinkUnavailable")}: ${String(error)}`;
        });
      void onOpenUrl((urls) => {
        void handleDeepLinkUrls(urls);
      })
        .then((unlisten) => {
          unlistenDeepLinks = unlisten;
        })
        .catch((error) => {
          message = `${t("msg.deepLinkListenerUnavailable")}: ${String(error)}`;
        });
    }
    void listen<PackageDescriptor[]>("bakingrl-packages-changed", (event) => {
      packages = event.payload;
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    void listen<OverlayLayoutCatalog>("bakingrl-overlay-layouts-changed", (event) => {
      overlayLayouts = event.payload;
    }).then((unlisten) => {
      unlistenOverlays = unlisten;
    });
    void listen<PagesFile>("bakingrl-pages-changed", (event) => {
      pages = event.payload;
    }).then((unlisten) => {
      unlistenPages = unlisten;
    });
    void listen<TelemetryConnectionStatus>("bakingrl-telemetry-status", (event) => {
      telemetryStatus = event.payload;
    }).then((unlisten) => {
      unlistenTelemetryStatus = unlisten;
    });
    void listen<GameEventFrame>("bakingrl-telemetry", (event) => {
      const payload = event.payload;
      recordTelemetryFrame({
        Event: typeof payload?.Event === "string" ? payload.Event : "Unknown",
        Data: payload?.Data ?? payload
      });
    }).then((unlisten) => {
      unlistenTelemetry = unlisten;
    });
    return () => {
      unlistenPackages?.();
      unlistenOverlays?.();
      unlistenPages?.();
      unlistenDeepLinks?.();
      unlistenTelemetryStatus?.();
      unlistenTelemetry?.();
    };
  });
</script>

<main>
  <ConfirmDialog
    open={confirmRequest !== null}
    title={confirmRequest?.title}
    message={confirmRequest?.message}
    confirmLabel={confirmRequest?.confirmLabel}
    cancelLabel={t("common.cancel")}
    danger
    onconfirm={confirmAction}
    oncancel={cancelConfirmation}
  />

  {#if telemetryHelpOpen}
    <div class="telemetry-help-overlay">
      <button
        type="button"
        class="telemetry-help-scrim"
        aria-label={t("telemetry.helpClose")}
        onclick={closeTelemetryHelp}
      ></button>
      <div
        class="telemetry-help-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="telemetry-help-title"
        tabindex="-1"
      >
        <div class="telemetry-help-copy">
          <span class="badge route">Rocket League Telemetry</span>
          <h2 id="telemetry-help-title">{t("telemetry.helpTitle")}</h2>
          <p>{t("telemetry.helpDesc")}</p>
        </div>

        <ol class="telemetry-help-steps">
          <li>{t("telemetry.stepClose")}</li>
          <li>{t("telemetry.stepOpen")} <code>&lt;Rocket League install&gt;\TAGame\Config\DefaultStatsAPI.ini</code>.</li>
          <li>{t("telemetry.stepPacket")}</li>
          <li>{t("telemetry.stepPort")} <code>{appSettings?.telemetry.rocket_league_port ?? telemetryStatus?.port ?? 49123}</code>.</li>
          <li>{t("telemetry.stepRestart")}</li>
        </ol>

        <div class="telemetry-help-note">
          <strong>{t("telemetry.expected")}</strong>
          <span>
            {t("telemetry.listensOn")}
            <code>{appSettings?.telemetry.rocket_league_host ?? telemetryStatus?.host ?? "127.0.0.1"}:{appSettings?.telemetry.rocket_league_port ?? telemetryStatus?.port ?? 49123}</code>.
            {t("telemetry.keepAligned")}
          </span>
        </div>

        <label class="checkbox-label telemetry-help-check">
          <input type="checkbox" bind:checked={telemetryHelpDontShow} />
          <span class="checkmark"></span>
          {t("telemetry.dontShow")}
        </label>

        <div class="telemetry-help-actions">
          <button class="btn-primary" onclick={closeTelemetryHelp}>OK</button>
        </div>
      </div>
    </div>
  {/if}

  <header>
    <div class="logo-area">
      <div class="logo-icon"></div>
      <div>
        <h1>BakingRL</h1>
        <p class="subtitle">Your RL Companion</p>
      </div>
    </div>
    <nav class="tabs" aria-label={t("home.title")}>
      <div class="tab-group primary">
        <button class="tab-btn" class:active={activeTab === "home"} onclick={() => (activeTab = "home")}>
          {t("nav.home")}
        </button>
        <button class="tab-btn" class:active={activeTab === "pages"} onclick={() => (activeTab = "pages")}>
          {t("nav.pages")}
        </button>
        <button class="tab-btn" class:active={activeTab === "overlays"} onclick={() => (activeTab = "overlays")}>
          {t("nav.overlays")}
        </button>
      </div>
      <div class="tab-group secondary">
        <button class="tab-btn" class:active={activeTab === "packages"} onclick={() => (activeTab = "packages")}>
          {t("nav.packages")}
        </button>
        <button class="tab-btn" class:active={activeTab === "developer"} onclick={() => (activeTab = "developer")}>
          {t("nav.developer")}
        </button>
        <button class="tab-btn" class:active={activeTab === "settings"} onclick={() => (activeTab = "settings")}>
          {t("nav.settings")}
        </button>
      </div>
      <div class="language-switch" aria-label={t("language.label")}>
        {#each localeOptions as option}
          <button
            type="button"
            class:active={locale === option.id}
            aria-pressed={locale === option.id}
            onclick={() => setLocale(option.id)}
          >
            {option.label}
          </button>
        {/each}
      </div>
    </nav>
  </header>

  <div class="content-area">
    {#if message}
      <div class="message-banner" role="alert">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"></circle><line x1="12" y1="16" x2="12" y2="12"></line><line x1="12" y1="8" x2="12.01" y2="8"></line></svg>
        <p>{message}</p>
        <button class="close-msg" onclick={() => (message = "")}>&times;</button>
      </div>
    {/if}

    {#if activeTab === "home"}
      <div class="home-layout">
        <section class="panel glass home-metric-bar" aria-label={t("home.overview")}>
          <button type="button" class="home-metric" onclick={() => (activeTab = "pages")}>
            <span class="home-metric-value">{pageCount}</span>
            <span>{t("home.pagesLabel")}</span>
          </button>
          <button type="button" class="home-metric" onclick={() => (activeTab = "overlays")}>
            <span class="home-metric-value">{overlayLayoutCount}</span>
            <span>{t("home.overlaysLabel")}</span>
          </button>
          <button type="button" class="home-metric" onclick={() => (activeTab = "packages")}>
            <span class="home-metric-value">{enabledPackageCount}/{packages.length}</span>
            <span>{t("home.activePackagesLabel")}</span>
            {#if packageErrorCount}
              <span class="home-metric-note">{packageErrorCount} {t("home.errors")}</span>
            {/if}
          </button>
        </section>

        <section class="home-section">
          <div class="home-section-header">
            <div>
              <h2>{t("home.favoritePages")}</h2>
              <p class="desc">{t("home.favoritePagesDesc")}</p>
            </div>
          </div>

          {#if favoritePages.length}
            <div class="home-card-grid">
              {#each favoritePages as page (page.id)}
                <article class="panel glass home-page-card">
                  <div class="home-card-copy">
                    <span class="field-label">{page.settings.open_target === "window" ? t("pages.window") : t("pages.inApp")}</span>
                    <h3>{page.name}</h3>
                    <p>{page.layers.length} {t("common.layers")} · {pageItemCount(page)} {t("common.items")}</p>
                  </div>
                  <div class="home-card-actions">
                    <button class="btn-primary small" onclick={() => void openPage(page.id)}>{t("common.open")}</button>
                    <button class="btn-outline small" onclick={() => openPageEditor(page.id)}>{t("common.edit")}</button>
                  </div>
                </article>
              {/each}
            </div>
          {:else}
            <div class="panel glass home-empty-row">
              <p>{t("home.noFavoritePages")}</p>
            </div>
          {/if}
        </section>

        <section class="home-section">
          <div class="home-section-header">
            <div>
              <h2>{t("home.mainLayouts")}</h2>
              <p class="desc">{t("home.mainLayoutsDesc")}</p>
            </div>
          </div>

          <div class="home-card-grid main-layouts">
            {#each homeMainLayouts as entry}
              <article class="panel glass home-layout-card">
                <div class="home-card-copy">
                  <span class="field-label">{entry.label}</span>
                  <h3>{entry.layout?.name ?? t("home.noLayout")}</h3>
                  {#if entry.layout}
                    <p>{tx("home.layoutMeta", { layers: layoutLayerCount(entry.layout), items: layoutItemCount(entry.layout) })}</p>
                  {:else}
                    <p>{t("overlays.empty")}</p>
                  {/if}
                </div>
                <div class="home-card-actions">
                  <button class="btn-primary small" onclick={() => entry.layout && void openLayoutEditor(entry.layout.id)} disabled={!entry.layout}>
                    {t("common.edit")}
                  </button>
                  <button class="btn-outline small" onclick={() => entry.layout && openPreview(layoutUrl(entry.layout.id))} disabled={!entry.layout || !obsBaseUrl}>
                    {t("common.preview")}
                  </button>
                </div>
              </article>
            {/each}
          </div>
        </section>
      </div>

    {:else if activeTab === "packages"}
      <div class="split-layout">
        <div class="packages-list">
          <div class="section-header">
            <h2>{t("packages.installedTitle")}</h2>
            <button class="btn-secondary small" onclick={reloadPackages} disabled={busy}>
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.3"/></svg>
              {t("common.reload")}
            </button>
          </div>
          <section class="package-grid">
            {#each packages as pkg (pkg.id)}
              <article class="package-card" class:disabled={!pkg.enabled}>
                <div class="package-head">
                  <div class="pkg-title-area">
                    <h3 class="pkg-title">{pkg.name}</h3>
                    <span class="pkg-version">v{pkg.version}</span>
                  </div>
                  <div class="pkg-actions">
                    <span class="status-badge {pkg.enabled ? 'enabled' : 'disabled'}">
                      {pkg.enabled ? t("common.active") : t("common.disabled")}
                    </span>
                    <button class="icon-btn" onclick={() => togglePackage(pkg)} disabled={busy} title={pkg.enabled ? t("common.disabled") : t("common.enabled")}>
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        {#if pkg.enabled}
                          <rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect><line x1="9" y1="9" x2="15" y2="15"></line><line x1="15" y1="9" x2="9" y2="15"></line>
                        {:else}
                          <polyline points="20 6 9 17 4 12"></polyline>
                        {/if}
                      </svg>
                    </button>
                    <button class="icon-btn danger" onclick={() => removePackage(pkg)} disabled={busy} title={t("common.remove")}>
                      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                    </button>
                  </div>
                </div>

                <div class="pkg-meta">
                  <p class="pkg-id">{pkg.id}</p>
                  <p class="pkg-author">{t("packages.by")} {pkg.author ?? t("packages.unknownAuthor")}</p>
                </div>

                {#if pkg.error}
                  <div class="pkg-error">{pkg.error}</div>
                {/if}

                <div class="pkg-stats">
                  <div class="stat"><span class="stat-val">{pkg.exports.visuals.length}</span> {t("packages.visuals")}</div>
                  <div class="stat"><span class="stat-val">{pkg.exports.components.length}</span> {t("packages.components")}</div>
                  <div class="stat"><span class="stat-val">{pkg.exports.services.length}</span> {t("packages.services")}</div>
                  <div class="stat"><span class="stat-val">{pkg.exports.pages.length}</span> {t("packages.pages")}</div>
                  <div class="stat"><span class="stat-val">{pkg.exports.layouts.length}</span> {t("packages.layouts")}</div>
                </div>

                <details class="pkg-details">
                  <summary>{t("packages.details")}</summary>
                  <div class="details-content">
                    <div class="exports-list">
                      {#if pkg.exports.visuals.length}
                        <h4>Visuals</h4>
                        <ul>
                          {#each pkg.exports.visuals as visual}
                            <li>{visual.name} <span>({visual.default_width}x{visual.default_height})</span></li>
                          {/each}
                        </ul>
                      {/if}
                      {#if pkg.exports.services.length}
                        <h4>Services</h4>
                        <ul>
                          {#each pkg.exports.services as service}
                            <li>{service.name} <span>({service.methods.length} {t("packages.methods")})</span></li>
                          {/each}
                        </ul>
                      {/if}
                      {#if pkg.exports.pages.length}
                        <h4>{t("packages.pageTemplates")}</h4>
                        <ul>
                          {#each pkg.exports.pages as page}
                            <li>{page.title ?? page.name} <span>({page.path})</span></li>
                          {/each}
                        </ul>
                      {/if}
                      {#if pkg.exports.layouts.length}
                        <h4>{t("packages.layoutTemplates")}</h4>
                        <ul>
                          {#each pkg.exports.layouts as layoutTemplate}
                            <li>{layoutTemplate.title ?? layoutTemplate.name} <span>({layoutTemplate.path})</span></li>
                          {/each}
                        </ul>
                      {/if}
                    </div>
                    <div class="permissions-view">
                      <div class="permission-summary">
                        <span class="permission-count">{permissionTotal(pkg.effective_permissions)}</span>
                        <span>{t("packages.effectivePermissions")}</span>
                      </div>
                      {#if permissionTotal(pkg.effective_permissions) > 0}
                        <div class="permission-grid">
                          {#each permissionSections(pkg.effective_permissions) as section}
                            <section class="permission-card">
                              <h4>{section.title}</h4>
                              {#each section.rows as row}
                                <div class="permission-row">
                                  <span class="permission-label">{row.label}</span>
                                  {#if row.values.length}
                                    <div class="permission-chips">
                                      {#each row.values as value}
                                        <span class="permission-chip">{value}</span>
                                      {/each}
                                    </div>
                                  {:else}
                                    <span class="permission-empty">{row.emptyLabel}</span>
                                  {/if}
                                </div>
                              {/each}
                            </section>
                          {/each}
                        </div>
                      {:else}
                        <p class="permission-none">{t("packages.noExtraPermissions")}</p>
                      {/if}
                    </div>
                  </div>
                </details>
              </article>
            {/each}
            {#if packages.length === 0}
              <div class="empty-state">
                <p>{t("packages.noneInstalled")}</p>
              </div>
            {/if}
          </section>
        </div>

        <div class="sidebar">
          <section class="panel glass">
            <h2>{t("packages.installTitle")}</h2>
            <p class="desc">{t("packages.installDesc")}</p>

            <div class="install-form">
              <div class="input-group">
                <label for="bundlePath">{t("packages.localFile")}</label>
                <div class="row">
                  <input id="bundlePath" bind:value={bundlePath} placeholder="/path/to/plugin.brlp" disabled={busy} />
                  <button class="btn-primary" onclick={inspectInstallFile} disabled={busy || !bundlePath.trim()}>{t("common.inspect")}</button>
                </div>
              </div>

              <div class="input-group">
                <label for="bundleUrl">{t("packages.directUrl")}</label>
                <div class="row">
                  <input id="bundleUrl" bind:value={bundleUrl} placeholder="https://example.com/plugin.brlp" disabled={busy} />
                  <button class="btn-primary" onclick={installFromUrl} disabled={busy || !bundleUrl.trim()}>{t("common.inspect")}</button>
                </div>
              </div>

              <div class="input-group">
                <label for="gitRepo">{t("packages.gitRepo")}</label>
                <div class="row">
                  <input id="gitRepo" bind:value={gitRepo} placeholder="https://github.com/user/repo" disabled={busy} />
                  <input bind:value={gitRev} placeholder={t("packages.gitRevPlaceholder")} disabled={busy} class="input-small" />
                  <button class="btn-primary" onclick={installFromGit} disabled={busy || !gitRepo.trim()}>{t("common.inspect")}</button>
                </div>
              </div>
            </div>
          </section>

          {#if pendingInstall}
            <section class="panel highlight glass" style="margin-top: 24px;">
              <div class="panel-header">
                <h2>{t("packages.confirmInstall")}</h2>
                <span class="badge">{pendingInstall.kind}</span>
              </div>
              <div class="install-info">
                <h3>{pendingInstall.inspection.manifest.name} <span class="version">v{pendingInstall.inspection.manifest.version}</span></h3>
                <p class="id">{pendingInstall.inspection.manifest.id}</p>
                <p class="source">{pendingInstall.label}</p>
              </div>

              <div class="stats-grid">
                <div class="stat-box">
                  <span class="val">{inspectionExportCount(pendingInstall.inspection)}</span>
                  <span class="lbl">{t("common.exports")}</span>
                </div>
                <div class="stat-box">
                  <span class="val">{pendingInstall.inspection.file_count}</span>
                  <span class="lbl">{t("common.files")}</span>
                </div>
                <div class="stat-box">
                  <span class="val">{Math.round(pendingInstall.inspection.uncompressed_size / 1024)} KB</span>
                  <span class="lbl">{t("common.size")}</span>
                </div>
              </div>

              <div class="security-status">
                <div class="status-item">
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class={pendingInstall.inspection.hashes_present ? "good" : "warn"}><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"></path><polyline points="22 4 12 14.01 9 11.01"></polyline></svg>
                  Hashes {pendingInstall.inspection.hashes_present ? "Verified" : "Missing"}
                </div>
                <div class="status-item">
                  <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class={signatureStatus(pendingInstall.inspection) === "verified" ? "good" : "warn"}><rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg>
                  Signature: <span style="text-transform: capitalize">{signatureStatus(pendingInstall.inspection)}</span>
                </div>
              </div>

              <details class="pkg-details mt-10">
                <summary>{t("packages.securityDetails")}</summary>
                <div class="details-content">
                  <div class="permissions-view">
                    <div class="permission-summary">
                      <span class="permission-count">{permissionTotal(pendingInstall.inspection.manifest.permissions)}</span>
                      <span>{t("packages.permissionsRequired")}</span>
                    </div>
                    {#if permissionTotal(pendingInstall.inspection.manifest.permissions) > 0}
                      <div class="permission-grid">
                        {#each permissionSections(pendingInstall.inspection.manifest.permissions) as section}
                          <section class="permission-card">
                            <h4>{section.title}</h4>
                            {#each section.rows as row}
                              <div class="permission-row">
                                <span class="permission-label">{row.label}</span>
                                {#if row.values.length}
                                  <div class="permission-chips">
                                    {#each row.values as value}
                                      <span class="permission-chip">{value}</span>
                                    {/each}
                                  </div>
                                {:else}
                                  <span class="permission-empty">{row.emptyLabel}</span>
                                {/if}
                              </div>
                            {/each}
                          </section>
                        {/each}
                      </div>
                    {:else}
                      <p class="permission-none">{t("packages.noInstallPermissions")}</p>
                    {/if}
                  </div>
                  <h4>{t("packages.bundleSha")}</h4>
                  <pre class="break-all">{pendingInstall.inspection.sha256}</pre>
                  {#if pendingInstall.inspection.signature_public_key}
                    <h4>{t("packages.signaturePublicKey")}</h4>
                    <pre class="break-all">{pendingInstall.inspection.signature_public_key}</pre>
                  {/if}
                </div>
              </details>

              <div class="actions bottom-actions">
                <button class="btn-primary full-width" onclick={confirmPendingInstall} disabled={busy}>{t("packages.installPackage")}</button>
                <button class="btn-secondary full-width" onclick={cancelPendingInstall} disabled={busy}>{t("common.cancel")}</button>
              </div>
            </section>
          {/if}
        </div>
      </div>

    {:else if activeTab === "overlays"}
      <div class="overlays-layout">
        <section class="panel glass obs-help-panel">
          <div class="obs-help-content">
            <div class="obs-help-copy">
              <h2>{t("overlays.obsTitle")}</h2>
              <p class="desc">{t("overlays.obsDesc")}</p>
            </div>
            <div class="obs-url-box">
              <span class="field-label">{t("overlays.generalUrl")}</span>
              <code>{streamUrl()}</code>
              <button class="btn-primary" onclick={() => copyText(streamUrl(), t("overlays.generalUrl"))} disabled={!obsBaseUrl}>
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
                {t("overlays.copyGeneralUrl")}
              </button>
            </div>
          </div>
        </section>

        <section class="panel glass overlay-create-panel">
          <div class="section-header">
            <div>
              <h2>{t("overlays.layoutsTitle")}</h2>
              <p class="desc">{t("overlays.layoutsDesc")}</p>
            </div>
            <div class="control-group">
              <label for="newLayoutName">{t("overlays.createLayout")}</label>
              <div class="row">
                <input id="newLayoutName" bind:value={newLayoutName} placeholder={t("overlays.newLayoutPlaceholder")} />
                <button class="btn-primary" onclick={createOverlayLayout} disabled={busy}>{t("common.create")}</button>
              </div>
            </div>
          </div>
        </section>

        {#if layoutTemplates.length}
          <section class="panel glass page-template-panel">
            <div class="section-header">
              <div>
                <h2>{t("overlays.templateTitle")}</h2>
                <p class="desc">{t("overlays.templateDesc")}</p>
              </div>
            </div>
            <div class="template-grid">
              {#each layoutTemplates as entry}
                <article class="template-card">
                  <div>
                    <h3>{entry.layoutTemplate.title ?? entry.layoutTemplate.name}</h3>
                    <p>{entry.layoutTemplate.description ?? entry.package.name}</p>
                    <span>{entry.package.id}/{entry.layoutTemplate.name}</span>
                  </div>
                  <button class="btn-primary small" onclick={() => importPackageLayout(entry.package.id, entry.layoutTemplate.name)} disabled={busy}>
                    {t("common.import")}
                  </button>
                </article>
              {/each}
            </div>
          </section>
        {/if}

        {#if overlayLayouts?.layouts.length}
          <section class="overlay-layout-list" aria-label={t("overlays.layoutsTitle")}>
            {#each overlayLayouts.layouts as layout (layout.id)}
              <article class="overlay-layout-card" class:expanded={expandedLayoutId === layout.id}>
                <div class="overlay-layout-top">
                  <div class="overlay-summary">
                    <button
                      type="button"
                      class="overlay-expand-btn"
                      aria-label={layout.name}
                      aria-expanded={expandedLayoutId === layout.id}
                      onclick={() => toggleLayoutDetails(layout.id)}
                    >
                      <svg class:rotated={expandedLayoutId === layout.id} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
                    </button>
                    <input
                      class="overlay-name-input"
                      aria-label={t("overlays.layoutName")}
                      value={layout.name}
                      onblur={(event) => handleLayoutNameBlur(layout, event)}
                      onkeydown={handleLayoutNameKeydown}
                    />
                    <span class="layout-badges">
                      {#if isInGameLayout(layout)}
                        <span class="badge route">{t("overlays.ingame")}</span>
                      {/if}
                      {#if isStreamLayout(layout)}
                        <span class="badge route stream">{t("overlays.stream")}</span>
                      {/if}
                      {#if !isInGameLayout(layout) && !isStreamLayout(layout)}
                        <span class="badge muted">{t("common.unassigned")}</span>
                      {/if}
                      {#if layout.template_source}
                        <span class="badge muted">{t("common.imported")}</span>
                      {/if}
                    </span>
                    <span class="overlay-card-stats">
                      {layoutLayerCount(layout)} {t("common.layers")} · {layoutItemCount(layout)} {t("common.plugins")}
                    </span>
                  </div>

                  <div class="layout-actions">
                    <button class="btn-secondary small" onclick={() => routeOverlayLayout(layout.id)} disabled={busy || isInGameLayout(layout)}>
                      {t("overlays.setIngame")}
                    </button>
                    <button class="btn-secondary small" onclick={() => routeOverlayLayout(layout.id, true)} disabled={busy || isStreamLayout(layout)}>
                      {t("overlays.setStream")}
                    </button>
                    <button class="btn-outline small" onclick={() => copyText(layoutUrl(layout.id), t("common.copyUrl"))} disabled={!obsBaseUrl}>
                      {t("common.copyUrl")}
                    </button>
                    <button class="btn-outline small" onclick={() => openPreview(layoutUrl(layout.id))} disabled={!obsBaseUrl}>
                      {t("common.preview")}
                    </button>
                    <button class="btn-primary small" onclick={() => void openLayoutEditor(layout.id)}>
                      {t("common.edit")}
                    </button>
                    <button class="icon-btn danger" onclick={() => deleteLayout(layout)} disabled={busy || (overlayLayouts?.layouts.length ?? 0) <= 1} title={t("confirm.deleteLayoutTitle")}>
                      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                    </button>
                  </div>
                </div>

                {#if expandedLayoutId === layout.id}
                  <div class="overlay-layout-details">
                    {#if layout.template_source}
                      <div class="page-meta-grid">
                        <div>
                          <span class="field-label">{t("common.source")}</span>
                          <strong>{layout.template_source}</strong>
                        </div>
                        <div>
                          <span class="field-label">{t("common.canvas")}</span>
                          <strong>{layout.width}x{layout.height}</strong>
                        </div>
                        <div>
                          <span class="field-label">{t("common.items")}</span>
                          <strong>{layoutItemCount(layout)}</strong>
                        </div>
                      </div>
                    {/if}

                    <div class="layers-grid compact">
                      {#each sortedLayers(layout) as layer}
                        <div class="layer-card">
                          <div class="layer-header">
                            <h3>{layer.name}</h3>
                            {#if layer.kind === "event"}
                              <span class="badge event">{t("overlays.eventLayer")}</span>
                            {/if}
                          </div>

                          {#if layer.items.length === 0}
                            <p class="empty-state small">{t("common.noItems")}</p>
                          {:else}
                            <div class="items-list">
                              {#each layer.items as item}
                                <div class="layout-item">
                                  <div class="item-info">
                                    <span class="item-name">{item.name || `${item.package_id}/${item.export_name}`}</span>
                                    <span class="item-coords">Pos: {Math.round(item.x)},{Math.round(item.y)} · Size: {Math.round(item.width)}x{Math.round(item.height)}</span>
                                  </div>
                                  <button class="icon-btn danger" onclick={() => removeItem(layout, item.id)} disabled={busy || item.locked || layer.locked} title={t("confirm.removeItemTitle")}>
                                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
                                  </button>
                                </div>
                              {/each}
                            </div>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  </div>
                {/if}
              </article>
            {/each}
          </section>
        {:else}
          <div class="empty-state large">
            <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" style="margin-bottom: 1rem; opacity: 0.5;"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect><rect x="7" y="7" width="3" height="9"></rect><rect x="14" y="7" width="3" height="5"></rect></svg>
            <p>{t("overlays.empty")}</p>
          </div>
        {/if}
      </div>

    {:else if activeTab === "pages"}
      <div class="overlays-layout pages-layout">
        <section class="panel glass overlay-create-panel">
          <div class="section-header">
            <div>
              <h2>{t("pages.title")}</h2>
              <p class="desc">{t("pages.desc")}</p>
            </div>
            <div class="control-group">
              <label for="newPageName">{t("pages.createPage")}</label>
              <div class="row">
                <input id="newPageName" bind:value={newPageName} placeholder={t("pages.newPagePlaceholder")} />
                <button class="btn-primary" onclick={createPage} disabled={busy}>{t("common.create")}</button>
              </div>
            </div>
          </div>
        </section>

        {#if pageTemplates.length}
          <section class="panel glass page-template-panel">
            <div class="section-header">
              <div>
                <h2>{t("pages.templatesTitle")}</h2>
                <p class="desc">{t("pages.templatesDesc")}</p>
              </div>
            </div>
            <div class="template-grid">
              {#each pageTemplates as entry}
                <article class="template-card">
                  <div>
                    <h3>{entry.page.title ?? entry.page.name}</h3>
                    <p>{entry.page.description ?? entry.package.name}</p>
                    <span>{entry.package.id}/{entry.page.name}</span>
                  </div>
                  <button class="btn-primary small" onclick={() => importPackagePage(entry.package.id, entry.page.name)} disabled={busy}>
                    {t("common.import")}
                  </button>
                </article>
              {/each}
            </div>
          </section>
        {/if}

        {#if pages?.pages.length}
          <section class="overlay-layout-list page-list" aria-label={t("pages.title")}>
            {#each pages.pages as page (page.id)}
              <article class="overlay-layout-card page-card" class:expanded={expandedPageId === page.id}>
                <div class="overlay-layout-top">
                  <button
                    type="button"
                    class="overlay-summary"
                    aria-expanded={expandedPageId === page.id}
                    onclick={() => togglePageDetails(page.id)}
                  >
                    <span class="overlay-summary-title">
                      <svg class:rotated={expandedPageId === page.id} xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"></polyline></svg>
                      <span class="overlay-name">{page.name}</span>
                    </span>
                    <span class="layout-badges">
                      <span class="badge route">{page.settings.open_target === "window" ? t("pages.window") : t("pages.inApp")}</span>
                      {#if page.favorite}
                        <span class="badge route stream">{t("pages.favoriteBadge")}</span>
                      {/if}
                      {#if page.template_source}
                        <span class="badge muted">{t("common.imported")}</span>
                      {/if}
                    </span>
                    <span class="overlay-card-stats">
                      {page.width}x{page.height} · {page.layers.length} {t("common.layers")} · {pageItemCount(page)} {t("common.items")}
                    </span>
                  </button>

                  <div class="layout-actions">
                    <button
                      class="icon-btn favorite"
                      class:active={page.favorite}
                      onclick={() => void togglePageFavorite(page)}
                      disabled={busy}
                      title={page.favorite ? t("pages.unfavorite") : t("pages.favorite")}
                    >
                      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill={page.favorite ? "currentColor" : "none"} stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon></svg>
                    </button>
                    <button class="btn-primary small" onclick={() => void openPage(page.id)} disabled={busy}>
                      {t("common.open")}
                    </button>
                    <button class="btn-outline small" onclick={() => openPageEditor(page.id)}>
                      {t("common.edit")}
                    </button>
                    <button class="btn-outline small" onclick={() => void duplicatePage(page.id)} disabled={busy}>
                      {t("common.duplicate")}
                    </button>
                    <button class="icon-btn danger" onclick={() => deletePage(page)} disabled={busy} title={t("confirm.deletePageTitle")}>
                      <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
                    </button>
                  </div>
                </div>

                {#if expandedPageId === page.id}
                  <div class="overlay-layout-details">
                    <div class="overlay-detail-controls">
                      <div class="input-group compact">
                        <label for={`pageName-${page.id}`}>{t("pages.title")}</label>
                        <input
                          id={`pageName-${page.id}`}
                          value={page.name}
                          onblur={(event) => handlePageNameBlur(page, event)}
                          onkeydown={handleLayoutNameKeydown}
                        />
                      </div>
                      <div class="input-group compact">
                        <label for={`pageOpenTarget-${page.id}`}>{t("pages.openTarget")}</label>
                        <select
                          id={`pageOpenTarget-${page.id}`}
                          value={page.settings.open_target}
                          onchange={(event) => void updatePageOpenTarget(page, event.currentTarget.value as "app" | "window")}
                        >
                          <option value="app">{t("pages.inApp")}</option>
                          <option value="window">{t("pages.newWindow")}</option>
                        </select>
                      </div>
                    </div>

                    <div class="page-meta-grid">
                      <div>
                          <span class="field-label">{t("common.canvas")}</span>
                        <strong>{page.width}x{page.height}</strong>
                      </div>
                      <div>
                          <span class="field-label">{t("pages.pluginVisuals")}</span>
                        <strong>{pagePluginCount(page)}</strong>
                      </div>
                      <div>
                          <span class="field-label">{t("pages.background")}</span>
                        <strong>{page.background.kind}</strong>
                      </div>
                      <div>
                          <span class="field-label">{t("common.source")}</span>
                          <strong>{page.template_source ?? t("common.custom")}</strong>
                      </div>
                    </div>

                    <div class="layers-grid compact">
                      {#each page.layers as layer}
                        <div class="layer-card">
                          <div class="layer-header">
                            <h3>{layer.name}</h3>
                          </div>
                          {#if layer.items.length === 0}
                            <p class="empty-state small">{t("common.noItems")}</p>
                          {:else}
                            <div class="items-list">
                              {#each layer.items as item}
                                <div class="layout-item">
                                  <div class="item-info">
                                    <span class="item-name">{item.name}</span>
                                    <span class="item-coords">{item.kind} · Pos: {Math.round(item.x)},{Math.round(item.y)} · Size: {Math.round(item.width)}x{Math.round(item.height)}</span>
                                  </div>
                                </div>
                              {/each}
                            </div>
                          {/if}
                        </div>
                      {/each}
                    </div>
                  </div>
                {/if}
              </article>
            {/each}
          </section>
        {:else}
          <div class="empty-state large">
            <p>{t("pages.empty")}</p>
          </div>
        {/if}
      </div>

    {:else if activeTab === "developer"}
      <div class="developer-layout">
        <div class="developer-column">
          <section class="panel glass developer-panel">
            <div class="developer-panel-header">
              <div>
                <h2>{t("developer.telemetryTitle")}</h2>
                <p class="desc">{t("developer.telemetryDesc")}</p>
              </div>
              <div class="developer-header-actions">
                <div class="sort-toggle" aria-label={t("developer.sortLabel")}>
                  <button
                    type="button"
                    class:active={developerTelemetrySort === "recent"}
                    aria-pressed={developerTelemetrySort === "recent"}
                    onclick={() => (developerTelemetrySort = "recent")}
                  >
                    {t("developer.recent")}
                  </button>
                  <button
                    type="button"
                    class:active={developerTelemetrySort === "alpha"}
                    aria-pressed={developerTelemetrySort === "alpha"}
                    onclick={() => (developerTelemetrySort = "alpha")}
                  >
                    A-Z
                  </button>
                </div>
                <span
                  class="telemetry-status"
                  class:connected={telemetryConnected}
                  class:connecting={telemetryStatus?.state === "connecting"}
                  title={telemetryStatus?.message ?? `${telemetryStatus?.host ?? appSettings?.telemetry.rocket_league_host ?? "127.0.0.1"}:${telemetryStatus?.port ?? appSettings?.telemetry.rocket_league_port ?? 49122}`}
                >
                  <span class="status-dot"></span>
                  {telemetryStatusLabel}
                </span>
              </div>
            </div>

            {#if sortedDeveloperTelemetryGroups.length}
              <div class="telemetry-list">
                {#each sortedDeveloperTelemetryGroups as group (group.eventName)}
                  <details class="developer-event">
                    <summary>
                      <span class="developer-event-main">
                        <span class="developer-event-title">
                          <span class="developer-event-name">{group.eventName}</span>
                          <span class="developer-event-count">{group.count} {group.count === 1 ? t("developer.frame") : t("developer.frames")}</span>
                        </span>
                        <span class="developer-event-time">{group.latest.receivedAt}</span>
                      </span>
                    </summary>
                    <pre>{formatJson(group.latest.frame)}</pre>
                  </details>
                {/each}
              </div>
            {:else}
              <div class="empty-state">
                <p>{t("developer.emptyTelemetry")}</p>
              </div>
            {/if}
          </section>

          <section class="panel glass developer-panel">
            <div class="developer-panel-header">
              <div>
                <h2>{t("developer.registryTitle")}</h2>
                <p class="desc">{t("developer.registryDesc")}</p>
              </div>
              <button class="btn-secondary small" onclick={refreshRegistryEntries} disabled={busy}>
                {t("common.refresh")}
              </button>
            </div>

            {#if registryEntries.length}
              <div class="registry-list">
                {#each registryEntries as entry (entry.key)}
                  <details class="registry-entry">
                    <summary>
                      <span class="registry-key">{entry.key}</span>
                    </summary>
                    <pre>{formatJson(entry.value)}</pre>
                  </details>
                {/each}
              </div>
            {:else}
              <div class="empty-state">
                <p>{t("developer.emptyRegistry")}</p>
              </div>
            {/if}
          </section>
        </div>

        <aside class="panel glass developer-panel frame-composer">
          <h2>{t("developer.sendFrameTitle")}</h2>
          <p class="desc">{t("developer.sendFrameDesc")}</p>

          <div class="input-group">
            <label for="developerFrameTemplate">{t("developer.template")}</label>
            <select id="developerFrameTemplate" bind:value={developerFrameTemplate} onchange={loadDeveloperFrameTemplate}>
              {#each RL_TELEMETRY_EVENT_NAMES as template}
                <option value={template}>{template}</option>
              {/each}
            </select>
          </div>

          <div class="input-group frame-editor-group">
            <label for="developerFrameJson">{t("developer.frameJson")}</label>
            <textarea
              id="developerFrameJson"
              class="developer-frame-editor"
              bind:value={developerFrameJson}
              spellcheck="false"
            ></textarea>
          </div>

          <div class="actions bottom-actions horizontal">
            <button class="btn-secondary" onclick={loadDeveloperFrameTemplate} disabled={busy}>{t("common.reset")}</button>
            <button class="btn-primary" onclick={sendDeveloperFrame} disabled={busy}>{t("developer.sendFrame")}</button>
          </div>
        </aside>
      </div>

    {:else if activeTab === "settings" && appSettings}
      <div class="settings-layout">
        <aside class="settings-sidebar panel glass" aria-label={t("nav.settings")}>
          <button
            type="button"
            class="settings-nav-item"
            class:active={settingsSection === "appearance"}
            aria-pressed={settingsSection === "appearance"}
            onclick={() => (settingsSection = "appearance")}
          >
            <span class="nav-title">{t("settings.appearance")}</span>
            <span class="nav-subtitle">{t("settings.appearanceDesc")}</span>
          </button>
          <button
            type="button"
            class="settings-nav-item"
            class:active={settingsSection === "telemetry"}
            aria-pressed={settingsSection === "telemetry"}
            onclick={() => (settingsSection = "telemetry")}
          >
            <span class="nav-title">{t("settings.telemetry")}</span>
            <span class="nav-subtitle">{t("settings.telemetryDesc")}</span>
          </button>
          <button
            type="button"
            class="settings-nav-item"
            class:active={settingsSection === "overlay"}
            aria-pressed={settingsSection === "overlay"}
            onclick={() => (settingsSection = "overlay")}
          >
            <span class="nav-title">{t("settings.overlay")}</span>
            <span class="nav-subtitle">{t("settings.overlayDesc")}</span>
          </button>
        </aside>

        <section class="panel glass settings-panel">
          {#if settingsSection === "appearance"}
            <div class="settings-heading">
              <h2>{t("settings.appearance")}</h2>
            </div>
            <div class="input-group compact language-setting">
              <label for="localeSelect">{t("settings.interfaceLanguage")}</label>
              <select
                id="localeSelect"
                value={locale}
                onchange={(event) => setLocale(event.currentTarget.value as Locale)}
              >
                <option value="fr">Français</option>
                <option value="en">English</option>
              </select>
            </div>
            <div class="theme-grid">
              {#each THEMES as theme}
                <button
                  type="button"
                  class="theme-option"
                  class:active={currentTheme === theme.id}
                  aria-pressed={currentTheme === theme.id}
                  onclick={() => selectTheme(theme.id)}
                  style={`--theme-preview-bg:${theme.preview.background};--theme-preview-surface:${theme.preview.surface};--theme-preview-accent:${theme.preview.accent};--theme-preview-text:${theme.preview.text};`}
                >
                  <span class="theme-preview" aria-hidden="true">
                    <span class="theme-preview-panel"></span>
                    <span class="theme-preview-line"></span>
                    <span class="theme-preview-swatches">
                      <span class="theme-swatch accent"></span>
                      <span class="theme-swatch text"></span>
                    </span>
                  </span>
                  <span class="theme-copy">
                    <span class="theme-name">{theme.label}</span>
                    <span class="theme-description">{theme.description}</span>
                  </span>
                </button>
              {/each}
            </div>
          {:else if settingsSection === "telemetry"}
            <div class="settings-heading">
              <h2>{t("settings.telemetry")}</h2>
              <div class="settings-heading-actions">
                <button type="button" class="btn-secondary small" onclick={() => openTelemetryHelp()}>
                  {t("settings.setupHelp")}
                </button>
                <span
                  class="telemetry-status"
                  class:connected={telemetryConnected}
                  class:connecting={telemetryStatus?.state === "connecting"}
                  title={telemetryStatus?.message ?? `${telemetryStatus?.host ?? appSettings.telemetry.rocket_league_host}:${telemetryStatus?.port ?? appSettings.telemetry.rocket_league_port}`}
                >
                  <span class="status-dot"></span>
                  {telemetryStatusLabel}
                </span>
              </div>
            </div>
            <div class="form-grid">
              <div class="input-group">
                <label for="telemetryHost">{t("settings.host")}</label>
                <input id="telemetryHost" bind:value={appSettings.telemetry.rocket_league_host} />
              </div>
              <div class="input-group">
                <label for="telemetryPort">{t("settings.port")}</label>
                <input id="telemetryPort" type="number" bind:value={appSettings.telemetry.rocket_league_port} />
              </div>
            </div>
            <div class="actions bottom-actions">
              <button class="btn-primary" onclick={saveAppSettings} disabled={busy}>{t("common.saveSettings")}</button>
            </div>
          {:else}
            <div class="settings-heading">
              <h2>{t("settings.overlay")}</h2>
            </div>
            <div class="form-grid">
              <div class="input-group">
                <label for="overlayFps">{t("settings.updateRate")}</label>
                <input id="overlayFps" type="number" min="1" max="120" bind:value={appSettings.overlay.update_rate_fps} />
              </div>
              {#if appSettings.overlay.use_monitor_size}
                <div class="input-group">
                  <label for="overlayMonitor">{t("settings.overlayMonitor")}</label>
                  <select id="overlayMonitor" bind:value={appSettings.overlay.monitor_id}>
                    <option value="">{t("settings.currentPrimary")}</option>
                    {#each overlayMonitors as monitor}
                      <option value={monitor.id}>
                        {monitor.name} · {monitor.width}x{monitor.height}{monitor.primary ? " · primary" : ""}{monitor.current ? " · current" : ""}
                      </option>
                    {/each}
                  </select>
                </div>
              {:else}
                <div class="input-group">
                  <span class="field-label">{t("settings.overlaySize")}</span>
                  <div class="row">
                    <input type="number" min="1" bind:value={appSettings.overlay.screen_width} aria-label="Overlay width" />
                    <input type="number" min="1" bind:value={appSettings.overlay.screen_height} aria-label="Overlay height" />
                  </div>
                </div>
              {/if}
            </div>
            <label class="checkbox-label mt-10">
              <input type="checkbox" bind:checked={appSettings.overlay.use_monitor_size} />
              <span class="checkmark"></span>
              {t("settings.useFullMonitor")}
            </label>
            <label class="checkbox-label mt-10">
              <input type="checkbox" bind:checked={appSettings.overlay.hide_when_game_unfocused} />
              <span class="checkmark"></span>
              {t("settings.hideUnfocused")}
            </label>
            <div class="actions bottom-actions">
              <button class="btn-primary" onclick={saveAppSettings} disabled={busy}>{t("common.saveSettings")}</button>
            </div>
          {/if}
        </section>
      </div>
    {/if}
  </div>
</main>

<style>
  main {
    height: 100vh;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* Header & Navigation */
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 20px 32px;
    background: rgba(15, 17, 21, 0.8);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--border-color);
    position: sticky;
    top: 0;
    z-index: 100;
  }

  .logo-area {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .logo-icon {
    width: 32px;
    height: 32px;
    background: linear-gradient(135deg, var(--accent), #8b5cf6);
    border-radius: 8px;
    box-shadow: 0 4px 12px rgba(59, 130, 246, 0.3);
  }

  h1 {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .subtitle {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
    max-width: 300px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .tab-group,
  .language-switch {
    display: flex;
    align-items: center;
    gap: 2px;
    background: rgba(255, 255, 255, 0.03);
    padding: 4px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-color);
  }

  .tab-group.secondary {
    border-left-color: rgba(255, 255, 255, 0.18);
  }

  .tab-btn {
    background: transparent;
    border: none;
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
    transition: var(--transition);
    white-space: nowrap;
  }

  .tab-btn:hover {
    color: var(--text-primary);
  }

  .tab-btn.active {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-primary);
    box-shadow: 0 1px 3px rgba(0,0,0,0.2);
  }

  .language-switch {
    gap: 0;
  }

  .language-switch button {
    min-width: 34px;
    padding: 7px 9px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 700;
  }

  .language-switch button.active {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-primary);
  }

  /* Main Layout */
  .content-area {
    padding: 32px;
    flex: 1;
    max-width: 1400px;
    margin: 0 auto;
    width: 100%;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    overscroll-behavior: contain;
    scrollbar-gutter: stable;
    box-sizing: border-box;
  }

  .split-layout {
    display: grid;
    grid-template-columns: 1fr 340px;
    gap: 32px;
    align-items: start;
  }

  .home-layout {
    max-width: 1120px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  .home-metric-bar {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0;
    padding: 0;
    overflow: hidden;
  }

  .home-metric {
    min-height: 96px;
    padding: 18px 22px;
    border: 0;
    border-right: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-primary);
    text-align: left;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 5px;
    cursor: pointer;
  }

  .home-metric:last-child {
    border-right: 0;
  }

  .home-metric:hover {
    background: rgba(255, 255, 255, 0.045);
  }

  .home-metric-value {
    font-size: 28px;
    line-height: 1;
    font-weight: 750;
  }

  .home-metric span:not(.home-metric-value):not(.home-metric-note) {
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 700;
  }

  .home-metric-note {
    color: var(--danger);
    font-size: 12px;
  }

  .home-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .home-section-header {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
  }

  .home-section-header h2 {
    margin-bottom: 4px;
  }

  .home-section-header .desc {
    margin-bottom: 0;
  }

  .home-card-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .home-layout-card,
  .home-page-card,
  .home-empty-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    min-width: 0;
    padding: 16px;
  }

  .home-card-copy {
    min-width: 0;
  }

  .home-card-copy h3 {
    margin-top: 4px;
    margin-bottom: 5px;
    font-size: 18px;
  }

  .home-card-copy p {
    margin: 0;
    color: var(--text-muted);
    font-size: 12px;
  }

  .home-card-actions {
    display: flex;
    flex: none;
    gap: 8px;
  }

  .home-empty-row {
    min-height: 88px;
  }

  .home-empty-row p {
    margin: 0;
    color: var(--text-muted);
    font-size: 13px;
  }

  /* Typography & Utilities */
  h2 {
    font-size: 16px;
    font-weight: 600;
    margin: 0 0 8px 0;
    color: var(--text-primary);
  }

  h3 {
    font-size: 14px;
    font-weight: 600;
    margin: 0;
  }

  p {
    margin: 0 0 8px 0;
    color: var(--text-secondary);
    line-height: 1.5;
  }

  .desc {
    font-size: 13px;
    color: var(--text-muted);
    margin-bottom: 16px;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 20px;
  }

  .mt-10 { margin-top: 10px; }
  .break-all { word-break: break-all; white-space: pre-wrap; }

  /* Panels and Cards */
  .panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-lg);
    padding: 24px;
  }

  .panel.glass {
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
  }

  .panel.highlight {
    border-color: rgba(59, 130, 246, 0.3);
    background: linear-gradient(180deg, rgba(59, 130, 246, 0.05) 0%, var(--bg-panel) 100%);
  }

  /* Messages */
  .message-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    background: rgba(59, 130, 246, 0.1);
    border: 1px solid rgba(59, 130, 246, 0.2);
    color: #93c5fd;
    padding: 12px 16px;
    border-radius: var(--radius-md);
    margin-bottom: 24px;
    font-size: 14px;
    animation: slideDown 0.3s ease;
  }

  @keyframes slideDown {
    from { opacity: 0; transform: translateY(-10px); }
    to { opacity: 1; transform: translateY(0); }
  }

  .message-banner p { margin: 0; flex: 1; color: inherit; }
  .close-msg {
    background: none; border: none; color: inherit; font-size: 20px;
    cursor: pointer; padding: 0 4px; opacity: 0.7;
  }
  .close-msg:hover { opacity: 1; }

  .telemetry-help-overlay {
    position: fixed;
    inset: 0;
    z-index: 1900;
    display: grid;
    place-items: center;
    padding: 24px;
  }

  .telemetry-help-scrim {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    border-radius: 0;
    background: rgba(0, 0, 0, 0.58);
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }

  .telemetry-help-dialog {
    position: relative;
    z-index: 1;
    width: min(620px, 100%);
    max-height: min(760px, calc(100vh - 48px));
    overflow: auto;
    padding: 22px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: var(--bg-panel);
    box-shadow: 0 24px 72px rgba(0, 0, 0, 0.5);
  }

  .telemetry-help-copy {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    margin-bottom: 16px;
  }

  .telemetry-help-copy h2 {
    margin: 0;
    color: var(--text-primary);
    font-size: 18px;
  }

  .telemetry-help-copy p {
    margin: 0;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.45;
  }

  .telemetry-help-steps {
    margin: 0;
    padding-left: 22px;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.55;
  }

  .telemetry-help-steps li {
    margin-bottom: 8px;
  }

  .telemetry-help-steps code,
  .telemetry-help-note code {
    padding: 2px 5px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.28);
    color: var(--text-primary);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .telemetry-help-note {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 14px;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.45;
  }

  .telemetry-help-note strong {
    color: var(--text-primary);
  }

  .telemetry-help-check {
    margin-top: 16px;
  }

  .telemetry-help-actions {
    display: flex;
    justify-content: flex-end;
    margin-top: 18px;
  }

  /* Form Elements */
  .input-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }

  .input-group label,
  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  input, select, textarea {
    background: rgba(0, 0, 0, 0.2);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    font-size: 14px;
    font-family: inherit;
    transition: var(--transition);
    flex: 1;
    min-width: 0;
  }

  input:focus, select:focus, textarea:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.2);
  }

  input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .input-small { width: 80px; flex: none; }

  select {
    appearance: none;
    width: 100%;
    padding-right: 28px;
  }

  textarea {
    width: 100%;
    resize: vertical;
    line-height: 1.45;
  }

  /* Buttons */
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font-family: inherit;
    font-size: 14px;
    font-weight: 500;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: var(--transition);
    border: 1px solid transparent;
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    pointer-events: none;
  }

  .btn-primary {
    background: var(--accent);
    color: white;
    padding: 8px 16px;
    box-shadow: 0 2px 8px rgba(59, 130, 246, 0.25);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--accent-hover);
    box-shadow: 0 4px 12px rgba(59, 130, 246, 0.4);
  }

  .btn-secondary {
    background: rgba(255, 255, 255, 0.05);
    border-color: var(--border-color);
    color: var(--text-primary);
    padding: 8px 16px;
  }

  .btn-secondary:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    border-color: var(--border-color-focus);
  }

  .btn-outline {
    background: transparent;
    border-color: var(--border-color);
    color: var(--text-secondary);
    padding: 8px 16px;
  }

  .btn-outline:hover:not(:disabled) {
    border-color: var(--text-primary);
    color: var(--text-primary);
  }

  button.small {
    padding: 6px 12px;
    font-size: 13px;
  }

  .icon-btn {
    background: rgba(255, 255, 255, 0.05);
    border: 1px solid transparent;
    color: var(--text-secondary);
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: var(--radius-sm);
  }

  .icon-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-primary);
  }

  .icon-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.1);
    color: var(--danger);
  }

  .icon-btn.favorite.active,
  .icon-btn.favorite:hover:not(:disabled) {
    background: rgba(250, 204, 21, 0.12);
    border-color: rgba(250, 204, 21, 0.24);
    color: #facc15;
  }

  .full-width { width: 100%; }

  /* Package Grid & Cards */
  .package-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 16px;
  }

  .package-card {
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    padding: 20px;
    transition: var(--transition);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .package-card:hover {
    border-color: var(--border-color-focus);
    background: var(--bg-panel-hover);
    transform: translateY(-2px);
  }

  .package-card.disabled {
    opacity: 0.7;
    filter: grayscale(0.5);
  }

  .package-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }

  .pkg-title-area { display: flex; flex-direction: column; gap: 4px; }
  .pkg-title { font-size: 16px; color: var(--text-primary); margin: 0; }
  .pkg-version { font-size: 12px; color: var(--text-muted); font-family: monospace; background: rgba(0,0,0,0.3); padding: 2px 6px; border-radius: 4px; width: fit-content; }

  .pkg-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .pkg-meta { display: flex; flex-direction: column; gap: 2px; }
  .pkg-id { font-size: 11px; font-family: monospace; color: var(--text-muted); margin: 0; }
  .pkg-author { font-size: 13px; color: var(--text-secondary); margin: 0; }

  .pkg-error {
    font-size: 12px;
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.1);
    padding: 8px;
    border-radius: 4px;
    border-left: 2px solid var(--danger);
  }

  .pkg-stats {
    display: flex;
    gap: 12px;
    background: rgba(0, 0, 0, 0.2);
    padding: 10px 12px;
    border-radius: var(--radius-sm);
  }

  .stat { font-size: 12px; color: var(--text-secondary); }
  .stat-val { font-weight: 600; color: var(--text-primary); }

  .pkg-details {
    font-size: 13px;
    border-top: 1px solid var(--border-color);
    padding-top: 12px;
  }

  .pkg-details summary {
    color: var(--text-secondary);
    cursor: pointer;
    user-select: none;
    font-weight: 500;
  }
  .pkg-details summary:hover { color: var(--text-primary); }

  .details-content {
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .exports-list h4 { margin: 0 0 8px 0; font-size: 12px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .exports-list ul { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 4px; }
  .exports-list li { font-size: 13px; color: var(--text-secondary); display: flex; justify-content: space-between; }
  .exports-list li span { color: var(--text-muted); font-size: 12px; }

  .permissions-view {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .permission-summary {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    width: fit-content;
    padding: 6px 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
  }

  .permission-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    border-radius: 999px;
    background: rgba(59, 130, 246, 0.16);
    color: var(--accent);
    font-size: 12px;
  }

  .permission-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 10px;
  }

  .permission-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.16);
  }

  .permission-card h4 {
    margin: 0;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 700;
  }

  .permission-row {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .permission-label {
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
  }

  .permission-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    min-width: 0;
  }

  .permission-chip {
    max-width: 100%;
    padding: 3px 6px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-secondary);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 11px;
    overflow-wrap: anywhere;
  }

  .permission-empty {
    color: var(--text-muted);
    font-size: 12px;
  }

  .permission-none {
    margin: 0;
    padding: 10px 12px;
    border: 1px dashed var(--border-color);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-size: 12px;
  }

  pre {
    margin: 0;
    padding: 12px;
    background: rgba(0, 0, 0, 0.3);
    border-radius: var(--radius-sm);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 11px;
    color: #a5b4fc;
    overflow-x: auto;
    border: 1px solid rgba(255,255,255,0.05);
  }

  /* Badges */
  .status-badge {
    font-size: 11px;
    font-weight: 600;
    padding: 4px 8px;
    border-radius: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .status-badge.enabled { background: var(--success-bg); color: var(--success); }
  .status-badge.disabled { background: rgba(255, 255, 255, 0.1); color: var(--text-secondary); }

  .badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-secondary);
    text-transform: uppercase;
  }
  .badge.event { background: rgba(245, 158, 11, 0.15); color: var(--warn); }

  /* Install Pending Sidebar */
  .panel-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
  .install-info { margin-bottom: 20px; }
  .install-info h3 { font-size: 18px; margin-bottom: 4px; }
  .install-info .version { font-size: 14px; color: var(--text-muted); font-weight: normal; }
  .install-info .id { font-family: monospace; font-size: 12px; color: var(--text-secondary); margin-bottom: 4px;}
  .install-info .source { font-size: 12px; color: var(--accent); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    margin-bottom: 20px;
  }
  .stat-box {
    background: rgba(0,0,0,0.2);
    padding: 12px 8px;
    border-radius: var(--radius-sm);
    text-align: center;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .stat-box .val { font-size: 16px; font-weight: 600; color: var(--text-primary); }
  .stat-box .lbl { font-size: 11px; color: var(--text-muted); text-transform: uppercase; }

  .security-status {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: rgba(255,255,255,0.02);
    padding: 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-color);
  }
  .status-item {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-secondary);
  }
  .status-item svg.good { color: var(--success); }
  .status-item svg.warn { color: var(--warn); }

  .bottom-actions { margin-top: 24px; display: flex; flex-direction: column; gap: 8px; }

  /* Overlays Layout */
  .overlays-layout {
    display: flex;
    flex-direction: column;
    gap: 18px;
    max-width: 1120px;
    margin: 0 auto;
  }

  .obs-help-panel,
  .overlay-create-panel {
    padding: 20px;
  }

  .obs-help-content {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(300px, 430px);
    gap: 20px;
    align-items: center;
  }

  .obs-help-copy .desc {
    margin-bottom: 0;
  }

  .obs-url-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.16);
  }

  .obs-url-box code {
    display: block;
    min-width: 0;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.28);
    color: var(--text-primary);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .overlay-create-panel .section-header {
    align-items: flex-end;
    gap: 20px;
    margin-bottom: 0;
  }

  .overlay-create-panel .section-header > div:first-child {
    min-width: 240px;
  }

  .overlay-create-panel .desc {
    margin-bottom: 0;
  }

  .control-group { flex: 1; min-width: 200px; }
  .control-group label { display: block; font-size: 12px; font-weight: 500; color: var(--text-secondary); margin-bottom: 8px; }

  .overlay-layout-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .overlay-layout-card {
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    overflow: hidden;
    transition: var(--transition);
  }

  .overlay-layout-card:hover,
  .overlay-layout-card.expanded {
    border-color: var(--border-color-focus);
    background: var(--bg-panel-hover);
  }

  .overlay-layout-top {
    display: grid;
    grid-template-columns: minmax(260px, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 12px 14px;
  }

  .overlay-summary {
    display: grid;
    grid-template-columns: auto minmax(180px, 1fr) auto auto;
    gap: 12px;
    align-items: center;
    min-width: 0;
    padding: 8px 0;
    border: none;
    background: transparent;
    color: var(--text-primary);
    text-align: left;
  }

  .overlay-expand-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-muted);
  }

  .overlay-expand-btn:hover {
    color: var(--text-primary);
  }

  .overlay-expand-btn svg {
    transition: transform 0.18s ease;
  }

  .overlay-expand-btn svg.rotated {
    transform: rotate(90deg);
  }

  .overlay-name-input {
    min-width: 0;
    width: 100%;
    padding: 6px 8px;
    border-color: transparent;
    background: transparent;
    color: var(--text-primary);
    font-size: 15px;
    font-weight: 700;
  }

  .overlay-name-input:hover,
  .overlay-name-input:focus {
    border-color: var(--border-color);
    background: rgba(0, 0, 0, 0.18);
  }

  .layout-badges {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 5px;
  }

  .badge.route {
    background: rgba(59, 130, 246, 0.16);
    color: var(--accent);
  }

  .badge.route.stream {
    background: var(--success-bg);
    color: var(--success);
  }

  .badge.muted {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-muted);
  }

  .overlay-card-stats {
    color: var(--text-muted);
    font-size: 12px;
    white-space: nowrap;
  }

  .layout-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    flex-wrap: wrap;
  }

  .overlay-layout-details {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 0 14px 14px 14px;
    border-top: 1px solid var(--border-color);
  }

  .overlay-detail-controls {
    display: grid;
    grid-template-columns: minmax(180px, 280px) minmax(260px, 1fr);
    gap: 14px;
    padding-top: 14px;
  }

  .input-group.compact {
    margin-bottom: 0;
  }

  .layers-grid {
    display: grid;
    gap: 16px;
  }

  .layers-grid.compact {
    gap: 10px;
  }

  .layer-card {
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    overflow: hidden;
  }

  .layer-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 16px 20px;
    background: rgba(255,255,255,0.02);
    border-bottom: 1px solid var(--border-color);
  }
  .layer-header h3 { margin: 0; font-size: 15px; }

  .layers-grid.compact .layer-header {
    padding: 12px 14px;
  }

  .items-list {
    display: flex;
    flex-direction: column;
  }

  .layout-item {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 20px;
    border-bottom: 1px solid rgba(255,255,255,0.03);
    transition: var(--transition);
  }
  .layout-item:last-child { border-bottom: none; }
  .layout-item:hover { background: rgba(255,255,255,0.02); }

  .layers-grid.compact .layout-item {
    padding: 10px 14px;
  }

  .item-info { display: flex; flex-direction: column; gap: 4px; }
  .item-name { font-size: 14px; font-weight: 500; color: var(--text-primary); }
  .item-coords { font-size: 12px; color: var(--text-muted); font-family: monospace; }

  .page-template-panel {
    padding: 20px;
  }

  .template-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 12px;
  }

  .template-card {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 14px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    background: rgba(0, 0, 0, 0.16);
  }

  .template-card h3 {
    margin: 0 0 4px 0;
    font-size: 15px;
  }

  .template-card p {
    margin: 0 0 8px 0;
    color: var(--text-secondary);
    font-size: 13px;
  }

  .template-card span {
    color: var(--text-muted);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 11px;
  }

  .page-meta-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px;
  }

  .page-meta-grid > div {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    padding: 10px 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.14);
  }

  .page-meta-grid strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }

  /* Developer Layout */
  .developer-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(320px, 420px);
    gap: 24px;
    align-items: stretch;
    max-width: 1180px;
    height: calc(100vh - 176px);
    min-height: 0;
    margin: 0 auto;
  }

  .developer-column {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-direction: column;
    gap: 16px;
  }

  .developer-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .developer-column .developer-panel:first-child {
    flex: 1 1 0;
  }

  .developer-column .developer-panel:last-child {
    flex: 0 1 34%;
    min-height: 168px;
  }

  .developer-panel-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 16px;
  }

  .developer-panel-header .desc {
    margin-bottom: 0;
  }

  .developer-header-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    flex-wrap: wrap;
  }

  .sort-toggle {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(255, 255, 255, 0.04);
  }

  .sort-toggle button {
    padding: 4px 8px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
  }

  .sort-toggle button:hover {
    color: var(--text-primary);
  }

  .sort-toggle button.active {
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-primary);
  }

  .telemetry-list,
  .registry-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1 1 auto;
    min-height: 0;
    max-height: none;
    overflow: auto;
    padding-right: 4px;
  }

  .developer-event,
  .registry-entry {
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: rgba(0, 0, 0, 0.16);
    flex: none;
    overflow: hidden;
  }

  .developer-event summary,
  .registry-entry summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    color: var(--text-secondary);
    cursor: pointer;
    user-select: none;
  }

  .developer-event[open] summary,
  .registry-entry[open] summary {
    border-bottom: 1px solid var(--border-color);
    color: var(--text-primary);
  }

  .developer-event pre,
  .registry-entry pre {
    border: none;
    border-radius: 0;
    background: rgba(0, 0, 0, 0.24);
  }

  .developer-event pre {
    max-height: min(52vh, 520px);
    overflow: auto;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .developer-event-main {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    min-width: 0;
  }

  .developer-event-title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .developer-event-name,
  .registry-key {
    min-width: 0;
    color: var(--text-primary);
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    font-weight: 700;
    overflow-wrap: anywhere;
  }

  .developer-event-count {
    flex: none;
    padding: 2px 6px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
  }

  .developer-event-time {
    flex: none;
    color: var(--text-muted);
    font-size: 11px;
  }

  .frame-composer {
    position: sticky;
    top: 0;
    max-height: 100%;
  }

  .frame-editor-group {
    flex: 1 1 auto;
    min-height: 0;
  }

  .developer-frame-editor {
    flex: 1 1 auto;
    min-height: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    white-space: pre;
  }

  .bottom-actions.horizontal {
    flex-direction: row;
    justify-content: flex-end;
  }

  /* Settings Layout */
  .settings-layout {
    display: grid;
    grid-template-columns: minmax(210px, 250px) minmax(0, 640px);
    align-items: start;
    gap: 20px;
    max-width: 920px;
    margin: 0 auto;
  }

  .settings-sidebar {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    position: sticky;
    top: 0;
  }

  .settings-nav-item {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 5px;
    width: 100%;
    padding: 11px 12px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-secondary);
    text-align: left;
  }

  .settings-nav-item:hover {
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-primary);
  }

  .settings-nav-item.active {
    background: rgba(255, 255, 255, 0.08);
    border-color: var(--border-color-focus);
    color: var(--text-primary);
  }

  .nav-title {
    font-size: 13px;
    font-weight: 700;
    color: inherit;
  }

  .nav-subtitle {
    font-size: 11px;
    line-height: 1.3;
    color: var(--text-muted);
  }

  .settings-panel {
    min-height: 280px;
  }

  .settings-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 16px;
  }
  .settings-heading-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    flex-wrap: wrap;
  }
  .telemetry-status {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    flex: none;
    padding: 4px 9px;
    border: 1px solid var(--border-color);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 650;
  }
  .telemetry-status.connected {
    border-color: color-mix(in srgb, var(--success) 45%, transparent);
    background: var(--success-bg);
    color: var(--success);
  }
  .telemetry-status.connecting {
    border-color: color-mix(in srgb, var(--warn) 45%, transparent);
    background: rgba(245, 158, 11, 0.1);
    color: var(--warn);
  }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: currentColor;
    box-shadow: 0 0 10px currentColor;
  }
  .form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 10px;
  }

  .theme-option {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    gap: 12px;
    align-items: center;
    width: 100%;
    min-height: 76px;
    padding: 10px;
    background: rgba(0, 0, 0, 0.18);
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    text-align: left;
  }

  .theme-option:hover {
    background: rgba(255, 255, 255, 0.05);
    border-color: var(--border-color-focus);
  }

  .theme-option.active {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .theme-preview {
    position: relative;
    display: block;
    width: 52px;
    height: 42px;
    overflow: hidden;
    background: var(--theme-preview-bg);
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: var(--radius-sm);
  }

  .theme-preview-panel {
    position: absolute;
    inset: 8px 7px 7px 7px;
    background: var(--theme-preview-surface);
    border-radius: 3px;
  }

  .theme-preview-line {
    position: absolute;
    left: 12px;
    right: 17px;
    top: 15px;
    height: 3px;
    background: var(--theme-preview-text);
    border-radius: 2px;
    opacity: 0.75;
  }

  .theme-preview-swatches {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: flex;
    gap: 3px;
  }

  .theme-swatch {
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .theme-swatch.accent { background: var(--theme-preview-accent); }
  .theme-swatch.text { background: var(--theme-preview-text); }

  .theme-copy {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  .theme-name {
    overflow: hidden;
    color: var(--text-primary);
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .theme-description {
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.3;
  }

  @media (max-width: 820px) {
    header {
      align-items: stretch;
      flex-direction: column;
    }

    .tabs {
      align-items: stretch;
      flex-direction: column;
    }

    .tab-group,
    .language-switch {
      width: 100%;
      justify-content: stretch;
    }

    .tab-btn,
    .language-switch button {
      flex: 1;
    }

    .home-metric-bar,
    .home-card-grid {
      grid-template-columns: 1fr;
    }

    .home-layout-card,
    .home-page-card,
    .home-empty-row,
    .home-section-header {
      align-items: flex-start;
      flex-direction: column;
    }

    .home-metric {
      border-right: 0;
      border-bottom: 1px solid var(--border-color);
    }

    .home-metric:last-child {
      border-bottom: 0;
    }

    .home-card-actions {
      width: 100%;
    }

    .home-card-actions button {
      flex: 1;
    }

    .settings-layout {
      grid-template-columns: 1fr;
    }

    .settings-sidebar {
      position: static;
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }

    .settings-nav-item {
      min-height: 72px;
    }
  }

  @media (min-width: 821px) and (max-width: 980px) {
    .home-card-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 980px) {
    .obs-help-content,
    .overlay-layout-top,
    .overlay-detail-controls {
      grid-template-columns: 1fr;
    }

    .overlay-summary {
      grid-template-columns: auto minmax(0, 1fr);
      gap: 8px;
    }

    .overlay-summary .layout-badges,
    .overlay-summary .overlay-card-stats {
      grid-column: 1 / -1;
    }

    .layout-badges,
    .layout-actions {
      justify-content: flex-start;
    }

    .developer-layout {
      grid-template-columns: 1fr;
      height: auto;
      min-height: 0;
    }

    .frame-composer {
      position: static;
      max-height: none;
    }

    .developer-frame-editor {
      min-height: 320px;
    }
  }

  /* Checkbox styling */
  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
    font-size: 14px;
    color: var(--text-secondary);
    user-select: none;
  }
  .checkbox-label input {
    position: absolute;
    opacity: 0;
    cursor: pointer;
    height: 0;
    width: 0;
  }
  .checkmark {
    height: 18px;
    width: 18px;
    background-color: rgba(0,0,0,0.3);
    border: 1px solid var(--border-color);
    border-radius: 4px;
    position: relative;
    transition: var(--transition);
  }
  .checkbox-label:hover input ~ .checkmark { border-color: var(--text-secondary); }
  .checkbox-label input:checked ~ .checkmark { background-color: var(--accent); border-color: var(--accent); }
  .checkmark:after {
    content: ""; position: absolute; display: none;
    left: 5px; top: 2px; width: 4px; height: 8px;
    border: solid white; border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }
  .checkbox-label input:checked ~ .checkmark:after { display: block; }

  .empty-state {
    padding: 32px;
    text-align: center;
    color: var(--text-muted);
    background: rgba(0,0,0,0.1);
    border-radius: var(--radius-md);
    border: 1px dashed var(--border-color);
  }
  .empty-state.large { padding: 64px 32px; }
  .empty-state p { margin: 0; }
</style>
