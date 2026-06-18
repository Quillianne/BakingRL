<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";

  const state = getDashboardContext();

  const packagesWithIssues = $derived(
    state.packages.filter(
      (pkg) =>
        pkg.error ||
        state.hasPackageCompatibilityIssue(pkg) ||
        pkg.dependencies.some((dependency) => dependency.status !== "satisfied" && dependency.status !== "optional_missing")
    )
  );
  const latestDiagnostics = $derived(state.developerErrors.slice(0, 4));
  const runningPackages = $derived(state.packages.filter((pkg) => state.isPackageEnabled(pkg)));
</script>

<div class="page-title">
  <div>
    <h1>{state.t("home.title")}</h1>
    <p>{state.t("home.desc")}</p>
  </div>
</div>

<div class="section-stack">
  <section class="metric-grid home-runtime-grid" aria-label={state.t("home.runtimeStatus")}>
    <button class="metric-cell" type="button" onclick={() => state.openTelemetryHelp()}>
      <strong>{state.telemetryStatusLabel}</strong>
      <span>{state.t("home.telemetryState")}</span>
      <small>{state.telemetryAddress}</small>
    </button>
    <a class="metric-cell" href="/plugins">
      <strong>{state.enabledPackageCount}/{state.packages.length}</strong>
      <span>{state.t("home.activePackagesLabel")}</span>
      <small>{state.t("home.packageRuntime")}</small>
    </a>
    <a class="metric-cell" href="/developer">
      <strong>{state.developerErrors.length}</strong>
      <span>{state.t("home.diagnostics")}</span>
      <small>{state.t("home.diagnosticsMeta")}</small>
    </a>
  </section>

  <section class="card-grid">
    <article class="studio-card home-runtime-card">
      <div>
        <h3>{state.t("home.runtimeStatus")}</h3>
        <p>{state.t("home.runtimeStatusDesc")}</p>
      </div>
      <div class="runtime-version-card">
        <span class="runtime-api-copy">
          <small>{state.t("developer.runtimeApiVersion")}</small>
          <strong>{state.runtimeInfo?.runtimeApiVersion ?? "n/a"}</strong>
        </span>
        <span class="runtime-api-badge">{state.runtimeInfo?.supportedRuntimeApi ?? "n/a"}</span>
      </div>
      <div class="card-actions">
        <a class="btn-secondary" href="/settings">{state.t("home.settings")}</a>
        <button class="btn-outline" type="button" onclick={() => void state.reloadPackages()} disabled={state.busy || state.packagesReloading}>
          {state.t("common.reload")}
        </button>
      </div>
    </article>

    <article class="studio-card">
      <div>
        <h3>{state.t("home.packages")}</h3>
        <p>{state.t("home.packagesDesc")}</p>
      </div>
      {#if runningPackages.length}
        <ul class="home-admin-list">
          {#each runningPackages.slice(0, 5) as pkg (pkg.id)}
            <li>
              <span>
                <strong>{pkg.name}</strong>
                <small>{pkg.id}</small>
              </span>
              <span class="status-pill connected">{state.t("common.enabled")}</span>
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty-state compact">
          <p>{state.t("packages.noneInstalled")}</p>
        </div>
      {/if}
      <div class="card-actions">
        <a class="btn-primary" href="/plugins">{state.t("home.managePackages")}</a>
      </div>
    </article>

    <article class="studio-card">
      <div>
        <h3>{state.t("home.diagnostics")}</h3>
        <p>{state.t("home.diagnosticsDesc")}</p>
      </div>
      {#if latestDiagnostics.length}
        <ul class="home-admin-list">
          {#each latestDiagnostics as entry (entry.id)}
            <li>
              <span>
                <strong>{entry.source}</strong>
                <small>{entry.message}</small>
              </span>
              <small>{entry.receivedAt}</small>
            </li>
          {/each}
        </ul>
      {:else}
        <div class="empty-state compact">
          <p>{state.t("home.noDiagnostics")}</p>
        </div>
      {/if}
      <div class="card-actions">
        <a class="btn-secondary" href="/developer">{state.t("nav.developer")}</a>
      </div>
    </article>
  </section>

  {#if packagesWithIssues.length}
    <section class="studio-panel">
      <div class="panel-heading">
        <h2>{state.t("home.packageIssues")}</h2>
        <p>{state.t("home.packageIssuesDesc")}</p>
      </div>
      <ul class="home-admin-list issue-list">
        {#each packagesWithIssues as pkg (pkg.id)}
          <li>
            <span>
              <strong>{pkg.name}</strong>
              <small>{pkg.error ?? pkg.compatibility.message ?? pkg.dependencies.find((dependency) => dependency.message)?.message ?? pkg.id}</small>
            </span>
            <span class="status-pill disconnected">{state.packageDisplayStateLabel(pkg)}</span>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>
