<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import LayoutCard from "$lib/dashboard/LayoutCard.svelte";
  import LayoutThumbnail from "$lib/dashboard/LayoutThumbnail.svelte";
  import type { PageLayout } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();
  let createDialogOpen = $state(false);
  let templateDialogOpen = $state(false);
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
  </div>
  <div class="inline-actions">
    {#if dashboard.pageTemplates.length}
      <button class="btn-secondary" onclick={() => (templateDialogOpen = true)} disabled={dashboard.busy}>
        {dashboard.t("common.import")}
      </button>
    {/if}
    <button class="btn-primary" onclick={() => (createDialogOpen = true)} disabled={dashboard.busy}>
      {dashboard.t("common.create")}
    </button>
  </div>
</div>

<div class="section-stack">
  {#if dashboard.pages?.pages.length}
    <section class="card-grid" aria-label={dashboard.t("pages.title")}>
      {#each dashboard.pages.pages as page (page.id)}
        <LayoutCard
          name={page.name}
          ariaLabel={dashboard.t("pages.title")}
          summary={pageSummary(page)}
          actionLabel={dashboard.t("common.open")}
          variant="page"
          badges={pageBadges(page)}
          onNameBlur={(event) => handleNameBlur(page, event)}
          onDelete={() => dashboard.deletePage(page)}
          onOpen={() => dashboard.openPage(page.id)}
          deleteDisabled={dashboard.busy}
          deleteTitle={dashboard.t("confirm.deletePageTitle")}
        >
          {#snippet preview()}
            <LayoutThumbnail thumbnail={page.thumbnail} layout={page} kind="page" themeKey={dashboard.currentTheme} />
          {/snippet}

          {#snippet actions()}
            <button
              class="icon-button"
              onclick={() => void dashboard.openPageEditor(page.id)}
              disabled={dashboard.busy}
              title={dashboard.t("common.edit")}
              aria-label={dashboard.t("common.edit")}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M12 20h9"></path>
                <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"></path>
              </svg>
            </button>
            <button
              class="icon-button"
              class:favorite-toggle={true}
              class:active={page.favorite}
              onclick={() => void dashboard.togglePageFavorite(page)}
              disabled={dashboard.busy}
              aria-pressed={page.favorite}
              title={page.favorite ? dashboard.t("pages.unfavorite") : dashboard.t("pages.favorite")}
              aria-label={page.favorite ? dashboard.t("pages.unfavorite") : dashboard.t("pages.favorite")}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill={page.favorite ? "currentColor" : "none"} stroke="currentColor" stroke-width="2">
                <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon>
              </svg>
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

{#if templateDialogOpen}
  <div class="modal-layer">
    <button type="button" class="modal-scrim" aria-label={dashboard.t("common.cancel")} onclick={() => (templateDialogOpen = false)}></button>
    <div class="studio-modal create-page-modal" role="dialog" aria-modal="true" aria-labelledby="import-page-title">
      <div class="modal-heading">
        <div>
          <h2 id="import-page-title">{dashboard.t("pages.templatesTitle")}</h2>
        </div>
        <button type="button" class="icon-button" aria-label={dashboard.t("common.cancel")} onclick={() => (templateDialogOpen = false)}>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 6 6 18"></path>
            <path d="m6 6 12 12"></path>
          </svg>
        </button>
      </div>
      <div class="template-list">
        {#each dashboard.pageTemplates as entry}
          <button
            type="button"
            class="template-row"
            onclick={() => {
              templateDialogOpen = false;
              void dashboard.importPackagePage(entry.package.id, entry.page.name);
            }}
            disabled={dashboard.busy}
            title={`${entry.package.id}/${entry.page.name}`}
          >
            <span>{entry.page.title ?? entry.page.name}</span>
            <small>{entry.package.name}</small>
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}

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
