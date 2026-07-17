import { goto } from "$app/navigation";
import { tick } from "svelte";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { open } from "@tauri-apps/plugin-dialog";
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
  ExtensionHostRuntimeStatusEvent,
  MarketplaceInstallPlan,
  MarketplaceInstallResult,
  MarketplaceSnapshot,
  PackageDescriptor,
  PendingInstall,
  PluginDiagnosticEvent,
  PreparedPackageInstall,
  RegistryEntry,
  RuntimeInfo,
  RuntimeErrorEvent,
  RuntimeLogEvent,
  SidecarRuntimeStatusEvent,
  TelemetryConnectionStatus,
  ToastMessage,
  ToastTone
} from "$lib/dashboard/types";

const TELEMETRY_HELP_DISMISSED_KEY = "bakingrl.telemetryHelp.dismissed";
const TELEMETRY_HELP_LAUNCH_SHOWN_KEY = "bakingrl.telemetryHelp.launchShown";
const PACKAGE_TOGGLE_ROLLBACK_MS = 5000;
const PACKAGE_RELOAD_MIN_SPIN_MS = 450;
const PACKAGE_FILE_OPENED_EVENT = "bakingrl-package-files-opened";
const REGISTRY_CHANGED_EVENT = "bakingrl-registry-changed";
const DEVELOPER_TELEMETRY_FLUSH_MS = 500;

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

function parseRuntimeApiVersion(value: string | null | undefined) {
  const parts = typeof value === "string" ? value.split(".").map((part) => Number(part)) : [];
  if (parts.length !== 3 || parts.some((part) => !Number.isInteger(part) || part < 0)) {
    return null;
  }
  return { major: parts[0], minor: parts[1], patch: parts[2] };
}

function minimumSupportedRuntimeApi(value: string | null | undefined) {
  const minimum = typeof value === "string" ? value.split(" - ")[0]?.trim() : null;
  return parseRuntimeApiVersion(minimum);
}

export class DashboardState {
  locale = $state<Locale>("fr");
  packages = $state<PackageDescriptor[]>([]);
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
  runtimeInfo = $state<RuntimeInfo | null>(null);
  telemetryHelpOpen = $state(false);
  telemetryHelpDontShow = $state(false);
  registryEntries = $state<RegistryEntry[]>([]);
  developerTelemetry = $state<DeveloperTelemetryEntry[]>([]);
  developerTelemetryGroups = $state<DeveloperTelemetryGroup[]>([]);
  developerTelemetryBuffer: DeveloperTelemetryEntry[] = [];
  developerTelemetryFlushTimer: ReturnType<typeof setTimeout> | null = null;
  pluginDiagnosticsHydrated = false;
  developerErrors = $state<DeveloperErrorEntry[]>([]);
  developerTelemetrySort = $state<DeveloperTelemetrySort>("arrival");
  developerFrameTemplate = $state<DeveloperFrameTemplate>("UpdateState");
  developerFrameJson = $state(telemetryFrameTemplateJson("UpdateState"));
  marketplaceSnapshot = $state<MarketplaceSnapshot | null>(null);
  marketplaceLoading = $state(false);
  marketplaceError = $state<string | null>(null);
  marketplacePlan = $state<MarketplaceInstallPlan | null>(null);
  marketplaceAcceptedPublishers = $state<string[]>([]);
  marketplaceFirstRunOpen = $state(false);
  marketplaceFirstRunSelection = $state<string[]>([]);
  marketplacePlanFromFirstRun = false;
  toasts = $state<ToastMessage[]>([]);

  get telemetryConnected() {
    return this.telemetryStatus?.state === "connected";
  }

