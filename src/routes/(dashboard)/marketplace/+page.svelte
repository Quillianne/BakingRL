<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getDashboardContext } from "$lib/dashboard/context";
  import type {
    MarketplaceApprovedVersion,
    MarketplaceCatalogPackage
  } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();

  type MarketplaceFilter = "all" | "recommended" | "new";

  let selectedPackageId = $state<string | null>(null);
  let activeFilter = $state<MarketplaceFilter>("all");

  const packages = $derived(dashboard.marketplace?.packages ?? []);
  const recommendedIds = $derived(new Set(dashboard.marketplace?.sections.recommended ?? []));
  const newIds = $derived(new Set(dashboard.marketplace?.sections.new ?? []));
  const filteredPackages = $derived(
    packages.filter((pkg) => {
      if (activeFilter === "recommended") return recommendedIds.has(pkg.id);
      if (activeFilter === "new") return newIds.has(pkg.id);
      return true;
    })
  );
  const selectedPackage = $derived(filteredPackages.find((pkg) => pkg.id === selectedPackageId) ?? filteredPackages[0] ?? null);

  onMount(() => {
    void dashboard.loadMarketplace();
  });

  function titleFor(pkg: MarketplaceCatalogPackage) {
    return pkg.listing?.displayName ?? pkg.id;
  }

  function descriptionFor(pkg: MarketplaceCatalogPackage) {
    return pkg.listing?.shortDescription ?? dashboard.t("marketplace.noListing");
  }

  function parseVersion(value: string) {
    const normalized = value.trim().replace(/^v/i, "").split("+", 1)[0];
    const [core, prerelease = ""] = normalized.split("-", 2);
    const parts = core.split(".");
    if (parts.length < 1 || parts.length > 3 || parts.some((part) => !/^\d+$/.test(part))) {
      return null;
    }
    const numbers = parts.map((part) => Number(part));
    while (numbers.length < 3) numbers.push(0);
    return {
      numbers,
      prerelease: prerelease ? prerelease.split(".") : []
    };
  }

  function comparePrerelease(left: string[], right: string[]) {
    if (!left.length && !right.length) return 0;
    if (!left.length) return 1;
    if (!right.length) return -1;

    const length = Math.max(left.length, right.length);
    for (let index = 0; index < length; index += 1) {
      const leftPart = left[index];
      const rightPart = right[index];
      if (leftPart === undefined) return -1;
      if (rightPart === undefined) return 1;
      if (leftPart === rightPart) continue;

      const leftNumeric = /^\d+$/.test(leftPart);
      const rightNumeric = /^\d+$/.test(rightPart);
      if (leftNumeric && rightNumeric) return Number(leftPart) - Number(rightPart);
      if (leftNumeric) return -1;
      if (rightNumeric) return 1;
      return leftPart.localeCompare(rightPart);
    }

    return 0;
  }

  function compareVersions(left: string, right: string) {
    const parsedLeft = parseVersion(left);
    const parsedRight = parseVersion(right);
    if (!parsedLeft || !parsedRight) return left === right ? 0 : -1;

    for (let index = 0; index < 3; index += 1) {
      const difference = parsedLeft.numbers[index] - parsedRight.numbers[index];
      if (difference !== 0) return difference;
    }

    return comparePrerelease(parsedLeft.prerelease, parsedRight.prerelease);
  }

  function approvedVersion(pkg: MarketplaceCatalogPackage) {
    return pkg.approvedVersions
      .filter((version) => !version.revoked && version.review.status === "approved")
      .reduce<MarketplaceApprovedVersion | null>(
        (latest, version) => (!latest || compareVersions(version.version, latest.version) > 0 ? version : latest),
        null
      );
  }

  function installedPackage(pkg: MarketplaceCatalogPackage) {
    return dashboard.packages.find((installed) => installed.id === pkg.id) ?? null;
  }

  function installState(pkg: MarketplaceCatalogPackage) {
    const installed = installedPackage(pkg);
    const latest = approvedVersion(pkg);
    if (!installed) return { kind: "available" as const, installed, latest };
    if (latest && compareVersions(installed.version, latest.version) < 0) {
      return { kind: "update" as const, installed, latest };
    }
    return { kind: "installed" as const, installed, latest };
  }

  function installStateLabel(state: ReturnType<typeof installState>) {
    if (state.kind === "update") return dashboard.t("marketplace.updateAvailable");
    if (state.kind === "installed") return dashboard.t("marketplace.installed");
    return "";
  }

  function installButtonLabel(state: ReturnType<typeof installState>) {
    if (state.kind === "update") return dashboard.t("marketplace.updatePackage");
    if (state.kind === "installed") return dashboard.t("marketplace.installed");
    return dashboard.t("marketplace.installReviewed");
  }

  function installStateDetail(state: ReturnType<typeof installState>) {
    if (!state.installed) return "";
    if (state.kind === "update" && state.latest) {
      return dashboard.tx("marketplace.installedVersionLatest", {
        installed: state.installed.version,
        latest: state.latest.version
      });
    }
    return dashboard.tx("marketplace.installedVersion", { version: state.installed.version });
  }

  function packageCard(pkg: MarketplaceCatalogPackage) {
    selectedPackageId = pkg.id;
  }

  function setFilter(filter: MarketplaceFilter) {
    activeFilter = filter;
    selectedPackageId = null;
  }

  async function openExternal(url: string | null | undefined) {
    if (!url) return;
    try {
      await openUrl(url);
    } catch (error) {
      dashboard.notifyError(error);
    }
  }
