<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import type { OverlayLayout } from "$lib/dashboard/types";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";

  const state = getDashboardContext();

  function handleNameBlur(layout: OverlayLayout, event: FocusEvent) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.value.trim()) {
      input.value = layout.name;
      return;
    }
    void state.renameLayout(layout, input.value);
  }
</script>

<div class="page-title">
  <div>
    <h1>{state.t("overlays.layoutsTitle")}</h1>
    <p>{state.t("overlays.layoutsDesc")}</p>
  </div>
</div>

<div class="section-stack">
  <section class="studio-panel">
    <div class="panel-heading">
      <div>
        <h2>{state.t("overlays.obsTitle")}</h2>
        <p>{state.t("overlays.obsDesc")}</p>
      </div>
      <button class="btn-primary" onclick={() => void state.copyText(state.streamUrl(), state.t("overlays.generalUrl"))} disabled={!state.obsBaseUrl}>
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
          <rect x="9" y="9" width="13" height="13" rx="2"></rect>
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
        </svg>
        {state.t("overlays.copyGeneralUrl")}
      </button>
    </div>
    <code class="mono">{state.streamUrl()}</code>
  </section>

  <section class="studio-panel">
    <div class="panel-heading">
      <div>
        <h2>{state.t("overlays.createLayout")}</h2>
        <p>{state.t("overlays.layoutsDesc")}</p>
      </div>
    </div>
    <div class="form-row">
      <div class="input-group">
        <label for="newLayoutName">{state.t("overlays.newLayoutPlaceholder")}</label>
        <input id="newLayoutName" bind:value={state.newLayoutName} />
      </div>
      <button class="btn-primary" onclick={() => void state.createOverlayLayout()} disabled={state.busy}>
        {state.t("common.create")}
      </button>
    </div>
  </section>

  {#if state.layoutTemplates.length}
    <section>
      <div class="section-heading">
        <div>
          <h2>{state.t("overlays.templateTitle")}</h2>
          <p>{state.t("overlays.templateDesc")}</p>
        </div>
      </div>
      <div class="card-grid">
        {#each state.layoutTemplates as entry}
          <article class="studio-card">
            <div>
              <h3>{entry.layoutTemplate.title ?? entry.layoutTemplate.name}</h3>
              <p>{entry.layoutTemplate.description ?? entry.package.name}</p>
              <span class="package-id">{entry.package.id}/{entry.layoutTemplate.name}</span>
            </div>
            <button class="btn-primary" onclick={() => void state.importPackageLayout(entry.package.id, entry.layoutTemplate.name)} disabled={state.busy}>
              {state.t("common.import")}
            </button>
          </article>
        {/each}
      </div>
    </section>
  {/if}

  {#if state.overlayLayouts?.layouts.length}
    <section class="card-grid" aria-label={state.t("overlays.layoutsTitle")}>
      {#each state.overlayLayouts.layouts as layout (layout.id)}
        <article class="studio-card">
          <div class="thumb-preview" aria-hidden="true"></div>
          <div class="card-heading">
            <div class="package-meta">
              <input
                aria-label={state.t("overlays.layoutName")}
                value={layout.name}
                onblur={(event) => handleNameBlur(layout, event)}
                onkeydown={(event) => event.key === "Enter" && (event.currentTarget as HTMLInputElement).blur()}
              />
              <span class="package-id">{layout.id}</span>
            </div>
          </div>

          <div class="badge-row">
            {#if state.isInGameLayout(layout)}
              <span class="badge route">{state.t("overlays.ingame")}</span>
            {/if}
            {#if state.isStreamLayout(layout)}
              <span class="badge stream">{state.t("overlays.stream")}</span>
            {/if}
            {#if !state.isInGameLayout(layout) && !state.isStreamLayout(layout)}
              <span class="badge muted">{state.t("common.unassigned")}</span>
            {/if}
            {#if layout.template_source}
              <span class="badge muted">{state.t("common.imported")}</span>
            {/if}
          </div>

          <p>
            {layout.width}x{layout.height} · {state.layoutLayerCount(layout)} {state.t("common.layers")} ·
            {state.layoutItemCount(layout)} {state.t("common.items")}
          </p>

          <div class="card-actions">
            <button class="btn-primary" onclick={() => void state.openLayoutEditor(layout.id)}>
              {state.t("common.edit")}
            </button>
            <button class="btn-outline" onclick={() => state.openPreview(state.layoutUrl(layout.id))} disabled={!state.obsBaseUrl}>
              {state.t("common.preview")}
            </button>
            <button class="btn-outline" onclick={() => void state.copyText(state.layoutUrl(layout.id), state.t("common.copyUrl"))} disabled={!state.obsBaseUrl}>
              {state.t("common.copyUrl")}
            </button>
            <button class="btn-secondary" onclick={() => void state.routeOverlayLayout(layout.id)} disabled={state.busy || state.isInGameLayout(layout)}>
              {state.t("overlays.setIngame")}
            </button>
            <button class="btn-secondary" onclick={() => void state.routeOverlayLayout(layout.id, true)} disabled={state.busy || state.isStreamLayout(layout)}>
              {state.t("overlays.setStream")}
            </button>
            <button class="icon-button" onclick={() => state.deleteLayout(layout)} disabled={state.busy || (state.overlayLayouts?.layouts.length ?? 0) <= 1} title={state.t("confirm.deleteLayoutTitle")}>
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
      <p>{state.t("overlays.empty")}</p>
    </div>
  {/if}
</div>
