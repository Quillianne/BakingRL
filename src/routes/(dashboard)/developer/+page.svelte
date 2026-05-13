<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import { telemetryFrameTemplates } from "$lib/dashboard/state.svelte";
  import type { GameEventFrame } from "$lib/rlTelemetry";

  const dashboard = getDashboardContext();

  type PathSegment = string | number;

  type FrameValueKind = "string" | "number" | "boolean" | "null";

  type JsonEditorLine =
    | {
        id: string;
        kind: "open" | "close";
        depth: number;
        keyText: string;
        token: "{" | "}" | "[" | "]";
        trailing: boolean;
      }
    | {
        id: string;
        kind: "value";
        depth: number;
        keyText: string;
        path: PathSegment[];
        valueKind: FrameValueKind;
        value: string | number | boolean | null;
        editable: boolean;
        trailing: boolean;
      };

  type PrimitiveValue = string | number | boolean | null;

  type CompositeValue = Record<string, unknown> | unknown[];

  type DeveloperToolPanel = "registry" | "errors" | "send";

  let activeToolPanel = $state<DeveloperToolPanel>("registry");
  let frameDialogOpen = $state(false);
  let frameDialogFrame = $state<GameEventFrame | null>(null);
  let frameDialogError = $state("");

  const frameJsonLines = $derived(frameDialogFrame ? jsonEditorLines(frameDialogFrame) : []);
  const frameFieldCount = $derived(frameDialogFrame ? editableFieldCount(frameDialogFrame.Data) : 0);

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  function cloneFrame(frame: GameEventFrame): GameEventFrame {
    return JSON.parse(JSON.stringify(frame)) as GameEventFrame;
  }

  function editableFieldCount(value: unknown): number {
    if (Array.isArray(value)) {
      return value.reduce((total, entry) => total + editableFieldCount(entry), 0);
    }
    if (isRecord(value)) {
      return Object.values(value).reduce<number>((total, entry) => total + editableFieldCount(entry), 0);
    }
    const kind = value === null ? "null" : typeof value;
    return kind === "string" || kind === "number" || kind === "boolean" || kind === "null" ? 1 : 0;
  }

  function pathKey(path: PathSegment[]) {
    return path.length ? path.map((segment) => String(segment)).join(".") : "root";
  }

  function keyText(key: PathSegment | null) {
    return typeof key === "string" ? `${JSON.stringify(key)}: ` : "";
  }

  function valueKind(value: unknown): FrameValueKind {
    if (value === null) return "null";
    if (typeof value === "number") return "number";
    if (typeof value === "boolean") return "boolean";
    return "string";
  }

  function primitiveValue(value: unknown): PrimitiveValue {
    const kind = valueKind(value);
    if (kind === "number") return Number(value);
    if (kind === "boolean") return value === true;
    if (kind === "null") return null;
    return String(value ?? "");
  }

  function jsonEditorLines(
    value: unknown,
    path: PathSegment[] = [],
    depth = 0,
    key: PathSegment | null = null,
    trailing = false
  ): JsonEditorLine[] {
    if (Array.isArray(value)) {
      return compositeLines(value, path, depth, key, trailing);
    }
    if (isRecord(value)) {
      return compositeLines(value, path, depth, key, trailing);
    }
    return [
      {
        id: `${pathKey(path)}.value`,
        kind: "value",
        depth,
        keyText: keyText(key),
        path,
        valueKind: valueKind(value),
        value: primitiveValue(value),
        editable: path[0] === "Data",
        trailing
      }
    ];
  }

  function compositeLines(
    value: CompositeValue,
    path: PathSegment[],
    depth: number,
    key: PathSegment | null,
    trailing: boolean
  ): JsonEditorLine[] {
    const isArray = Array.isArray(value);
    const entries = isArray
      ? value.map((entry, index) => [index, entry] as const)
      : Object.entries(value);
    const lines: JsonEditorLine[] = [
      {
        id: `${pathKey(path)}.open`,
        kind: "open",
        depth,
        keyText: keyText(key),
        token: isArray ? "[" : "{",
        trailing: false
      }
    ];
    entries.forEach(([entryKey, entryValue], index) => {
      lines.push(
        ...jsonEditorLines(
          entryValue,
          [...path, entryKey],
          depth + 1,
          isArray ? null : entryKey,
          index < entries.length - 1
        )
      );
    });
    lines.push({
      id: `${pathKey(path)}.close`,
      kind: "close",
      depth,
      keyText: "",
      token: isArray ? "]" : "}",
      trailing
    });
    return lines;
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

  function updateFrameValue(path: PathSegment[], value: PrimitiveValue) {
    if (!frameDialogFrame) return;
    const nextFrame = cloneFrame(frameDialogFrame);
    if (!path.length) {
      frameDialogFrame = nextFrame;
      syncDialogFrame();
      return;
    }
    if (path.length === 1 && path[0] === "Event") {
      nextFrame.Event = String(value);
      frameDialogFrame = nextFrame;
      syncDialogFrame();
      return;
    }
    let target: unknown = nextFrame as unknown;
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
      updateFrameValue(path, 0);
      return;
    }
    const value = input.valueAsNumber;
    if (Number.isFinite(value)) updateFrameValue(path, value);
  }

  function updateNullFrameField(path: PathSegment[], input: HTMLInputElement) {
    updateFrameValue(path, input.value.trim() ? input.value : null);
  }

  function readonlyJsonValue(value: PrimitiveValue) {
    return JSON.stringify(value);
  }

  function selectToolPanel(panel: DeveloperToolPanel) {
    activeToolPanel = panel;
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
    </div>
  </div>

  <div class="developer-layout">
    <div class="developer-column developer-tools-column">
      <section class="studio-panel developer-panel developer-tool-panel" class:active={activeToolPanel === "registry"}>
        <div class="panel-heading developer-tool-heading">
          <button
            class="developer-tool-toggle"
            aria-expanded={activeToolPanel === "registry"}
            aria-controls="developer-registry-panel"
            onclick={() => selectToolPanel("registry")}
          >
            <svg class:rotated={activeToolPanel === "registry"} viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="9 18 15 12 9 6"></polyline>
            </svg>
            <span>
              <strong>{dashboard.t("developer.registryTitle")}</strong>
            </span>
          </button>
        </div>

        {#if activeToolPanel === "registry"}
          <div id="developer-registry-panel" class="developer-panel-body">
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
        {/if}
      </section>

      <section class="studio-panel developer-panel developer-tool-panel" class:active={activeToolPanel === "errors"}>
        <div class="panel-heading developer-tool-heading">
          <button
            class="developer-tool-toggle"
            aria-expanded={activeToolPanel === "errors"}
            aria-controls="developer-errors-panel"
            onclick={() => selectToolPanel("errors")}
          >
            <svg class:rotated={activeToolPanel === "errors"} viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="9 18 15 12 9 6"></polyline>
            </svg>
            <span>
              <strong>{dashboard.t("developer.errorsTitle")}</strong>
            </span>
          </button>
          {#if activeToolPanel === "errors"}
            <button class="btn-secondary" onclick={() => dashboard.clearDeveloperErrors()} disabled={!dashboard.developerErrors.length}>
              {dashboard.t("developer.clearErrors")}
            </button>
          {/if}
        </div>

        {#if activeToolPanel === "errors"}
          <div id="developer-errors-panel" class="developer-panel-body">
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
        {/if}
      </section>

      <section class="studio-panel developer-panel developer-tool-panel developer-send-panel" class:active={activeToolPanel === "send"}>
        <div class="panel-heading developer-tool-heading">
          <button
            class="developer-tool-toggle"
            aria-expanded={activeToolPanel === "send"}
            aria-controls="developer-send-panel"
            onclick={() => selectToolPanel("send")}
          >
            <svg class:rotated={activeToolPanel === "send"} viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="9 18 15 12 9 6"></polyline>
            </svg>
            <span>
              <strong>{dashboard.t("developer.sendFrameTitle")}</strong>
            </span>
          </button>
        </div>

        {#if activeToolPanel === "send"}
          <div id="developer-send-panel" class="developer-panel-body">
            <div class="section-stack">
              <div class="input-group">
                <label for="developerFrameTemplate">{dashboard.t("developer.template")}</label>
                <select id="developerFrameTemplate" bind:value={dashboard.developerFrameTemplate} onchange={handleFrameTemplateChange}>
                  {#each telemetryFrameTemplates as template}
                    <option value={template}>{template}</option>
                  {/each}
                </select>
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
        {/if}
      </section>
    </div>

    <section class="studio-panel developer-panel developer-telemetry-panel">
      <div class="panel-heading">
        <div>
          <h2>{dashboard.t("developer.telemetryTitle")}</h2>
        </div>
        <div class="inline-actions">
          <div class="badge-row" aria-label={dashboard.t("developer.sortLabel")}>
            <button
              class="btn-outline sort-option"
              class:active={dashboard.developerTelemetrySort === "arrival"}
              onclick={() => (dashboard.developerTelemetrySort = "arrival")}
              aria-pressed={dashboard.developerTelemetrySort === "arrival"}
              aria-label={dashboard.t("developer.arrivalOrder")}
              title={dashboard.t("developer.arrivalOrder")}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                <path d="M12 6v6l4 2"></path>
                <circle cx="12" cy="12" r="9"></circle>
              </svg>
            </button>
            <button
              class="btn-outline sort-option"
              class:active={dashboard.developerTelemetrySort === "alpha"}
              onclick={() => (dashboard.developerTelemetrySort = "alpha")}
              aria-pressed={dashboard.developerTelemetrySort === "alpha"}
              aria-label={dashboard.t("developer.alphabeticalOrder")}
              title={dashboard.t("developer.alphabeticalOrder")}
            >
              <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
                <text x="4" y="9" fill="currentColor" font-size="8" font-weight="800">A</text>
                <text x="4" y="20" fill="currentColor" font-size="8" font-weight="800">Z</text>
                <path d="M16 5v14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"></path>
                <path d="m13 16 3 3 3-3" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"></path>
              </svg>
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
          <h2 id="frame-editor-title">{dashboard.t("developer.sendFrameTitle")}</h2>
        </div>

        {#if frameDialogError}
          <div class="empty-state error-state">
            <p>{frameDialogError}</p>
          </div>
        {:else if frameDialogFrame}
          <div class="frame-editor-body">
            <div class="frame-summary dialog-summary">
              <span class="field-label">{dashboard.t("developer.template")}</span>
              <strong>{dashboard.developerFrameTemplate}</strong>
              <span>{frameFieldCount} {frameFieldCount === 1 ? dashboard.t("developer.field") : dashboard.t("developer.fields")}</span>
            </div>

            {#if frameJsonLines.length}
              <div class="json-frame-editor" aria-label="Telemetry frame JSON">
                {#each frameJsonLines as line (line.id)}
                  <div class="json-line" style={`--json-depth: ${line.depth};`}>
                    {#if line.keyText}
                      <span class="json-key">{line.keyText}</span>
                    {/if}

                    {#if line.kind === "open" || line.kind === "close"}
                      <span class="json-punctuation">{line.token}</span>
                    {:else if line.kind === "value"}
                      {#if line.editable}
                        {#if line.valueKind === "boolean"}
                          <select
                            class="json-value-input boolean"
                            value={String(line.value)}
                            onchange={(event) => updateFrameValue(line.path, event.currentTarget.value === "true")}
                          >
                            <option value="true">true</option>
                            <option value="false">false</option>
                          </select>
                        {:else if line.valueKind === "number"}
                          <input
                            class="json-value-input number"
                            type="number"
                            step="any"
                            value={String(line.value)}
                            oninput={(event) => updateNumberFrameField(line.path, event.currentTarget)}
                          />
                        {:else if line.valueKind === "null"}
                          <input
                            class="json-value-input string"
                            value=""
                            placeholder="null"
                            oninput={(event) => updateNullFrameField(line.path, event.currentTarget)}
                          />
                        {:else}
                          <span class="json-string-quote">"</span><input
                            class="json-value-input string"
                            value={String(line.value)}
                            oninput={(event) => updateFrameValue(line.path, event.currentTarget.value)}
                          /><span class="json-string-quote">"</span>
                        {/if}
                      {:else}
                        <span class="json-readonly-value">{readonlyJsonValue(line.value)}</span>
                      {/if}
                    {/if}

                    {#if line.trailing}
                      <span class="json-punctuation">,</span>
                    {/if}
                  </div>
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
