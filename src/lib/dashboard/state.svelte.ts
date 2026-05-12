import { goto } from "$app/navigation";
import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { createLayoutThumbnail } from "$lib/layoutThumbnail";
import { captureRouteReturnState, returnStateQuery, storePendingRouteReturn } from "$lib/returnState";
import {
  RL_TELEMETRY_EVENT_NAMES,
  telemetryFrameTemplateJson,
  type GameEventFrame
} from "$lib/rlTelemetry";
import {
  getInitialLocale,
  storeLocale,
  translations,
  type Locale,
  type TranslationKey
} from "$lib/i18n";
import { applyTheme, DEFAULT_THEME, getStoredTheme, type ThemeId } from "$lib/themes";
import type {
  AppSettings,
  BundleInspection,
  ConfirmRequest,
  DeveloperFrameTemplate,
  DeveloperErrorEntry,
  DeveloperTelemetryEntry,
  DeveloperTelemetryGroup,
  DeveloperTelemetrySort,
  OverlayLayout,
  OverlayLayoutCatalog,
  OverlayMonitor,
  ObsGatewayStatus,
  PackageDescriptor,
  PageLayout,
  PagesFile,
  PendingInstall,
  PermissionSection,
  PermissionShape,
  PreparedPackageInstall,
  RecentActivityEntry,
  RegistryEntry,
  RuntimeInfo,
  RuntimeErrorEvent,
  TelemetryConnectionStatus,
  ToastMessage,
  ToastTone
} from "$lib/dashboard/types";

const TELEMETRY_HELP_DISMISSED_KEY = "bakingrl.telemetryHelp.dismissed";
const TELEMETRY_HELP_LAUNCH_SHOWN_KEY = "bakingrl.telemetryHelp.launchShown";
const PACKAGE_TOGGLE_ROLLBACK_MS = 5000;
const PACKAGE_RELOAD_MIN_SPIN_MS = 450;
const PACKAGE_FILE_OPENED_EVENT = "bakingrl-package-files-opened";

type PendingPackageToggle = {
  enabled: boolean;
  previousEnabled: boolean;
};

function isTauriRuntime() {
  return typeof window !== "undefined" && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);
}

async function waitForNextPaint() {
  await tick();
  if (typeof window === "undefined") return;
  await new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => resolve());
    });
  });
}

function permissionValueList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === "string") : [];
}

function parseRuntimeApiVersion(value: string | null | undefined) {
  const parts = typeof value === "string" ? value.split(".").map((part) => Number(part)) : [];
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part) || part < 0)) {
    return null;
  }
  return { major: parts[0], minor: parts[1], patch: parts[2] };
}

export class DashboardState {
  locale = $state<Locale>("fr");
  packages = $state<PackageDescriptor[]>([]);
  overlayLayouts = $state<OverlayLayoutCatalog | null>(null);
  pages = $state<PagesFile | null>(null);
  appSettings = $state<AppSettings | null>(null);
  bundlePath = $state("");
  bundleUrl = $state("");
  gitRepo = $state("");
  gitRev = $state("");
  busy = $state(false);
  packagesReloading = $state(false);
  pendingPackageToggles = $state<Record<string, PendingPackageToggle>>({});
  packageToggleRollbackTimers = new Map<string, ReturnType<typeof setTimeout>>();
  pendingInstall = $state<PendingInstall | null>(null);
  currentTheme = $state<ThemeId>(DEFAULT_THEME);
  confirmRequest = $state<ConfirmRequest | null>(null);
  telemetryStatus = $state<TelemetryConnectionStatus | null>(null);
  obsGatewayStatus = $state<ObsGatewayStatus | null>(null);
  runtimeInfo = $state<RuntimeInfo | null>(null);
  telemetryHelpOpen = $state(false);
  telemetryHelpDontShow = $state(false);
  overlayMonitors = $state<OverlayMonitor[]>([]);
  registryEntries = $state<RegistryEntry[]>([]);
  developerTelemetry = $state<DeveloperTelemetryEntry[]>([]);
  developerTelemetryGroups = $state<DeveloperTelemetryGroup[]>([]);
  developerErrors = $state<DeveloperErrorEntry[]>([]);
  developerTelemetrySort = $state<DeveloperTelemetrySort>("recent");
  developerFrameTemplate = $state<DeveloperFrameTemplate>("UpdateState");
  developerFrameJson = $state(telemetryFrameTemplateJson("UpdateState"));
  toasts = $state<ToastMessage[]>([]);

  get obsBaseUrl() {
    return this.appSettings ? `http://${this.appSettings.obs.host}:${this.appSettings.obs.port}` : "";
  }

  get obsAccessTokenQuery() {
    const token = this.appSettings?.obs.access_token;
    return token ? `?token=${encodeURIComponent(token)}` : "";
  }

  get telemetryConnected() {
    return this.telemetryStatus?.state === "connected";
  }

  get telemetryStatusLabel() {
    if (!this.telemetryStatus) return this.t("common.loading");
    if (this.telemetryStatus.state === "connected") return this.t("common.connected");
    if (this.telemetryStatus.state === "connecting") return this.t("common.connecting");
    return this.t("common.disconnected");
  }

  get obsGatewayStatusLabel() {
    if (!this.obsGatewayStatus) return this.t("common.loading");
    return this.obsGatewayStatus.running ? this.t("common.running") : this.t("common.stopped");
  }

