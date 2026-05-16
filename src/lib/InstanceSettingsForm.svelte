<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ColorField from "$lib/ColorField.svelte";

  type SettingsItem = {
    settings: Record<string, unknown>;
  };

  type JsonSchema = {
    title?: string;
    description?: string;
    type?: string | string[];
    format?: string;
    default?: unknown;
    enum?: unknown[];
    oneOf?: JsonSchemaOption[];
    anyOf?: JsonSchemaOption[];
    items?: JsonSchema;
    properties?: Record<string, JsonSchema>;
    required?: string[];
    minimum?: number;
    maximum?: number;
    minLength?: number;
    maxLength?: number;
  };

  type JsonSchemaOption = {
    const?: unknown;
    title?: string;
    description?: string;
  };

  type SettingsField = {
    key: string;
    label: string;
    description: string;
    type: "string" | "number" | "integer" | "boolean" | "array";
    format: string;
    required: boolean;
    defaultValue: unknown;
    options: { value: string | number | boolean; label: string }[];
    minimum?: number;
    maximum?: number;
    minLength?: number;
    maxLength?: number;
  };

  const {
    item,
    packageId = null,
    exportName = null,
    onpreview = () => {},
    oncommit
  }: {
    item: SettingsItem;
    packageId?: string | null;
    exportName?: string | null;
    onpreview?: () => void | Promise<void>;
    oncommit: () => void | Promise<void>;
  } = $props();

  let schema = $state<JsonSchema | null>(null);
  let loading = $state(false);
  let error = $state("");
  let requestId = 0;

  const fields = $derived(schemaToFields(schema));

  $effect(() => {
    const currentPackageId = packageId;
    const currentExportName = exportName;
    const id = ++requestId;
    schema = null;
    error = "";
    loading = false;
    if (!currentPackageId || !currentExportName) return;
    loading = true;
    void loadSchema(currentPackageId, currentExportName, id);
  });

  async function loadSchema(packageId: string, exportName: string, id: number) {
    try {
      const nextSchema = await invoke<JsonSchema | null>("get_visual_settings_schema", {
        packageId,
        exportName
      });
      if (id !== requestId) return;
      schema = nextSchema;
    } catch (caught) {
      if (id !== requestId) return;
      error = String(caught);
    } finally {
      if (id === requestId) loading = false;
    }
  }

  function schemaToFields(schema: JsonSchema | null): SettingsField[] {
    if (!schema?.properties) return [];
    const required = new Set(schema.required ?? []);
    return Object.entries(schema.properties)
      .map(([key, property]) => fieldFromSchema(key, property, required.has(key)))
      .filter((field): field is SettingsField => field !== null);
  }

  function fieldFromSchema(key: string, property: JsonSchema, required: boolean): SettingsField | null {
    const type = schemaType(property);
    if (type !== "string" && type !== "number" && type !== "integer" && type !== "boolean" && type !== "array") return null;
    const options = optionsFromSchema(property);
    if (type === "array" && !options.length) return null;
    return {
      key,
      label: property.title ?? labelFromKey(key),
      description: property.description ?? "",
      type,
      format: property.format ?? "",
      required,
      defaultValue: property.default,
      options,
      minimum: property.minimum,
      maximum: property.maximum,
      minLength: property.minLength,
      maxLength: property.maxLength
    };
  }

  function schemaType(property: JsonSchema) {
    const type = Array.isArray(property.type) ? property.type.find((entry) => entry !== "null") : property.type;
    if (type) return type;
    if (property.enum?.every((entry) => typeof entry === "boolean")) return "boolean";
    if (property.enum?.every((entry) => typeof entry === "number")) return "number";
    return "string";
  }

  function optionsFromSchema(property: JsonSchema) {
    if (property.type === "array" && property.items) return optionsFromSchema(property.items);
    if (property.enum) {
      return property.enum
        .filter((value): value is string | number | boolean => ["string", "number", "boolean"].includes(typeof value))
        .map((value) => ({ value, label: String(value) }));
    }
    const variants = property.oneOf ?? property.anyOf ?? [];
    return variants
      .filter((option) => option.const !== undefined && ["string", "number", "boolean"].includes(typeof option.const))
      .map((option) => ({
        value: option.const as string | number | boolean,
        label: option.title ?? String(option.const)
      }));
  }

  function labelFromKey(key: string) {
    return key
      .replace(/[_-]+/g, " ")
      .replace(/([a-z])([A-Z])/g, "$1 $2")
      .replace(/\b\w/g, (match) => match.toUpperCase());
  }

  function fieldValue(field: SettingsField) {
    const settings = settingsObject();
    if (Object.prototype.hasOwnProperty.call(settings, field.key)) return settings[field.key];
    if (field.defaultValue !== undefined) return field.defaultValue;
    if (field.type === "array") return [];
    if (field.type === "boolean") return false;
    if (field.type === "number" || field.type === "integer") return "";
    return "";
  }

  function settingsObject() {
    return item.settings && typeof item.settings === "object" && !Array.isArray(item.settings) ? item.settings : {};
  }

  function updateField(field: SettingsField, rawValue: string | number | boolean | unknown[], commit = true) {
    const next = { ...settingsObject() };
    if (field.type === "array") {
      next[field.key] = Array.isArray(rawValue) ? rawValue : [];
    } else if (field.type === "boolean") {
      next[field.key] = rawValue === true || rawValue === "true";
    } else if (field.type === "number" || field.type === "integer") {
      if (rawValue === "") {
        delete next[field.key];
      } else {
        const parsed = field.type === "integer" ? Number.parseInt(String(rawValue), 10) : Number(String(rawValue));
        if (Number.isFinite(parsed)) next[field.key] = parsed;
      }
    } else {
      next[field.key] = String(rawValue);
    }
    item.settings = next;
    void (commit ? oncommit() : onpreview());
  }

  function updateOptionField(field: SettingsField, rawValue: string, commit = true) {
    const option = field.options.find((entry) => String(entry.value) === rawValue);
    updateField(field, option?.value ?? rawValue, commit);
  }

  function arrayValues(field: SettingsField) {
    const value = fieldValue(field);
    return Array.isArray(value) ? value : [];
  }

  function optionChecked(field: SettingsField, option: SettingsField["options"][number]) {
    return arrayValues(field).some((value) => String(value) === String(option.value));
  }

  function toggleArrayOption(field: SettingsField, option: SettingsField["options"][number], checked: boolean) {
    const existing = arrayValues(field).filter((value) => String(value) !== String(option.value));
    updateField(field, checked ? [...existing, option.value] : existing);
  }

  function setJsonSettings(raw: string) {
    try {
      const parsed = raw.trim() ? JSON.parse(raw) : {};
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("Settings must be a JSON object.");
      }
      item.settings = parsed;
      error = "";
      void oncommit();
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    }
  }
