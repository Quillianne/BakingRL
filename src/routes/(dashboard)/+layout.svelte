<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import ConfirmDialog from "$lib/ConfirmDialog.svelte";
  import "$lib/dashboard/dashboard.css";
  import InstallConfirmationModal from "$lib/dashboard/InstallConfirmationModal.svelte";
  import TelemetryHelpDialog from "$lib/dashboard/TelemetryHelpDialog.svelte";
  import ToastViewport from "$lib/dashboard/ToastViewport.svelte";
  import { setDashboardContext } from "$lib/dashboard/context";
  import { createDashboardState } from "$lib/dashboard/state.svelte";

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
  const obsGatewayStateClass = $derived(
    state.obsGatewayStatus === null
      ? "connecting"
      : state.obsGatewayStatus.running
        ? "connected"
        : "disconnected"
  );

  function restoreEditorScroll() {
    const raw = sessionStorage.getItem("bakingrl.editorReturn");
    if (!raw) return;
    sessionStorage.removeItem("bakingrl.editorReturn");
    try {
      const parsed = JSON.parse(raw) as { scrollY?: number };
      const scrollY = Math.max(0, Math.round(Number(parsed.scrollY) || 0));
      requestAnimationFrame(() => {
        const scrollHost = document.querySelector(".studio-main") as HTMLElement | null;
        if (scrollHost) scrollHost.scrollTop = scrollY;
      });
    } catch {
      // Ignore invalid stale state.
    }
  }

  onMount(() => {
    const cleanup = state.start();
    restoreEditorScroll();
    return cleanup;
  });
</script>

<div class="studio-shell">
  <aside class="studio-rail" aria-label="BakingRL">
    <a class="rail-brand" href="/">
      <span class="brand-mark">B</span>
      <div>
        <h1>BakingRL</h1>
        <p>Your RL Companion</p>
      </div>
    </a>

    <nav class="rail-nav" aria-label="Primary">
      <a class="nav-link" class:active={$page.url.pathname === "/"} href="/">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
          <path d="m3 10 9-7 9 7"></path>
          <path d="M5 10v10h14V10"></path>
        </svg>
        <span>{state.t("nav.home")}</span>
      </a>
      <a class="nav-link" class:active={$page.url.pathname.startsWith("/overlays")} href="/overlays">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="3" y="4" width="18" height="14" rx="2"></rect>
          <path d="M8 21h8"></path>
          <path d="M12 18v3"></path>
        </svg>
        <span>{state.t("nav.overlays")}</span>
      </a>
      <a class="nav-link" class:active={$page.url.pathname.startsWith("/pages")} href="/pages">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path>
          <path d="M14 2v6h6"></path>
        </svg>
        <span>{state.t("nav.pages")}</span>
      </a>
      <a class="nav-link" class:active={$page.url.pathname.startsWith("/plugins")} href="/plugins">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M20 7h-9"></path>
          <path d="M14 17H5"></path>
          <circle cx="17" cy="17" r="3"></circle>
          <circle cx="7" cy="7" r="3"></circle>
        </svg>
        <span>{state.t("nav.packages")}</span>
        {#if state.packageErrorCount}
          <span class="nav-badge">{state.packageErrorCount}</span>
        {/if}
      </a>
      <a class="nav-link" class:active={$page.url.pathname.startsWith("/developer")} href="/developer">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
          <path d="m16 18 6-6-6-6"></path>
          <path d="m8 6-6 6 6 6"></path>
        </svg>
        <span>{state.t("nav.developer")}</span>
      </a>
    </nav>

    <div class="rail-spacer"></div>

    <section class="rail-vitals" aria-label="System vitals">
      <span class="rail-section-label">{state.t("shell.systemVitals")}</span>

      <button class="vital-card vital-action" type="button" onclick={() => state.openTelemetryHelp()}>
        <span class="vital-top">
          <span class="vital-label">RL Telemetry</span>
          <span class="status-pill {telemetryStateClass}">
            <span class="status-dot"></span>
            {state.telemetryStatusLabel}
          </span>
        </span>
      </button>

      <div class="vital-card">
        <span class="vital-top">
          <span class="vital-label">{state.t("shell.obsGateway")}</span>
          <span class="status-pill {obsGatewayStateClass}" title={state.obsGatewayStatus?.message ?? state.obsGatewayStatus?.address ?? ""}>
            <span class="status-dot"></span>
            {state.obsGatewayStatusLabel}
          </span>
        </span>
      </div>

      <a class="settings-link" class:active={$page.url.pathname.startsWith("/settings")} href="/settings">
        <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="3"></circle>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06A1.65 1.65 0 0 0 15 19.4a1.65 1.65 0 0 0-1 .6 1.65 1.65 0 0 0-.4 1.08V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 8.6 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-.6-1 1.65 1.65 0 0 0-1.08-.4H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 8.6a1.65 1.65 0 0 0-.33-1.82l-.06-.06A2 2 0 1 1 7.04 3.9l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-.6 1.65 1.65 0 0 0 .4-1.08V3a2 2 0 1 1 4 0v.09A1.65 1.65 0 0 0 15.4 4.6a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9c.14.31.36.58.6.8.28.24.64.4 1.08.4H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"></path>
        </svg>
        <span>{state.t("nav.settings")}</span>
      </a>
    </section>
  </aside>

  <main class="studio-main">
    <div class="studio-content">
      {@render children()}
    </div>
  </main>
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
<ToastViewport toasts={state.toasts} ondismiss={(id) => state.dismissToast(id)} />