  get telemetryAddress() {
    const host =
      this.telemetryStatus?.host ?? this.appSettings?.telemetry.rocket_league_host ?? "127.0.0.1";
    const port = this.telemetryStatus?.port ?? this.appSettings?.telemetry.rocket_league_port ?? 49123;
    return `${host}:${port}`;
  }

  get enabledPackageCount() {
    return this.packages.filter((pkg) => pkg.enabled).length;
  }

  get packageErrorCount() {
    return this.packages.filter((pkg) => pkg.error).length;
  }

  get overlayLayoutCount() {
    return this.overlayLayouts?.layouts.length ?? 0;
  }

  get pageCount() {
    return this.pages?.pages.length ?? 0;
  }

  get homeInGameLayout() {
    return (
      this.overlayLayouts?.layouts.find((layout) => layout.id === this.overlayLayouts?.active_layout_id) ??
      null
    );
  }

  get homeStreamLayout() {
    return (
      this.overlayLayouts?.layouts.find((layout) => layout.id === this.overlayLayouts?.stream_layout_id) ??
      null
    );
  }

  get favoritePages() {
    return this.pages?.pages.filter((page) => page.favorite) ?? [];
  }

  get recentActivity(): RecentActivityEntry[] {
    const pageEntries: RecentActivityEntry[] = (this.pages?.pages ?? []).map((page) => ({
      kind: "page",
      id: `page:${page.id}`,
      updatedAtMs: page.updated_at_ms || page.created_at_ms || 0,
      page
    }));
    const layoutEntries: RecentActivityEntry[] = (this.overlayLayouts?.layouts ?? []).map((layout) => ({
      kind: "layout",
      id: `layout:${layout.id}`,
      updatedAtMs: layout.updated_at_ms || layout.created_at_ms || 0,
      layout
    }));
    return [...pageEntries, ...layoutEntries]
      .sort((a, b) => b.updatedAtMs - a.updatedAtMs || a.id.localeCompare(b.id))
      .slice(0, 3);
  }

  get pageTemplates() {
    return this.packages
      .filter((pkg) => pkg.enabled)
      .flatMap((pkg) =>
        pkg.exports.pages.map((page) => ({
          package: pkg,
          page
        }))
      );
  }

  get layoutTemplates() {
    return this.packages
      .filter((pkg) => pkg.enabled)
      .flatMap((pkg) =>
        pkg.exports.layouts.map((layoutTemplate) => ({
          package: pkg,
          layoutTemplate
        }))
      );
  }

  get sortedDeveloperTelemetryGroups() {
    return [...this.developerTelemetryGroups].sort((a, b) => {
      if (this.developerTelemetrySort === "alpha") {
        return a.eventName.localeCompare(b.eventName);
      }
      return b.lastReceivedAt - a.lastReceivedAt || a.eventName.localeCompare(b.eventName);
    });
  }

  t(key: TranslationKey) {
    return translations[this.locale][key];
  }

  tx(key: TranslationKey, values: Record<string, string | number>) {
    return Object.entries(values).reduce(
      (text, [name, value]) => text.replaceAll(`{${name}}`, String(value)),
      this.t(key)
    );
  }

  setLocale(nextLocale: Locale) {
    this.locale = nextLocale;
    storeLocale(nextLocale);
  }

  selectTheme(themeId: ThemeId) {
    this.currentTheme = applyTheme(themeId);
  }

  notify(message: string, tone: ToastTone = "info", timeout = 4200) {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
    this.toasts = [...this.toasts, { id, tone, message }];
    if (typeof window !== "undefined") {
      window.setTimeout(() => this.dismissToast(id), timeout);
    }
  }

  notifyError(error: unknown) {
    const message = this.errorMessage(error);
    this.recordDeveloperError({
      kind: "app",
      source: "Dashboard",
      message
    });
    this.notify(message, "error", 6400);
  }

  dismissToast(id: string) {
    this.toasts = this.toasts.filter((toast) => toast.id !== id);
  }

  askConfirmation(request: ConfirmRequest) {
    this.confirmRequest = request;
  }

  cancelConfirmation() {
    this.confirmRequest = null;
  }

  async confirmAction() {
    const request = this.confirmRequest;
    this.confirmRequest = null;
    await request?.run();
  }

  setPackagesFromBackend(packages: PackageDescriptor[]) {
    this.packages = packages;
    this.reconcilePackageToggles(packages);
  }

  reconcilePackageToggles(packages: PackageDescriptor[]) {
    for (const [packageId, pending] of Object.entries(this.pendingPackageToggles)) {
      const actual = packages.find((pkg) => pkg.id === packageId);
      if (!actual || actual.status === "deleting" || actual.enabled === pending.enabled) {
        this.clearPackageToggle(packageId);
      }
    }
  }

  clearPackageToggle(packageId: string) {
    const timer = this.packageToggleRollbackTimers.get(packageId);
    if (timer) {
      clearTimeout(timer);
      this.packageToggleRollbackTimers.delete(packageId);
    }
    if (!this.pendingPackageToggles[packageId]) return;
    const remaining = { ...this.pendingPackageToggles };
    delete remaining[packageId];
    this.pendingPackageToggles = remaining;
  }

  clearPackageToggleTimers() {
    for (const timer of this.packageToggleRollbackTimers.values()) {
      clearTimeout(timer);
    }
    this.packageToggleRollbackTimers.clear();
  }