</script>

<div class="instance-settings">
  <div class="settings-title">
    <span>Instance Settings</span>
    {#if schema?.title}
      <small>{schema.title}</small>
    {/if}
  </div>

  {#if loading}
    <p class="settings-hint">Loading typed settings...</p>
  {:else if fields.length}
    <div class="typed-settings">
      {#each fields as field}
        <div class="typed-field">
          <span class="field-label">
            {field.label}
            {#if field.required}<em>required</em>{/if}
          </span>
          {#if field.description}
            <small>{field.description}</small>
          {/if}

          {#if field.type === "array"}
            <span class="checkbox-list">
              {#each field.options as option}
                <label>
                  <input
                    type="checkbox"
                    checked={optionChecked(field, option)}
                    onchange={(event) => toggleArrayOption(field, option, event.currentTarget.checked)}
                  />
                  <span>{option.label}</span>
                </label>
              {/each}
            </span>
          {:else if field.options.length}
            <select value={String(fieldValue(field))} onchange={(event) => updateOptionField(field, event.currentTarget.value)}>
              {#each field.options as option}
                <option value={String(option.value)}>{option.label}</option>
              {/each}
            </select>
          {:else if field.type === "boolean"}
            <span class="toggle-row">
              <input type="checkbox" checked={Boolean(fieldValue(field))} onchange={(event) => updateField(field, event.currentTarget.checked)} />
              <span>{Boolean(fieldValue(field)) ? "Enabled" : "Disabled"}</span>
            </span>
          {:else if field.type === "number" || field.type === "integer"}
            <input
              type="number"
              step={field.type === "integer" ? "1" : "any"}
              min={field.minimum}
              max={field.maximum}
              value={String(fieldValue(field))}
              oninput={(event) => updateField(field, event.currentTarget.value, false)}
              onblur={(event) => updateField(field, event.currentTarget.value)}
            />
          {:else if field.format === "color"}
            <ColorField value={String(fieldValue(field) || "#ffffff")} oncommit={(nextValue) => updateField(field, nextValue)} />
          {:else}
            <input
              value={String(fieldValue(field))}
              minlength={field.minLength}
              maxlength={field.maxLength}
              oninput={(event) => updateField(field, event.currentTarget.value, false)}
              onblur={(event) => updateField(field, event.currentTarget.value)}
            />
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <label class="json-fallback">
      <span>No typed schema declared for this visual.</span>
      <textarea value={JSON.stringify(item.settings ?? {}, null, 2)} onblur={(event) => setJsonSettings(event.currentTarget.value)}></textarea>
    </label>
  {/if}

  {#if error}
    <p class="settings-error">{error}</p>
  {/if}
</div>

<style>
  .instance-settings,
  .typed-settings {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .settings-title {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
  }

  .settings-title small,
  .settings-hint,
  .typed-field small,
  .json-fallback span {
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 500;
  }

  .typed-field,
  .json-fallback {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 650;
    color: var(--text-secondary);
  }

  .field-label em {
    color: var(--warn);
    font-size: 10px;
    font-style: normal;
    text-transform: uppercase;
  }

  .toggle-row,
  .checkbox-list label {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .checkbox-list {
    display: grid;
    gap: 7px;
  }

  .checkbox-list label {
    min-height: 30px;
    padding: 6px 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-panel-hover) 70%, transparent);
    color: var(--text-secondary);
    font-size: 12px;
  }

  input,
  select,
  textarea {
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    color: var(--text-primary);
    font: inherit;
    font-size: 13px;
    padding: 8px 10px;
  }

  input:focus,
  select:focus,
  textarea:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .toggle-row input[type="checkbox"],
  .checkbox-list input[type="checkbox"] {
    flex: none;
    width: auto;
    min-width: 0;
  }

  textarea {
    min-height: 120px;
    resize: vertical;
  }

  .settings-error {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--danger) 28%, transparent);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--danger) 8%, transparent);
    color: var(--danger);
    font-size: 12px;
  }
</style>
