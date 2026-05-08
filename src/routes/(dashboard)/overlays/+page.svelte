<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import LayoutCard from "$lib/dashboard/LayoutCard.svelte";
  import LayoutThumbnail from "$lib/dashboard/LayoutThumbnail.svelte";
  import type { OverlayLayout } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();
  let createDialogOpen = $state(false);
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
    <p>{dashboard.t("overlays.layoutsDesc")}</p>
  </div>
  <button class="btn-primary" onclick={() => (createDialogOpen = true)} disabled={dashboard.busy}>
    {dashboard.t("overlays.createLayout")}
  </button>
</div>

<div class="section-stack">
  <section class="studio-panel">
    <div class="panel-heading">
      <div>
        <h2>{dashboard.t("overlays.obsTitle")}</h2>
        <p>{dashboard.t("overlays.obsDesc")}</p>
      </div>
    </div>
    <div class="card-actions obs-copy-actions">
      <button class="btn-outline" onclick={() => void dashboard.copyText(dashboard.streamUrl(), dashboard.t("overlays.generalUrl"))} disabled={!dashboard.obsBaseUrl}>
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="9" y="9" width="13" height="13" rx="2"></rect>
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
        </svg>
        {dashboard.t("overlays.copyGeneralUrl")}
      </button>
    </div>
  </section>

  {#if dashboard.layoutTemplates.length}
    <section>
      <div class="section-heading">
        <div>
          <h2>{dashboard.t("overlays.templateTitle")}</h2>
          <p>{dashboard.t("overlays.templateDesc")}</p>
        </div>
      </div>
      <div class="card-grid">
        {#each dashboard.layoutTemplates as entry}
          <article class="studio-card">
            <div>
              <h3>{entry.layoutTemplate.title ?? entry.layoutTemplate.name}</h3>
              <p>{entry.layoutTemplate.description ?? entry.package.name}</p>
              <span class="package-id">{entry.package.id}/{entry.layoutTemplate.name}</span>
            </div>
            <button class="btn-primary" onclick={() => void dashboard.importPackageLayout(entry.package.id, entry.layoutTemplate.name)} disabled={dashboard.busy}>
              {dashboard.t("common.import")}
            </button>
          </article>
        {/each}
      </div>
    </section>
  {/if}

  {#if dashboard.overlayLayouts?.layouts.length}
    <section class="card-grid" aria-label={dashboard.t("overlays.layoutsTitle")}>
      {#each dashboard.overlayLayouts.layouts as layout (layout.id)}
        <LayoutCard
          name={layout.name}
          ariaLabel={dashboard.t("overlays.layoutName")}
          summary={layoutSummary(layout)}
          badges={layoutBadges(layout)}
          onNameBlur={(event) => handleNameBlur(layout, event)}
          onDelete={() => dashboard.deleteLayout(layout)}
          deleteDisabled={dashboard.busy || (dashboard.overlayLayouts?.layouts.length ?? 0) <= 1}
          deleteTitle={dashboard.t("confirm.deleteLayoutTitle")}
        >
          {#snippet preview()}
            <LayoutThumbnail thumbnail={layout.thumbnail} name={layout.name} />
          {/snippet}

          {#snippet actions()}
            <button class="btn-primary" onclick={() => void dashboard.openLayoutEditor(layout.id)}>
              {dashboard.t("common.edit")}
            </button>
            <button class="btn-outline" onclick={() => void dashboard.openPreview(dashboard.layoutUrl(layout.id))} disabled={!dashboard.obsBaseUrl}>
              {dashboard.t("common.preview")}
            </button>
            <button class="btn-outline" onclick={() => void dashboard.copyText(dashboard.layoutUrl(layout.id), dashboard.t("common.copyUrl"))} disabled={!dashboard.obsBaseUrl}>
              {dashboard.t("common.copyUrl")}
            </button>
            <button class="btn-secondary" onclick={() => void dashboard.routeOverlayLayout(layout.id)} disabled={dashboard.busy || dashboard.isInGameLayout(layout)}>
              {dashboard.t("overlays.setIngame")}
            </button>
            <button class="btn-secondary" onclick={() => void dashboard.routeOverlayLayout(layout.id, true)} disabled={dashboard.busy || dashboard.isStreamLayout(layout)}>
              {dashboard.t("overlays.setStream")}
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
            <p>{dashboard.t("overlays.createDialogDesc")}</p>
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