</script>

<div class="page-title">
  <div>
    <h1>{dashboard.t("marketplace.title")}</h1>
    <p>{dashboard.t("marketplace.desc")}</p>
  </div>
  <button class="btn-secondary" onclick={() => void dashboard.loadMarketplace({ refresh: true })} disabled={dashboard.marketplaceLoading || dashboard.busy}>
    <svg class="reload-icon" class:spinning={dashboard.marketplaceLoading} viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
      <path d="M21 12a9 9 0 0 1-15.5 6"></path>
      <path d="M3 12a9 9 0 0 1 15.5-6"></path>
      <path d="M18 3v6h-6"></path>
      <path d="M6 21v-6h6"></path>
    </svg>
    {dashboard.t("common.refresh")}
  </button>
</div>

{#if packages.length}
  <div class="studio-grid two-col marketplace-layout">
    <section class="marketplace-main">
      <section class="marketplace-section">
        <div class="section-heading">
          <h2>
            {activeFilter === "recommended"
              ? dashboard.t("marketplace.recommended")
              : activeFilter === "new"
                ? dashboard.t("marketplace.new")
                : dashboard.t("marketplace.allPackages")}
          </h2>
          <div class="inline-actions marketplace-filter-bar" aria-label={dashboard.t("marketplace.filters")}>
            <button type="button" class="btn-outline" class:active={activeFilter === "all"} onclick={() => setFilter("all")}>
              {dashboard.t("marketplace.allPackages")}
            </button>
            <button type="button" class="btn-outline" class:active={activeFilter === "recommended"} onclick={() => setFilter("recommended")}>
              {dashboard.t("marketplace.recommended")}
            </button>
            <button type="button" class="btn-outline" class:active={activeFilter === "new"} onclick={() => setFilter("new")}>
              {dashboard.t("marketplace.new")}
            </button>
          </div>
        </div>

        {#if filteredPackages.length}
          <div class="card-grid marketplace-card-grid">
            {#each filteredPackages as pkg (pkg.id)}
              {@const currentInstallState = installState(pkg)}
              {@const currentApprovedVersion = approvedVersion(pkg)}
              {@const currentCompatibility = currentApprovedVersion ? dashboard.marketplaceVersionCompatibility(currentApprovedVersion) : null}
              <button type="button" class="studio-card marketplace-card" class:active={selectedPackage?.id === pkg.id} onclick={() => packageCard(pkg)}>
                {#if pkg.listing?.bannerUrl}
                  <img class="marketplace-banner" src={pkg.listing.bannerUrl} alt="" loading="lazy" />
                {/if}
                <span class="marketplace-card-head">
                  {#if pkg.listing?.iconUrl}
                    <img class="marketplace-icon" src={pkg.listing.iconUrl} alt="" loading="lazy" />
                  {:else}
                    <span class="marketplace-icon fallback">{titleFor(pkg).slice(0, 1)}</span>
                  {/if}
                  <span class="marketplace-card-copy">
                    <strong>{titleFor(pkg)}</strong>
                    <span class="verified-developer-line">
                      {dashboard.t("marketplace.by")} {pkg.developerName ?? pkg.developerId}
                      {#if pkg.developerVerified}
                        <span class="verified-developer-check" title={dashboard.t("marketplace.verifiedDeveloper")} aria-label={dashboard.t("marketplace.verifiedDeveloper")}>
                          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                            <path d="m4 8 2.5 2.5L12 5"></path>
                          </svg>
                        </span>
                      {/if}
                    </span>
                    {#if activeFilter === "new"}
                      <span>{dashboard.t("marketplace.latestApproved")} {currentApprovedVersion?.version ?? "-"}</span>
                    {/if}
                    {#if currentInstallState.kind !== "available"}
                      <span>
                        {installStateDetail(currentInstallState)}
                      </span>
                    {/if}
                  </span>
                </span>
                {#if currentInstallState.kind !== "available"}
                  <span class="marketplace-status-pill" class:update={currentInstallState.kind === "update"}>
                    {installStateLabel(currentInstallState)}
                  </span>
                {/if}
                {#if currentApprovedVersion}
                  <span class={`marketplace-status-pill compatibility ${dashboard.marketplaceCompatibilityClass(currentCompatibility)}`} title={currentCompatibility?.message}>
                    {dashboard.marketplaceCompatibilityLabel(currentCompatibility)}
                  </span>
                {/if}
                <p>{descriptionFor(pkg)}</p>
              </button>
            {/each}
          </div>
        {:else}
          <div class="empty-state">
            <p>{dashboard.t("marketplace.emptyFilter")}</p>
          </div>
        {/if}
      </section>
    </section>

    <aside class="studio-panel marketplace-detail">
      {#if selectedPackage}
        {@const selectedApprovedVersion = approvedVersion(selectedPackage)}
        {@const selectedInstallState = installState(selectedPackage)}
        {@const selectedCompatibility = selectedApprovedVersion ? dashboard.marketplaceVersionCompatibility(selectedApprovedVersion) : null}
        {#if selectedPackage.listing?.bannerUrl}
          <img class="marketplace-detail-banner" src={selectedPackage.listing.bannerUrl} alt="" loading="lazy" />
        {/if}
        <div class="marketplace-detail-head">
          {#if selectedPackage.listing?.iconUrl}
            <img class="marketplace-icon large" src={selectedPackage.listing.iconUrl} alt="" loading="lazy" />
          {:else}
            <span class="marketplace-icon large fallback">{titleFor(selectedPackage).slice(0, 1)}</span>
          {/if}
          <div>
            <h2>{titleFor(selectedPackage)}</h2>
            <p class="verified-developer-line">
              {dashboard.t("marketplace.by")} {selectedPackage.developerName ?? selectedPackage.developerId}
              {#if selectedPackage.developerVerified}
                <span class="verified-developer-check" title={dashboard.t("marketplace.verifiedDeveloper")} aria-label={dashboard.t("marketplace.verifiedDeveloper")}>
                  <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                    <path d="m4 8 2.5 2.5L12 5"></path>
                  </svg>
                </span>
              {/if}
            </p>
          </div>
        </div>

        <p class="marketplace-long-description">{selectedPackage.listing?.longDescription ?? selectedPackage.listingError ?? dashboard.t("marketplace.noListing")}</p>

        {#if selectedPackage.listing?.tags.length}
          <div class="badge-row marketplace-tags" aria-label={dashboard.t("marketplace.tags")}>
            {#each selectedPackage.listing.tags as tag}
              <span class="badge muted">{tag}</span>
            {/each}
          </div>
        {/if}

        {#if selectedPackage.listing?.screenshots.length}
          <section class="marketplace-screenshots">
            <h3>{dashboard.t("marketplace.screenshots")}</h3>
            {#each selectedPackage.listing.screenshots as screenshot}
              <figure>
                <img src={screenshot.url} alt={screenshot.alt ?? ""} loading="lazy" />
                {#if screenshot.caption}
                  <figcaption>{screenshot.caption}</figcaption>
                {/if}
              </figure>
            {/each}
          </section>
        {/if}

        <div class="marketplace-version">
          <span>{dashboard.t("marketplace.version")}</span>
          <strong>{selectedApprovedVersion?.version ?? "-"}</strong>
        </div>

        <div class={`marketplace-compatibility-state ${dashboard.marketplaceCompatibilityClass(selectedCompatibility)}`} title={selectedCompatibility?.message}>
          <span>{dashboard.t("marketplace.compatibility")}</span>
          <strong>{dashboard.marketplaceCompatibilityLabel(selectedCompatibility)}</strong>
          <small>{selectedCompatibility?.message ?? ""}</small>
        </div>

        {#if selectedInstallState.kind !== "available"}
          <div class="marketplace-install-state" class:update={selectedInstallState.kind === "update"}>
            <span>{installStateLabel(selectedInstallState)}</span>
            <strong>{installStateDetail(selectedInstallState)}</strong>
          </div>
        {/if}

        <div class="card-actions marketplace-actions">
          <button
            class="btn-primary"
            onclick={() => selectedApprovedVersion && void dashboard.installMarketplacePackage(selectedPackage, selectedApprovedVersion.version)}
            disabled={dashboard.busy || !selectedApprovedVersion || selectedInstallState.kind === "installed" || selectedCompatibility?.status !== "compatible"}
          >
            {installButtonLabel(selectedInstallState)}
          </button>
          <button class="btn-secondary" onclick={() => void openExternal(selectedPackage.repo)}>
            {dashboard.t("marketplace.openRepo")}
          </button>
          {#if selectedPackage.listing?.links.docs}
            <button class="btn-secondary" onclick={() => void openExternal(selectedPackage.listing?.links.docs)}>
              {dashboard.t("marketplace.openDocs")}
            </button>
          {/if}
        </div>
      {/if}
    </aside>
  </div>
{:else}
  <section class="studio-panel marketplace-empty">
    <p>{dashboard.marketplaceLoading ? dashboard.t("common.loading") : dashboard.t("marketplace.empty")}</p>
  </section>
{/if}
