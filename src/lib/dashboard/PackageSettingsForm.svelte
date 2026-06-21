<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import ColorField from "$lib/ColorField.svelte";
  import { getInitialLocale, translations } from "$lib/i18n";
  import type { JsonSchema, PackageConfigurationState, PackageSecretDescriptor } from "$lib/dashboard/types";

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
    secret: boolean;
    restartRequired: boolean;
  };

  const {
    packageId,
    configuration,
    secretOnly = false
  }: {
    packageId: string;
    configuration: PackageConfigurationState;
    secretOnly?: boolean;
  } = $props();

  let values = $state<Record<string, unknown>>({});
  let savedValues = $state<Record<string, unknown>>({});
  let secrets = $state<PackageSecretDescriptor[]>([]);
  let secretDrafts = $state<Record<string, string>>({});
  let busy = $state(false);
  let message = $state("");
  let error = $state("");
  let initializedFor = "";
  let currentConfiguration = $state<PackageConfigurationState>({
    packageId: "",
    title: "",
    hasSettingsWebview: false,
    schema: null,
    values: {},
    secrets: [],
    secretStoreAvailable: false,
    secretStoreError: null
  });
  const locale = getInitialLocale();
  const t = translations[locale];

  const fields = $derived(schemaToFields(currentConfiguration.schema).filter((field) => !field.secret));
  const visibleFields = $derived(secretOnly ? [] : fields);
  const visibleSecrets = $derived(secretOnly ? secrets : []);
  const settingsDirty = $derived(JSON.stringify(values) !== JSON.stringify(savedValues));
  const restartRequiredDirty = $derived(visibleFields.some((field) => field.restartRequired && fieldDirty(field)));

  $effect(() => {
    const signature = JSON.stringify(configuration);
    if (initializedFor === signature) return;
    initializedFor = signature;
    applyConfigurationState(configuration);
    secretDrafts = {};
    message = "";
    error = "";
  });

  function applyConfigurationState(nextConfiguration: PackageConfigurationState) {
    currentConfiguration = nextConfiguration;
    values = { ...(nextConfiguration.values ?? {}) };
    savedValues = { ...(nextConfiguration.values ?? {}) };
    secrets = nextConfiguration.secrets.map((secret) => ({ ...secret }));
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
      maxLength: property.maxLength,
      secret: property["x-bakingrl-secret"] === true,
      restartRequired: property["x-bakingrl-restart-required"] === true
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
    if (Object.prototype.hasOwnProperty.call(values, field.key)) return values[field.key];
    if (field.defaultValue !== undefined) return field.defaultValue;
    if (field.type === "array") return [];
    if (field.type === "boolean") return false;
    if (field.type === "number" || field.type === "integer") return "";
    return "";
  }

  function savedFieldValue(field: SettingsField) {
    if (Object.prototype.hasOwnProperty.call(savedValues, field.key)) return savedValues[field.key];
    if (field.defaultValue !== undefined) return field.defaultValue;
    if (field.type === "array") return [];
    if (field.type === "boolean") return false;
    if (field.type === "number" || field.type === "integer") return "";
    return "";
  }

  function fieldDirty(field: SettingsField) {
    return JSON.stringify(fieldValue(field)) !== JSON.stringify(savedFieldValue(field));
  }

  async function saveValues() {
    const nextValues = { ...values };
    busy = true;
    error = "";
    message = "";
    try {
      values = await invoke<Record<string, unknown>>("save_package_settings", {
        packageId,
        values: nextValues
      });
      currentConfiguration = {
        ...currentConfiguration,
        values
      };
      savedValues = { ...values };
      message = t["msg.settingsSaved"];
    } catch (caught) {
      error = String(caught);
    } finally {
      busy = false;
    }
  }

  function resetValues() {
    values = { ...savedValues };
    error = "";
    message = "";
  }

  function updateField(field: SettingsField, rawValue: string | number | boolean | unknown[]) {
    const next = { ...values };
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
    values = next;
    message = "";
  }

  function updateOptionField(field: SettingsField, rawValue: string) {
    const option = field.options.find((entry) => String(entry.value) === rawValue);
    updateField(field, option?.value ?? rawValue);
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

  async function saveSecret(secret: PackageSecretDescriptor) {
    const value = secretDrafts[secret.key] ?? "";
    if (!value) return;
    busy = true;
    error = "";
    message = "";
    try {
      const nextState = await invoke<PackageConfigurationState>("set_package_secret", {
        packageId,
        key: secret.key,
        value
      });
      applyConfigurationState(nextState);
      secretDrafts = { ...secretDrafts, [secret.key]: "" };
      const updatedSecret = nextState.secrets.find((entry) => entry.key === secret.key);
      if (updatedSecret?.configured) {
        message = t["msg.secretSaved"];
      } else {
        error = nextState.secretStoreError ?? t["msg.secretStateUnconfirmed"];
      }
    } catch (caught) {
      error = String(caught);
    } finally {
      busy = false;
    }
  }

  async function clearSecret(secret: PackageSecretDescriptor) {
    busy = true;
    error = "";
    message = "";
    try {
      const nextState = await invoke<PackageConfigurationState>("delete_package_secret", {
        packageId,
        key: secret.key
      });
      applyConfigurationState(nextState);
      secretDrafts = { ...secretDrafts, [secret.key]: "" };
      message = t["msg.secretDeleted"];
    } catch (caught) {
      error = String(caught);
    } finally {
      busy = false;
    }
  }
</script>

<div class="package-settings-form">
  {#if !secretOnly && visibleFields.length}
    <section class="settings-section">
      <div class="settings-section-head">
        <h2>{t["packageSettings.title"]}</h2>
        {#if currentConfiguration.schema?.description}
          <p>{currentConfiguration.schema.description}</p>
        {/if}
      </div>
      <div class="typed-settings">
        {#each visibleFields as field}
          <div class="typed-field">
            <span class="field-label">
              {field.label}
              {#if field.required}<em>{t["common.required"]}</em>{/if}
              {#if field.restartRequired}<strong class="restart-required">{t["packageSettings.restartRequired"]}</strong>{/if}
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
                      disabled={busy}
                      onchange={(event) => toggleArrayOption(field, option, event.currentTarget.checked)}
                    />
                    <span>{option.label}</span>
                  </label>
                {/each}
              </span>
            {:else if field.options.length}
              <select value={String(fieldValue(field))} disabled={busy} onchange={(event) => updateOptionField(field, event.currentTarget.value)}>
                {#each field.options as option}
                  <option value={String(option.value)}>{option.label}</option>
                {/each}
              </select>
            {:else if field.type === "boolean"}
              <span class="toggle-row">
                <input type="checkbox" checked={Boolean(fieldValue(field))} disabled={busy} onchange={(event) => updateField(field, event.currentTarget.checked)} />
                <span>{Boolean(fieldValue(field)) ? t["common.enabled"] : t["common.disabled"]}</span>
              </span>
            {:else if field.type === "number" || field.type === "integer"}
              <input
                type="number"
                step={field.type === "integer" ? "1" : "any"}
                min={field.minimum}
                max={field.maximum}
                value={String(fieldValue(field))}
                disabled={busy}
                oninput={(event) => updateField(field, event.currentTarget.value)}
              />
            {:else if field.format === "color"}
              <ColorField value={String(fieldValue(field) || "#ffffff")} oncommit={(nextValue) => updateField(field, nextValue)} />
            {:else}
              <input
                type={field.format === "password" ? "password" : "text"}
                value={String(fieldValue(field))}
                minlength={field.minLength}
                maxlength={field.maxLength}
                disabled={busy}
                oninput={(event) => updateField(field, event.currentTarget.value)}
              />
            {/if}
          </div>
        {/each}
      </div>
      <div class="settings-actions">
        {#if restartRequiredDirty}
          <p class="settings-restart-hint">{t["packageSettings.restartRequiredPending"]}</p>
        {/if}
        <button type="button" class="btn-outline" disabled={busy || !settingsDirty} onclick={resetValues}>
          {t["common.cancel"]}
        </button>
        <button type="button" class="btn-primary" disabled={busy || !settingsDirty} onclick={() => void saveValues()}>
          {t["common.saveSettings"]}
        </button>
      </div>
    </section>
  {/if}

  {#if visibleSecrets.length}
    <section class="settings-section">
      <div class="settings-section-head">
        <h2>{t["packageSettings.secretsTitle"]}</h2>
        <p>{t["packageSettings.secretHelp"]}</p>
      </div>
      {#if !currentConfiguration.secretStoreAvailable}
        <p class="settings-error">{currentConfiguration.secretStoreError ?? t["packageSettings.secretStoreUnavailable"]}</p>
      {/if}
      <div class="typed-settings">
        {#each visibleSecrets as secret}
          <div class="typed-field">
            <span class="field-label">
              {secret.label}
              {#if secret.required}<em>{t["common.required"]}</em>{/if}
              <strong class:configured={secret.configured}>{secret.configured ? t["packageSettings.configured"] : t["packageSettings.notConfigured"]}</strong>
            </span>
            {#if secret.description}
              <small>{secret.description}</small>
            {/if}
            <div class="secret-row">
              <input
                type="password"
                placeholder={secret.configured ? t["packageSettings.newSecretValue"] : t["packageSettings.secretValue"]}
                value={secretDrafts[secret.key] ?? ""}
                disabled={busy || !currentConfiguration.secretStoreAvailable}
                oninput={(event) => (secretDrafts = { ...secretDrafts, [secret.key]: event.currentTarget.value })}
              />
              <button type="button" class="btn-primary" disabled={busy || !currentConfiguration.secretStoreAvailable || !(secretDrafts[secret.key] ?? "")} onclick={() => void saveSecret(secret)}>
                {t["packageSettings.saveSecret"]}
              </button>
              <button type="button" class="btn-outline" disabled={busy || !currentConfiguration.secretStoreAvailable || !secret.configured} onclick={() => void clearSecret(secret)}>
                {t["packageSettings.clearSecret"]}
              </button>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if !visibleFields.length && !visibleSecrets.length}
    <p class="settings-hint">{t["packageSettings.empty"]}</p>
  {/if}

  {#if message}
    <p class="settings-message">{message}</p>
  {/if}
  {#if error}
    <p class="settings-error">{error}</p>
  {/if}
</div>

<style>
  .package-settings-form,
  .settings-section,
  .typed-settings {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 12px;
  }

  .package-settings-form {
    width: min(720px, calc(100vw - 48px));
    max-height: calc(var(--app-content-height, 100vh) - 88px);
    overflow: auto;
    padding: 18px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--editor-bg-panel);
  }

  .settings-section-head h2 {
    margin: 0;
    color: var(--text-primary);
    font-size: 14px;
  }

  .settings-section-head p,
  .typed-field small,
  .settings-hint {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: 11px;
  }

  .typed-field {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  .field-label {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 800;
  }

  .field-label em,
  .field-label strong {
    padding: 2px 6px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--text-muted) 14%, transparent);
    color: var(--text-muted);
    font-size: 10px;
    font-style: normal;
    font-weight: 800;
    text-transform: uppercase;
  }

  .field-label strong.configured {
    background: color-mix(in srgb, var(--success) 18%, transparent);
    color: var(--success);
  }

  .field-label strong.restart-required {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    color: var(--accent);
  }

  input,
  select {
    width: 100%;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--bg-panel);
    color: var(--text-primary);
    font: inherit;
    font-size: 12px;
  }

  input,
  select {
    height: 34px;
    padding: 0 10px;
  }

  .checkbox-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .checkbox-list label,
  .toggle-row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
    font-size: 12px;
  }

  .checkbox-list input,
  .toggle-row input {
    width: auto;
    height: auto;
  }

  .secret-row {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) max-content max-content;
    gap: 8px;
  }

  .settings-actions {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
  }

  .settings-restart-hint {
    margin: 0 auto 0 0;
    color: var(--accent);
    font-size: 11px;
    font-weight: 700;
  }

  .btn-primary,
  .btn-outline {
    height: 34px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    font-weight: 800;
    white-space: nowrap;
    cursor: pointer;
  }

  .btn-primary {
    border: 1px solid color-mix(in srgb, var(--accent) 68%, transparent);
    background: var(--accent);
    color: #fff;
  }

  .btn-outline {
    border: 1px solid var(--border-color);
    background: var(--bg-panel-hover);
    color: var(--text-primary);
  }

  button:disabled,
  input:disabled,
  select:disabled {
    cursor: not-allowed;
    opacity: 0.58;
  }

  .settings-message,
  .settings-error {
    margin: 0;
    font-size: 12px;
    font-weight: 700;
  }

  .settings-message {
    color: var(--success);
  }

  .settings-error {
    color: var(--danger);
  }

  @media (max-width: 560px) {
    .package-settings-form {
      width: calc(100vw - 24px);
      padding: 14px;
    }

    .secret-row {
      grid-template-columns: 1fr;
    }
  }
</style>
