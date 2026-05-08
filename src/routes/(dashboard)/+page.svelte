<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import LayoutThumbnail from "$lib/dashboard/LayoutThumbnail.svelte";

  const state = getDashboardContext();

  const mainLayouts = $derived([
    { kind: "ingame", label: state.t("home.ingameLayout"), layout: state.homeInGameLayout },
    { kind: "stream", label: state.t("home.streamLayout"), layout: state.homeStreamLayout }
  ] as const);
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
        <article class="thumb-card home-thumb-card">
          <div class="thumb-preview" aria-hidden="true" style={entry.layout ? `--layout-width: ${entry.layout.width};` : ""}>
            {#if entry.layout}
              <LayoutThumbnail thumbnail={entry.layout.thumbnail} name={entry.layout.name} />
            {/if}
          </div>
          <div class="thumb-copy">
            <div class="thumb-title-row">
              <span class="thumb-title">{entry.layout?.name ?? state.t("home.noLayout")}</span>
              <span class="badge {entry.kind === 'stream' ? 'stream' : 'route'}">{entry.label}</span>
            </div>
            <div class="card-actions">
              <button
                class="btn-secondary home-main-action"
                onclick={() => entry.layout && void state.openPreview(entry.kind === "stream" ? state.streamUrl() : state.layoutUrl(entry.layout.id))}
                disabled={!entry.layout || !state.obsBaseUrl}
              >
                {state.t("common.preview")}
              </button>
              <button
                class="icon-button home-edit-action"
                onclick={() => entry.layout && void state.openLayoutEditor(entry.layout.id)}
                disabled={!entry.layout}
                title={state.t("common.edit")}
                aria-label={state.t("common.edit")}
              >
                <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                  <path d="M12 20h9"></path>
                  <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"></path>
                </svg>
              </button>
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
          <article class="thumb-card home-thumb-card">
            <div class="thumb-preview" aria-hidden="true" style={`--layout-width: ${page.width};`}>
              <LayoutThumbnail thumbnail={page.thumbnail} name={page.name} />
            </div>
            <div class="thumb-copy">
              <span class="thumb-title">{page.name}</span>
              <div class="card-actions">
                <button class="btn-primary home-main-action" onclick={() => void state.openPage(page.id)}>
                  {state.t("common.open")}
                </button>
                <button
                  class="icon-button home-edit-action"
                  onclick={() => void state.openPageEditor(page.id)}
                  title={state.t("common.edit")}
                  aria-label={state.t("common.edit")}
                >
                  <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
                    <path d="M12 20h9"></path>
                    <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"></path>
                  </svg>
                </button>
              </div>
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
</div>
