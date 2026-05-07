<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import { telemetryFrameTemplates } from "$lib/dashboard/state.svelte";

  const state = getDashboardContext();
</script>

<div class="page-title">
  <div>
    <h1>{state.t("nav.developer")}</h1>
    <p>{state.t("developer.telemetryDesc")}</p>
  </div>
</div>

<div class="developer-layout">
  <div class="developer-column">
    <section class="studio-panel scroll-panel">
      <div class="panel-heading">
        <div>
          <h2>{state.t("developer.telemetryTitle")}</h2>
          <p>{state.t("developer.telemetryDesc")}</p>
        </div>
        <div class="inline-actions">
          <div class="badge-row" aria-label={state.t("developer.sortLabel")}>
            <button
              class="btn-outline"
              class:active={state.developerTelemetrySort === "recent"}
              onclick={() => (state.developerTelemetrySort = "recent")}
            >
              {state.t("developer.recent")}
            </button>
            <button
              class="btn-outline"
              class:active={state.developerTelemetrySort === "alpha"}
              onclick={() => (state.developerTelemetrySort = "alpha")}
            >
              A-Z
            </button>
          </div>
          <span
            class="status-pill {state.telemetryStatus?.state === 'connected' ? 'connected' : state.telemetryStatus?.state === 'connecting' ? 'connecting' : 'disconnected'}"
            title={state.telemetryStatus?.message ?? state.telemetryAddress}
          >
            <span class="status-dot"></span>
            {state.telemetryStatusLabel}
          </span>
        </div>
      </div>

      {#if state.sortedDeveloperTelemetryGroups.length}
        <div class="section-stack">
          {#each state.sortedDeveloperTelemetryGroups as group (group.eventName)}
            <details class="telemetry-event">
              <summary>
                <span class="event-title">{group.eventName}</span>
                <span>{group.count} {group.count === 1 ? state.t("developer.frame") : state.t("developer.frames")} · {group.latest.receivedAt}</span>
              </summary>
              <pre>{state.formatJson(group.latest.frame)}</pre>
            </details>
          {/each}
        </div>
      {:else}
        <div class="empty-state">
          <p>{state.t("developer.emptyTelemetry")}</p>
        </div>
      {/if}
    </section>

    <section class="studio-panel scroll-panel">
      <div class="panel-heading">
        <div>
          <h2>{state.t("developer.registryTitle")}</h2>
          <p>{state.t("developer.registryDesc")}</p>
        </div>
        <button class="btn-secondary" onclick={() => void state.refreshRegistryEntries()} disabled={state.busy}>
          {state.t("common.refresh")}
        </button>
      </div>

      {#if state.registryEntries.length}
        <div class="section-stack">
          {#each state.registryEntries as entry (entry.key)}
            <details class="registry-entry">
              <summary>
                <span class="registry-key">{entry.key}</span>
              </summary>
              <pre>{state.formatJson(entry.value)}</pre>
            </details>
          {/each}
        </div>
      {:else}
        <div class="empty-state">
          <p>{state.t("developer.emptyRegistry")}</p>
        </div>
      {/if}
    </section>
  </div>

  <aside class="studio-panel">
    <div class="panel-heading">
      <div>
        <h2>{state.t("developer.sendFrameTitle")}</h2>
        <p>{state.t("developer.sendFrameDesc")}</p>
      </div>
    </div>

    <div class="section-stack">
      <div class="input-group">
        <label for="developerFrameTemplate">{state.t("developer.template")}</label>
        <select id="developerFrameTemplate" bind:value={state.developerFrameTemplate} onchange={() => state.loadDeveloperFrameTemplate()}>
          {#each telemetryFrameTemplates as template}
            <option value={template}>{template}</option>
          {/each}
        </select>
      </div>

      <div class="input-group">
        <label for="developerFrameJson">{state.t("developer.frameJson")}</label>
        <textarea id="developerFrameJson" bind:value={state.developerFrameJson} spellcheck="false" rows="22"></textarea>
      </div>

      <div class="card-actions">
        <button class="btn-secondary" onclick={() => state.loadDeveloperFrameTemplate()} disabled={state.busy}>
          {state.t("common.reset")}
        </button>
        <button class="btn-primary" onclick={() => void state.sendDeveloperFrame()} disabled={state.busy}>
          {state.t("developer.sendFrame")}
        </button>
      </div>
    </div>
  </aside>
</div>
