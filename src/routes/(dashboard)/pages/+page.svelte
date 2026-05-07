<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import LayoutCard from "$lib/dashboard/LayoutCard.svelte";
  import type { PageLayout } from "$lib/dashboard/types";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";

  const dashboard = getDashboardContext();
  let createDialogOpen = $state(false);
  let createPageName = $state("");
  let createOpenTarget = $state<"app" | "window">("app");
  let createWidth = $state(1440);
  let createHeight = $state(900);

  type CardBadge = {
    label: string;
    tone?: "route" | "stream" | "muted" | "warn";
  };

  function handleNameBlur(page: PageLayout, event: FocusEvent) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.value.trim()) {
      input.value = page.name;
      return;
    }
    void dashboard.renamePage(page, input.value);
  }

  function resetCreateDialog() {
    createPageName = "";
    createOpenTarget = "app";
    createWidth = 1440;
    createHeight = 900;
  }

  function closeCreateDialog() {
    createDialogOpen = false;
    resetCreateDialog();
  }

  async function submitCreatePage() {
    const created = await dashboard.createPage({
      name: createPageName,
      openTarget: createOpenTarget,
      width: createWidth,
      height: createHeight
    });
    if (created) closeCreateDialog();
  }

  function pageBadges(page: PageLayout): CardBadge[] {
    const badges: CardBadge[] = [
      { label: page.settings.open_target === "window" ? dashboard.t("pages.window") : dashboard.t("pages.inApp"), tone: "route" }
    ];
    if (page.favorite) badges.push({ label: dashboard.t("pages.favoriteBadge"), tone: "stream" });
    if (page.template_source) badges.push({ label: dashboard.t("common.imported"), tone: "muted" });
    return badges;
  }

  function pageSummary(page: PageLayout) {
    return `${page.width}x${page.height} · ${page.layers.length} ${dashboard.t("common.layers")} · ${dashboard.pageItemCount(page)} ${dashboard.t("common.items")}`;
  }
</script>

<div class="page-title">
  <div>
    <h1>{dashboard.t("pages.title")}</h1>
    <p>{dashboard.t("pages.desc")}</p>
  </div>
  <button class="btn-primary" onclick={() => (createDialogOpen = true)} disabled={dashboard.busy}>
    {dashboard.t("pages.createPage")}
  </button>
</div>

