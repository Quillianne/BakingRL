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
  <section class="hero-preview">
    <div class="preview-frame" aria-label="Active stream layout preview" style={state.homeStreamLayout ? `--layout-width: ${state.homeStreamLayout.width};` : ""}>
      {#if state.homeStreamLayout}
        <OverlayRenderer layoutId={state.homeStreamLayout.id} layoutOverride={state.homeStreamLayout} mode="editor" preview={true} />
      {:else}
        <div class="empty-state">
          <p>{state.t("home.noLayout")}</p>
        </div>
      {/if}
    </div>

    <div class="hero-copy">
      <div>
        <span class="field-label">{state.t("home.streamLayout")}</span>
        <h2>{state.homeStreamLayout?.name ?? state.t("home.noLayout")}</h2>
        <p>
          {#if state.homeStreamLayout}
            {state.tx("home.layoutMeta", {
              layers: state.layoutLayerCount(state.homeStreamLayout),
              items: state.layoutItemCount(state.homeStreamLayout)
            })}
          {:else}
            {state.t("overlays.empty")}
          {/if}
        </p>
      </div>

      <div class="card-actions">
        <button
          class="btn-primary"
          onclick={() => state.homeStreamLayout && void state.openLayoutEditor(state.homeStreamLayout.id)}
          disabled={!state.homeStreamLayout}
        >
          {state.t("common.edit")}
        </button>
        <a class="btn-secondary" href="/pages">{state.t("pages.createPage")}</a>
        <a class="btn-outline" href="/plugins">{state.t("packages.installTitle")}</a>
      </div>
    </div>
  </section>

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
        <p>{state.t("home.recentActivityDesc")}</p>
      </div>
    </div>

    {#if state.recentPages.length || state.recentLayouts.length}
      <div class="card-grid">
        {#each state.recentPages as page (page.id)}
          <article class="thumb-card">
            <div class="thumb-preview" aria-hidden="true" style={`--layout-width: ${page.width};`}>
              <OverlayRenderer layoutOverride={page} mode="page" preview={true} />
            </div>
            <div class="thumb-copy">
              <span class="thumb-title">{page.name}</span>
              <div class="badge-row">
                <span class="badge route">{state.t("nav.pages")}</span>
                {#if page.template_source}
                  <span class="badge muted">{state.t("common.imported")}</span>
                {/if}
              </div>
              <p>{page.width}x{page.height} · {state.pageItemCount(page)} {state.t("common.items")}</p>
              <div class="card-actions">
                <button class="btn-outline" onclick={() => void state.openPageEditor(page.id)}>
                  {state.t("common.edit")}
                </button>
              </div>
            </div>
          </article>
        {/each}
        {#each state.recentLayouts as layout (layout.id)}
          <article class="thumb-card">
            <div class="thumb-preview" aria-hidden="true" style={`--layout-width: ${layout.width};`}>
              <OverlayRenderer layoutOverride={layout} mode="editor" preview={true} />
            </div>
            <div class="thumb-copy">
              <span class="thumb-title">{layout.name}</span>
              <div class="badge-row">
                <span class="badge route">{state.t("nav.overlays")}</span>
                {#if state.isStreamLayout(layout)}
                  <span class="badge stream">{state.t("overlays.stream")}</span>
                {/if}
                {#if state.isInGameLayout(layout)}
                  <span class="badge route">{state.t("overlays.ingame")}</span>
                {/if}
              </div>
              <p>{state.layoutLayerCount(layout)} {state.t("common.layers")} · {state.layoutItemCount(layout)} {state.t("common.items")}</p>
              <div class="card-actions">
                <button class="btn-outline" onclick={() => void state.openLayoutEditor(layout.id)}>
                  {state.t("common.edit")}
                </button>
              </div>
            </div>
          </article>
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
        <p>{state.t("home.favoritePagesDesc")}</p>
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
        <p>{state.t("home.mainLayoutsDesc")}</p>
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
              <button class="btn-outline" onclick={() => entry.layout && state.openPreview(state.layoutUrl(entry.layout.id))} disabled={!entry.layout || !state.obsBaseUrl}>
                {state.t("common.preview")}
              </button>
            </div>
          </div>
        </article>
      {/each}
    </div>
  </section>
</div>
