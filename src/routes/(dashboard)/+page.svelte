<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    ArrowRight,
    ArrowUpRight,
    BarChart3,
    Blocks,
    CircleCheck,
    RadioTower,
    RefreshCw,
    Settings2,
    TriangleAlert
  } from "@lucide/svelte";
  import { getDashboardContext } from "$lib/dashboard/context";
  import {
    resolvePackagePrimaryAction
  } from "$lib/dashboard/packagePresentation";
  import type { PackageDescriptor } from "$lib/dashboard/types";

  const state = getDashboardContext();
  const diagnosticIssues = $derived(
    state.developerErrors.filter((entry) => entry.severity !== "info")
  );
  const latestIssues = $derived(diagnosticIssues.slice(0, 3));
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
  const toolPackages = $derived(
    state.packages
      .filter((pkg) => resolvePackagePrimaryAction(pkg) !== null)
      .sort((a, b) => a.name.localeCompare(b.name))
  );
  const telemetryStateClass = $derived(
    state.telemetryStatus?.state === "connected"
      ? "connected"
      : state.telemetryStatus?.state === "connecting"
        ? "connecting"
        : "disconnected"
  );

  function packageActionLabel(pkg: PackageDescriptor) {
    const action = resolvePackagePrimaryAction(pkg);
    return state.tx(action?.configuration ? "home.configureTool" : "home.openTool", {
      name: pkg.name
    });
  }

  function packageActionDisabled(pkg: PackageDescriptor) {
    const action = resolvePackagePrimaryAction(pkg);
    if (!action) return true;
    if (state.busy || state.isPackageTogglePending(pkg) || state.isPackageDeleting(pkg)) return true;
    if (action.kind === "settings") return !pkg.has_public_settings;
    return !state.isPackageEnabled(pkg) || !state.isPackageCompatible(pkg);
  }

  async function openPackage(pkg: PackageDescriptor) {
    const action = resolvePackagePrimaryAction(pkg);
    if (!action) return;
    try {
      if (action.kind === "webview") {
        await invoke("open_package_webview", { packageId: pkg.id, webviewId: action.target });
      } else {
        await invoke("open_package_configuration", { packageId: pkg.id });
      }
    } catch (error) {
      state.notifyError(error);
    }
  }
</script>

<header class="page-title workspace-page-title">
  <div>
    <span class="page-index">{state.t("home.eyebrow")}</span>
    <h1>{state.t("home.title")}</h1>
    <p>{state.t("home.desc")}</p>
  </div>
  <div class="page-tools">
    <button
      class="btn-secondary header-action"
      type="button"
      onclick={() => void state.reloadPackages()}
      disabled={state.busy || state.packagesReloading}
    >
      <RefreshCw size={15} strokeWidth={1.8} class={state.packagesReloading ? "spinning" : ""} />
      {state.t("common.reload")}
    </button>
    <a class="icon-button" href="/settings" aria-label={state.t("nav.settings")} title={state.t("nav.settings")}>
      <Settings2 size={16} strokeWidth={1.8} />
    </a>
  </div>
</header>

<section class="home-toolbox" aria-label={state.t("home.toolsTitle")}>
  <header class="home-toolbox-heading">
    <div>
      <span>{state.t("home.toolsEyebrow")}</span>
      <h2>{state.t("home.toolsTitle")}</h2>
      <p>{state.t("home.toolsDesc")}</p>
    </div>
    <strong>{toolPackages.length}</strong>
  </header>

  <div class="home-tool-grid">
    {#if toolPackages.length}
      {#each toolPackages as pkg (pkg.id)}
        {@const action = resolvePackagePrimaryAction(pkg)}
        <button
          class="home-tool-card"
          type="button"
          onclick={() => void openPackage(pkg)}
          disabled={packageActionDisabled(pkg)}
        >
          <span class="home-tool-mark" aria-hidden="true">{pkg.name.slice(0, 2).toLocaleUpperCase()}</span>
          <span class="home-tool-copy">
            <strong>{pkg.name}</strong>
            <small>{state.packageDisplayStateLabel(pkg)}</small>
          </span>
          <span class:configuration={action?.configuration} class="home-tool-action">
            {packageActionLabel(pkg)}
            <ArrowUpRight size={15} strokeWidth={1.8} />
          </span>
        </button>
      {/each}
    {:else}
      <p class="home-tool-empty">{state.t("home.noTools")}</p>
    {/if}
  </div>

  <footer class="home-toolbox-footer">
    <span>{state.t("home.toolsFooter")}</span>
    <a href="/plugins">{state.t("home.managePackages")} <ArrowRight size={14} /></a>
  </footer>
</section>

<section class="home-status-strip" aria-label={state.t("home.systemStatus")}>
  <button class="home-status-item" type="button" onclick={() => state.openTelemetryHelp()}>
    <RadioTower size={18} strokeWidth={1.7} />
    <span>
      <small>{state.t("home.telemetryState")}</small>
      <strong>{state.telemetryStatusLabel}</strong>
    </span>
    <i class={telemetryStateClass} aria-hidden="true"></i>
  </button>

  <a class="home-status-item" href="/plugins">
    <Blocks size={18} strokeWidth={1.7} />
    <span>
      <small>{state.t("home.activePackagesLabel")}</small>
      <strong>{state.enabledPackageCount} / {state.packages.length}</strong>
    </span>
    <i class:connected={packagesWithIssues.length === 0} class:disconnected={packagesWithIssues.length > 0} aria-hidden="true"></i>
  </a>

  <a class="home-status-item" class:needs-attention={diagnosticIssues.length > 0} href="/developer">
    {#if diagnosticIssues.length}
      <TriangleAlert size={18} strokeWidth={1.7} />
    {:else}
      <CircleCheck size={18} strokeWidth={1.7} />
    {/if}
    <span>
      <small>{state.t("home.diagnostics")}</small>
      <strong>{diagnosticIssues.length ? state.tx("home.issueCount", { count: diagnosticIssues.length }) : state.t("home.noIssues")}</strong>
    </span>
    <i class:connected={diagnosticIssues.length === 0} class:disconnected={diagnosticIssues.length > 0} aria-hidden="true"></i>
  </a>
</section>

{#if latestIssues.length}
  <section class="home-issue-panel">
    <header>
      <span><TriangleAlert size={16} strokeWidth={1.7} /> {state.t("home.attentionRequired")}</span>
      <a href="/developer">{state.t("home.viewDiagnostics")} <ArrowRight size={14} /></a>
    </header>
    <div>
      {#each latestIssues as entry (entry.id)}
        <article>
          <BarChart3 size={15} strokeWidth={1.7} />
          <span>
            <strong>{entry.source}</strong>
            <small>{entry.message}</small>
          </span>
          <time>{entry.receivedAt}</time>
        </article>
      {/each}
    </div>
  </section>
{/if}
