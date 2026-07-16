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
  </div>
</div>

<div class="section-stack">
  <section class="home-status-band" aria-label={state.t("home.runtimeStatus")}>
    <button class="home-status-item" type="button" onclick={() => state.openTelemetryHelp()}>
      <span class="home-status-indicator" class:connected={state.telemetryStatus?.state === "connected"} class:connecting={state.telemetryStatus?.state === "connecting"}></span>
      <strong>{state.telemetryStatusLabel}</strong>
      <span>{state.t("home.telemetryState")}</span>
      <small>{state.telemetryAddress}</small>
    </button>
    <a class="home-status-item" href="/plugins">
      <strong>{state.enabledPackageCount}/{state.packages.length}</strong>
      <span>{state.t("home.activePackagesLabel")}</span>
      <small>{state.t("home.packageRuntime")}</small>
    </a>
    <a class="home-status-item" href="/developer">
      <strong>{state.developerErrors.length}</strong>
      <span>{state.t("home.diagnostics")}</span>
      <small>{state.t("home.diagnosticsMeta")}</small>
    </a>
  </section>

  <section class="home-workspace">
    <article class="home-workspace-section home-runtime-section">
      <header class="home-workspace-heading">
        <h2>{state.t("home.runtimeStatus")}</h2>
      </header>
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

    <article class="home-workspace-section">
      <header class="home-workspace-heading">
        <h2>{state.t("home.packages")}</h2>
        <span>{runningPackages.length}</span>
      </header>
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
        <p class="home-empty-row">{state.t("packages.noneInstalled")}</p>
      {/if}
      <div class="card-actions">
        <a class="btn-primary" href="/plugins">{state.t("home.managePackages")}</a>
      </div>
    </article>

    <article class="home-workspace-section home-diagnostics-section">
      <header class="home-workspace-heading">
        <h2>{state.t("home.diagnostics")}</h2>
        <span>{latestDiagnostics.length}</span>
      </header>
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
        <p class="home-empty-row">{state.t("home.noDiagnostics")}</p>
      {/if}
      <div class="card-actions">
        <a class="btn-secondary" href="/developer">{state.t("nav.developer")}</a>
      </div>
    </article>
  </section>

  {#if packagesWithIssues.length}
    <section class="home-issue-section">
      <div class="panel-heading">
        <h2>{state.t("home.packageIssues")}</h2>
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
