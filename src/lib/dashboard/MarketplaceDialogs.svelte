<script lang="ts">
  import type { DashboardState } from "$lib/dashboard/state.svelte";
  import type { MarketplaceInstallPlan, MarketplacePermissions } from "$lib/dashboard/types";

  const { state }: { state: DashboardState } = $props();

  type PlannedPackage = MarketplaceInstallPlan["packages"][number];

  function permissionCount(permissions: MarketplacePermissions) {
    return (
      permissions.bus.read.length +
      permissions.bus.publish.length +
      permissions.registry.read.length +
      permissions.registry.write.length +
      permissions.network.http.length +
      permissions.network.websocket.length +
      permissions.network.listen.length +
      permissions.storage.read.length +
      permissions.storage.write.length
    );
  }

  function nativeCapabilityCount(pkg: PlannedPackage) {
    return (
      (pkg.nativeCapabilities.node ? 1 : 0) +
      pkg.nativeCapabilities.sidecars.length +
      pkg.nativeCapabilities.surfaces.length
    );
  }

  function permissionEntries(permissions: MarketplacePermissions) {
    return [
      ...permissions.bus.read.map((value) => `bus.read: ${value}`),
      ...permissions.bus.publish.map((value) => `bus.publish: ${value}`),
      ...permissions.registry.read.map((value) => `registry.read: ${value}`),
      ...permissions.registry.write.map((value) => `registry.write: ${value}`),
      ...permissions.network.http.map((value) => `network.http: ${JSON.stringify(value)}`),
      ...permissions.network.websocket.map((value) => `network.websocket: ${JSON.stringify(value)}`),
      ...permissions.network.listen.map((value) => `network.listen: ${JSON.stringify(value)}`),
      ...permissions.storage.read.map((value) => `storage.read: ${value}`),
      ...permissions.storage.write.map((value) => `storage.write: ${value}`)
    ];
  }

  function nativeCapabilityEntries(pkg: PlannedPackage) {
    return [
      ...(pkg.nativeCapabilities.node
        ? [`node: ${pkg.nativeCapabilities.node.platforms.join(", ")}`]
        : []),
      ...pkg.nativeCapabilities.sidecars.map(
        (sidecar) => `sidecar.${sidecar.id}: ${sidecar.platforms.join(", ")}`
      ),
      ...pkg.nativeCapabilities.surfaces.map(
        (surface) => `surface.${surface.id}: ${surface.platforms.join(", ")}`
      )
    ];
  }

  function operationLabel(operation: PlannedPackage["operation"]) {
    if (operation === "update") return state.t("marketplace.operationUpdate");
    if (operation === "downgrade") return state.t("marketplace.operationDowngrade");
    if (operation === "reinstall") return state.t("marketplace.operationReinstall");
    return state.t("marketplace.operationInstall");
  }

  function publisherAccepted(trustId: string) {
    return state.marketplaceAcceptedPublishers.includes(trustId);
  }
</script>

