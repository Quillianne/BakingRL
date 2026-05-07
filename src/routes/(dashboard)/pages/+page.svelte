<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import type { PageLayout } from "$lib/dashboard/types";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";

  const state = getDashboardContext();

  function handleNameBlur(page: PageLayout, event: FocusEvent) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.value.trim()) {
      input.value = page.name;
      return;
    }
    void state.renamePage(page, input.value);
  }
</script>

<div class="page-title">
  <div>
    <h1>{state.t("pages.title")}</h1>
    <p>{state.t("pages.desc")}</p>
  </div>
</div>

<div class="studio-grid two-col">
  <div class="section-stack">
    <section class="studio-panel">
      <div class="panel-heading">
        <div>
          <h2>{state.t("pages.createPage")}</h2>
          <p>{state.t("pages.desc")}</p>
        </div>
      </div>
      <div class="form-row">
        <div class="input-group">
          <label for="newPageName">{state.t("pages.newPagePlaceholder")}</label>
          <input id="newPageName" bind:value={state.newPageName} />
        </div>
        <button class="btn-primary" onclick={() => void state.createPage()} disabled={state.busy}>
          {state.t("common.create")}
        </button>
      </div>
    </section>

    {#if state.pages?.pages.length}
      <section class="card-grid" aria-label={state.t("pages.title")}>
        {#each state.pages.pages as page (page.id)}
          <article class="studio-card">
            <div class="thumb-preview" aria-hidden="true"></div>
            <div class="card-heading">
              <div class="package-meta">
                <input
                  aria-label={state.t("pages.title")}
                  value={page.name}
                  onblur={(event) => handleNameBlur(page, event)}
                  onkeydown={(event) => event.key === "Enter" && (event.currentTarget as HTMLInputElement).blur()}
                />
                <span class="package-id">{page.id}</span>
              </div>
              <button
                class="icon-button"
                class:active={page.favorite}
                onclick={() => void state.togglePageFavorite(page)}
                title={page.favorite ? state.t("pages.unfavorite") : state.t("pages.favorite")}
              >
                <svg viewBox="0 0 24 24" width="15" height="15" fill={page.favorite ? "currentColor" : "none"} stroke="currentColor" stroke-width="2">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"></polygon>
                </svg>
              </button>
            </div>

            <div class="badge-row">
              <span class="badge route">{page.settings.open_target === "window" ? state.t("pages.window") : state.t("pages.inApp")}</span>
              {#if page.favorite}
                <span class="badge stream">{state.t("pages.favoriteBadge")}</span>
              {/if}
              {#if page.template_source}
                <span class="badge muted">{state.t("common.imported")}</span>
              {/if}
            </div>

            <p>{page.width}x{page.height} · {page.layers.length} {state.t("common.layers")} · {state.pageItemCount(page)} {state.t("common.items")}</p>

            <div class="input-group">
              <label for={`pageOpenTarget-${page.id}`}>{state.t("pages.openTarget")}</label>
              <select
                id={`pageOpenTarget-${page.id}`}
                value={page.settings.open_target}
                onchange={(event) => void state.updatePageOpenTarget(page, event.currentTarget.value as "app" | "window")}
              >
                <option value="app">{state.t("pages.inApp")}</option>
                <option value="window">{state.t("pages.newWindow")}</option>
              </select>
            </div>

            <div class="card-actions">
              <button class="btn-primary" onclick={() => void state.openPage(page.id)} disabled={state.busy}>
                {state.t("common.open")}
              </button>
              <button class="btn-outline" onclick={() => void state.openPageEditor(page.id)}>
                {state.t("common.edit")}
              </button>
              <button class="btn-outline" onclick={() => void state.duplicatePage(page.id)} disabled={state.busy}>
                {state.t("common.duplicate")}
              </button>
              <button class="icon-button" onclick={() => state.deletePage(page)} disabled={state.busy} title={state.t("confirm.deletePageTitle")}>
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M3 6h18"></path>
                  <path d="M8 6V4h8v2"></path>
                  <path d="M19 6v14H5V6"></path>
                </svg>
              </button>
            </div>
          </article>
        {/each}
      </section>
    {:else}
      <div class="empty-state">
        <p>{state.t("pages.empty")}</p>
      </div>
    {/if}
  </div>

  <aside class="studio-panel install-panel">
    <div class="panel-heading">
      <div>
        <h2>{state.t("pages.templatesTitle")}</h2>
        <p>{state.t("pages.templatesDesc")}</p>
      </div>
    </div>

    {#if state.pageTemplates.length}
      <div class="section-stack">
        {#each state.pageTemplates as entry}
          <article class="studio-card">
            <div>
              <h3>{entry.page.title ?? entry.page.name}</h3>
              <p>{entry.page.description ?? entry.package.name}</p>
              <span class="package-id">{entry.package.id}/{entry.page.name}</span>
            </div>
            <button class="btn-primary" onclick={() => void state.importPackagePage(entry.package.id, entry.page.name)} disabled={state.busy}>
              {state.t("common.import")}
            </button>
          </article>
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <p>{state.t("packages.noneInstalled")}</p>
      </div>
    {/if}
  </aside>
</div>
