<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import LayoutCard from "$lib/dashboard/LayoutCard.svelte";
  import LayoutThumbnail from "$lib/dashboard/LayoutThumbnail.svelte";
  import type { OverlayLayout } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();
  let createDialogOpen = $state(false);
  let templateDialogOpen = $state(false);
  let createLayoutName = $state("");
  let createLayoutWidth = $state(1920);
  let createLayoutHeight = $state(1080);

  type CardBadge = {
    label: string;
    tone?: "route" | "stream" | "muted" | "warn";
  };

  function handleNameBlur(layout: OverlayLayout, event: FocusEvent) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.value.trim()) {
      input.value = layout.name;
      return;
    }
    void dashboard.renameLayout(layout, input.value);
  }

  function resetCreateDialog() {
    createLayoutName = "";
    createLayoutWidth = 1920;
    createLayoutHeight = 1080;
  }

  function closeCreateDialog() {
    createDialogOpen = false;
    resetCreateDialog();
  }

  async function submitCreateLayout() {
    const created = await dashboard.createOverlayLayout({
      name: createLayoutName,
      width: createLayoutWidth,
      height: createLayoutHeight
    });
    if (created) closeCreateDialog();
  }

  function layoutBadges(layout: OverlayLayout): CardBadge[] {
    const badges: CardBadge[] = [];
    if (dashboard.isInGameLayout(layout)) badges.push({ label: dashboard.t("overlays.ingame"), tone: "route" });
    if (dashboard.isStreamLayout(layout)) badges.push({ label: dashboard.t("overlays.stream"), tone: "stream" });
    if (!dashboard.isInGameLayout(layout) && !dashboard.isStreamLayout(layout)) {
      badges.push({ label: dashboard.t("common.unassigned"), tone: "muted" });
    }
    if (layout.template_source) badges.push({ label: dashboard.t("common.imported"), tone: "muted" });
    return badges;
  }

  function layoutSummary(layout: OverlayLayout) {
    return `${layout.width}x${layout.height} · ${dashboard.layoutLayerCount(layout)} ${dashboard.t("common.layers")} · ${dashboard.layoutItemCount(layout)} ${dashboard.t("common.items")}`;
  }
</script>

