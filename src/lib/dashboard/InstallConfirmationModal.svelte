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
          {#if state.pendingInstall.inspection.verified_developer || state.pendingInstall.inspection.manifest.author}
            <p class="verified-developer-line">
              {state.t("marketplace.by")} {state.pendingInstall.inspection.verified_developer?.name ?? state.pendingInstall.inspection.manifest.author}
              {#if state.pendingInstall.inspection.verified_developer}
                <span class="verified-developer-check" title={state.t("marketplace.verifiedDeveloper")} aria-label={state.t("marketplace.verifiedDeveloper")}>
                  <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                    <path d="m4 8 2.5 2.5L12 5"></path>
                  </svg>
                </span>
              {/if}
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

      <div class="permissions-view">
        <div class="permission-summary">
          <span class="permission-count">{state.permissionTotal(state.pendingInstall.inspection.manifest.permissions)}</span>
          <span>{state.t("packages.permissionsRequired")}</span>
        </div>
        {#if state.permissionTotal(state.pendingInstall.inspection.manifest.permissions) > 0}
          <div class="permission-grid">
            {#each state.permissionSections(state.pendingInstall.inspection.manifest.permissions) as section}
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
          <p class="permission-none">{state.t("packages.noInstallPermissions")}</p>
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
