<script lang="ts">
  import {
    ArrowRight,
    Blocks,
    CircleAlert,
    RadioTower,
    RefreshCw,
    Settings2,
    SquareTerminal
  } from "@lucide/svelte";
  import { getDashboardContext } from "$lib/dashboard/context";

  const state = getDashboardContext();

  const packagesWithIssues = $derived(
    state.packages.filter(
      (pkg) =>
        pkg.error ||
        state.hasPackageCompatibilityIssue(pkg) ||
        pkg.dependencies.some(
          (dependency) =>
            dependency.status !== "satisfied" && dependency.status !== "optional_missing"
        )
    )
  );
  const latestDiagnostics = $derived(state.developerErrors.slice(0, 6));
  const runningPackages = $derived(state.packages.filter((pkg) => state.isPackageEnabled(pkg)));
  const telemetryStateClass = $derived(
    state.telemetryStatus?.state === "connected"
      ? "connected"
      : state.telemetryStatus?.state === "connecting"
        ? "connecting"
        : "disconnected"
  );
</script>

<header class="page-title control-page-title">
  <div>
    <span class="page-index">01 / {state.t("shell.workspaceName")}</span>
    <h1>{state.t("home.title")}</h1>
  </div>
  <div class="page-tools">
    <button
      class="icon-button"
      type="button"
      onclick={() => void state.reloadPackages()}
      disabled={state.busy || state.packagesReloading}
      aria-label={state.t("common.reload")}
      title={state.t("common.reload")}
    >
      <RefreshCw size={16} strokeWidth={1.8} class={state.packagesReloading ? "spinning" : ""} />
    </button>
    <a class="icon-button" href="/settings" aria-label={state.t("nav.settings")} title={state.t("nav.settings")}>
      <Settings2 size={16} strokeWidth={1.8} />
    </a>
  </div>
</header>

<section class="signal-board" aria-label={state.t("home.runtimeStatus")}>
  <button class="signal-channel" type="button" onclick={() => state.openTelemetryHelp()}>
    <span class="channel-number">01</span>
    <RadioTower size={20} strokeWidth={1.6} />
    <span class="channel-copy">
      <small>{state.t("home.telemetryState")}</small>
      <strong>{state.telemetryStatusLabel}</strong>
      <span>{state.telemetryAddress}</span>
    </span>
    <i class={telemetryStateClass} aria-hidden="true"></i>
  </button>

  <a class="signal-channel" href="/plugins">
    <span class="channel-number">02</span>
    <Blocks size={20} strokeWidth={1.6} />
    <span class="channel-copy">
      <small>{state.t("home.activePackagesLabel")}</small>
      <strong>{state.enabledPackageCount} / {state.packages.length}</strong>
      <span>{state.t("home.packageRuntime")}</span>
    </span>
    <i class:connected={packagesWithIssues.length === 0} class:disconnected={packagesWithIssues.length > 0} aria-hidden="true"></i>
  </a>

  <a class="signal-channel" href="/developer">
    <span class="channel-number">03</span>
    <SquareTerminal size={20} strokeWidth={1.6} />
    <span class="channel-copy">
      <small>{state.t("home.diagnostics")}</small>
      <strong>{state.developerErrors.length}</strong>
      <span>{state.t("home.diagnosticsMeta")}</span>
    </span>
    <i class:connected={state.developerErrors.length === 0} class:disconnected={state.developerErrors.length > 0} aria-hidden="true"></i>
  </a>
</section>

<div class="control-workgrid">
  <section class="control-section package-operations">
    <header class="control-section-heading">
      <div>
        <span>02</span>
        <h2>{state.t("home.packages")}</h2>
      </div>
      <strong>{runningPackages.length}</strong>
    </header>

    <div class="operations-list">
      {#if runningPackages.length}
        {#each runningPackages as pkg, index (pkg.id)}
          <div class="operation-row">
            <span class="operation-index">{String(index + 1).padStart(2, "0")}</span>
            <i class="connected" aria-hidden="true"></i>
            <span class="operation-name">
              <strong>{pkg.name}</strong>
              <small>{pkg.id}</small>
            </span>
            <span class="operation-version">v{pkg.version}</span>
            <span class="operation-state">{state.packageDisplayStateLabel(pkg)}</span>
          </div>
        {/each}
      {:else}
        <p class="control-empty">{state.t("packages.noneInstalled")}</p>
      {/if}
    </div>

    <a class="section-command" href="/plugins">
      {state.t("home.managePackages")}
      <ArrowRight size={15} strokeWidth={1.8} />
    </a>
  </section>

  <section class="control-section diagnostic-feed">
    <header class="control-section-heading">
      <div>
        <span>03</span>
        <h2>{state.t("home.diagnostics")}</h2>
      </div>
      <strong>{latestDiagnostics.length}</strong>
    </header>

    <div class="diagnostic-list">
      {#if latestDiagnostics.length}
        {#each latestDiagnostics as entry (entry.id)}
          <div class="diagnostic-row">
            <CircleAlert size={15} strokeWidth={1.7} />
            <span>
              <strong>{entry.source}</strong>
              <small>{entry.message}</small>
            </span>
            <time>{entry.receivedAt}</time>
          </div>
        {/each}
      {:else}
        <p class="control-empty">{state.t("home.noDiagnostics")}</p>
      {/if}
    </div>

    <a class="section-command" href="/developer">
      {state.t("nav.developer")}
      <ArrowRight size={15} strokeWidth={1.8} />
    </a>
  </section>
</div>

{#if packagesWithIssues.length}
  <section class="control-section issue-board">
    <header class="control-section-heading warning">
      <div>
        <span>04</span>
        <h2>{state.t("home.packageIssues")}</h2>
      </div>
      <strong>{packagesWithIssues.length}</strong>
    </header>
    <div class="operations-list">
      {#each packagesWithIssues as pkg (pkg.id)}
        <div class="operation-row issue">
          <CircleAlert size={15} strokeWidth={1.7} />
          <span class="operation-name">
            <strong>{pkg.name}</strong>
            <small>{pkg.error ?? pkg.compatibility.message ?? pkg.dependencies.find((dependency) => dependency.message)?.message ?? pkg.id}</small>
          </span>
          <span class="operation-state">{state.packageDisplayStateLabel(pkg)}</span>
        </div>
      {/each}
    </div>
  </section>
{/if}
