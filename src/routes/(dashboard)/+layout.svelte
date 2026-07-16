<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { Activity, Blocks, House, Settings2, SquareTerminal } from "@lucide/svelte";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import "$lib/dashboard/dashboard.css";
  import InstallConfirmationModal from "$lib/dashboard/InstallConfirmationModal.svelte";
  import MarketplaceDialogs from "$lib/dashboard/MarketplaceDialogs.svelte";
  import TelemetryHelpDialog from "$lib/dashboard/TelemetryHelpDialog.svelte";
  import ToastViewport from "$lib/dashboard/ToastViewport.svelte";
  import { setDashboardContext } from "$lib/dashboard/context";
  import { createDashboardState } from "$lib/dashboard/state.svelte";
  import { consumeRouteScrollRestore } from "$lib/returnState";

  let { children } = $props();

  const state = createDashboardState();
  setDashboardContext(state);

  const telemetryStateClass = $derived(
    state.telemetryStatus?.state === "connected"
      ? "connected"
      : state.telemetryStatus?.state === "connecting"
        ? "connecting"
        : "disconnected"
  );

  function restoreEditorScroll() {
    const restoreState = consumeRouteScrollRestore();
    if (!restoreState) return;
    const scrollY = Math.max(0, Math.round(Number(restoreState.scrollY) || 0));
    requestAnimationFrame(() => {
      const scrollHost = document.querySelector(".studio-main") as HTMLElement | null;
      if (scrollHost) scrollHost.scrollTop = scrollY;
    });
  }

  onMount(() => {
    const cleanup = state.start();
    restoreEditorScroll();
    return cleanup;
  });
</script>

<div class="studio-shell">
  <header class="studio-commandbar">
    <a class="rail-brand" href="/" aria-label="BakingRL">
      <span class="rail-brand-mark" aria-hidden="true"><b>B</b><b>RL</b></span>
      <span class="rail-brand-copy">
        <strong>BakingRL</strong>
        <small>{state.t("shell.workspaceName")}</small>
      </span>
    </a>

    <nav class="rail-nav" aria-label="Primary">
      <a class="nav-link" class:active={$page.url.pathname === "/"} href="/" aria-label={state.t("nav.home")} title={state.t("nav.home")}>
        <House size={17} strokeWidth={1.8} />
        <span>{state.t("nav.home")}</span>
      </a>
      <a class="nav-link" class:active={$page.url.pathname.startsWith("/plugins")} href="/plugins" aria-label={state.t("nav.packages")} title={state.t("nav.packages")}>
        <Blocks size={17} strokeWidth={1.8} />
        <span>{state.t("nav.packages")}</span>
        {#if state.packageErrorCount}
          <span class="nav-count">{state.packageErrorCount}</span>
        {/if}
      </a>
      <a class="nav-link" class:active={$page.url.pathname.startsWith("/developer")} href="/developer" aria-label={state.t("nav.developer")} title={state.t("nav.developer")}>
        <SquareTerminal size={17} strokeWidth={1.8} />
        <span>{state.t("nav.developer")}</span>
      </a>
      <a class="nav-link settings-link" class:active={$page.url.pathname.startsWith("/settings")} href="/settings" aria-label={state.t("nav.settings")} title={state.t("nav.settings")}>
        <Settings2 size={17} strokeWidth={1.8} />
        <span>{state.t("nav.settings")}</span>
      </a>
    </nav>

    <button class="command-telemetry {telemetryStateClass}" type="button" onclick={() => state.openTelemetryHelp()} title={state.telemetryStatusLabel}>
      <Activity size={17} strokeWidth={1.8} />
      <span>
        <small>{state.t("shell.telemetry")}</small>
        <strong>{state.telemetryStatusLabel}</strong>
      </span>
      <i aria-hidden="true"></i>
    </button>
  </header>

  <main class="studio-main" class:developer-main={$page.url.pathname.startsWith("/developer")}>
    <div class="studio-content">
      {@render children()}
    </div>
  </main>

  <footer class="studio-statusbar">
    <button type="button" onclick={() => state.openTelemetryHelp()}>
      <i class={telemetryStateClass}></i>
      {state.telemetryStatusLabel}
      <span>{state.telemetryAddress}</span>
    </button>
    <span>{state.t("shell.runtimeApi")} <strong>{state.runtimeInfo?.runtimeApiVersion ?? "n/a"}</strong></span>
    <span>{state.enabledPackageCount}/{state.packages.length} {state.t("common.plugins")}</span>
    {#if state.packageErrorCount}
      <a href="/plugins" class="statusbar-error">{state.packageErrorCount} {state.t("common.error")}</a>
    {/if}
  </footer>
</div>

<ConfirmDialog
  open={state.confirmRequest !== null}
  title={state.confirmRequest?.title}
  message={state.confirmRequest?.message}
  confirmLabel={state.confirmRequest?.confirmLabel}
  cancelLabel={state.t("common.cancel")}
  danger={state.confirmRequest?.danger ?? false}
  onconfirm={() => void state.confirmAction()}
  oncancel={() => state.cancelConfirmation()}
/>

<TelemetryHelpDialog {state} />
<InstallConfirmationModal {state} />
<MarketplaceDialogs {state} />
<ToastViewport toasts={state.toasts} ondismiss={(id) => state.dismissToast(id)} />
