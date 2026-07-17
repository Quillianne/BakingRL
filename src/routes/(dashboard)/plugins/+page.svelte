<script lang="ts">
  import { tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    ArrowUpRight,
    Ellipsis,
    ExternalLink,
    FolderOpen,
    LockKeyhole,
    Power,
    RefreshCw,
    ScanSearch,
    Search,
    Settings2,
    Trash2,
    X
  } from "@lucide/svelte";
  import { getDashboardContext } from "$lib/dashboard/context";
  import {
    packageCategories,
    packageConfigurationIsPrimary,
    resolvePackagePrimaryAction
  } from "$lib/dashboard/packagePresentation";
  import type {
    ExtensionHostRuntimeStatus,
    MarketplacePackage,
    MarketplacePackageVersion,
    PackageDependencyDescriptor,
    PackageDescriptor,
    PluginRuntimeSidecarDescriptor,
    SidecarRuntimeStatus
  } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();

  type PackageContributionRow = {
    name: string;
    meta?: string;
    webviewId?: string;
  };

  type PackageContributionSection = {
    title: string;
    count: number;
    rows: PackageContributionRow[];
  };

  type RelationStateClass = "connected" | "connecting" | "disconnected";
  type PluginView = "marketplace" | "installed" | "updates";

  type PackageRelationRow = {
    name: string;
    meta?: string;
    status: string;
    statusClass: RelationStateClass;
  };

  let detailPackageId = $state<string | null>(null);
  let pluginView = $state<PluginView>("installed");
  let marketplaceQuery = $state("");
  let installedQuery = $state("");
  let installedCategory = $state<string | null>(null);
  let detailDialog = $state<HTMLDivElement | null>(null);
  let detailOpener: HTMLElement | null = null;
  const detailPackage = $derived(dashboard.packages.find((pkg) => pkg.id === detailPackageId) ?? null);
  const installedCategories = $derived(
    [...new Set(dashboard.packages.flatMap((pkg) => packageCategories(pkg)))].sort((left, right) =>
      packageCategoryLabel(left).localeCompare(packageCategoryLabel(right))
    )
  );
  const filteredInstalledPackages = $derived(
    dashboard.packages.filter((pkg) => installedPackageMatches(pkg, installedQuery, installedCategory))
  );
  const filteredMarketplacePackages = $derived(
    (dashboard.marketplaceSnapshot?.catalogue.packages ?? []).filter((pkg) =>
      marketplacePackageMatches(pkg, marketplaceQuery)
    )
  );
  const marketplaceUpdates = $derived(
    filteredMarketplacePackages.filter((pkg) => isMarketplaceUpdate(pkg))
  );
  const displayedMarketplacePackages = $derived(
    pluginView === "updates" ? marketplaceUpdates : filteredMarketplacePackages
  );

  $effect(() => {
    if (installedCategory && !installedCategories.includes(installedCategory)) {
      installedCategory = null;
    }
  });

  $effect(() => {
    if (!detailPackage) return;
    void tick().then(() => detailDialog?.focus());
  });

  function versionParts(value: string) {
    const match = /^(\d+)\.(\d+)\.(\d+)/.exec(value);
    return match ? match.slice(1).map(Number) : [0, 0, 0];
  }

  function compareVersions(left: string, right: string) {
    const leftParts = versionParts(left);
    const rightParts = versionParts(right);
    for (let index = 0; index < 3; index += 1) {
      const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
      if (difference !== 0) return difference;
    }
    return left.localeCompare(right);
  }

  function latestMarketplaceVersion(pkg: MarketplacePackage): MarketplacePackageVersion | null {
    return (
      [...pkg.versions]
        .filter(
          (version) =>
            version.status === "active" &&
            version.channel === "stable" &&
            runtimeApiSupported(version.runtimeApi)
        )
        .sort((left, right) => compareVersions(right.version, left.version))[0] ?? null
    );
  }

  function installedMarketplacePackage(pkg: MarketplacePackage) {
    return dashboard.packages.find((installed) => installed.id === pkg.id) ?? null;
  }

  function runtimeApiSupported(value: string) {
    const declared = versionParts(value);
    const [minimumRaw, maximumRaw] = (dashboard.runtimeInfo?.supportedRuntimeApi ?? "2.3.0 - 2.4.0")
      .split(" - ")
      .map((part) => part.trim());
    const minimum = versionParts(minimumRaw ?? "2.3.0");
    const maximum = versionParts(maximumRaw ?? "2.4.0");
    const declaredMinor = (declared[0] ?? 0) * 1000 + (declared[1] ?? 0);
    const minimumMinor = (minimum[0] ?? 0) * 1000 + (minimum[1] ?? 0);
    const maximumMinor = (maximum[0] ?? 0) * 1000 + (maximum[1] ?? 0);
    return declaredMinor >= minimumMinor && declaredMinor <= maximumMinor;
  }

  function isMarketplaceUpdate(pkg: MarketplacePackage) {
    const installed = installedMarketplacePackage(pkg);
    const latest = latestMarketplaceVersion(pkg);
    return Boolean(installed && latest && compareVersions(latest.version, installed.version) > 0);
  }

  function marketplacePackageMatches(pkg: MarketplacePackage, query: string) {
    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return true;
    const listing = pkg.listing.snapshot;
    return [listing.displayName, listing.shortDescription, pkg.id, ...listing.tags]
      .join(" ")
      .toLocaleLowerCase()
      .includes(normalized);
  }

  function marketplaceDeveloper(pkg: MarketplacePackage) {
    return dashboard.marketplaceSnapshot?.catalogue.developers.find(
      (developer) => developer.id === pkg.developerId
    );
  }

  function marketplaceVerificationLabel(pkg: MarketplacePackage) {
    const verification = marketplaceDeveloper(pkg)?.verification ?? "unverified";
    if (verification === "official") return dashboard.t("marketplace.official");
    if (verification === "verified") return dashboard.t("marketplace.verified");
    return dashboard.t("marketplace.unverified");
  }

  function marketplaceActionLabel(pkg: MarketplacePackage) {
    const latest = latestMarketplaceVersion(pkg);
    const installed = installedMarketplacePackage(pkg);
    if (!latest || pkg.status !== "active") return dashboard.t("marketplace.unavailable");
    if (!installed) return dashboard.t("marketplace.install");
    if (compareVersions(latest.version, installed.version) > 0) return dashboard.t("marketplace.update");
    return dashboard.t("marketplace.installed");
  }

  function marketplaceActionDisabled(pkg: MarketplacePackage) {
    const latest = latestMarketplaceVersion(pkg);
    const installed = installedMarketplacePackage(pkg);
    return (
      dashboard.busy ||
      !dashboard.marketplaceSnapshot?.installable ||
      pkg.status !== "active" ||
      !latest ||
      Boolean(installed && compareVersions(latest.version, installed.version) <= 0)
    );
  }

  function openPackageDetails(pkg: PackageDescriptor) {
    detailOpener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    detailPackageId = pkg.id;
  }

  function closePackageDetails() {
    const opener = detailOpener;
    detailPackageId = null;
    detailOpener = null;
    void tick().then(() => opener?.focus());
  }

  function handlePackageDetailKeydown(event: KeyboardEvent) {
    if (!detailPackage) return;
    if (event.key === "Escape") {
      event.preventDefault();
      closePackageDetails();
      return;
    }
    if (event.key !== "Tab" || !detailDialog) return;

    const focusable = Array.from(
      detailDialog.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])'
      )
    ).filter((element) => !element.hidden && element.getAttribute("aria-hidden") !== "true");
    if (!focusable.length) {
      event.preventDefault();
      detailDialog.focus();
      return;
    }

    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    const active = document.activeElement;
    if (event.shiftKey && (active === first || active === detailDialog)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  async function openPackageConfiguration(pkg: PackageDescriptor) {
    try {
      await invoke("open_package_configuration", { packageId: pkg.id });
    } catch (error) {
      dashboard.notifyError(error);
    }
  }

  async function openPackageSecrets(pkg: PackageDescriptor) {
    try {
      await invoke("open_package_secrets", { packageId: pkg.id });
    } catch (error) {
      dashboard.notifyError(error);
    }
  }

  function installedPackageMatches(pkg: PackageDescriptor, query: string, category: string | null) {
    const categories = packageCategories(pkg);
    if (category && !categories.includes(category)) return false;

    const normalized = query.trim().toLocaleLowerCase();
    if (!normalized) return true;
    return [
      pkg.name,
      pkg.id,
      pkg.author ?? "",
      ...categories,
      ...categories.map((item) => packageCategoryLabel(item))
    ]
      .join(" ")
      .toLocaleLowerCase()
      .includes(normalized);
  }

  function packageCategoryLabel(category: string) {
    switch (category) {
      case "layouts":
        return dashboard.t("packages.categoryLayouts");
      case "broadcast":
        return dashboard.t("packages.categoryBroadcast");
      case "statistics":
        return dashboard.t("packages.categoryStatistics");
      case "integration":
        return dashboard.t("packages.categoryIntegration");
      case "visuals":
        return dashboard.t("packages.categoryVisuals");
      case "development":
        return dashboard.t("packages.categoryDevelopment");
      case "utility":
        return dashboard.t("packages.categoryUtility");
      default:
        return category;
    }
  }

  function packagePrimaryActionLabel(pkg: PackageDescriptor) {
    const action = resolvePackagePrimaryAction(pkg);
    return dashboard.tx(action?.configuration ? "packages.configurePlugin" : "packages.openPlugin", {
      name: pkg.name
    });
  }

  function packageSidecars(pkg: PackageDescriptor) {
    return pkg.runtime?.sidecars ?? [];
  }

  function sidecarStatus(pkg: PackageDescriptor, sidecarId: string): SidecarRuntimeStatus | null {
    return pkg.sidecarStatuses?.[sidecarId] ?? null;
  }

  function sidecarRuntimeClass(status: SidecarRuntimeStatus | null) {
    if (!status) return "connecting";
    if (status.running && status.healthy !== false) return "connected";
    if (status.running) return "disconnected";
    if (status.crashCount > 0 || status.healthy === false) return "disconnected";
    return "connecting";
  }

  function sidecarRuntimeLabel(status: SidecarRuntimeStatus | null) {
    if (!status) return dashboard.t("packages.sidecarNotObserved");
    if (status.running) return dashboard.t("common.running");
    if (status.crashCount > 0) return dashboard.t("packages.sidecarCrashed");
    return dashboard.t("common.stopped");
  }

  function sidecarHealthLabel(status: SidecarRuntimeStatus | null) {
    if (!status || status.healthy == null) return dashboard.t("packages.sidecarHealthUnknown");
    return status.healthy ? dashboard.t("packages.sidecarHealthy") : dashboard.t("packages.sidecarUnhealthy");
  }

  function sidecarRuntimeTitle(sidecar: PluginRuntimeSidecarDescriptor, status: SidecarRuntimeStatus | null) {
    return `${sidecar.id}: ${sidecarRuntimeLabel(status)} · ${sidecarHealthLabel(status)}`;
  }

  function sidecarLastHealthCheck(status: SidecarRuntimeStatus | null) {
    if (!status?.lastHealthCheckMs) return "n/a";
    return new Date(status.lastHealthCheckMs).toLocaleTimeString();
  }

  function sidecarLastExitCode(status: SidecarRuntimeStatus | null) {
    if (!status || status.lastExitCode == null) return "n/a";
    return String(status.lastExitCode);
  }

  function extensionHostRuntimeStatus(pkg: PackageDescriptor): ExtensionHostRuntimeStatus | null {
    return pkg.extensionHostStatus ?? null;
  }

  function extensionHostRuntimeClass(status: ExtensionHostRuntimeStatus | null): RelationStateClass {
    if (!status) return "connecting";
    if (status.running && status.state === "running") return "connected";
    if (status.state === "crashed" || status.crashCount > 0 || status.lastError) return "disconnected";
    return "connecting";
  }

  function extensionHostRuntimeLabel(status: ExtensionHostRuntimeStatus | null) {
    if (!status) return dashboard.t("packages.extensionHostNotObserved");
    if (status.running && status.state === "running") return dashboard.t("common.running");
    if (status.state === "crashed" || status.crashCount > 0) return dashboard.t("packages.extensionHostCrashed");
    if (status.state === "starting") return dashboard.t("common.connecting");
    if (status.state === "stopping") return dashboard.t("packages.runtimeStopping");
    return dashboard.t("common.stopped");
  }

  function dependencyStatusClass(dependency: PackageDependencyDescriptor): RelationStateClass {
    if (dependency.status === "satisfied") return "connected";
    if (dependency.status === "pending" || dependency.status === "optional_missing") return "connecting";
    return "disconnected";
  }

  function dependencyStatusLabel(dependency: PackageDependencyDescriptor) {
    switch (dependency.status) {
      case "satisfied":
        return dashboard.t("packages.dependencySatisfied");
      case "optional_missing":
        return dashboard.t("packages.dependencyOptionalMissing");
      case "missing":
        return dashboard.t("packages.dependencyMissing");
      case "disabled":
        return dashboard.t("packages.dependencyDisabled");
      case "incompatible":
        return dashboard.t("packages.dependencyIncompatible");
      case "version_mismatch":
        return dashboard.t("packages.dependencyVersionMismatch");
      default:
        return dashboard.t("packages.dependencyPending");
    }
  }

  function packageLabel(packageId: string) {
    const pkg = dashboard.packages.find((candidate) => candidate.id === packageId);
    return pkg ? `${pkg.name} · ${pkg.id}` : packageId;
  }

  function dependencyMeta(dependency: PackageDependencyDescriptor) {
    const parts = [
      dependency.version ?? dashboard.t("packages.anyVersion"),
      dependency.optional ? dashboard.t("packages.optionalDependency") : dashboard.t("packages.requiredDependency")
    ];
    if (dependency.message) parts.push(dependency.message);
    return parts.join(" · ");
  }

  function dependencyRows(pkg: PackageDescriptor): PackageRelationRow[] {
    return pkg.dependencies.map((dependency) => ({
      name: packageLabel(dependency.package_id),
      meta: dependencyMeta(dependency),
      status: dependencyStatusLabel(dependency),
      statusClass: dependencyStatusClass(dependency)
    }));
  }

  function dependentRows(pkg: PackageDescriptor): PackageRelationRow[] {
    return dashboard.packages
      .filter((candidate) => candidate.id !== pkg.id)
      .flatMap((candidate) =>
        candidate.dependencies
          .filter((dependency) => dependency.package_id === pkg.id)
          .map((dependency) => ({
            name: `${candidate.name} · ${candidate.id}`,
            meta: dependencyMeta(dependency),
            status: dependencyStatusLabel(dependency),
            statusClass: dependencyStatusClass(dependency)
          }))
      );
  }

  function targetPackageId(target: string) {
    return target.split("/", 1)[0] ?? "";
  }

  function incomingContributionRows(pkg: PackageDescriptor): PackageRelationRow[] {
    return dashboard.packages
      .filter((candidate) => candidate.id !== pkg.id)
      .flatMap((provider) =>
        provider.contributions.contributions
          .filter((contribution) => targetPackageId(contribution.target) === pkg.id)
          .map((contribution) => ({
            name: `${contribution.title ?? contribution.name} · ${provider.name}`,
            meta: contribution.kind
              ? `${contribution.target} · ${contribution.kind}`
              : contribution.target,
            status: dashboard.packageDisplayStateLabel(provider),
            statusClass: dashboard.packageDisplayStateClass(provider) as RelationStateClass
          }))
      );
  }

  async function openPackageWebview(pkg: PackageDescriptor, webviewId: string) {
    try {
      await invoke("open_package_webview", { packageId: pkg.id, webviewId });
    } catch (error) {
      dashboard.notifyError(error);
    }
  }

  async function openPrimaryPackageAction(pkg: PackageDescriptor) {
    const action = resolvePackagePrimaryAction(pkg);
    if (!action) return;
    if (action.kind === "webview") {
      await openPackageWebview(pkg, action.target);
      return;
    }
    await openPackageConfiguration(pkg);
  }

  function primaryPackageActionDisabled(pkg: PackageDescriptor) {
    const action = resolvePackagePrimaryAction(pkg);
    if (!action) return true;
    if (dashboard.busy || dashboard.isPackageTogglePending(pkg) || dashboard.isPackageDeleting(pkg)) {
      return true;
    }
    if (action.kind === "settings") return !pkg.has_public_settings;
    return !dashboard.isPackageEnabled(pkg) || !dashboard.isPackageCompatible(pkg);
  }

  function packageContributionSections(pkg: PackageDescriptor): PackageContributionSection[] {
    const sections: PackageContributionSection[] = [
      {
        title: dashboard.t("packages.services"),
        count: pkg.contributions.services.length,
        rows: pkg.contributions.services.map((service) => ({
          name: service.name,
          meta: `${service.methods.length} ${dashboard.t("packages.methods")}`
        }))
      },
      {
        title: dashboard.t("packages.extensionPoints"),
        count: pkg.contributions.extension_points.length,
        rows: pkg.contributions.extension_points.map((point) => ({
          name: point.title ?? point.name,
          meta: point.reference
        }))
      },
      {
        title: dashboard.t("packages.contributionBindings"),
        count: pkg.contributions.contributions.length,
        rows: pkg.contributions.contributions.map((contribution) => ({
          name: contribution.title ?? contribution.name,
          meta: contribution.kind ? `${contribution.target} · ${contribution.kind}` : contribution.target
        }))
      },
      {
        title: dashboard.t("packages.resources"),
        count: pkg.contributions.resources.length,
        rows: pkg.contributions.resources.map((resource) => ({
          name: resource.name,
          meta: resource.resource_type
            ? `${resource.resource_type} · ${resource.visibility}`
            : resource.visibility
        }))
      },
      {
        title: dashboard.t("packages.webviews"),
        count: pkg.contributions.webviews.length,
        rows: pkg.contributions.webviews.map((webview) => ({
          name: webview.title ?? webview.name,
          meta: webview.kind
            ? `${webview.kind} · ${webview.entry ?? webview.path ?? webview.route ?? ""}`
            : webview.entry ?? webview.path ?? webview.route ?? undefined,
          webviewId: webview.name
        }))
      }
    ];
    return sections.filter((section) => section.count > 0);
  }
</script>

<svelte:window onkeydown={handlePackageDetailKeydown} />

<header class="page-title control-page-title">
  <div>
    <span class="page-index">{dashboard.t("packages.eyebrow")}</span>
    <h1>{dashboard.t("marketplace.pluginsTitle")}</h1>
    <p>{dashboard.t("packages.pageDesc")}</p>
  </div>
  <button
    class="btn-secondary header-action"
    onclick={() =>
      void (pluginView === "installed"
        ? dashboard.reloadPackages()
        : dashboard.refreshMarketplace(true, true))}
    disabled={dashboard.busy || dashboard.marketplaceLoading}
  >
    <RefreshCw size={16} strokeWidth={1.8} class={dashboard.packagesReloading || dashboard.marketplaceLoading ? "spinning" : ""} />
    {pluginView === "installed" ? dashboard.t("common.reload") : dashboard.t("marketplace.refresh")}
  </button>
</header>

<div class="plugin-view-tabs" role="group" aria-label={dashboard.t("marketplace.pluginsTitle")}>
  <button
    type="button"
    aria-pressed={pluginView === "marketplace"}
    class:active={pluginView === "marketplace"}
    onclick={() => (pluginView = "marketplace")}
  >
    {dashboard.t("marketplace.tabMarketplace")}
  </button>
  <button
    type="button"
    aria-pressed={pluginView === "installed"}
    class:active={pluginView === "installed"}
    onclick={() => (pluginView = "installed")}
  >
    {dashboard.t("marketplace.tabInstalled")}
    <small>{dashboard.packages.length}</small>
  </button>
  <button
    type="button"
    aria-pressed={pluginView === "updates"}
    class:active={pluginView === "updates"}
    onclick={() => (pluginView = "updates")}
  >
    {dashboard.t("marketplace.tabUpdates")}
    {#if marketplaceUpdates.length}<small>{marketplaceUpdates.length}</small>{/if}
  </button>
</div>

{#if pluginView === "marketplace" || pluginView === "updates"}
  <div class="marketplace-toolbar">
    <label class="marketplace-search">
      <Search size={16} strokeWidth={1.8} aria-hidden="true" />
      <input
        bind:value={marketplaceQuery}
        aria-label={dashboard.t("marketplace.searchPlaceholder")}
        placeholder={dashboard.t("marketplace.searchPlaceholder")}
      />
    </label>
    {#if dashboard.marketplaceSnapshot?.source === "cache"}
      <span class="marketplace-source">{dashboard.t("marketplace.cachedCatalogue")}</span>
    {/if}
  </div>

  {#if dashboard.marketplaceError}
    <div class="callout marketplace-status-callout">
      <strong>{dashboard.t("marketplace.catalogueUnavailable")}</strong>
      <span>{dashboard.marketplaceError}</span>
    </div>
  {:else if dashboard.marketplaceSnapshot?.expired}
    <div class="callout marketplace-status-callout warning">
      <strong>{dashboard.t("marketplace.catalogueExpired")}</strong>
    </div>
  {:else if dashboard.marketplaceSnapshot?.warning}
    <div class="callout marketplace-status-callout warning">
      <span>{dashboard.marketplaceSnapshot.warning}</span>
    </div>
  {/if}

  {#if dashboard.marketplaceLoading && !dashboard.marketplaceSnapshot}
    <div class="empty-state"><p>{dashboard.t("common.loading")}</p></div>
  {:else if displayedMarketplacePackages.length}
    <div class="marketplace-list">
      {#each displayedMarketplacePackages as pkg (pkg.id)}
        {@const listing = pkg.listing.snapshot}
        {@const latest = latestMarketplaceVersion(pkg)}
        {@const developer = marketplaceDeveloper(pkg)}
        <article class="marketplace-item">
          <div class="marketplace-item-icon" aria-hidden="true">
            {#if listing.media.icon}
              <img src={listing.media.icon.url} alt="" />
            {:else}
              <span>{listing.displayName.slice(0, 2).toLocaleUpperCase()}</span>
            {/if}
          </div>
          <div class="marketplace-item-main">
            <div class="marketplace-item-heading">
              <h2>{listing.displayName}</h2>
              <span class="marketplace-flags">
                {#if dashboard.marketplaceSnapshot?.catalogue.sections.recommended.includes(pkg.id)}
                  {dashboard.t("marketplace.recommended")}
                {/if}
                {#if dashboard.marketplaceSnapshot?.catalogue.sections.new.includes(pkg.id)}
                  {dashboard.t("marketplace.new")}
                {/if}
              </span>
            </div>
            <p class="marketplace-description">{listing.shortDescription}</p>
            <p class="marketplace-publisher">{developer?.name ?? pkg.developerId} / {marketplaceVerificationLabel(pkg)}</p>
          </div>
          <span class="marketplace-version">{latest ? `v${latest.version}` : "n/a"}</span>
          <button
            class={isMarketplaceUpdate(pkg) ? "btn-primary" : "btn-secondary"}
            onclick={() => void dashboard.prepareMarketplaceInstall([pkg.id])}
            disabled={marketplaceActionDisabled(pkg)}
          >
            {marketplaceActionLabel(pkg)}
          </button>
        </article>
      {/each}
    </div>
  {:else}
    <div class="empty-state">
      <p>{pluginView === "updates" ? dashboard.t("marketplace.noUpdates") : dashboard.t("marketplace.noPackages")}</p>
    </div>
  {/if}
{:else}
<div class="installed-workspace">
  <section class="installed-registry">
    <header class="installed-registry-head">
      <span>{dashboard.t("packages.installedTitle")}</span>
      <strong>
        {filteredInstalledPackages.length === dashboard.packages.length
          ? dashboard.packages.length
          : `${filteredInstalledPackages.length}/${dashboard.packages.length}`}
      </strong>
    </header>
    {#if dashboard.packages.length}
      <div class="installed-toolbar">
        <label class="installed-search">
          <Search size={15} strokeWidth={1.8} aria-hidden="true" />
          <input
            bind:value={installedQuery}
            aria-label={dashboard.t("packages.searchInstalled")}
            placeholder={dashboard.t("packages.searchInstalled")}
          />
        </label>
        {#if installedCategories.length}
          <div
            class="installed-category-filters"
            role="group"
            aria-label={dashboard.t("packages.filterByCategory")}
          >
            <button
              type="button"
              class:active={installedCategory === null}
              aria-pressed={installedCategory === null}
              onclick={() => (installedCategory = null)}
            >
              {dashboard.t("packages.allCategories")}
            </button>
            {#each installedCategories as category (category)}
              <button
                type="button"
                class:active={installedCategory === category}
                aria-pressed={installedCategory === category}
                onclick={() => (installedCategory = installedCategory === category ? null : category)}
              >
                {packageCategoryLabel(category)}
              </button>
            {/each}
          </div>
        {/if}
      </div>

      {#if filteredInstalledPackages.length}
      <div class="package-registry-list">
        {#each filteredInstalledPackages as pkg (pkg.id)}
          {@const primaryAction = resolvePackagePrimaryAction(pkg)}
          <article
            class="package-registry-row"
            class:disabled={!dashboard.isPackageEnabled(pkg)}
            title={`${dashboard.contributionCount(pkg)} ${dashboard.t("packages.elements")}`}
          >
            <div class="package-row-identity">
              <h3>{pkg.name}</h3>
              {#if packageCategories(pkg).length}
                <p class="package-purpose">
                  {#each packageCategories(pkg) as category (category)}
                    <span>{packageCategoryLabel(category)}</span>
                  {/each}
                </p>
              {/if}
              <small>{pkg.id} · v{pkg.version}</small>
              {#if pkg.error}<small class="package-row-error">{pkg.error}</small>{/if}
            </div>

            <button
              class="package-state-toggle {dashboard.packageDisplayStateClass(pkg)}"
              type="button"
              onclick={() => void dashboard.togglePackage(pkg)}
              disabled={dashboard.isPackageToggleDisabled(pkg) || dashboard.isPackageTogglePending(pkg)}
              aria-pressed={dashboard.isPackageToggleButtonEnabled(pkg)}
              title={dashboard.packageDisplayStateTitle(pkg)}
            >
              <Power size={14} strokeWidth={1.9} />
              {dashboard.packageDisplayStateLabel(pkg)}
            </button>

            {#if primaryAction}
              <button
                class="btn-secondary package-primary-action"
                type="button"
                onclick={() => void openPrimaryPackageAction(pkg)}
                disabled={primaryPackageActionDisabled(pkg)}
              >
                <span>{packagePrimaryActionLabel(pkg)}</span>
                <ArrowUpRight size={15} strokeWidth={1.8} />
              </button>
            {:else}
              <span class="package-no-primary-action">{dashboard.t("packages.noPrimaryAction")}</span>
            {/if}

            <details class="package-action-menu">
              <summary class="icon-button" title={dashboard.t("packages.moreActions")} aria-label={dashboard.t("packages.moreActions")}>
                <Ellipsis size={17} strokeWidth={1.8} />
              </summary>
              <div>
                <button type="button" onclick={() => openPackageDetails(pkg)}>
                  <ScanSearch size={15} strokeWidth={1.8} />
                  {dashboard.t("packages.details")}
                </button>
                {#if pkg.has_public_settings && !packageConfigurationIsPrimary(pkg)}
                  <button type="button" onclick={() => openPackageConfiguration(pkg)} disabled={dashboard.isPackageDeleting(pkg)}>
                    <Settings2 size={15} strokeWidth={1.8} />
                    {dashboard.t("packages.configuration")}
                  </button>
                {/if}
                {#if pkg.has_secrets}
                  <button type="button" onclick={() => openPackageSecrets(pkg)} disabled={dashboard.isPackageDeleting(pkg)}>
                    <LockKeyhole size={15} strokeWidth={1.8} />
                    {dashboard.t("packages.secrets")}
                  </button>
                {/if}
                <button class="danger" type="button" onclick={() => dashboard.removePackage(pkg)} disabled={dashboard.isPackageActionDisabled(pkg)}>
                  <Trash2 size={15} strokeWidth={1.8} />
                  {dashboard.t("common.remove")}
                </button>
              </div>
            </details>
          </article>
        {/each}
      </div>
      {:else}
        <div class="empty-state installed-empty-state">
          <p>{dashboard.t("packages.noFilteredPackages")}</p>
        </div>
      {/if}
    {:else}
      <div class="empty-state">
        <p>{dashboard.t("packages.noneInstalled")}</p>
      </div>
    {/if}
  </section>

  <details class="manual-install-panel">
    <summary>
      <span>
        <FolderOpen size={16} strokeWidth={1.8} />
        <strong>{dashboard.t("packages.manualInstall")}</strong>
        <small>{dashboard.t("packages.manualInstallDesc")}</small>
      </span>
      <span>{dashboard.t("packages.advanced")}</span>
    </summary>

    <div class="install-source-list">
      <section class="install-source">
        <label for="bundlePath">{dashboard.t("packages.localFile")}</label>
        <div class="form-row">
          <input
            id="bundlePath"
            bind:value={dashboard.bundlePath}
            placeholder="/path/to/plugin.brlp"
            readonly
            onclick={() => void dashboard.chooseInstallFile()}
            disabled={dashboard.busy}
          />
          <button class="icon-button" onclick={() => void dashboard.chooseInstallFile()} disabled={dashboard.busy} title={dashboard.t("common.browse")} aria-label={dashboard.t("common.browse")}>
            <FolderOpen size={15} strokeWidth={1.8} />
          </button>
          <button class="btn-primary" onclick={() => void dashboard.inspectInstallFile()} disabled={dashboard.busy || !dashboard.bundlePath.trim()}>
            <ScanSearch size={15} strokeWidth={1.8} />
            {dashboard.t("common.inspect")}
          </button>
        </div>
      </section>

      <section class="install-source">
        <label for="bundleUrl">{dashboard.t("packages.directUrl")}</label>
        <div class="form-row">
          <input id="bundleUrl" bind:value={dashboard.bundleUrl} placeholder="https://example.com/plugin.brlp" disabled={dashboard.busy} />
          <button class="icon-button" onclick={() => void dashboard.installFromUrl()} disabled={dashboard.busy || !dashboard.bundleUrl.trim()} title={dashboard.t("common.inspect")} aria-label={dashboard.t("common.inspect")}>
            <ExternalLink size={15} strokeWidth={1.8} />
          </button>
        </div>
      </section>

      <section class="install-source">
        <label for="gitRepo">{dashboard.t("packages.gitRepo")}</label>
        <input id="gitRepo" bind:value={dashboard.gitRepo} placeholder="https://github.com/user/repo" disabled={dashboard.busy} />
        <div class="form-row">
          <input
            bind:value={dashboard.gitRev}
            aria-label={dashboard.t("packages.gitRevPlaceholder")}
            placeholder={dashboard.t("packages.gitRevPlaceholder")}
            disabled={dashboard.busy}
          />
          <button class="btn-primary" onclick={() => void dashboard.installFromGit()} disabled={dashboard.busy || !dashboard.gitRepo.trim()}>
            <ScanSearch size={15} strokeWidth={1.8} />
            {dashboard.t("common.inspect")}
          </button>
        </div>
      </section>
    </div>
  </details>
</div>
{/if}

{#if detailPackage}
  <div class="modal-layer">
    <button
      type="button"
      class="modal-scrim"
      aria-label={dashboard.t("common.close")}
      onclick={closePackageDetails}
    ></button>
    <div
      bind:this={detailDialog}
      class="studio-modal package-detail-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="package-detail-title"
      tabindex="-1"
    >
      <div class="modal-heading package-detail-heading">
        <div class="package-detail-heading-main">
          <div class="package-detail-title-row">
            <h2 id="package-detail-title">{detailPackage.name}</h2>
            <span class="status-pill {dashboard.packageDisplayStateClass(detailPackage)}" title={dashboard.packageDisplayStateTitle(detailPackage)}>
              <span class="status-dot"></span>
              {dashboard.packageDisplayStateLabel(detailPackage)}
            </span>
          </div>
          <p>v{detailPackage.version} · {dashboard.t("packages.by")} {detailPackage.author ?? dashboard.t("packages.unknownAuthor")}</p>
        </div>
        <button type="button" class="icon-button package-detail-close" aria-label={dashboard.t("common.close")} onclick={closePackageDetails}>
          <X size={16} strokeWidth={1.8} />
        </button>
      </div>

      <div class="package-detail-scroll">
        {#if detailPackage.error}
          <div class="callout">
            <strong>{dashboard.t("common.error")}</strong>
            <span>{detailPackage.error}</span>
          </div>
        {/if}

      <div class="package-detail-summary">
        <div class="package-detail-stat">
          <strong>{dashboard.packageDisplayStateLabel(detailPackage)}</strong>
          <span>{dashboard.t("common.state")}</span>
        </div>
        <div class="package-detail-stat">
          <strong>{dashboard.contributionCount(detailPackage)}</strong>
          <span>{dashboard.t("packages.elements")}</span>
        </div>
        <div class="package-detail-stat">
          <strong>
            {detailPackage.has_public_settings || detailPackage.has_secrets
              ? dashboard.t("common.enabled")
              : dashboard.t("common.disabled")}
          </strong>
          <span>{dashboard.t("packages.configuration")}</span>
        </div>
        <div class="package-detail-stat">
          <strong>{detailPackage.compatibility.bakingrlApi ?? "n/a"}</strong>
          <span>{dashboard.t("packages.compatibility")}</span>
        </div>
      </div>

      <section class="package-detail-section">
        <div class="package-detail-section-head">
          <h3>{dashboard.t("packages.runtimeRelationsTitle")}</h3>
          <span class="section-count">
            {dependencyRows(detailPackage).length + dependentRows(detailPackage).length + incomingContributionRows(detailPackage).length}
          </span>
        </div>
        <div class="runtime-relation-grid">
          <article class="runtime-relation-card">
            <div class="runtime-relation-head">
              <h4>{dashboard.t("packages.nodeRuntimeTitle")}</h4>
              <span class="status-pill runtime-mini {extensionHostRuntimeClass(extensionHostRuntimeStatus(detailPackage))}">
                <span class="status-dot"></span>
                {extensionHostRuntimeLabel(extensionHostRuntimeStatus(detailPackage))}
              </span>
            </div>
            {#if detailPackage.runtime?.node}
              <dl class="runtime-relation-facts">
                <div>
                  <dt>{dashboard.t("packages.nodeRuntimeEntry")}</dt>
                  <dd>{detailPackage.runtime.node.entry}</dd>
                </div>
                <div>
                  <dt>{dashboard.t("packages.sidecarRestarts")}</dt>
                  <dd>{extensionHostRuntimeStatus(detailPackage)?.restartCount ?? 0}</dd>
                </div>
                <div>
                  <dt>{dashboard.t("packages.sidecarCrashes")}</dt>
                  <dd>{extensionHostRuntimeStatus(detailPackage)?.crashCount ?? 0}</dd>
                </div>
              </dl>
              {#if extensionHostRuntimeStatus(detailPackage)?.lastError}
                <p class="runtime-relation-error">{extensionHostRuntimeStatus(detailPackage)?.lastError}</p>
              {/if}
            {:else}
              <p class="empty-note">{dashboard.t("packages.nodeRuntimeNotDeclared")}</p>
            {/if}
          </article>

          <article class="runtime-relation-card">
            <div class="runtime-relation-head">
              <h4>{dashboard.t("packages.dependenciesTitle")}</h4>
              <span class="section-count">{dependencyRows(detailPackage).length}</span>
            </div>
            {#if dependencyRows(detailPackage).length}
              <ul class="runtime-relation-list">
                {#each dependencyRows(detailPackage) as row}
                  <li>
                    <div>
                      <strong>{row.name}</strong>
                      {#if row.meta}<span>{row.meta}</span>{/if}
                    </div>
                    <span class="status-pill runtime-mini {row.statusClass}">
                      <span class="status-dot"></span>
                      {row.status}
                    </span>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="empty-note">{dashboard.t("packages.noDependencies")}</p>
            {/if}
          </article>

          <article class="runtime-relation-card">
            <div class="runtime-relation-head">
              <h4>{dashboard.t("packages.dependentsTitle")}</h4>
              <span class="section-count">{dependentRows(detailPackage).length}</span>
            </div>
            {#if dependentRows(detailPackage).length}
              <ul class="runtime-relation-list">
                {#each dependentRows(detailPackage) as row}
                  <li>
                    <div>
                      <strong>{row.name}</strong>
                      {#if row.meta}<span>{row.meta}</span>{/if}
                    </div>
                    <span class="status-pill runtime-mini {row.statusClass}">
                      <span class="status-dot"></span>
                      {row.status}
                    </span>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="empty-note">{dashboard.t("packages.noDependents")}</p>
            {/if}
          </article>

          <article class="runtime-relation-card">
            <div class="runtime-relation-head">
              <h4>{dashboard.t("packages.incomingContributionsTitle")}</h4>
              <span class="section-count">{incomingContributionRows(detailPackage).length}</span>
            </div>
            {#if incomingContributionRows(detailPackage).length}
              <ul class="runtime-relation-list">
                {#each incomingContributionRows(detailPackage) as row}
                  <li>
                    <div>
                      <strong>{row.name}</strong>
                      {#if row.meta}<span>{row.meta}</span>{/if}
                    </div>
                    <span class="status-pill runtime-mini {row.statusClass}">
                      <span class="status-dot"></span>
                      {row.status}
                    </span>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="empty-note">{dashboard.t("packages.noIncomingContributions")}</p>
            {/if}
          </article>
        </div>
      </section>

      {#if packageSidecars(detailPackage).length}
        <section class="package-detail-section">
          <div class="package-detail-section-head">
            <h3>{dashboard.t("packages.sidecarRuntimeTitle")}</h3>
            <span class="section-count">{packageSidecars(detailPackage).length}</span>
          </div>
          <div class="sidecar-runtime-list">
            {#each packageSidecars(detailPackage) as sidecar}
              {@const status = sidecarStatus(detailPackage, sidecar.id)}
              <article class="sidecar-runtime-item">
                <div class="sidecar-runtime-head">
                  <div class="sidecar-runtime-title">
                    <strong>{sidecar.id}</strong>
                    <span>{sidecar.protocol} · {sidecar.activation}</span>
                  </div>
                  <span
                    class="status-pill runtime-mini {sidecarRuntimeClass(status)}"
                    title={sidecarRuntimeTitle(sidecar, status)}
                  >
                    <span class="status-dot"></span>
                    {sidecarRuntimeLabel(status)}
                  </span>
                </div>
                <div class="sidecar-runtime-grid">
                  <div>
                    <span>{dashboard.t("packages.sidecarHealth")}</span>
                    <strong>{sidecarHealthLabel(status)}</strong>
                  </div>
                  <div>
                    <span>{dashboard.t("packages.sidecarRestarts")}</span>
                    <strong>{status?.restartCount ?? 0}</strong>
                  </div>
                  <div>
                    <span>{dashboard.t("packages.sidecarCrashes")}</span>
                    <strong>{status?.crashCount ?? 0}</strong>
                  </div>
                  <div>
                    <span>{dashboard.t("packages.sidecarExitCode")}</span>
                    <strong>{sidecarLastExitCode(status)}</strong>
                  </div>
                  <div>
                    <span>{dashboard.t("packages.sidecarLastHealthCheck")}</span>
                    <strong>{sidecarLastHealthCheck(status)}</strong>
                  </div>
                </div>
                {#if status?.lastHealthError}
                  <div class="sidecar-runtime-error">
                    <strong>{dashboard.t("packages.sidecarLastHealthError")}</strong>
                    <span>{status.lastHealthError}</span>
                  </div>
                {/if}
              </article>
            {/each}
          </div>
        </section>
      {/if}

      <section class="package-detail-section">
        <div class="package-detail-section-head">
          <h3>{dashboard.t("packages.contributionsTitle")}</h3>
          <span class="section-count">{dashboard.contributionCount(detailPackage)}</span>
        </div>
        {#if dashboard.contributionCount(detailPackage) > 0}
          <div class="contribution-section-grid">
            {#each packageContributionSections(detailPackage) as section}
              <section class="contribution-section-card">
                <div class="contribution-section-head">
                  <h4>{section.title}</h4>
                  <span>{section.count}</span>
                </div>
                <ul class="contribution-items">
                  {#each section.rows as row}
                    <li class="contribution-item">
                      <div class="contribution-item-main">
                        <span class="contribution-name">{row.name}</span>
                        {#if row.meta}
                          <span class="contribution-meta">{row.meta}</span>
                        {/if}
                      </div>
                      {#if row.webviewId}
                        <button
                          type="button"
                          class="icon-button contribution-action"
                          onclick={() => void openPackageWebview(detailPackage, row.webviewId ?? "")}
                          disabled={!dashboard.isPackageEnabled(detailPackage) || !dashboard.isPackageCompatible(detailPackage)}
                          title={dashboard.t("packages.openWebview")}
                          aria-label={dashboard.t("packages.openWebview")}
                        >
                          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M7 17 17 7"></path>
                            <path d="M8 7h9v9"></path>
                          </svg>
                        </button>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </section>
            {/each}
          </div>
        {:else}
          <p class="empty-note">{dashboard.t("packages.noContributions")}</p>
        {/if}
      </section>

      </div>

      <div class="modal-actions">
        <button type="button" class="btn-secondary" onclick={closePackageDetails}>
          {dashboard.t("common.close")}
        </button>
        <button
          type="button"
          class={dashboard.isPackageToggleButtonEnabled(detailPackage) ? "btn-secondary" : "btn-primary"}
          onclick={() => void dashboard.togglePackage(detailPackage)}
          disabled={dashboard.isPackageToggleDisabled(detailPackage) || dashboard.isPackageTogglePending(detailPackage)}
        >
          {dashboard.isPackageToggleButtonEnabled(detailPackage) ? dashboard.t("common.disable") : dashboard.t("common.enable")}
        </button>
      </div>
    </div>
  </div>
{/if}
