<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { getDashboardContext } from "$lib/dashboard/context";
  import type { MarketplaceCatalogPackage } from "$lib/dashboard/types";

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

  function approvedVersion(pkg: MarketplaceCatalogPackage) {
    return pkg.approvedVersions.find((version) => !version.revoked && version.review.status === "approved") ?? null;
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
                  <span>
                    <strong>{titleFor(pkg)}</strong>
                    <span>
                      {#if activeFilter === "new"}
                        {dashboard.t("marketplace.latestApproved")} {approvedVersion(pkg)?.version ?? "-"}
                      {:else}
                        {dashboard.t("marketplace.by")} {pkg.developerName ?? pkg.developerId}
                      {/if}
                    </span>
                  </span>
                </span>
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
            <p>{dashboard.t("marketplace.by")} {selectedPackage.developerName ?? selectedPackage.developerId}</p>
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
          <strong>{approvedVersion(selectedPackage)?.version ?? "-"}</strong>
        </div>

        <div class="card-actions marketplace-actions">
          <button class="btn-primary" onclick={() => void dashboard.installMarketplacePackage(selectedPackage)} disabled={dashboard.busy || !approvedVersion(selectedPackage)}>
            {dashboard.t("marketplace.installReviewed")}
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
