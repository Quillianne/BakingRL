<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import { telemetryFrameTemplates } from "$lib/dashboard/state.svelte";
  import type { GameEventFrame } from "$lib/rlTelemetry";

  const dashboard = getDashboardContext();

  type PathSegment = string | number;

  type FrameField = {
    id: string;
    label: string;
    path: PathSegment[];
    kind: "string" | "number" | "boolean" | "null";
    value: string | number | boolean | null;
  };

  let frameDialogOpen = $state(false);
  let frameDialogFrame = $state<GameEventFrame | null>(null);
  let frameDialogError = $state("");

  const frameFields = $derived(frameDialogFrame ? editableFields(frameDialogFrame.Data) : []);

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  function cloneFrame(frame: GameEventFrame): GameEventFrame {
    return JSON.parse(JSON.stringify(frame)) as GameEventFrame;
  }

  function pathKey(path: PathSegment[]) {
    return path.map((segment) => String(segment)).join(".");
  }

  function pathLabel(path: PathSegment[]) {
    if (!path.length) return "Data";
    return path.reduce<string>((label, segment) => {
      if (typeof segment === "number") return `${label}[${segment}]`;
      return label ? `${label}.${segment}` : segment;
    }, "");
  }

  function editableFields(value: unknown, path: PathSegment[] = []): FrameField[] {
    if (Array.isArray(value)) {
      return value.flatMap((entry, index) => editableFields(entry, [...path, index]));
    }
    if (isRecord(value)) {
      return Object.entries(value).flatMap(([key, entry]) => editableFields(entry, [...path, key]));
    }
    const kind = value === null ? "null" : typeof value;
    if (kind === "string" || kind === "number" || kind === "boolean" || kind === "null") {
      return [
        {
          id: pathKey(path),
          label: pathLabel(path),
          path,
          kind,
          value: value as string | number | boolean | null
        }
      ];
    }
    return [
      {
        id: pathKey(path),
        label: pathLabel(path),
        path,
        kind: "string",
        value: String(value ?? "")
      }
    ];
  }

  function parseDeveloperFrame() {
    const parsed = JSON.parse(dashboard.developerFrameJson) as unknown;
    if (!isRecord(parsed)) throw new Error("Frame must be a JSON object.");
    const eventName = typeof parsed.Event === "string" && parsed.Event.trim()
      ? parsed.Event.trim()
      : dashboard.developerFrameTemplate;
    return {
      Event: eventName,
      Data: parsed.Data ?? {}
    } as GameEventFrame;
  }

  function syncDialogFrame() {
    if (!frameDialogFrame) return;
    dashboard.developerFrameJson = JSON.stringify(frameDialogFrame, null, 2);
  }

  function openFrameDialog() {
    try {
      frameDialogFrame = cloneFrame(parseDeveloperFrame());
      frameDialogError = "";
    } catch (error) {
      frameDialogFrame = null;
      frameDialogError = dashboard.errorMessage(error);
    }
    frameDialogOpen = true;
  }

  function closeFrameDialog() {
    frameDialogOpen = false;
  }

  function resetFrameDialog() {
    dashboard.loadDeveloperFrameTemplate();
    openFrameDialog();
  }

  function handleFrameTemplateChange() {
    dashboard.loadDeveloperFrameTemplate();
    if (frameDialogOpen) openFrameDialog();
  }

  function updateFrameField(path: PathSegment[], value: string | number | boolean | null) {
    if (!frameDialogFrame) return;
    const nextFrame = cloneFrame(frameDialogFrame);
    if (!path.length) {
      nextFrame.Data = value;
      frameDialogFrame = nextFrame;
      syncDialogFrame();
      return;
    }
    let target: unknown = nextFrame.Data;
    for (const segment of path.slice(0, -1)) {
      target = Array.isArray(target) && typeof segment === "number"
        ? target[segment]
        : isRecord(target)
          ? target[String(segment)]
          : undefined;
    }
    const last = path[path.length - 1];
    if (Array.isArray(target) && typeof last === "number") {
      target[last] = value;
    } else if (isRecord(target) && typeof last === "string") {
      target[last] = value;
    }
    frameDialogFrame = nextFrame;
    syncDialogFrame();
  }

  function updateNumberFrameField(path: PathSegment[], input: HTMLInputElement) {
    if (!input.value.trim()) {
      updateFrameField(path, 0);
      return;
    }
    const value = input.valueAsNumber;
    if (Number.isFinite(value)) updateFrameField(path, value);
  }

  async function sendFrameFromDialog() {
    syncDialogFrame();
    await dashboard.sendDeveloperFrame();
  }
