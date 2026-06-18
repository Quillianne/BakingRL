<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getDashboardContext } from "$lib/dashboard/context";
  import type {
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

  let detailPackageId = $state<string | null>(null);
  const detailPackage = $derived(dashboard.packages.find((pkg) => pkg.id === detailPackageId) ?? null);

  function openPackageDetails(pkg: PackageDescriptor) {
    detailPackageId = pkg.id;
  }

  function closePackageDetails() {
    detailPackageId = null;
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

  function primaryWebview(pkg: PackageDescriptor) {
    return (
      pkg.contributions.webviews.find((webview) => webview.kind === "settings" && webview.name === "settings") ??
      pkg.contributions.webviews.find((webview) => webview.kind === "settings") ??
      pkg.contributions.webviews[0] ??
      null
    );
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

  async function openPackageWebview(pkg: PackageDescriptor, webviewId: string) {
    try {
      await invoke("open_package_webview", { packageId: pkg.id, webviewId });
    } catch (error) {
      dashboard.notifyError(error);
    }
  }

  async function openPrimaryPackageWebview(pkg: PackageDescriptor) {
    const webview = primaryWebview(pkg);
    if (!webview) return;
    await openPackageWebview(pkg, webview.name);
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
      },
      {
        title: dashboard.t("packages.assets"),
        count: pkg.contributions.assets.length,
        rows: pkg.contributions.assets.map((asset) => ({ name: asset.name }))
      },
      {
        title: dashboard.t("packages.schemas"),
        count: pkg.contributions.schemas.length,
        rows: pkg.contributions.schemas.map((schema) => ({ name: schema.name }))
      }
    ];
    return sections.filter((section) => section.count > 0);
  }
</script>

<div class="page-title">
  <div>
    <h1>{dashboard.t("packages.installedTitle")}</h1>
  </div>
  <button class="btn-secondary" onclick={() => void dashboard.reloadPackages()} disabled={dashboard.busy}>
    <svg class="reload-icon" class:spinning={dashboard.packagesReloading} viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M21 12a9 9 0 0 1-15.5 6"></path>
      <path d="M3 12a9 9 0 0 1 15.5-6"></path>
      <path d="M18 3v6h-6"></path>
      <path d="M6 21v-6h6"></path>
    </svg>
    {dashboard.t("common.reload")}
  </button>
</div>

<div class="studio-grid two-col">
  <section>
    {#if dashboard.packages.length}
      <div class="card-grid">
        {#each dashboard.packages as pkg (pkg.id)}
          <article
            class="studio-card package-card"
            class:disabled={!dashboard.isPackageEnabled(pkg)}
            title={`${dashboard.contributionCount(pkg)} ${dashboard.t("packages.elements")}`}
          >
            <div class="package-head">
              <div class="package-meta">
                <div class="package-title-row">
                  <h3>{pkg.name}</h3>
                  <span class="status-pill {dashboard.packageDisplayStateClass(pkg)}" title={dashboard.packageDisplayStateTitle(pkg)}>
                    <span class="status-dot"></span>
                    {dashboard.packageDisplayStateLabel(pkg)}
                  </span>
                  <div class="package-card-tools">
                    {#if pkg.contributions.webviews.length}
                      <button
                        class="icon-button"
                        onclick={() => void openPrimaryPackageWebview(pkg)}
                        disabled={!dashboard.isPackageEnabled(pkg) || !dashboard.isPackageCompatible(pkg)}
                        title={dashboard.t("packages.openWebview")}
                        aria-label={dashboard.t("packages.openWebview")}
                      >
                        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <rect x="3" y="4" width="18" height="14" rx="2"></rect>
                          <path d="M8 20h8"></path>
                          <path d="M12 18v2"></path>
                        </svg>
                      </button>
                    {/if}
                    {#if pkg.has_public_settings}
                      <button
                        class="icon-button"
                        onclick={() => openPackageConfiguration(pkg)}
                        disabled={dashboard.isPackageDeleting(pkg)}
                        title={dashboard.t("packages.configuration")}
                        aria-label={dashboard.t("packages.configuration")}
                      >
                        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <path d="M9.67 4.14a2.34 2.34 0 0 1 4.66 0 2.34 2.34 0 0 0 3.32 1.91 2.34 2.34 0 0 1 2.33 4.03 2.34 2.34 0 0 0 0 3.84 2.34 2.34 0 0 1-2.33 4.03 2.34 2.34 0 0 0-3.32 1.91 2.34 2.34 0 0 1-4.66 0 2.34 2.34 0 0 0-3.32-1.91 2.34 2.34 0 0 1-2.33-4.03 2.34 2.34 0 0 0 0-3.84 2.34 2.34 0 0 1 2.33-4.03 2.34 2.34 0 0 0 3.32-1.91Z"></path>
                          <circle cx="12" cy="12" r="3"></circle>
                        </svg>
                      </button>
                    {/if}
                    {#if pkg.has_secrets}
                      <button
                        class="icon-button secret"
                        onclick={() => openPackageSecrets(pkg)}
                        disabled={dashboard.isPackageDeleting(pkg)}
                        title={dashboard.t("packages.secrets")}
                        aria-label={dashboard.t("packages.secrets")}
                      >
                        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <rect x="5" y="10" width="14" height="10" rx="2"></rect>
                          <path d="M8 10V7a4 4 0 0 1 8 0v3"></path>
                          <path d="M12 14v2"></path>
                        </svg>
                      </button>
                    {/if}
                    <button
                      class="icon-button danger"
                      onclick={() => dashboard.removePackage(pkg)}
                      disabled={dashboard.isPackageActionDisabled(pkg)}
                      title={dashboard.t("common.remove")}
                      aria-label={dashboard.t("common.remove")}
                    >
                      <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M3 6h18"></path>
                        <path d="M8 6V4h8v2"></path>
                        <path d="M19 6l-1 14H6L5 6"></path>
                        <path d="M10 11v5"></path>
                        <path d="M14 11v5"></path>
                      </svg>
                    </button>
                  </div>
                </div>
                <p>{dashboard.t("packages.by")} {pkg.author ?? dashboard.t("packages.unknownAuthor")} · v{pkg.version}</p>
              </div>
            </div>

            {#if pkg.error}
              <div class="callout">
                <strong>{dashboard.t("common.error")}</strong>
                <span>{pkg.error}</span>
              </div>
            {/if}

            {#if packageSidecars(pkg).length}
              <div class="package-runtime-strip" aria-label={dashboard.t("packages.sidecarRuntimeTitle")}>
                {#each packageSidecars(pkg) as sidecar}
                  {@const status = sidecarStatus(pkg, sidecar.id)}
                  <span
                    class="status-pill runtime-mini {sidecarRuntimeClass(status)}"
                    title={sidecarRuntimeTitle(sidecar, status)}
                  >
                    <span class="status-dot"></span>
                    {sidecar.id}
                  </span>
                {/each}
              </div>
            {/if}

            <div class="card-actions package-actions">
              <button class="btn-outline" onclick={() => openPackageDetails(pkg)}>
                {dashboard.t("packages.details")}
              </button>
              <button
                class={dashboard.isPackageToggleButtonEnabled(pkg) ? "btn-secondary" : "btn-primary"}
                onclick={() => void dashboard.togglePackage(pkg)}
                disabled={dashboard.isPackageToggleDisabled(pkg) || dashboard.isPackageTogglePending(pkg)}
              >
                {dashboard.isPackageToggleButtonEnabled(pkg) ? dashboard.t("common.disable") : dashboard.t("common.enable")}
              </button>
            </div>
          </article>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <p>{dashboard.t("packages.noneInstalled")}</p>
      </div>
    {/if}
  </section>

  <aside class="studio-panel install-panel">
    <div class="panel-heading">
      <div>
        <h2>{dashboard.t("packages.installTitle")}</h2>
      </div>
    </div>

    <div class="section-stack">
      <div class="input-group">
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
          <button class="btn-secondary" onclick={() => void dashboard.chooseInstallFile()} disabled={dashboard.busy}>
            {dashboard.t("common.browse")}
          </button>
          <button class="btn-primary" onclick={() => void dashboard.inspectInstallFile()} disabled={dashboard.busy || !dashboard.bundlePath.trim()}>
            {dashboard.t("common.inspect")}
          </button>
        </div>
      </div>

      <div class="input-group">
        <label for="bundleUrl">{dashboard.t("packages.directUrl")}</label>
        <div class="form-row">
          <input id="bundleUrl" bind:value={dashboard.bundleUrl} placeholder="https://example.com/plugin.brlp" disabled={dashboard.busy} />
          <button class="btn-primary" onclick={() => void dashboard.installFromUrl()} disabled={dashboard.busy || !dashboard.bundleUrl.trim()}>
            {dashboard.t("common.inspect")}
          </button>
        </div>
      </div>

      <div class="input-group">
        <label for="gitRepo">{dashboard.t("packages.gitRepo")}</label>
        <input id="gitRepo" bind:value={dashboard.gitRepo} placeholder="https://github.com/user/repo" disabled={dashboard.busy} />
        <div class="form-row">
          <input bind:value={dashboard.gitRev} placeholder={dashboard.t("packages.gitRevPlaceholder")} disabled={dashboard.busy} />
          <button class="btn-primary" onclick={() => void dashboard.installFromGit()} disabled={dashboard.busy || !dashboard.gitRepo.trim()}>
            {dashboard.t("common.inspect")}
          </button>
        </div>
      </div>
    </div>
  </aside>
</div>

{#if detailPackage}
  <div class="modal-layer">
    <button
      type="button"
      class="modal-scrim"
      aria-label={dashboard.t("common.cancel")}
      onclick={closePackageDetails}
    ></button>
    <div
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
        <button type="button" class="icon-button package-detail-close" aria-label={dashboard.t("common.cancel")} onclick={closePackageDetails}>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 6 6 18"></path>
            <path d="m6 6 12 12"></path>
          </svg>
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
          {dashboard.t("common.cancel")}
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
