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
              <button class="btn-primary" onclick={() => entry.layout && void state.openLayoutEditor(entry.layout.id)} disabled={!entry.layout}>
                {state.t("common.edit")}
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
                <button class="btn-outline" onclick={() => void state.openPage(page.id)}>{state.t("common.open")}</button>
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
