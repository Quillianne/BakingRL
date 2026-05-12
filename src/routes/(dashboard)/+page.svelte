<script lang="ts">
  import { onMount } from "svelte";
  import { getDashboardContext } from "$lib/dashboard/context";
  import LayoutThumbnail from "$lib/dashboard/LayoutThumbnail.svelte";
  import type { MarketplaceCatalogPackage } from "$lib/dashboard/types";

  const state = getDashboardContext();

  const mainLayouts = $derived([
    { kind: "ingame", label: state.t("home.ingameLayout"), layout: state.homeInGameLayout },
    { kind: "stream", label: state.t("home.streamLayout"), layout: state.homeStreamLayout }
  ] as const);
  const recommendedMarketplacePackages = $derived(
    sectionPackages(state.marketplace?.sections.recommended ?? []).filter(
      (pkg) => !state.packages.some((installed) => installed.id === pkg.id)
    )
  );

  onMount(() => {
    if (!state.marketplace && !state.marketplaceLoading) {
      void state.loadMarketplace();
    }
  });

  function sectionPackages(ids: string[]) {
    const byId = new Map((state.marketplace?.packages ?? []).map((pkg) => [pkg.id, pkg]));
    return ids.map((id) => byId.get(id)).filter((pkg): pkg is MarketplaceCatalogPackage => Boolean(pkg));
  }

  function titleFor(pkg: MarketplaceCatalogPackage) {
    return pkg.listing?.displayName ?? pkg.id;
  }

  function approvedVersion(pkg: MarketplaceCatalogPackage) {
    return pkg.approvedVersions.find((version) => !version.revoked && version.review.status === "approved") ?? null;
  }
</script>

<div class="page-title">
  <div>
    <h1>{state.t("home.title")}</h1>
    <p>{state.t("home.desc")}</p>
  </div>
</div>

<div class="section-stack">
  <section>
    <div class="section-heading">
      <div>
        <h2>{state.t("home.mainLayouts")}</h2>
      </div>
    </div>

    <div class="card-grid home-card-grid">
      {#each mainLayouts as entry}
        <article
          class="thumb-card home-thumb-card home-click-card {entry.kind === 'stream' ? 'role-stream' : 'role-route'}"
          title={entry.layout ? `${entry.label} · ${entry.layout.width}x${entry.layout.height}` : entry.label}
        >
          <div class="preview-click-zone home-preview-zone">
            <div
              class="thumb-preview"
              aria-hidden="true"
              style={entry.layout ? `--layout-width: ${entry.layout.width};` : ""}
              data-action-label={entry.layout ? state.t("common.edit") : ""}
            >
              {#if entry.layout}
                <LayoutThumbnail thumbnail={entry.layout.thumbnail} layout={entry.layout} kind="overlay" themeKey={state.currentTheme} />
              {/if}
            </div>
            {#if entry.layout}
              <button
                type="button"
                class="preview-hit-target"
                aria-label={`${state.t("common.edit")} ${entry.layout?.name ?? ""}`}
                onclick={() => entry.layout && void state.openLayoutEditor(entry.layout.id)}
              ></button>
            {/if}
          </div>
          <div class="thumb-copy">
            <div class="thumb-title-row">
              <span class="thumb-title">{entry.layout?.name ?? state.t("home.noLayout")}</span>
            </div>
          </div>
        </article>
      {/each}
    </div>
  </section>

  <section>
    <div class="section-heading">
      <div>
        <h2>{state.t("home.favoritePages")}</h2>
      </div>
    </div>

    {#if state.favoritePages.length}
      <div class="card-grid home-card-grid">
        {#each state.favoritePages as page (page.id)}
          <article
            class="thumb-card home-thumb-card home-click-card"
            title={`${page.width}x${page.height}`}
          >
            <div class="preview-click-zone home-preview-zone">
              <div class="thumb-preview" aria-hidden="true" style={`--layout-width: ${page.width};`} data-action-label={state.t("common.open")}>
                <LayoutThumbnail thumbnail={page.thumbnail} layout={page} kind="page" themeKey={state.currentTheme} />
              </div>
              <button
                type="button"
                class="preview-hit-target"
                aria-label={`${state.t("common.open")} ${page.name}`}
                onclick={() => void state.openPage(page.id)}
              ></button>
            </div>
            <div class="thumb-copy">
              <span class="thumb-title">{page.name}</span>
            </div>
          </article>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <p>{state.t("home.noFavoritePages")}</p>
      </div>
    {/if}
  </section>

  {#if recommendedMarketplacePackages.length}
    <section>
      <div class="section-heading">
        <div>
          <h2>{state.t("home.recommendedPackages")}</h2>
        </div>
      </div>

      <div class="card-grid home-marketplace-grid">
        {#each recommendedMarketplacePackages as pkg (pkg.id)}
          <article class="studio-card marketplace-card home-marketplace-card">
            <span class="marketplace-card-head">
              {#if pkg.listing?.iconUrl}
                <img class="marketplace-icon" src={pkg.listing.iconUrl} alt="" loading="lazy" />
              {:else}
                <span class="marketplace-icon fallback">{titleFor(pkg).slice(0, 1)}</span>
              {/if}
              <span>
                <strong>{titleFor(pkg)}</strong>
                <span>{state.t("marketplace.latestApproved")} {approvedVersion(pkg)?.version ?? "-"}</span>
              </span>
            </span>
            <div class="card-actions home-marketplace-actions">
              <button class="btn-primary" onclick={() => void state.installMarketplacePackage(pkg)} disabled={state.busy || !approvedVersion(pkg)}>
                {state.t("marketplace.installReviewed")}
              </button>
            </div>
          </article>
        {/each}
      </div>
    </section>
  {/if}
</div>