</script>

<div class="developer-page">
  <div class="page-title">
    <div>
      <h1>{dashboard.t("nav.developer")}</h1>
      <p>{dashboard.t("developer.telemetryDesc")}</p>
    </div>
  </div>

  <div class="developer-layout">
    <div class="developer-column developer-tools-column">
      <section class="studio-panel developer-panel">
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("developer.registryTitle")}</h2>
            <p>{dashboard.t("developer.registryDesc")}</p>
          </div>
          <button class="btn-secondary" onclick={() => void dashboard.refreshRegistryEntries()} disabled={dashboard.busy}>
            {dashboard.t("common.refresh")}
          </button>
        </div>

        <div class="developer-panel-body">
          {#if dashboard.registryEntries.length}
            <div class="section-stack">
              {#each dashboard.registryEntries as entry (entry.key)}
                <details class="registry-entry">
                  <summary>
                    <span class="registry-key">{entry.key}</span>
                  </summary>
                  <pre>{dashboard.formatJson(entry.value)}</pre>
                </details>
              {/each}
            </div>
          {:else}
            <div class="empty-state">
              <p>{dashboard.t("developer.emptyRegistry")}</p>
            </div>
          {/if}
        </div>
      </section>

      <section class="studio-panel developer-panel">
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("developer.errorsTitle")}</h2>
            <p>{dashboard.t("developer.errorsDesc")}</p>
          </div>
          <button class="btn-secondary" onclick={() => dashboard.clearDeveloperErrors()} disabled={!dashboard.developerErrors.length}>
            {dashboard.t("developer.clearErrors")}
          </button>
        </div>

        <div class="developer-panel-body">
          {#if dashboard.developerErrors.length}
            <div class="section-stack">
              {#each dashboard.developerErrors as error (error.id)}
                <details class="developer-error">
                  <summary>
                    <span class="error-source">{error.source}</span>
                    <span>{error.kind} · {error.receivedAt}</span>
                  </summary>
                  <pre>{error.message}</pre>
                </details>
              {/each}
            </div>
          {:else}
            <div class="empty-state">
              <p>{dashboard.t("developer.emptyErrors")}</p>
            </div>
          {/if}
        </div>
      </section>

      <section class="studio-panel developer-panel developer-send-panel">
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("developer.sendFrameTitle")}</h2>
            <p>{dashboard.t("developer.sendFrameDesc")}</p>
          </div>
        </div>

        <div class="developer-panel-body">
          <div class="section-stack">
            <div class="input-group">
              <label for="developerFrameTemplate">{dashboard.t("developer.template")}</label>
              <select id="developerFrameTemplate" bind:value={dashboard.developerFrameTemplate} onchange={handleFrameTemplateChange}>
                {#each telemetryFrameTemplates as template}
                  <option value={template}>{template}</option>
                {/each}
              </select>
            </div>

            <div class="frame-summary">
              <span class="field-label">Event</span>
              <strong>{dashboard.developerFrameTemplate}</strong>
            </div>

            <div class="card-actions">
              <button class="btn-secondary" onclick={openFrameDialog}>
                {dashboard.t("common.edit")}
              </button>
              <button class="btn-secondary" onclick={() => dashboard.loadDeveloperFrameTemplate()} disabled={dashboard.busy}>
                {dashboard.t("common.reset")}
              </button>
              <button class="btn-primary" onclick={() => void dashboard.sendDeveloperFrame()} disabled={dashboard.busy}>
                {dashboard.t("developer.sendFrame")}
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>

    <section class="studio-panel developer-panel developer-telemetry-panel">
      <div class="panel-heading">
        <div>
          <h2>{dashboard.t("developer.telemetryTitle")}</h2>
          <p>{dashboard.t("developer.telemetryDesc")}</p>
        </div>
        <div class="inline-actions">
          <div class="badge-row" aria-label={dashboard.t("developer.sortLabel")}>
            <button
              class="btn-outline"
              class:active={dashboard.developerTelemetrySort === "recent"}
              onclick={() => (dashboard.developerTelemetrySort = "recent")}
            >
              {dashboard.t("developer.recent")}
            </button>
            <button
              class="btn-outline"
              class:active={dashboard.developerTelemetrySort === "alpha"}
              onclick={() => (dashboard.developerTelemetrySort = "alpha")}
            >
              A-Z
            </button>
          </div>
        </div>
      </div>

      <div class="developer-panel-body">
        {#if dashboard.sortedDeveloperTelemetryGroups.length}
          <div class="section-stack">
            {#each dashboard.sortedDeveloperTelemetryGroups as group (group.eventName)}
              <details class="telemetry-event">
                <summary>
                  <span class="event-title">{group.eventName}</span>
                  <span>{group.count} {group.count === 1 ? dashboard.t("developer.frame") : dashboard.t("developer.frames")} · {group.latest.receivedAt}</span>
                </summary>
                <pre>{dashboard.formatJson(group.latest.frame)}</pre>
              </details>
            {/each}
          </div>
        {:else}
          <div class="empty-state">
            <p>{dashboard.t("developer.emptyTelemetry")}</p>
          </div>
        {/if}
      </div>
    </section>
  </div>

  {#if frameDialogOpen}
    <div class="modal-layer">
      <button class="modal-scrim" aria-label={dashboard.t("common.cancel")} onclick={closeFrameDialog}></button>
      <div class="studio-modal frame-editor-modal" role="dialog" aria-modal="true" aria-labelledby="frame-editor-title">
        <div class="modal-heading">
          <h2 id="frame-editor-title">{dashboard.t("developer.sendFrameTitle")} · {frameDialogFrame?.Event ?? dashboard.developerFrameTemplate}</h2>
          <p>{dashboard.t("developer.sendFrameDesc")}</p>
        </div>

        {#if frameDialogError}
          <div class="empty-state error-state">
            <p>{frameDialogError}</p>
          </div>
        {:else if frameDialogFrame}
          <div class="frame-editor-body">
            <div class="frame-summary dialog-summary">
              <span class="field-label">Event</span>
              <strong>{frameDialogFrame.Event}</strong>
              <span>{frameFields.length} {frameFields.length === 1 ? dashboard.t("developer.field") : dashboard.t("developer.fields")}</span>
            </div>

            {#if frameFields.length}
              <div class="typed-field-grid">
                {#each frameFields as field (field.id)}
                  <label class="typed-field" class:boolean-field={field.kind === "boolean"}>
                    <span>{field.label}</span>
                    {#if field.kind === "boolean"}
                      <input
                        type="checkbox"
                        checked={field.value === true}
                        onchange={(event) => updateFrameField(field.path, event.currentTarget.checked)}
                      />
                    {:else if field.kind === "number"}
                      <input
                        type="number"
                        step="any"
                        value={String(field.value)}
                        oninput={(event) => updateNumberFrameField(field.path, event.currentTarget)}
                      />
                    {:else}
                      <input
                        value={field.value === null ? "" : String(field.value)}
                        oninput={(event) => updateFrameField(field.path, field.kind === "null" && !event.currentTarget.value ? null : event.currentTarget.value)}
                      />
                    {/if}
                  </label>
                {/each}
              </div>
            {:else}
              <div class="empty-state">
                <p>Data</p>
              </div>
            {/if}
          </div>
        {/if}

        <div class="modal-actions">
          <button class="btn-secondary" onclick={resetFrameDialog} disabled={dashboard.busy}>
            {dashboard.t("common.reset")}
          </button>
          <button class="btn-secondary" onclick={closeFrameDialog}>
            {dashboard.t("common.cancel")}
          </button>
          <button class="btn-primary" onclick={() => void sendFrameFromDialog()} disabled={dashboard.busy || !!frameDialogError}>
            {dashboard.t("developer.sendFrame")}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