{#if state.marketplaceFirstRunOpen && state.marketplaceSnapshot && !state.telemetryHelpOpen && !state.pendingInstall && !state.confirmRequest}
  <div class="modal-layer">
    <button
      type="button"
      class="modal-scrim"
      aria-label={state.t("common.cancel")}
      onclick={() => void state.completeMarketplaceFirstRun()}
    ></button>
    <div
      class="studio-modal marketplace-first-run-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="marketplace-first-run-title"
      tabindex="-1"
    >
      <div class="modal-heading">
        <span class="badge route">Marketplace</span>
        <h2 id="marketplace-first-run-title">{state.t("marketplace.firstRunTitle")}</h2>
        <p>{state.t("marketplace.firstRunDescription")}</p>
      </div>

      <div class="marketplace-selection-list">
        {#each state.marketplaceFirstRunPackages as pkg (pkg.id)}
          <label class="marketplace-selection-row">
            <input
              type="checkbox"
              checked={state.marketplaceFirstRunSelection.includes(pkg.id)}
              onchange={() => state.toggleMarketplaceFirstRunPackage(pkg.id)}
            />
            <span>
              <strong>{pkg.listing.snapshot.displayName}</strong>
              <small>{pkg.listing.snapshot.shortDescription}</small>
            </span>
          </label>
        {/each}
      </div>

      <div class="callout marketplace-trust-note">
        <strong>{state.t("marketplace.trustTitle")}</strong>
        <span>{state.t("marketplace.trustDisclaimer")}</span>
      </div>

      <div class="modal-actions">
        <button class="btn-secondary" onclick={() => void state.completeMarketplaceFirstRun()} disabled={state.busy}>
          {state.t("marketplace.notNow")}
        </button>
        <button
          class="btn-primary"
          onclick={() => void state.prepareMarketplaceInstall(state.marketplaceFirstRunSelection, true)}
          disabled={state.busy || state.marketplaceFirstRunSelection.length === 0}
        >
          {state.t("marketplace.reviewSelection")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if state.marketplacePlan}
  <div class="modal-layer">
    <button
      type="button"
      class="modal-scrim"
      aria-label={state.t("common.cancel")}
      onclick={() => void state.cancelMarketplaceInstall()}
    ></button>
    <div
      class="studio-modal marketplace-review-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="marketplace-review-title"
      tabindex="-1"
    >
      <div class="modal-heading">
        <span class="badge route">Marketplace</span>
        <h2 id="marketplace-review-title">{state.t("marketplace.reviewTitle")}</h2>
        <p>{state.tx("marketplace.reviewCount", { count: state.marketplacePlan.packages.length })}</p>
      </div>

      <div class="marketplace-review-scroll">
        <section class="marketplace-review-section">
          <div class="marketplace-review-heading">
            <h3>{state.t("marketplace.packagesTitle")}</h3>
            <span class="section-count">{state.marketplacePlan.packages.length}</span>
          </div>
          <div class="marketplace-plan-list">
            {#each state.marketplacePlan.packages as pkg (pkg.packageId)}
              <article class="marketplace-plan-row">
                <div class="marketplace-plan-main">
                  <strong>{pkg.displayName}</strong>
                  <span>{pkg.packageId} · v{pkg.version}</span>
                </div>
                <div class="marketplace-plan-facts">
                  <span>{operationLabel(pkg.operation)}</span>
                  <span>{state.tx("marketplace.permissionCount", { count: permissionCount(pkg.permissions) })}</span>
                  {#if nativeCapabilityCount(pkg) > 0}
                    <span>{state.tx("marketplace.nativeCount", { count: nativeCapabilityCount(pkg) })}</span>
                  {/if}
                  {#if pkg.dependencies.length > 0}
                    <span>{state.tx("marketplace.dependencyCount", { count: pkg.dependencies.length })}</span>
                  {/if}
                </div>
                {#if permissionEntries(pkg.permissions).length || nativeCapabilityEntries(pkg).length || pkg.dependencies.length}
                  <details class="marketplace-plan-details">
                    <summary>{state.t("marketplace.accessDetails")}</summary>
                    {#if permissionEntries(pkg.permissions).length}
                      <strong>{state.t("marketplace.permissionsTitle")}</strong>
                      <ul>
                        {#each permissionEntries(pkg.permissions) as permission}
                          <li><code>{permission}</code></li>
                        {/each}
                      </ul>
                    {/if}
                    {#if nativeCapabilityEntries(pkg).length}
                      <strong>{state.t("marketplace.nativeCapabilitiesTitle")}</strong>
                      <ul>
                        {#each nativeCapabilityEntries(pkg) as capability}
                          <li><code>{capability}</code></li>
                        {/each}
                      </ul>
                    {/if}
                    {#if pkg.dependencies.length}
                      <strong>{state.t("marketplace.dependenciesTitle")}</strong>
                      <ul>
                        {#each pkg.dependencies as dependency}
                          <li><code>{dependency}</code></li>
                        {/each}
                      </ul>
                    {/if}
                  </details>
                {/if}
              </article>
            {/each}
          </div>
        </section>

        <section class="marketplace-review-section">
          <div class="marketplace-review-heading">
            <h3>{state.t("marketplace.publishersTitle")}</h3>
            <span class="section-count">{state.marketplacePlan.publishers.length}</span>
          </div>
          <div class="marketplace-publisher-list">
            {#each state.marketplacePlan.publishers as publisher (publisher.trustId)}
              <div class="marketplace-publisher-row">
                <div>
                  <strong>{publisher.name}</strong>
                  <span>{publisher.developerId} · {publisher.verification}</span>
                  <code>{publisher.keyFingerprint}</code>
                </div>
                {#if publisher.trusted}
                  <span class="status-pill connected">
                    <span class="status-dot"></span>
                    {state.t("marketplace.trusted")}
                  </span>
                {:else}
                  <label class="marketplace-publisher-consent">
                    <input
                      type="checkbox"
                      checked={publisherAccepted(publisher.trustId)}
                      onchange={() => state.toggleMarketplacePublisher(publisher.trustId)}
                    />
                    <span>{state.t("marketplace.trustPublisher")}</span>
                  </label>
                {/if}
              </div>
            {/each}
          </div>
        </section>

        <div class="callout marketplace-trust-note">
          <strong>{state.t("marketplace.trustTitle")}</strong>
          <span>{state.t("marketplace.trustDisclaimer")}</span>
        </div>
      </div>

      <div class="modal-actions">
        <button class="btn-secondary" onclick={() => void state.cancelMarketplaceInstall()} disabled={state.busy}>
          {state.t("common.cancel")}
        </button>
        <button
          class="btn-primary"
          onclick={() => void state.commitMarketplaceInstall()}
          disabled={state.busy || !state.marketplaceConsentComplete}
        >
          {state.t("marketplace.installApproved")}
        </button>
      </div>
    </div>
  </div>
{/if}