<div class="studio-grid two-col">
  <div class="section-stack">
    {#if dashboard.pages?.pages.length}
      <section class="card-grid" aria-label={dashboard.t("pages.title")}>
        {#each dashboard.pages.pages as page (page.id)}
          <LayoutCard
            name={page.name}
            ariaLabel={dashboard.t("pages.title")}
            summary={pageSummary(page)}
            badges={pageBadges(page)}
            onNameBlur={(event) => handleNameBlur(page, event)}
            onDelete={() => dashboard.deletePage(page)}
            deleteDisabled={dashboard.busy}
            deleteTitle={dashboard.t("confirm.deletePageTitle")}
          >
            {#snippet preview()}
              <OverlayRenderer source="page" layoutOverride={page} mode="page" preview={true} />
            {/snippet}

            {#snippet tools()}
              <button
                class="icon-button"
                class:active={page.favorite}
                onclick={() => void dashboard.togglePageFavorite(page)}
                title={page.favorite ? dashboard.t("pages.unfavorite") : dashboard.t("pages.favorite")}
              >
                <svg viewBox="0 0 24 24" width="15" height="15" fill={page.favorite ? "currentColor" : "none"} stroke="currentColor" stroke-width="2">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon>
                </svg>
              </button>
            {/snippet}

            {#snippet actions()}
              <button class="btn-primary" onclick={() => void dashboard.openPage(page.id)} disabled={dashboard.busy}>
                {dashboard.t("common.open")}
              </button>
              <button class="btn-outline" onclick={() => void dashboard.openPageEditor(page.id)}>
                {dashboard.t("common.edit")}
              </button>
              <button class="btn-outline" onclick={() => void dashboard.duplicatePage(page.id)} disabled={dashboard.busy}>
                {dashboard.t("common.duplicate")}
              </button>
            {/snippet}
          </LayoutCard>
        {/each}
      </section>
    {:else}
      <div class="empty-state">
        <p>{dashboard.t("pages.empty")}</p>
      </div>
    {/if}
  </div>

  <aside class="studio-panel install-panel">
    <div class="panel-heading">
      <div>
        <h2>{dashboard.t("pages.templatesTitle")}</h2>
        <p>{dashboard.t("pages.templatesDesc")}</p>
      </div>
    </div>

    {#if dashboard.pageTemplates.length}
      <div class="section-stack">
        {#each dashboard.pageTemplates as entry}
          <article class="studio-card">
            <div>
              <h3>{entry.page.title ?? entry.page.name}</h3>
              <p>{entry.page.description ?? entry.package.name}</p>
              <span class="package-id">{entry.package.id}/{entry.page.name}</span>
            </div>
            <button class="btn-primary" onclick={() => void dashboard.importPackagePage(entry.package.id, entry.page.name)} disabled={dashboard.busy}>
              {dashboard.t("common.import")}
            </button>
          </article>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <p>{dashboard.t("packages.noneInstalled")}</p>
      </div>
    {/if}
  </aside>
</div>

{#if createDialogOpen}
  <div class="modal-layer">
    <button type="button" class="modal-scrim" aria-label={dashboard.t("common.cancel")} onclick={closeCreateDialog}></button>
    <div
      class="studio-modal create-page-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="create-page-title"
    >
      <form
        class="create-page-form"
        onsubmit={(event) => {
          event.preventDefault();
          void submitCreatePage();
        }}
      >
        <div class="modal-heading">
          <div>
            <h2 id="create-page-title">{dashboard.t("pages.createDialogTitle")}</h2>
            <p>{dashboard.t("pages.createDialogDesc")}</p>
          </div>
          <button type="button" class="icon-button" aria-label={dashboard.t("common.cancel")} onclick={closeCreateDialog}>
            <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M18 6 6 18"></path>
              <path d="m6 6 12 12"></path>
            </svg>
          </button>
        </div>

        <div class="section-stack">
          <div class="input-group">
            <label for="createPageName">{dashboard.t("pages.pageName")}</label>
            <input id="createPageName" bind:value={createPageName} placeholder={dashboard.t("pages.newPagePlaceholder")} />
          </div>

          <div class="input-group">
            <span class="field-label">{dashboard.t("pages.openTarget")}</span>
            <div class="target-options">
              <button
                type="button"
                class="target-option"
                class:active={createOpenTarget === "app"}
                aria-pressed={createOpenTarget === "app"}
                onclick={() => (createOpenTarget = "app")}
              >
                <strong>{dashboard.t("pages.inApp")}</strong>
                <span>{dashboard.t("pages.inAppDesc")}</span>
              </button>
              <button
                type="button"
                class="target-option"
                class:active={createOpenTarget === "window"}
                aria-pressed={createOpenTarget === "window"}
                onclick={() => (createOpenTarget = "window")}
              >
                <strong>{dashboard.t("pages.newWindow")}</strong>
                <span>{dashboard.t("pages.newWindowDesc")}</span>
              </button>
            </div>
          </div>

          <div class="input-group">
            <span class="field-label">{dashboard.t("common.size")}</span>
            <div class="size-grid">
              <label for="createPageWidth">
                {dashboard.t("common.width")}
                <input id="createPageWidth" type="number" min="320" bind:value={createWidth} />
              </label>
              <label for="createPageHeight">
                {dashboard.t("common.height")}
                <input id="createPageHeight" type="number" min="240" bind:value={createHeight} />
              </label>
            </div>
          </div>
        </div>

        <div class="modal-actions">
          <button type="button" class="btn-secondary" onclick={closeCreateDialog}>
            {dashboard.t("common.cancel")}
          </button>
          <button type="submit" class="btn-primary" disabled={dashboard.busy}>
            {dashboard.t("common.create")}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