<div class="page-title">
  <div>
    <h1>{dashboard.t("overlays.layoutsTitle")}</h1>
  </div>
  <div class="inline-actions">
    {#if dashboard.layoutTemplates.length}
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
  <section class="studio-panel">
    <div class="panel-heading">
      <div>
        <h2>{dashboard.t("overlays.obsTitle")}</h2>
      </div>
    </div>
    <div class="card-actions obs-copy-actions">
      <button class="btn-outline" onclick={() => void dashboard.copyText(dashboard.streamUrl(), dashboard.t("overlays.generalUrl"))} disabled={!dashboard.obsBaseUrl}>
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M10 13a5 5 0 0 0 7.1 0l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1"></path>
          <path d="M14 11a5 5 0 0 0-7.1 0l-2 2A5 5 0 0 0 12 20.1l1.1-1.1"></path>
        </svg>
        {dashboard.t("overlays.copyGeneralUrl")}
      </button>
    </div>
  </section>

  {#if dashboard.overlayLayouts?.layouts.length}
    <section class="card-grid" aria-label={dashboard.t("overlays.layoutsTitle")}>
      {#each dashboard.overlayLayouts.layouts as layout (layout.id)}
        <LayoutCard
          name={layout.name}
          ariaLabel={dashboard.t("overlays.layoutName")}
          summary={layoutSummary(layout)}
          actionLabel={dashboard.t("common.edit")}
          variant="overlay"
          badges={layoutBadges(layout)}
          onNameBlur={(event) => handleNameBlur(layout, event)}
          onDelete={() => dashboard.deleteLayout(layout)}
          onOpen={() => dashboard.openLayoutEditor(layout.id)}
          deleteDisabled={dashboard.busy || (dashboard.overlayLayouts?.layouts.length ?? 0) <= 1}
          deleteTitle={dashboard.t("confirm.deleteLayoutTitle")}
        >
          {#snippet preview()}
            <LayoutThumbnail thumbnail={layout.thumbnail} layout={layout} kind="overlay" themeKey={dashboard.currentTheme} />
          {/snippet}

          {#snippet actions()}
            <button
              type="button"
              class={`route-toggle route-ingame ${dashboard.isInGameLayout(layout) ? "active" : ""}`}
              aria-label={dashboard.isInGameLayout(layout) ? dashboard.t("overlays.ingame") : dashboard.t("overlays.setIngame")}
              aria-pressed={dashboard.isInGameLayout(layout)}
              title={dashboard.isInGameLayout(layout) ? dashboard.t("overlays.ingame") : dashboard.t("overlays.setIngame")}
              onclick={() => !dashboard.isInGameLayout(layout) && void dashboard.routeOverlayLayout(layout.id)}
              disabled={dashboard.busy}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M6 12h4"></path>
                <path d="M8 10v4"></path>
                <path d="M15 13h.01"></path>
                <path d="M18 11h.01"></path>
                <path d="M17.32 5H6.68a4 4 0 0 0-3.98 3.6C2.6 9.4 2 14.5 2 16a3 3 0 0 0 3 3c1 0 1.5-.5 2-1l1.4-1.4A2 2 0 0 1 9.8 16h4.4a2 2 0 0 1 1.4.6L17 18c.5.5 1 1 2 1a3 3 0 0 0 3-3c0-1.5-.6-6.6-.7-7.4A4 4 0 0 0 17.32 5Z"></path>
              </svg>
            </button>
            <button
              type="button"
              class={`route-toggle route-stream ${dashboard.isStreamLayout(layout) ? "active" : ""}`}
              aria-label={dashboard.isStreamLayout(layout) ? dashboard.t("overlays.stream") : dashboard.t("overlays.setStream")}
              aria-pressed={dashboard.isStreamLayout(layout)}
              title={dashboard.isStreamLayout(layout) ? dashboard.t("overlays.stream") : dashboard.t("overlays.setStream")}
              onclick={() => !dashboard.isStreamLayout(layout) && void dashboard.routeOverlayLayout(layout.id, true)}
              disabled={dashboard.busy}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="4" width="18" height="12" rx="2"></rect>
                <path d="m10 8 5 3-5 3Z"></path>
                <path d="M8 19h8"></path>
                <path d="M12 16v3"></path>
              </svg>
            </button>
            <span class="layout-action-spacer" aria-hidden="true"></span>
            <button
              class="icon-button"
              onclick={() => void dashboard.openPreview(dashboard.layoutUrl(layout.id))}
              disabled={!dashboard.obsBaseUrl}
              title={dashboard.t("common.preview")}
              aria-label={dashboard.t("common.preview")}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z"></path>
                <circle cx="12" cy="12" r="3"></circle>
              </svg>
            </button>
            <button
              class="icon-button"
              onclick={() => void dashboard.copyText(dashboard.layoutUrl(layout.id), dashboard.t("common.copyUrl"))}
              disabled={!dashboard.obsBaseUrl}
              title={dashboard.t("common.copyUrl")}
              aria-label={dashboard.t("common.copyUrl")}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M10 13a5 5 0 0 0 7.1 0l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1"></path>
                <path d="M14 11a5 5 0 0 0-7.1 0l-2 2A5 5 0 0 0 12 20.1l1.1-1.1"></path>
              </svg>
            </button>
          {/snippet}
        </LayoutCard>
      {/each}
    </section>
  {:else}
    <div class="empty-state">
      <p>{dashboard.t("overlays.empty")}</p>
    </div>
  {/if}
</div>

{#if createDialogOpen}
  <div class="modal-layer">
    <button type="button" class="modal-scrim" aria-label={dashboard.t("common.cancel")} onclick={closeCreateDialog}></button>
    <div
      class="studio-modal create-page-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="create-layout-title"
    >
      <form
        class="create-page-form"
        onsubmit={(event) => {
          event.preventDefault();
          void submitCreateLayout();
        }}
      >
        <div class="modal-heading">
          <div>
            <h2 id="create-layout-title">{dashboard.t("overlays.createDialogTitle")}</h2>
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
            <label for="createLayoutName">{dashboard.t("overlays.layoutName")}</label>
            <input id="createLayoutName" bind:value={createLayoutName} placeholder={dashboard.t("overlays.newLayoutPlaceholder")} />
          </div>

          <div class="input-group">
            <span class="field-label">{dashboard.t("common.size")}</span>
            <div class="size-grid">
              <label for="createLayoutWidth">
                {dashboard.t("common.width")}
                <input id="createLayoutWidth" type="number" min="320" bind:value={createLayoutWidth} />
              </label>
              <label for="createLayoutHeight">
                {dashboard.t("common.height")}
                <input id="createLayoutHeight" type="number" min="240" bind:value={createLayoutHeight} />
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

{#if templateDialogOpen}
  <div class="modal-layer">
    <button type="button" class="modal-scrim" aria-label={dashboard.t("common.cancel")} onclick={() => (templateDialogOpen = false)}></button>
    <div class="studio-modal create-page-modal" role="dialog" aria-modal="true" aria-labelledby="import-layout-title">
      <div class="modal-heading">
        <div>
          <h2 id="import-layout-title">{dashboard.t("overlays.templateTitle")}</h2>
        </div>
        <button type="button" class="icon-button" aria-label={dashboard.t("common.cancel")} onclick={() => (templateDialogOpen = false)}>
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M18 6 6 18"></path>
            <path d="m6 6 12 12"></path>
          </svg>
        </button>
      </div>
      <div class="template-list">
        {#each dashboard.layoutTemplates as entry}
          <button
            type="button"
            class="template-row"
            onclick={() => {
              templateDialogOpen = false;
              void dashboard.importPackageLayout(entry.package.id, entry.layoutTemplate.name);
            }}
            disabled={dashboard.busy}
            title={`${entry.package.id}/${entry.layoutTemplate.name}`}
          >
            <span>{entry.layoutTemplate.title ?? entry.layoutTemplate.name}</span>
            <small>{entry.package.name}</small>
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}