  get telemetryStatusLabel() {
    if (!this.telemetryStatus) return this.t("common.loading");
    if (this.telemetryStatus.state === "connected") return this.t("common.connected");
    if (this.telemetryStatus.state === "connecting") return this.t("common.connecting");
    return this.t("common.disconnected");
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

  get marketplaceFirstRunPackages() {
    const snapshot = this.marketplaceSnapshot;
    if (!snapshot) return [];
    const selected = new Set(snapshot.catalogue.sections.firstRun);
    return snapshot.catalogue.packages.filter((pkg) => selected.has(pkg.id));
  }

  get marketplaceConsentComplete() {
    const accepted = new Set(this.marketplaceAcceptedPublishers);
    return (
      this.marketplacePlan?.publishers.every(
        (publisher) => publisher.trusted || accepted.has(publisher.trustId)
      ) ?? false
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
      severity: "error",
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
    this.appSettings = await invoke<AppSettings>("get_app_settings");
    this.telemetryStatus = await invoke<TelemetryConnectionStatus>("get_telemetry_status");
    const telemetrySnapshot = await invoke<GameEventFrame | null>("get_telemetry_snapshot");
    if (telemetrySnapshot) this.recordTelemetryFrame(telemetrySnapshot);
    this.registryEntries = await invoke<RegistryEntry[]>("registry_entries");
    await this.hydratePluginDiagnostics();
  }

  async hydratePluginDiagnostics() {
    if (this.pluginDiagnosticsHydrated) return;
    const diagnostics = await invoke<PluginDiagnosticEvent[]>("list_plugin_diagnostics");
    for (const diagnostic of diagnostics) {
      const scope = diagnostic.packageId ? `${diagnostic.source}:${diagnostic.packageId}` : diagnostic.source;
      this.recordDeveloperError({
        severity: diagnostic.severity,
        kind: diagnostic.severity,
        source: scope,
        message: `[${diagnostic.phase}] ${diagnostic.message}`,
        timestampMs: diagnostic.timestampMs
      });
    }
    this.pluginDiagnosticsHydrated = true;
  }

  async refreshMarketplace(refresh = false, announceErrors = false) {
    this.marketplaceLoading = true;
    try {
      const snapshot = await invoke<MarketplaceSnapshot>("get_marketplace_snapshot", { refresh });
      this.marketplaceSnapshot = snapshot;
      this.marketplaceError = null;
      if (snapshot.firstRunPending && snapshot.catalogue.sections.firstRun.length > 0) {
        this.marketplaceFirstRunSelection = snapshot.catalogue.sections.firstRun.filter((packageId) =>
          snapshot.catalogue.packages.some((pkg) => pkg.id === packageId)
        );
        this.marketplaceFirstRunOpen = true;
      }
    } catch (error) {
      this.marketplaceError = this.errorMessage(error);
      if (announceErrors) this.notifyError(error);
    } finally {
      this.marketplaceLoading = false;
    }
  }

  toggleMarketplaceFirstRunPackage(packageId: string) {
    const selection = new Set(this.marketplaceFirstRunSelection);
    if (selection.has(packageId)) selection.delete(packageId);
    else selection.add(packageId);
    this.marketplaceFirstRunSelection = [...selection];
  }

  toggleMarketplacePublisher(trustId: string) {
    const accepted = new Set(this.marketplaceAcceptedPublishers);
    if (accepted.has(trustId)) accepted.delete(trustId);
    else accepted.add(trustId);
    this.marketplaceAcceptedPublishers = [...accepted];
  }

  async prepareMarketplaceInstall(packageIds: string[], fromFirstRun = false) {
    if (!packageIds.length) return;
    this.busy = true;
    try {
      const plan = await invoke<MarketplaceInstallPlan>("prepare_marketplace_install", {
        packageIds
      });
      this.marketplacePlan = plan;
      this.marketplacePlanFromFirstRun = fromFirstRun;
      this.marketplaceAcceptedPublishers = plan.publishers
        .filter((publisher) => publisher.trusted)
        .map((publisher) => publisher.trustId);
      if (fromFirstRun) this.marketplaceFirstRunOpen = false;
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async commitMarketplaceInstall() {
    const plan = this.marketplacePlan;
    if (!plan || !this.marketplaceConsentComplete) return;
    this.busy = true;
    try {
      await invoke<MarketplaceInstallResult>("commit_marketplace_install", {
        transactionId: plan.transactionId,
        acceptedPublishers: this.marketplaceAcceptedPublishers
      });
      const completeFirstRun = this.marketplacePlanFromFirstRun;
      this.marketplacePlan = null;
      this.marketplacePlanFromFirstRun = false;
      this.marketplaceAcceptedPublishers = [];
      if (completeFirstRun) {
        await this.completeMarketplaceFirstRun();
      }
      this.setPackagesFromBackend(await invoke<PackageDescriptor[]>("list_packages"));
      this.notify(this.t("msg.marketplaceInstalled"), "success");
    } catch (error) {
      this.notifyError(error);
    } finally {
      this.busy = false;
    }
  }

  async cancelMarketplaceInstall() {
    const plan = this.marketplacePlan;
    const reopenFirstRun = this.marketplacePlanFromFirstRun;
    this.marketplacePlan = null;
    this.marketplacePlanFromFirstRun = false;
    this.marketplaceAcceptedPublishers = [];
    if (reopenFirstRun) this.marketplaceFirstRunOpen = true;
    if (!plan) return;
    try {
      await invoke("discard_marketplace_install", { transactionId: plan.transactionId });
    } catch {
      // Prepared transaction cleanup is best effort.
    }
  }

  async completeMarketplaceFirstRun() {
    try {
      await invoke("complete_marketplace_first_run");
      this.marketplaceFirstRunOpen = false;
      if (this.marketplaceSnapshot) {
        this.marketplaceSnapshot = { ...this.marketplaceSnapshot, firstRunPending: false };
      }
    } catch (error) {
      this.notifyError(error);
    }
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
    const hostVersion = parseRuntimeApiVersion(this.runtimeInfo?.runtimeApiVersion ?? "1.0.0");
    const minVersion = minimumSupportedRuntimeApi(this.runtimeInfo?.supportedRuntimeApi) ?? hostVersion;
    if (!targetVersion || !hostVersion) {
      return "unknown_runtime_api" as const;
    }
    if (
      minVersion &&
      targetVersion.major === hostVersion.major &&
      targetVersion.major === minVersion.major &&
      targetVersion.minor >= minVersion.minor &&
      targetVersion.minor <= hostVersion.minor
    ) {
      return "compatible" as const;
    }
    if (
      targetVersion.major < (minVersion?.major ?? hostVersion.major) ||
      (minVersion && targetVersion.major === minVersion.major && targetVersion.minor < minVersion.minor)
    ) {
      return "incompatible" as const;
    }
    if (targetVersion.major < hostVersion.major) {
      return "incompatible" as const;
    }
    return "requires_newer_host" as const;
  }

  inspectionCompatibilityLabel(inspection: BundleInspection) {
    const status = this.runtimeApiCompatibility(inspection.manifest.bakingrlApi);
    if (status === "compatible") return this.t("packages.compatible");
    if (status === "requires_newer_host") return this.t("packages.requiresNewerHost");
    if (status === "unknown_runtime_api") return this.t("packages.unknownRuntimeApi");
    return this.t("packages.incompatible");
  }

  hasInspectionCompatibilityIssue(inspection: BundleInspection) {
    return this.runtimeApiCompatibility(inspection.manifest.bakingrlApi) !== "compatible";
  }

  inspectionCompatibilityClass(inspection: BundleInspection) {
    const status = this.runtimeApiCompatibility(inspection.manifest.bakingrlApi);
    if (status === "compatible") return "good";
    if (status === "requires_newer_host") return "warn";
    return "danger";
  }

  inspectionCompatibilityMessage(inspection: BundleInspection) {
    const runtimeApi = inspection.manifest.bakingrlApi;
    const hostRange = this.runtimeInfo?.supportedRuntimeApi ?? this.runtimeInfo?.runtimeApiVersion ?? "n/a";
    if (!runtimeApi) return this.t("packages.missingRuntimeApiMessage");
    return this.tx("packages.runtimeApiMessage", { runtimeApi, hostRange });
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
      } else if (sourceKind === "git") {
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

  async saveAppSettings(settings: AppSettings | null = this.appSettings) {
    if (!settings) return false;
    this.busy = true;
    try {
      const nextSettings = JSON.parse(JSON.stringify(settings)) as AppSettings;
      this.appSettings = await invoke<AppSettings>("save_app_settings", { settings: nextSettings });
      this.notify(this.t("msg.settingsSaved"), "success");
      return true;
    } catch (error) {
      this.notifyError(error);
      return false;
    } finally {
      this.busy = false;
    }
  }

  contributionCount(pkg: PackageDescriptor) {
    return (
      pkg.contributions.commands.length +
      pkg.contributions.services.length +
      pkg.contributions.extension_points.length +
      pkg.contributions.contributions.length +
      pkg.contributions.resources.length +
      pkg.contributions.webviews.length
    );
  }

  formatJson(value: unknown) {
    return JSON.stringify(value, null, 2);
  }

  inspectionContributionCount(inspection: BundleInspection) {
    const contributes = inspection.manifest.contributes ?? {};
    return (
      (contributes.commands?.length ?? 0) +
      (contributes.services?.length ?? 0) +
      (contributes.extensionPoints?.length ?? 0) +
      (contributes.contributions?.length ?? 0) +
      (contributes.resources?.length ?? 0) +
      (contributes.webviews?.length ?? 0) +
      (contributes.settings ? 1 : 0)
    );
  }

  signatureStatus(inspection: BundleInspection) {
    if (inspection.signature_verified) return "verified";
    if (inspection.signature_present) return "invalid";
    return "missing";
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
    this.developerTelemetryBuffer.push(entry);
    this.scheduleDeveloperTelemetryFlush();
  }

  scheduleDeveloperTelemetryFlush() {
    if (this.developerTelemetryFlushTimer) return;
    this.developerTelemetryFlushTimer = setTimeout(() => this.flushDeveloperTelemetry(), DEVELOPER_TELEMETRY_FLUSH_MS);
  }

  flushDeveloperTelemetry() {
    const entries = this.developerTelemetryBuffer;
    this.developerTelemetryBuffer = [];
    this.developerTelemetryFlushTimer = null;
    if (!entries.length) return;

    this.developerTelemetry = [...entries].reverse().concat(this.developerTelemetry).slice(0, 80);

    const groups = new Map(this.developerTelemetryGroups.map((group) => [group.eventName, group]));
    for (const entry of entries) {
      const existingGroup = groups.get(entry.eventName);
      groups.set(
        entry.eventName,
        existingGroup
          ? {
              ...existingGroup,
              latest: entry,
              count: existingGroup.count + 1,
              lastReceivedAt: entry.receivedAtMs
            }
          : {
              eventName: entry.eventName,
              latest: entry,
              count: 1,
              lastReceivedAt: entry.receivedAtMs
            }
      );
    }
    this.developerTelemetryGroups = [...groups.values()];
  }

  clearDeveloperTelemetryFlushTimer() {
    if (this.developerTelemetryFlushTimer) {
      clearTimeout(this.developerTelemetryFlushTimer);
      this.developerTelemetryFlushTimer = null;
    }
    this.developerTelemetryBuffer = [];
  }

  errorMessage(error: unknown) {
    return error instanceof Error ? error.message : String(error);
  }

  recordDeveloperError(error: {
    severity?: DeveloperErrorEntry["severity"];
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
      severity: error.severity ?? "error",
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
      severity: "error",
      kind: error.kind || "runtime",
      source: error.source || "Runtime",
      message,
      timestampMs: error.timestamp_ms
    });
    this.notify(`${entry.source}: ${entry.message}`, "error", 8000);
  }

  recordRuntimeLog(log: RuntimeLogEvent) {
    const line = log.line || "";
    this.recordDeveloperError({
      severity: "info",
      kind: log.kind || "log",
      source: log.source || "Runtime",
      message: log.stream ? `[${log.stream}] ${line}` : line
    });
  }

  recordSidecarRuntimeStatus(event: SidecarRuntimeStatusEvent) {
    this.packages = this.packages.map((pkg) => {
      if (pkg.id !== event.packageId) return pkg;
      return {
        ...pkg,
        sidecarStatuses: {
          ...(pkg.sidecarStatuses ?? {}),
          [event.sidecarId]: event.status
        }
      };
    });
  }

  recordExtensionHostRuntimeStatus(event: ExtensionHostRuntimeStatusEvent) {
    this.packages = this.packages.map((pkg) => {
      if (pkg.id !== event.packageId) return pkg;
      return {
        ...pkg,
        extensionHostStatus: event.status
      };
    });
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
    if (!isTauriRuntime()) {
      return () => {
        this.clearPackageToggleTimers();
        this.clearDeveloperTelemetryFlushTimer();
      };
    }
    if (!this.telemetryHelpDismissed() && !this.telemetryHelpShownThisLaunch()) {
      this.markTelemetryHelpShownThisLaunch();
      this.openTelemetryHelp(false);
    }
    void this.refresh().catch((error) => this.notifyError(error));
    void this.refreshMarketplace(true);

    let unlistenPackages: (() => void) | undefined;
    let unlistenDeepLinks: (() => void) | undefined;
    let unlistenPackageFiles: (() => void) | undefined;
    let unlistenTelemetryStatus: (() => void) | undefined;
    let unlistenTelemetry: (() => void) | undefined;
    let unlistenRegistry: (() => void) | undefined;
    let unlistenRuntimeErrors: (() => void) | undefined;
    let unlistenRuntimeLogs: (() => void) | undefined;
    let unlistenExtensionHostStatuses: (() => void) | undefined;
    let unlistenSidecarStatuses: (() => void) | undefined;

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
    void listen<RegistryEntry>(REGISTRY_CHANGED_EVENT, (event) => {
      this.registryEntries = [
        ...this.registryEntries.filter((entry) => entry.key !== event.payload.key),
        event.payload
      ].sort((left, right) => left.key.localeCompare(right.key));
    }).then((unlisten) => {
      unlistenRegistry = unlisten;
    });
    void listen<RuntimeErrorEvent>("bakingrl-runtime-error", (event) => {
      this.recordRuntimeError(event.payload);
    }).then((unlisten) => {
      unlistenRuntimeErrors = unlisten;
    });
    void listen<RuntimeLogEvent>("bakingrl-runtime-log", (event) => {
      this.recordRuntimeLog(event.payload);
    }).then((unlisten) => {
      unlistenRuntimeLogs = unlisten;
    });
    void listen<ExtensionHostRuntimeStatusEvent>("bakingrl-extension-host-runtime-status", (event) => {
      this.recordExtensionHostRuntimeStatus(event.payload);
    }).then((unlisten) => {
      unlistenExtensionHostStatuses = unlisten;
    });
    void listen<SidecarRuntimeStatusEvent>("bakingrl-sidecar-runtime-status", (event) => {
      this.recordSidecarRuntimeStatus(event.payload);
    }).then((unlisten) => {
      unlistenSidecarStatuses = unlisten;
    });

    return () => {
      unlistenPackages?.();
      unlistenDeepLinks?.();
      unlistenPackageFiles?.();
      unlistenTelemetryStatus?.();
      unlistenTelemetry?.();
      unlistenRegistry?.();
      unlistenRuntimeErrors?.();
      unlistenRuntimeLogs?.();
      unlistenExtensionHostStatuses?.();
      unlistenSidecarStatuses?.();
      this.clearPackageToggleTimers();
      this.clearDeveloperTelemetryFlushTimer();
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
