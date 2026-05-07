<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import type { PackageDescriptor } from "$lib/dashboard/types";

  const state = getDashboardContext();

  function hasNetwork(pkg: PackageDescriptor) {
    return Boolean(pkg.effective_permissions.network?.http?.length || pkg.effective_permissions.network?.websocket?.length);
  }
</script>

<div class="page-title">
  <div>
    <h1>{state.t("packages.installedTitle")}</h1>
    <p>{state.t("packages.installDesc")}</p>
  </div>
  <button class="btn-secondary" onclick={() => void state.reloadPackages()} disabled={state.busy}>
    <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M21 12a9 9 0 0 1-15.5 6"></path>
      <path d="M3 12a9 9 0 0 1 15.5-6"></path>
      <path d="M18 3v6h-6"></path>
      <path d="M6 21v-6h6"></path>
    </svg>
    {state.t("common.reload")}
  </button>
</div>

<div class="studio-grid two-col">
  <section>
    {#if state.packages.length}
      <div class="card-grid">
        {#each state.packages as pkg (pkg.id)}
          <article class="studio-card package-card" class:disabled={!pkg.enabled}>
            <div class="package-head">
              <div class="package-meta">
                <h3>{pkg.name}</h3>
                <span class="package-id">{pkg.id}</span>
                <p>{state.t("packages.by")} {pkg.author ?? state.t("packages.unknownAuthor")} · v{pkg.version}</p>
              </div>
              <div class="inline-actions">
                <span class="status-pill {pkg.enabled ? 'connected' : 'disconnected'}">
                  <span class="status-dot"></span>
                  {pkg.enabled ? state.t("common.active") : state.t("common.disabled")}
                </span>
              </div>
            </div>

            {#if pkg.error}
              <div class="callout">
                <strong>{state.t("common.error")}</strong>
                <span>{pkg.error}</span>
              </div>
            {/if}

            <div class="badge-row">
              <span class="badge route">
                <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z"></path>
                </svg>
                {state.permissionTotal(pkg.effective_permissions)} {state.t("common.permissions")}
              </span>
              {#if hasNetwork(pkg)}
                <span class="badge warn">{state.t("packages.networkPermission")}</span>
              {/if}
              {#if pkg.exports.pages.length}
                <span class="badge muted">{pkg.exports.pages.length} {state.t("packages.pages")}</span>
              {/if}
              {#if pkg.exports.layouts.length}
                <span class="badge muted">{pkg.exports.layouts.length} {state.t("packages.layouts")}</span>
              {/if}
            </div>

            <div class="package-stats">
              <span class="package-stat"><strong>{pkg.exports.visuals.length}</strong>{state.t("packages.visuals")}</span>
              <span class="package-stat"><strong>{pkg.exports.components.length}</strong>{state.t("packages.components")}</span>
              <span class="package-stat"><strong>{pkg.exports.services.length}</strong>{state.t("packages.services")}</span>
              <span class="package-stat"><strong>{pkg.exports.pages.length}</strong>{state.t("packages.pages")}</span>
              <span class="package-stat"><strong>{pkg.exports.layouts.length}</strong>{state.t("packages.layouts")}</span>
            </div>

            <details class="details-list">
              <summary>{state.t("packages.details")}</summary>
              <div class="exports-list">
                {#if pkg.exports.visuals.length}
                  <section>
                    <h4>Visuals</h4>
                    <ul>
                      {#each pkg.exports.visuals as visual}
                        <li>{visual.name}<span>{visual.default_width}x{visual.default_height}</span></li>
                      {/each}
                    </ul>
                  </section>
                {/if}
                {#if pkg.exports.services.length}
                  <section>
                    <h4>Services</h4>
                    <ul>
                      {#each pkg.exports.services as service}
                        <li>{service.name}<span>{service.methods.length} {state.t("packages.methods")}</span></li>
                      {/each}
                    </ul>
                  </section>
                {/if}
                {#if pkg.exports.pages.length}
                  <section>
                    <h4>{state.t("packages.pageTemplates")}</h4>
                    <ul>
                      {#each pkg.exports.pages as page}
                        <li>{page.title ?? page.name}<span>{page.path}</span></li>
                      {/each}
                    </ul>
                  </section>
                {/if}
                {#if pkg.exports.layouts.length}
                  <section>
                    <h4>{state.t("packages.layoutTemplates")}</h4>
                    <ul>
                      {#each pkg.exports.layouts as layoutTemplate}
                        <li>{layoutTemplate.title ?? layoutTemplate.name}<span>{layoutTemplate.path}</span></li>
                      {/each}
                    </ul>
                  </section>
                {/if}
              </div>

              <div class="permissions-view">
                <div class="permission-summary">
                  <span class="permission-count">{state.permissionTotal(pkg.effective_permissions)}</span>
                  <span>{state.t("packages.effectivePermissions")}</span>
                </div>
                {#if state.permissionTotal(pkg.effective_permissions) > 0}
                  <div class="permission-grid">
                    {#each state.permissionSections(pkg.effective_permissions) as section}
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
                  <p class="permission-none">{state.t("packages.noExtraPermissions")}</p>
                {/if}
              </div>
            </details>

            <div class="card-actions">
              <button class="btn-secondary" onclick={() => void state.togglePackage(pkg)} disabled={state.busy}>
                {pkg.enabled ? state.t("common.disabled") : state.t("common.enabled")}
              </button>
              <button class="btn-danger" onclick={() => state.removePackage(pkg)} disabled={state.busy}>
                {state.t("common.remove")}
              </button>
            </div>
          </article>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <p>{state.t("packages.noneInstalled")}</p>
      </div>
    {/if}
  </section>

  <aside class="studio-panel install-panel">
    <div class="panel-heading">
      <div>
        <h2>{state.t("packages.installTitle")}</h2>
        <p>{state.t("packages.installDesc")}</p>
      </div>
    </div>

    <div class="section-stack">
      <div class="input-group">
        <label for="bundlePath">{state.t("packages.localFile")}</label>
        <div class="form-row">
          <input id="bundlePath" bind:value={state.bundlePath} placeholder="/path/to/plugin.brlp" disabled={state.busy} />
          <button class="btn-primary" onclick={() => void state.inspectInstallFile()} disabled={state.busy || !state.bundlePath.trim()}>
            {state.t("common.inspect")}
          </button>
        </div>
      </div>

      <div class="input-group">
        <label for="bundleUrl">{state.t("packages.directUrl")}</label>
        <div class="form-row">
          <input id="bundleUrl" bind:value={state.bundleUrl} placeholder="https://example.com/plugin.brlp" disabled={state.busy} />
          <button class="btn-primary" onclick={() => void state.installFromUrl()} disabled={state.busy || !state.bundleUrl.trim()}>
            {state.t("common.inspect")}
          </button>
        </div>
      </div>

      <div class="input-group">
        <label for="gitRepo">{state.t("packages.gitRepo")}</label>
        <input id="gitRepo" bind:value={state.gitRepo} placeholder="https://github.com/user/repo" disabled={state.busy} />
        <div class="form-row">
          <input bind:value={state.gitRev} placeholder={state.t("packages.gitRevPlaceholder")} disabled={state.busy} />
          <button class="btn-primary" onclick={() => void state.installFromGit()} disabled={state.busy || !state.gitRepo.trim()}>
            {state.t("common.inspect")}
          </button>
        </div>
      </div>
    </div>
  </aside>
</div>