  schedulePackageToggleRollback(packageId: string) {
    const existing = this.packageToggleRollbackTimers.get(packageId);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      const pending = this.pendingPackageToggles[packageId];
      if (!pending) return;
      this.clearPackageToggle(packageId);
    }, PACKAGE_TOGGLE_ROLLBACK_MS);
    this.packageToggleRollbackTimers.set(packageId, timer);
  }

  async refresh() {
    this.runtimeInfo = await invoke<RuntimeInfo>("get_runtime_info");
    this.setPackagesFromBackend(await invoke<PackageDescriptor[]>("list_packages"));
    this.overlayLayouts = await invoke<OverlayLayoutCatalog>("get_overlay_layouts");
    this.pages = await invoke<PagesFile>("get_pages");
    this.appSettings = await invoke<AppSettings>("get_app_settings");
    this.telemetryStatus = await invoke<TelemetryConnectionStatus>("get_telemetry_status");
    this.obsGatewayStatus = await invoke<ObsGatewayStatus>("get_obs_gateway_status");
    this.overlayMonitors = await invoke<OverlayMonitor[]>("list_overlay_monitors");
    this.registryEntries = await invoke<RegistryEntry[]>("registry_entries");
  }

  async reloadPackages() {
    const startedAtMs = Date.now();
    this.busy = true;
    this.packagesReloading = true;
    await waitForNextPaint();
    try {
      this.setPackagesFromBackend(await invoke<PackageDescriptor[]>("reload_packages"));
      this.notify(this.t("msg.packagesReloaded"), "success");
    } catch (error) {
      this.notifyError(error);
    } finally {
      const remainingMs = PACKAGE_RELOAD_MIN_SPIN_MS - (Date.now() - startedAtMs);
      if (remainingMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, remainingMs));
      }
      this.packagesReloading = false;
      this.busy = false;
    }
  }

  async togglePackage(pkg: PackageDescriptor) {
    if (this.isPackageDeleting(pkg)) return;
    const previousEnabled = pkg.enabled;
    const nextEnabled = !previousEnabled;
    if (nextEnabled && !this.isPackageCompatible(pkg)) {
      this.notify(pkg.compatibility.message ?? this.t("packages.incompatiblePackage"), "warning", 6400);
      return;
    }
    this.pendingPackageToggles = {
      ...this.pendingPackageToggles,
      [pkg.id]: {
        enabled: nextEnabled,
        previousEnabled
      }
    };
    this.schedulePackageToggleRollback(pkg.id);
    try {
      this.setPackagesFromBackend(
        await invoke<PackageDescriptor[]>("set_package_enabled", {
          packageId: pkg.id,
          enabled: nextEnabled
        })
      );
    } catch (error) {
      this.notifyError(error);
    }
  }

  isPackageEnabled(pkg: PackageDescriptor) {
    return pkg.status !== "deleting" && pkg.enabled;
  }

  isPackageToggleButtonEnabled(pkg: PackageDescriptor) {
    return this.pendingPackageToggles[pkg.id]?.enabled ?? pkg.enabled;
  }

  isPackageTogglePending(pkg: PackageDescriptor) {
    return Object.prototype.hasOwnProperty.call(this.pendingPackageToggles, pkg.id);
  }

  isPackageDeleting(pkg: PackageDescriptor) {
    return pkg.status === "deleting";
  }

  packageStateClass(pkg: PackageDescriptor) {
    if (this.isPackageDeleting(pkg)) return "connecting";
    return this.isPackageEnabled(pkg) ? "connected" : "disconnected";
  }

  packageStateLabel(pkg: PackageDescriptor) {
    if (this.isPackageDeleting(pkg)) return this.t("common.deleting");
    return this.isPackageEnabled(pkg) ? this.t("common.enabled") : this.t("common.disabled");
  }

  isPackageActionDisabled(pkg: PackageDescriptor) {
    return this.busy || this.isPackageDeleting(pkg);
  }

  isPackageToggleDisabled(pkg: PackageDescriptor) {
    return this.isPackageActionDisabled(pkg) || (!pkg.enabled && !this.isPackageCompatible(pkg));
  }

  isPackageCompatible(pkg: PackageDescriptor) {
    return pkg.compatibility.status === "compatible";
  }

  hasPackageCompatibilityIssue(pkg: PackageDescriptor) {
    return !this.isPackageCompatible(pkg);
  }

  packageDisplayStateClass(pkg: PackageDescriptor) {
    if (!this.isPackageDeleting(pkg) && this.hasPackageCompatibilityIssue(pkg)) {
      return this.packageCompatibilityClass(pkg);
    }
    return this.packageStateClass(pkg);
  }

  packageDisplayStateLabel(pkg: PackageDescriptor) {
    if (!this.isPackageDeleting(pkg) && this.hasPackageCompatibilityIssue(pkg)) {
      return this.packageCompatibilityLabel(pkg);
    }
    return this.packageStateLabel(pkg);
  }

  packageDisplayStateTitle(pkg: PackageDescriptor) {
    if (!this.isPackageDeleting(pkg) && this.hasPackageCompatibilityIssue(pkg)) {
      return pkg.compatibility.message ?? "";
    }
    return "";
  }

  packageCompatibilityClass(pkg: PackageDescriptor) {
    if (pkg.compatibility.status === "compatible") return "connected";
    if (pkg.compatibility.status === "requires_newer_host") return "connecting";
    return "disconnected";
  }

  packageCompatibilityLabel(pkg: PackageDescriptor) {
    if (pkg.compatibility.status === "compatible") return this.t("packages.compatible");
    if (pkg.compatibility.status === "requires_newer_host") return this.t("packages.requiresNewerHost");
    if (pkg.compatibility.status === "unknown_runtime_api") return this.t("packages.unknownRuntimeApi");
    return this.t("packages.incompatible");
  }

  runtimeApiCompatibility(runtimeApi: string | null | undefined) {
    const targetVersion = parseRuntimeApiVersion(runtimeApi);
    const hostVersion = parseRuntimeApiVersion(this.runtimeInfo?.runtimeApiVersion ?? "0.4.0");
    if (!targetVersion || !hostVersion) {
      return "unknown_runtime_api" as const;
    }
    if (targetVersion.major === hostVersion.major && targetVersion.minor === hostVersion.minor) {
      return "compatible" as const;
    }
    if (
      targetVersion.major < hostVersion.major ||
      (targetVersion.major === hostVersion.major && targetVersion.minor < hostVersion.minor)
    ) {
      return "incompatible" as const;
    }
    return "requires_newer_host" as const;
  }

  inspectionCompatibilityLabel(inspection: BundleInspection) {
    const status = this.runtimeApiCompatibility(inspection.manifest.compatibility?.runtimeApi);
    if (status === "compatible") return this.t("packages.compatible");
    if (status === "requires_newer_host") return this.t("packages.requiresNewerHost");
    if (status === "unknown_runtime_api") return this.t("packages.unknownRuntimeApi");
    return this.t("packages.incompatible");
  }

  hasInspectionCompatibilityIssue(inspection: BundleInspection) {
    return this.runtimeApiCompatibility(inspection.manifest.compatibility?.runtimeApi) !== "compatible";
  }

  inspectionCompatibilityClass(inspection: BundleInspection) {
    const status = this.runtimeApiCompatibility(inspection.manifest.compatibility?.runtimeApi);
    if (status === "compatible") return "good";
    if (status === "requires_newer_host") return "warn";
    return "danger";
  }

  inspectionCompatibilityMessage(inspection: BundleInspection) {
    const runtimeApi = inspection.manifest.compatibility?.runtimeApi;
    const sdk = inspection.manifest.compatibility?.sdk;
    const hostRange = this.runtimeInfo?.supportedRuntimeApi ?? ">=0.4.0 <0.5.0";
    if (!runtimeApi) return this.t("packages.missingRuntimeApiMessage");
    const sdkPart = sdk ? ` · SDK ${sdk}` : "";
    return this.tx("packages.runtimeApiMessage", { runtimeApi, hostRange }) + sdkPart;
  }

  removePackage(pkg: PackageDescriptor) {
    if (this.isPackageDeleting(pkg)) return;
    this.askConfirmation({
      title: this.t("confirm.removePackageTitle"),
      message: this.tx("confirm.removePackageMessage", { name: pkg.name }),
      confirmLabel: this.t("common.remove"),
      danger: true,
      run: () => this.removePackageConfirmed(pkg)
    });
  }

  async removePackageConfirmed(pkg: PackageDescriptor) {
    if (this.isPackageDeleting(pkg)) return;
    try {
      this.setPackagesFromBackend(
        await invoke<PackageDescriptor[]>("remove_package", {
          packageId: pkg.id
        })
      );
      this.notify(this.t("msg.packageRemovalStarted"), "info");
    } catch (error) {
      this.notifyError(error);
    }
  }

  async inspectInstallFile() {
    if (!this.bundlePath.trim()) return;
    await this.preparePackageFileInstall(this.bundlePath.trim(), false);
  }

  async preparePackageFileInstall(path: string, navigateToPlugins = true) {
    const normalizedPath = path.trim();
    if (!normalizedPath) return;
    this.busy = true;
    try {
      if (navigateToPlugins) {
        await this.navigate("/plugins");
      }
      this.bundlePath = normalizedPath;
      const inspection = await invoke<BundleInspection>("inspect_package_bundle", { path: normalizedPath });
      await this.setPendingInstall({
        kind: "file",
        label: normalizedPath,
        path: normalizedPath,
        source: `file:${normalizedPath}`,
        inspection
      });
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async takePendingPackageFileOpens() {
    try {
      const paths = await invoke<string[]>("take_pending_package_file_opens");
      await this.handlePackageFileOpenPaths(paths);
    } catch (error) {
      this.notifyError(error);
    }
  }

  async handlePackageFileOpenPaths(paths: string[] | null) {
    const path = paths?.find((entry) => typeof entry === "string" && entry.trim())?.trim();
    if (!path) return;
    await this.preparePackageFileInstall(path, true);
  }

  async chooseInstallFile() {
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "BakingRL Package",
            extensions: ["brlp"]
          }
        ]
      });
      if (typeof selected === "string") {
        this.bundlePath = selected;
      }
    } catch (error) {
      this.notifyError(error);
    }
  }

  async installFromUrl() {
    if (!this.bundleUrl.trim()) return;
    this.busy = true;
    try {
      const url = this.bundleUrl.trim();
      const prepared = await invoke<PreparedPackageInstall>("prepare_package_from_url", { url });
      await this.setPendingInstall({ kind: "url", label: url, ...prepared });
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async installFromGit() {
    if (!this.gitRepo.trim()) return;
    this.busy = true;
    try {
      const repo = this.gitRepo.trim();
      const rev = this.gitRev.trim() || null;
      const prepared = await invoke<PreparedPackageInstall>("prepare_package_from_git", { repo, rev });
      await this.setPendingInstall({
        kind: "git",
        label: rev ? `${repo}#${rev}` : repo,
        ...prepared
      });
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async confirmPendingInstall() {
    if (!this.pendingInstall) return;
    this.busy = true;
    try {
      const sourceKind = this.pendingInstall.kind;
      await invoke("install_prepared_package", {
        path: this.pendingInstall.path,
        source: this.pendingInstall.source
      });
      this.pendingInstall = null;
      if (sourceKind === "file") {
        this.bundlePath = "";
      } else if (sourceKind === "url") {
        this.bundleUrl = "";
      } else {
        this.gitRepo = "";
        this.gitRev = "";
      }
      await this.refresh();
      this.notify(
        sourceKind === "file"
          ? this.t("msg.installedFile")
          : sourceKind === "url"
            ? this.t("msg.installedUrl")
            : this.t("msg.installedGit"),
        "success"
      );
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async setPendingInstall(next: PendingInstall) {
    const previous = this.pendingInstall;
    this.pendingInstall = next;
    if (previous && previous.kind !== "file" && previous.path !== next.path) {
      try {
        await invoke("discard_prepared_package", { path: previous.path });
      } catch {
        // Cleanup is best effort; it should not block replacing the review modal.
      }
    }
  }

  async cancelPendingInstall() {
    const install = this.pendingInstall;
    this.pendingInstall = null;
    if (!install || install.kind === "file") return;
    try {
      await invoke("discard_prepared_package", { path: install.path });
    } catch {
      // Best effort cleanup.
    }
  }

  async createOverlayLayout(options: { name?: string; width?: number; height?: number } = {}) {
    this.busy = true;
    try {
      const previousIds = new Set((this.overlayLayouts?.layouts ?? []).map((layout) => layout.id));
      const createdCatalog = await invoke<OverlayLayoutCatalog>("create_overlay_layout", {
        name: options.name?.trim() || this.t("overlays.untitled"),
        width: Math.max(320, Math.round(Number(options.width) || 1920)),
        height: Math.max(240, Math.round(Number(options.height) || 1080))
      });
      this.overlayLayouts = await this.saveCreatedLayoutThumbnail(createdCatalog, previousIds);
      this.notify(this.t("msg.overlaySaved"), "success");
      return true;
    } catch (error) {
      this.notifyError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  async saveCreatedLayoutThumbnail(catalog: OverlayLayoutCatalog, previousIds: Set<string>) {
    const created =
      catalog.layouts.find((layout) => !previousIds.has(layout.id)) ??
      catalog.layouts.find((layout) => layout.id === catalog.active_layout_id);
    if (!created) return catalog;
    return await invoke<OverlayLayoutCatalog>("save_overlay_layout", {
      layout: {
        ...created,
        thumbnail: createLayoutThumbnail(created, { kind: "overlay" })
      }
    });
  }

  async saveLayout(layout: OverlayLayout) {
    this.busy = true;
    try {
      this.overlayLayouts = await invoke<OverlayLayoutCatalog>("save_overlay_layout", { layout });
      this.notify(this.t("msg.overlaySaved"), "success");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  layoutItemCount(layout: OverlayLayout) {
    return (layout.layers ?? []).reduce((total, layer) => total + layer.items.length, 0);
  }

  layoutLayerCount(layout: OverlayLayout) {
    return layout.layers?.length ?? 0;
  }

  isInGameLayout(layout: OverlayLayout) {
    return this.overlayLayouts?.active_layout_id === layout.id;
  }

  isStreamLayout(layout: OverlayLayout) {
    return this.overlayLayouts?.stream_layout_id === layout.id;
  }

  async renameLayout(layout: OverlayLayout, name: string) {
    const trimmed = name.trim();
    if (!trimmed || trimmed === layout.name) return;
    await this.saveLayout({ ...layout, name: trimmed });
  }

  deleteLayout(layout: OverlayLayout) {
    if ((this.overlayLayouts?.layouts.length ?? 0) <= 1) {
      this.notify(this.t("msg.overlayRequired"), "warning");
      return;
    }
    this.askConfirmation({
      title: this.t("confirm.deleteLayoutTitle"),
      message: this.tx("confirm.deleteLayoutMessage", { name: layout.name }),
      confirmLabel: this.t("common.delete"),
      danger: true,
      run: () => this.deleteLayoutConfirmed(layout.id)
    });
  }

  async deleteLayoutConfirmed(layoutId: string) {
    this.busy = true;
    try {
      this.overlayLayouts = await invoke<OverlayLayoutCatalog>("delete_overlay_layout", { layoutId });
      this.notify(this.t("msg.overlayDeleted"), "success");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async routeOverlayLayout(layoutId: string, stream = false) {
    this.busy = true;
    try {
      this.overlayLayouts = await invoke<OverlayLayoutCatalog>(
        stream ? "set_stream_overlay_layout" : "set_active_overlay_layout",
        { layoutId }
      );
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  sortedLayers(layout: OverlayLayout) {
    return [...(layout.layers ?? [])].sort((a, b) => {
      if (a.kind === "event" && b.kind !== "event") return 1;
      if (a.kind !== "event" && b.kind === "event") return -1;
      return a.order - b.order;
    });
  }

  layoutUrl(layoutId: string) {
    return `${this.obsBaseUrl}/overlay/layout/${encodeURIComponent(layoutId)}${this.obsAccessTokenQuery}`;
  }

  streamUrl() {
    return `${this.obsBaseUrl}/overlay/stream${this.obsAccessTokenQuery}`;
  }

  async copyText(value: string, label: string) {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
      this.notify(`${label} ${this.t("msg.copied")}`, "success");
    } catch (error) {
      this.notifyError(error);
    }
  }

  async openPreview(value: string) {
    if (!value) return;
    try {
      await openUrl(value);
    } catch (error) {
      const popup = window.open(value, "_blank", "noopener,noreferrer");
      if (!popup) this.notifyError(error);
    }
  }

  editorReturnQuery() {
    return returnStateQuery(captureRouteReturnState());
  }

  async openLayoutEditor(layoutId: string) {
    await this.navigate(`/editor/layout/${encodeURIComponent(layoutId)}${this.editorReturnQuery()}`);
  }

  async importPackageLayout(packageId: string, exportName: string) {
    this.busy = true;
    try {
      this.overlayLayouts = await invoke<OverlayLayoutCatalog>("import_package_layout", {
        packageId,
        exportName
      });
      this.notify(this.t("msg.overlayImported"), "success");
      await this.navigate("/overlays");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async createPage(options: { name?: string; openTarget?: "app" | "window"; width?: number; height?: number } = {}) {
    this.busy = true;
    try {
      const previousIds = new Set((this.pages?.pages ?? []).map((page) => page.id));
      const createdPages = await invoke<PagesFile>("create_page", {
        name: options.name?.trim() || this.t("pages.untitled"),
        openTarget: options.openTarget ?? "app",
        width: Math.max(320, Math.round(Number(options.width) || 1440)),
        height: Math.max(240, Math.round(Number(options.height) || 900))
      });
      this.pages = await this.saveCreatedPageThumbnail(createdPages, previousIds);
      this.notify(this.t("msg.pageSaved"), "success");
      return true;
    } catch (error) {
      this.notifyError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  async saveCreatedPageThumbnail(pagesFile: PagesFile, previousIds: Set<string>) {
    const created =
      pagesFile.pages.find((page) => !previousIds.has(page.id)) ??
      [...pagesFile.pages].sort((a, b) => b.created_at_ms - a.created_at_ms)[0];
    if (!created) return pagesFile;
    return await invoke<PagesFile>("save_page", {
      page: {
        ...created,
        thumbnail: createLayoutThumbnail(created, { kind: "page" })
      }
    });
  }

  async savePage(page: PageLayout) {
    this.busy = true;
    try {
      this.pages = await invoke<PagesFile>("save_page", { page });
      this.notify(this.t("msg.pageSaved"), "success");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  pageItemCount(page: PageLayout) {
    return page.layers.reduce((total, layer) => total + layer.items.length, 0);
  }

  pagePluginCount(page: PageLayout) {
    return page.layers.reduce(
      (total, layer) => total + layer.items.filter((item) => item.kind === "visual").length,
      0
    );
  }

  async renamePage(page: PageLayout, name: string) {
    const trimmed = name.trim();
    if (!trimmed || trimmed === page.name) return;
    await this.savePage({ ...page, name: trimmed });
  }

  async updatePageOpenTarget(page: PageLayout, openTarget: "app" | "window") {
    await this.savePage({ ...page, settings: { ...page.settings, open_target: openTarget } });
  }

  async togglePageFavorite(page: PageLayout) {
    await this.savePage({ ...page, favorite: !page.favorite });
  }

  async duplicatePage(pageId: string) {
    this.busy = true;
    try {
      this.pages = await invoke<PagesFile>("duplicate_page", { pageId });
      this.notify(this.t("msg.pageDuplicated"), "success");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  deletePage(page: PageLayout) {
    this.askConfirmation({
      title: this.t("confirm.deletePageTitle"),
      message: this.tx("confirm.deletePageMessage", { name: page.name }),
      confirmLabel: this.t("common.delete"),
      danger: true,
      run: () => this.deletePageConfirmed(page.id)
    });
  }

  async deletePageConfirmed(pageId: string) {
    this.busy = true;
    try {
      this.pages = await invoke<PagesFile>("delete_page", { pageId });
      this.notify(this.t("msg.pageDeleted"), "success");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async importPackagePage(packageId: string, exportName: string) {
    this.busy = true;
    try {
      this.pages = await invoke<PagesFile>("import_package_page", {
        packageId,
        exportName
      });
      this.notify(this.t("msg.pageImported"), "success");
      await this.navigate("/pages");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async openPage(pageId: string) {
    const returnState = captureRouteReturnState();
    storePendingRouteReturn(returnState);
    try {
      await invoke("open_page", { pageId });
    } catch {
      await this.navigate(`/page/${encodeURIComponent(pageId)}${returnStateQuery(returnState)}`);
    }
  }

  async openPageEditor(pageId: string) {
    await this.navigate(`/editor/page/${encodeURIComponent(pageId)}${this.editorReturnQuery()}`);
  }

  async saveAppSettings(settings: AppSettings | null = this.appSettings) {
    if (!settings) return false;
    this.busy = true;
    try {
      const nextSettings = JSON.parse(JSON.stringify(settings)) as AppSettings;
      nextSettings.overlay.monitor_id = nextSettings.overlay.monitor_id || null;
      this.appSettings = await invoke<AppSettings>("save_app_settings", { settings: nextSettings });
      this.overlayMonitors = await invoke<OverlayMonitor[]>("list_overlay_monitors");
      this.notify(this.t("msg.settingsSaved"), "success");
      return true;
    } catch (error) {
      this.notifyError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  exportCount(pkg: PackageDescriptor) {
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

  formatJson(value: unknown) {
    return JSON.stringify(value, null, 2);
  }

  inspectionExportCount(inspection: BundleInspection) {
    const exports = inspection.manifest.exports;
    return (
      Object.keys(exports.visuals ?? {}).length +
      Object.keys(exports.components ?? {}).length +
      Object.keys(exports.services ?? {}).length +
      Object.keys(exports.connectors ?? {}).length +
      Object.keys(exports.assets ?? {}).length +
      Object.keys(exports.schemas ?? {}).length +
      Object.keys(exports.pages ?? {}).length +
      Object.keys(exports.layouts ?? {}).length +
      (exports.configuration ? 1 : 0)
    );
  }

  signatureStatus(inspection: BundleInspection) {
    if (inspection.signature_verified) return "verified";
    if (inspection.signature_present) return "invalid";
    return "missing";
  }

  permissionSections(permissions: PermissionShape | null | undefined): PermissionSection[] {
    const bus = permissions?.bus ?? {};
    const registry = permissions?.registry ?? {};
    const network = permissions?.network ?? {};
    const storage = permissions?.storage;
    const storageRead = Array.isArray(storage) ? storage : storage?.read;
    const storageWrite = Array.isArray(storage) ? storage : storage?.write;

    return [
      {
        title: this.t("permissions.telemetryBus"),
        rows: [
          { label: this.t("permissions.readEvents"), values: permissionValueList(bus.read), emptyLabel: this.t("permissions.noReadEvents") },
          { label: this.t("permissions.publishEvents"), values: permissionValueList(bus.publish), emptyLabel: this.t("permissions.noPublishEvents") }
        ]
      },
      {
        title: this.t("permissions.registry"),
        rows: [
          { label: this.t("permissions.readKeys"), values: permissionValueList(registry.read), emptyLabel: this.t("permissions.noReadKeys") },
          { label: this.t("permissions.writeKeys"), values: permissionValueList(registry.write), emptyLabel: this.t("permissions.noWriteKeys") }
        ]
      },
      {
        title: this.t("permissions.network"),
        rows: [
          { label: this.t("permissions.httpHosts"), values: permissionValueList(network.http), emptyLabel: this.t("permissions.noHttp") },
          { label: this.t("permissions.websocketHosts"), values: permissionValueList(network.websocket), emptyLabel: this.t("permissions.noWebsocket") }
        ]
      },
      {
        title: this.t("permissions.storage"),
        rows: [
          { label: this.t("permissions.readStorage"), values: permissionValueList(storageRead), emptyLabel: this.t("permissions.noReadStorage") },
          { label: this.t("permissions.writeStorage"), values: permissionValueList(storageWrite), emptyLabel: this.t("permissions.noWriteStorage") }
        ]
      }
    ];
  }

  permissionTotal(permissions: PermissionShape | null | undefined) {
    return this.permissionSections(permissions).reduce(
      (total, section) => total + section.rows.reduce((rowTotal, row) => rowTotal + row.values.length, 0),
      0
    );
  }

  loadDeveloperFrameTemplate() {
    this.developerFrameJson = telemetryFrameTemplateJson(this.developerFrameTemplate);
  }

  recordTelemetryFrame(frame: GameEventFrame) {
    const receivedAtMs = Date.now();
    const entry: DeveloperTelemetryEntry = {
      id: `${receivedAtMs}-${Math.random().toString(36).slice(2)}`,
      receivedAt: new Date().toLocaleTimeString(),
      receivedAtMs,
      eventName: frame.Event,
      frame
    };
    this.developerTelemetry = [entry, ...this.developerTelemetry].slice(0, 80);
    const existingGroup = this.developerTelemetryGroups.find((group) => group.eventName === entry.eventName);
    if (!existingGroup) {
      this.developerTelemetryGroups = [
        ...this.developerTelemetryGroups,
        {
          eventName: entry.eventName,
          latest: entry,
          count: 1,
          lastReceivedAt: entry.receivedAtMs
        }
      ];
      return;
    }
    this.developerTelemetryGroups = this.developerTelemetryGroups.map((group) =>
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

  errorMessage(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  recordDeveloperError(error: {
    kind?: string;
    source?: string;
    message: string;
    timestampMs?: number;
  }) {
    const receivedAtMs = error.timestampMs ?? Date.now();
    const entry: DeveloperErrorEntry = {
      id: `${receivedAtMs}-${Math.random().toString(36).slice(2)}`,
      receivedAt: new Date(receivedAtMs).toLocaleTimeString(),
      receivedAtMs,
      kind: error.kind || "runtime",
      source: error.source || "BakingRL",
      message: error.message
    };
    this.developerErrors = [entry, ...this.developerErrors].slice(0, 120);
    return entry;
  }

  recordRuntimeError(error: RuntimeErrorEvent) {
    const message = error.message || this.t("developer.unknownError");
    const entry = this.recordDeveloperError({
      kind: error.kind || "runtime",
      source: error.source || "Runtime",
      message,
      timestampMs: error.timestamp_ms
    });
    this.notify(`${entry.source}: ${entry.message}`, "error", 8000);
  }

  clearDeveloperErrors() {
    this.developerErrors = [];
  }

  async sendDeveloperFrame() {
    try {
      const parsed = JSON.parse(this.developerFrameJson) as Partial<GameEventFrame>;
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
      this.notify(`${this.t("msg.developerFrameSent")} (${frame.Event})`, "success");
    } catch (error) {
      this.notifyError(error);
    }
  }

  telemetryHelpDismissed() {
    try {
      return localStorage.getItem(TELEMETRY_HELP_DISMISSED_KEY) === "true";
    } catch {
      return false;
    }
  }

  openTelemetryHelp(useStoredChoice = true) {
    this.telemetryHelpDontShow = useStoredChoice ? this.telemetryHelpDismissed() : false;
    this.telemetryHelpOpen = true;
  }

  closeTelemetryHelp() {
    try {
      if (this.telemetryHelpDontShow) {
        localStorage.setItem(TELEMETRY_HELP_DISMISSED_KEY, "true");
      } else {
        localStorage.removeItem(TELEMETRY_HELP_DISMISSED_KEY);
      }
    } catch {
      // The help can still close for this session.
    }
    this.telemetryHelpOpen = false;
  }

  async prepareDeepLinkInstall(deepLink: string) {
    this.busy = true;
    await this.navigate("/plugins");
    try {
      const prepared = await invoke<PreparedPackageInstall>("prepare_package_from_deep_link", {
        deepLink
      });
      const label = prepared.source.startsWith("deeplink:")
        ? prepared.source.slice("deeplink:".length)
        : deepLink;
      await this.setPendingInstall({ kind: "url", label, ...prepared });
    } catch (error) {
      this.notify(`${this.t("msg.deepLinkRejected")}: ${String(error)}`, "error", 6400);
    } finally {
      this.busy = false;
    }
  }

  async handleDeepLinkUrls(urls: string[] | null) {
    if (!urls?.length) return;
    await this.prepareDeepLinkInstall(urls[0]);
  }

  async navigate(path: string) {
    try {
      await goto(path);
    } catch {
      window.location.href = path;
    }
  }

  start() {
    this.locale = getInitialLocale();
    this.currentTheme = applyTheme(getStoredTheme());
    if (!this.telemetryHelpDismissed() && !this.telemetryHelpShownThisLaunch()) {
      this.markTelemetryHelpShownThisLaunch();
      this.openTelemetryHelp(false);
    }
    void this.refresh().catch((error) => this.notifyError(error));

    let unlistenPackages: (() => void) | undefined;
    let unlistenOverlays: (() => void) | undefined;
    let unlistenPages: (() => void) | undefined;
    let unlistenDeepLinks: (() => void) | undefined;
    let unlistenPackageFiles: (() => void) | undefined;
    let unlistenTelemetryStatus: (() => void) | undefined;
    let unlistenTelemetry: (() => void) | undefined;
    let unlistenRuntimeErrors: (() => void) | undefined;

    if (isTauriRuntime() && getCurrentWindow().label === "main") {
      void getCurrent()
        .then((urls) => this.handleDeepLinkUrls(urls))
        .catch((error) => {
          this.notify(`${this.t("msg.deepLinkUnavailable")}: ${String(error)}`, "warning", 6400);
        });
      void onOpenUrl((urls) => {
        void this.handleDeepLinkUrls(urls);
      })
        .then((unlisten) => {
          unlistenDeepLinks = unlisten;
        })
        .catch((error) => {
          this.notify(`${this.t("msg.deepLinkListenerUnavailable")}: ${String(error)}`, "warning", 6400);
        });
      void listen(PACKAGE_FILE_OPENED_EVENT, () => {
        void this.takePendingPackageFileOpens();
      })
        .then((unlisten) => {
          unlistenPackageFiles = unlisten;
          void this.takePendingPackageFileOpens();
        })
        .catch((error) => {
          this.notifyError(error);
        });
    }

    void listen<PackageDescriptor[]>("bakingrl-packages-changed", (event) => {
      this.setPackagesFromBackend(event.payload);
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    void listen<OverlayLayoutCatalog>("bakingrl-overlay-layouts-changed", (event) => {
      this.overlayLayouts = event.payload;
    }).then((unlisten) => {
      unlistenOverlays = unlisten;
    });
    void listen<PagesFile>("bakingrl-pages-changed", (event) => {
      this.pages = event.payload;
    }).then((unlisten) => {
      unlistenPages = unlisten;
    });
    void listen<TelemetryConnectionStatus>("bakingrl-telemetry-status", (event) => {
      this.telemetryStatus = event.payload;
    }).then((unlisten) => {
      unlistenTelemetryStatus = unlisten;
    });
    void listen<GameEventFrame>("bakingrl-telemetry", (event) => {
      const payload = event.payload;
      this.recordTelemetryFrame({
        Event: typeof payload?.Event === "string" ? payload.Event : "Unknown",
        Data: payload?.Data ?? payload
      });
    }).then((unlisten) => {
      unlistenTelemetry = unlisten;
    });
    void listen<RuntimeErrorEvent>("bakingrl-runtime-error", (event) => {
      this.recordRuntimeError(event.payload);
    }).then((unlisten) => {
      unlistenRuntimeErrors = unlisten;
    });

    return () => {
      unlistenPackages?.();
      unlistenOverlays?.();
      unlistenPages?.();
      unlistenDeepLinks?.();
      unlistenPackageFiles?.();
      unlistenTelemetryStatus?.();
      unlistenTelemetry?.();
      unlistenRuntimeErrors?.();
      this.clearPackageToggleTimers();
    };
  }

  telemetryHelpShownThisLaunch() {
    try {
      return sessionStorage.getItem(TELEMETRY_HELP_LAUNCH_SHOWN_KEY) === "true";
    } catch {
      return false;
    }
  }

  markTelemetryHelpShownThisLaunch() {
    try {
      sessionStorage.setItem(TELEMETRY_HELP_LAUNCH_SHOWN_KEY, "true");
    } catch {
      // Session storage is optional in browser fallback contexts.
    }
  }
}

export const telemetryFrameTemplates = RL_TELEMETRY_EVENT_NAMES;

export function createDashboardState() {
  return new DashboardState();
}
