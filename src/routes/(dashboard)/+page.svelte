<script lang="ts">
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
  import { getDashboardContext } from "$lib/dashboard/context";

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
  <section class="metric-grid" aria-label={state.t("home.overview")}>
    <a class="metric-cell" href="/pages">
      <strong>{state.pageCount}</strong>
      <span>{state.t("home.pagesLabel")}</span>
    </a>
    <a class="metric-cell" href="/overlays">
      <strong>{state.overlayLayoutCount}</strong>
      <span>{state.t("home.overlaysLabel")}</span>
    </a>
    <a class="metric-cell" href="/plugins">
      <strong>{state.enabledPackageCount}/{state.packages.length}</strong>
      <span>{state.t("home.activePackagesLabel")}</span>
      {#if state.packageErrorCount}
        <em class="metric-note">{state.packageErrorCount} {state.t("home.errors")}</em>
      {/if}
    </a>
  </section>

  <section>
    <div class="section-heading">
      <div>
        <h2>{state.t("home.recentActivity")}</h2>
      </div>
    </div>

    {#if state.recentActivity.length}
      <div class="card-grid">
        {#each state.recentActivity as entry (entry.id)}
          {#if entry.kind === "page"}
          <article class="thumb-card">
            <div class="thumb-preview" aria-hidden="true" style={`--layout-width: ${entry.page.width};`}>
              <OverlayRenderer layoutOverride={entry.page} mode="page" preview={true} />
            </div>
            <div class="thumb-copy">
              <span class="thumb-title">{entry.page.name}</span>
              <div class="badge-row">
                <span class="badge route">{state.t("nav.pages")}</span>
                {#if entry.page.template_source}
                  <span class="badge muted">{state.t("common.imported")}</span>
                {/if}
              </div>
              <p>{entry.page.width}x{entry.page.height} · {state.pageItemCount(entry.page)} {state.t("common.items")}</p>
              <div class="card-actions">
                <button class="btn-outline" onclick={() => void state.openPageEditor(entry.page.id)}>
                  {state.t("common.edit")}
                </button>
              </div>
            </div>
          </article>
          {:else}
          <article class="thumb-card">
            <div class="thumb-preview" aria-hidden="true" style={`--layout-width: ${entry.layout.width};`}>
              <OverlayRenderer layoutOverride={entry.layout} mode="editor" preview={true} />
            </div>
            <div class="thumb-copy">
              <span class="thumb-title">{entry.layout.name}</span>
              <div class="badge-row">
                <span class="badge route">{state.t("nav.overlays")}</span>
                {#if state.isStreamLayout(entry.layout)}
                  <span class="badge stream">{state.t("overlays.stream")}</span>
                {/if}
                {#if state.isInGameLayout(entry.layout)}
                  <span class="badge route">{state.t("overlays.ingame")}</span>
                {/if}
              </div>
              <p>{state.layoutLayerCount(entry.layout)} {state.t("common.layers")} · {state.layoutItemCount(entry.layout)} {state.t("common.items")}</p>
              <div class="card-actions">
                <button class="btn-outline" onclick={() => void state.openLayoutEditor(entry.layout.id)}>
                  {state.t("common.edit")}
                </button>
              </div>
            </div>
          </article>
          {/if}
        {/each}
      </div>
    {:else}
      <div class="empty-state">
        <p>{state.t("common.loading")}</p>
      </div>
    {/if}
  </section>

  <section>
    <div class="section-heading">
      <div>
        <h2>{state.t("home.favoritePages")}</h2>
      </div>
    </div>

    {#if state.favoritePages.length}
      <div class="card-grid">
        {#each state.favoritePages as page (page.id)}
          <article class="thumb-card">
            <div class="thumb-preview" aria-hidden="true" style={`--layout-width: ${page.width};`}>
              <OverlayRenderer layoutOverride={page} mode="page" preview={true} />
            </div>
            <div class="thumb-copy">
              <span class="thumb-title">{page.name}</span>
              <div class="badge-row">
                <span class="badge route">{page.settings.open_target === "window" ? state.t("pages.window") : state.t("pages.inApp")}</span>
                {#if page.template_source}
                  <span class="badge muted">{state.t("common.imported")}</span>
                {/if}
              </div>
              <p>{page.layers.length} {state.t("common.layers")} · {state.pageItemCount(page)} {state.t("common.items")}</p>
              <div class="card-actions">
                <button class="btn-primary" onclick={() => void state.openPage(page.id)}>{state.t("common.open")}</button>
                <button class="btn-outline" onclick={() => void state.openPageEditor(page.id)}>{state.t("common.edit")}</button>
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

  <section>
    <div class="section-heading">
      <div>
        <h2>{state.t("home.mainLayouts")}</h2>
      </div>
    </div>

    <div class="card-grid">
      {#each mainLayouts as entry}
        <article class="thumb-card">
          <div class="thumb-preview" aria-hidden="true" style={entry.layout ? `--layout-width: ${entry.layout.width};` : ""}>
            {#if entry.layout}
              <OverlayRenderer layoutOverride={entry.layout} mode="editor" preview={true} />
            {/if}
          </div>
          <div class="thumb-copy">
            <span class="thumb-title">{entry.layout?.name ?? state.t("home.noLayout")}</span>
            <div class="badge-row">
              <span class="badge {entry.kind === 'stream' ? 'stream' : 'route'}">{entry.label}</span>
              {#if entry.layout?.template_source}
                <span class="badge muted">{state.t("common.imported")}</span>
              {/if}
            </div>
            <p>
              {#if entry.layout}
                {state.tx("home.layoutMeta", {
                  layers: state.layoutLayerCount(entry.layout),
                  items: state.layoutItemCount(entry.layout)
                })}
              {:else}
                {state.t("overlays.empty")}
              {/if}
            </p>
            <div class="card-actions">
              <button class="btn-primary" onclick={() => entry.layout && void state.openLayoutEditor(entry.layout.id)} disabled={!entry.layout}>
                {state.t("common.edit")}
              </button>
              <button class="btn-outline" onclick={() => entry.layout && void state.openPreview(state.layoutUrl(entry.layout.id))} disabled={!entry.layout || !state.obsBaseUrl}>
                {state.t("common.preview")}
              </button>
            </div>
          </div>
        </article>
      {/each}
    </div>
  </section>
</div>
