<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import type { PackageDescriptor } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();

  type PackageExportRow = {
    name: string;
    meta?: string;
  };

  type PackageExportSection = {
    title: string;
    count: number;
    rows: PackageExportRow[];
  };

  let detailPackageId = $state<string | null>(null);
  const detailPackage = $derived(dashboard.packages.find((pkg) => pkg.id === detailPackageId) ?? null);

  function openPackageDetails(pkg: PackageDescriptor) {
    detailPackageId = pkg.id;
  }

  function closePackageDetails() {
    detailPackageId = null;
  }

  function packageExportSections(pkg: PackageDescriptor): PackageExportSection[] {
    return [
      {
        title: dashboard.t("packages.visuals"),
        count: pkg.exports.visuals.length,
        rows: pkg.exports.visuals.map((visual) => ({
          name: visual.name,
          meta: `${visual.default_width}x${visual.default_height}`
        }))
      },
      {
        title: dashboard.t("packages.components"),
        count: pkg.exports.components.length,
        rows: pkg.exports.components.map((component) => ({ name: component.name }))
      },
      {
        title: dashboard.t("packages.services"),
        count: pkg.exports.services.length,
        rows: pkg.exports.services.map((service) => ({
          name: service.name,
          meta: `${service.methods.length} ${dashboard.t("packages.methods")}`
        }))
      },
      {
        title: dashboard.t("packages.connectors"),
        count: pkg.exports.connectors.length,
        rows: pkg.exports.connectors.map((connector) => ({ name: connector.name }))
      },
      {
        title: dashboard.t("packages.assets"),
        count: pkg.exports.assets.length,
        rows: pkg.exports.assets.map((asset) => ({ name: asset.name }))
      },
      {
        title: dashboard.t("packages.schemas"),
        count: pkg.exports.schemas.length,
        rows: pkg.exports.schemas.map((schema) => ({ name: schema.name }))
      },
      {
        title: dashboard.t("packages.pageTemplates"),
        count: pkg.exports.pages.length,
        rows: pkg.exports.pages.map((page) => ({
          name: page.title ?? page.name,
          meta: page.path
        }))
      },
      {
        title: dashboard.t("packages.layoutTemplates"),
        count: pkg.exports.layouts.length,
        rows: pkg.exports.layouts.map((layoutTemplate) => ({
          name: layoutTemplate.title ?? layoutTemplate.name,
          meta: layoutTemplate.path
        }))
      }
    ].filter((section) => section.count > 0);
  }
</script>

<div class="page-title">
  <div>
    <h1>{dashboard.t("packages.installedTitle")}</h1>
    <p>{dashboard.t("packages.installDesc")}</p>
  </div>
  <button class="btn-secondary" onclick={() => void dashboard.reloadPackages()} disabled={dashboard.busy}>
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
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
          <article class="studio-card package-card" class:disabled={!dashboard.isPackageEnabled(pkg)}>
            <div class="package-head">
              <div class="package-meta">
                <div class="package-title-row">
                  <h3>{pkg.name}</h3>
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
                <p>{dashboard.t("packages.by")} {pkg.author ?? dashboard.t("packages.unknownAuthor")} · v{pkg.version}</p>
              </div>
            </div>

            <div class="package-card-summary">
              <div class="package-summary-item">
                <span class="package-summary-label">{dashboard.t("common.state")}</span>
                <span class="status-pill {dashboard.packageStateClass(pkg)}">
                  <span class="status-dot"></span>
                  {dashboard.packageStateLabel(pkg)}
                </span>
              </div>
              <div class="package-summary-item">
                <span class="package-summary-label">{dashboard.t("common.permissions")}</span>
                <strong>{dashboard.permissionTotal(pkg.effective_permissions)}</strong>
              </div>
            </div>

            {#if pkg.error}
              <div class="callout">
                <strong>{dashboard.t("common.error")}</strong>
                <span>{pkg.error}</span>
              </div>
            {/if}

            <div class="card-actions package-actions">
              <button class="btn-outline" onclick={() => openPackageDetails(pkg)}>
                {dashboard.t("packages.details")}
              </button>
              <button
                class={dashboard.isPackageToggleButtonEnabled(pkg) ? "btn-secondary" : "btn-primary"}
                onclick={() => void dashboard.togglePackage(pkg)}
                disabled={dashboard.isPackageActionDisabled(pkg) || dashboard.isPackageTogglePending(pkg)}
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
        <p>{dashboard.t("packages.installDesc")}</p>
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
            <span class="status-pill {dashboard.packageStateClass(detailPackage)}">
              <span class="status-dot"></span>
              {dashboard.packageStateLabel(detailPackage)}
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
          <strong>{dashboard.packageStateLabel(detailPackage)}</strong>
          <span>{dashboard.t("common.state")}</span>
        </div>
        <div class="package-detail-stat">
          <strong>{dashboard.permissionTotal(detailPackage.effective_permissions)}</strong>
          <span>{dashboard.t("common.permissions")}</span>
        </div>
        <div class="package-detail-stat">
          <strong>{dashboard.exportCount(detailPackage)}</strong>
          <span>{dashboard.t("packages.elements")}</span>
        </div>
      </div>

      <section class="package-detail-section">
        <div class="package-detail-section-head">
          <h3>{dashboard.t("packages.exportsTitle")}</h3>
          <span class="section-count">{dashboard.exportCount(detailPackage)}</span>
        </div>
        {#if dashboard.exportCount(detailPackage) > 0}
          <div class="export-section-grid">
            {#each packageExportSections(detailPackage) as section}
              <section class="export-section-card">
                <div class="export-section-head">
                  <h4>{section.title}</h4>
                  <span>{section.count}</span>
                </div>
                <ul class="export-items">
                  {#each section.rows as row}
                    <li class="export-item">
                      <span class="export-name">{row.name}</span>
                      {#if row.meta}
                        <span class="export-meta">{row.meta}</span>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </section>
            {/each}
          </div>
        {:else}
          <p class="permission-none">{dashboard.t("packages.noExports")}</p>
        {/if}
      </section>

      <section class="package-detail-section">
        <div class="package-detail-section-head">
          <h3>{dashboard.t("packages.effectivePermissions")}</h3>
          <span class="section-count">{dashboard.permissionTotal(detailPackage.effective_permissions)}</span>
        </div>
        {#if dashboard.permissionTotal(detailPackage.effective_permissions) > 0}
          <div class="permission-grid expanded">
            {#each dashboard.permissionSections(detailPackage.effective_permissions) as section}
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
          <p class="permission-none">{dashboard.t("packages.noExtraPermissions")}</p>
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
          disabled={dashboard.isPackageActionDisabled(detailPackage) || dashboard.isPackageTogglePending(detailPackage)}
        >
          {dashboard.isPackageToggleButtonEnabled(detailPackage) ? dashboard.t("common.disable") : dashboard.t("common.enable")}
        </button>
      </div>
    </div>
  </div>
{/if}
