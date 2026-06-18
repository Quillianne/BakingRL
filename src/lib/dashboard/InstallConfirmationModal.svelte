<script lang="ts">
  import type { DashboardState } from "$lib/dashboard/state.svelte";

  const {
    state
  }: {
    state: DashboardState;
  } = $props();

  const compactNumberFormatter = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1
  });

  function formatInstallSize(bytes: number) {
    const units = ["B", "KB", "MB", "GB"];
    let value = bytes;
    let unitIndex = 0;

    while (value >= 1024 && unitIndex < units.length - 1) {
      value /= 1024;
      unitIndex += 1;
    }

    const maximumFractionDigits = value >= 10 || unitIndex === 0 ? 0 : 1;
    return `${new Intl.NumberFormat(undefined, { maximumFractionDigits }).format(value)} ${units[unitIndex]}`;
  }
</script>

{#if state.pendingInstall}
  <div class="modal-layer">
    <button
      type="button"
      class="modal-scrim"
      aria-label={state.t("common.cancel")}
      onclick={() => void state.cancelPendingInstall()}
    ></button>
    <div
      class="studio-modal install-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="install-confirm-title"
      tabindex="-1"
    >
      <div class="modal-heading">
        <span class="badge route">{state.pendingInstall.kind}</span>
        <h2 id="install-confirm-title">{state.t("packages.confirmInstall")}</h2>
        <p>{state.pendingInstall.label}</p>
      </div>

      <div class="install-identity">
        <div>
          <h3>{state.pendingInstall.inspection.manifest.name}</h3>
          {#if state.pendingInstall.inspection.manifest.author}
            <p class="verified-developer-line">
              {state.t("packages.by")} {state.pendingInstall.inspection.manifest.author}
            </p>
          {/if}
          <p>{state.pendingInstall.inspection.manifest.id}</p>
        </div>
        <span class="version">v{state.pendingInstall.inspection.manifest.version}</span>
      </div>

      <div class="metric-grid compact install-metrics">
        <div class="metric-cell">
          <strong>{state.inspectionContributionCount(state.pendingInstall.inspection)}</strong>
          <span>{state.t("common.contributions")}</span>
        </div>
        <div class="metric-cell">
          <strong>{compactNumberFormatter.format(state.pendingInstall.inspection.file_count)}</strong>
          <span>{state.t("common.files")}</span>
        </div>
        <div class="metric-cell">
          <strong>{formatInstallSize(state.pendingInstall.inspection.uncompressed_size)}</strong>
          <span>{state.t("common.size")}</span>
        </div>
      </div>

      <div class="security-strip">
        <div class="security-item" class:good={state.pendingInstall.inspection.hashes_present} class:warn={!state.pendingInstall.inspection.hashes_present}>
          <span class="status-dot"></span>
          {state.pendingInstall.inspection.hashes_present ? state.t("packages.hashesVerified") : state.t("packages.hashesMissing")}
        </div>
        <div
          class="security-item"
          class:good={state.signatureStatus(state.pendingInstall.inspection) === "verified"}
          class:warn={state.signatureStatus(state.pendingInstall.inspection) !== "verified"}
        >
          <span class="status-dot"></span>
          {state.t("packages.signature")} {state.signatureStatus(state.pendingInstall.inspection)}
        </div>
        {#if state.hasInspectionCompatibilityIssue(state.pendingInstall.inspection)}
          <div
            class="security-item"
            class:warn={state.inspectionCompatibilityClass(state.pendingInstall.inspection) === "warn"}
            class:danger={state.inspectionCompatibilityClass(state.pendingInstall.inspection) === "danger"}
            title={state.inspectionCompatibilityMessage(state.pendingInstall.inspection)}
          >
            <span class="status-dot"></span>
            {state.inspectionCompatibilityLabel(state.pendingInstall.inspection)}
          </div>
        {/if}
      </div>

      <details class="technical-details">
        <summary>{state.t("packages.securityDetails")}</summary>
        <h4>{state.t("packages.bundleSha")}</h4>
        <pre>{state.pendingInstall.inspection.sha256}</pre>
        {#if state.pendingInstall.inspection.signature_public_key}
          <h4>{state.t("packages.signaturePublicKey")}</h4>
          <pre>{state.pendingInstall.inspection.signature_public_key}</pre>
        {/if}
      </details>

      <div class="modal-actions">
        <button class="btn-secondary" onclick={() => void state.cancelPendingInstall()} disabled={state.busy}>
          {state.t("common.cancel")}
        </button>
        <button class="btn-primary" onclick={() => void state.confirmPendingInstall()} disabled={state.busy}>
          {state.t("packages.installPackage")}
        </button>
      </div>
    </div>
  </div>
{/if}
