<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import { THEMES } from "$lib/themes";
  import type { Locale } from "$lib/i18n";
  import type { AppSettings } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();

  let section = $state<"general" | "runtime" | "advanced">("general");
  let draftSettings = $state<AppSettings | null>(null);
  let syncedSettingsSignature = "";
  const settingsDirty = $derived(Boolean(draftSettings && dashboard.appSettings && settingsSignature(draftSettings) !== settingsSignature(dashboard.appSettings)));

  $effect(() => {
    const settings = dashboard.appSettings;
    if (!settings) {
      draftSettings = null;
      syncedSettingsSignature = "";
      return;
    }
    const signature = settingsSignature(settings);
    if (signature === syncedSettingsSignature) return;
    syncedSettingsSignature = signature;
    draftSettings = cloneAppSettings(settings);
  });

  function cloneAppSettings(settings: AppSettings) {
    return JSON.parse(JSON.stringify(settings)) as AppSettings;
  }

  function settingsSignature(settings: AppSettings) {
    return JSON.stringify(settings);
  }

  async function saveDraftSettings() {
    if (!draftSettings) return;
    const saved = await dashboard.saveAppSettings(cloneAppSettings(draftSettings));
    if (saved && dashboard.appSettings) {
      syncedSettingsSignature = settingsSignature(dashboard.appSettings);
      draftSettings = cloneAppSettings(dashboard.appSettings);
    }
  }

  function cancelDraftSettings() {
    if (!dashboard.appSettings) return;
    syncedSettingsSignature = settingsSignature(dashboard.appSettings);
    draftSettings = cloneAppSettings(dashboard.appSettings);
  }
</script>

<div class="page-title">
  <div>
    <h1>{dashboard.t("nav.settings")}</h1>
  </div>
</div>

{#if draftSettings}
  <div class="settings-layout">
    <nav class="studio-panel settings-nav" aria-label={dashboard.t("nav.settings")}>
      <button class="btn-outline" class:active={section === "general"} onclick={() => (section = "general")}>
        {dashboard.t("settings.general")}
      </button>
      <button class="btn-outline" class:active={section === "runtime"} onclick={() => (section = "runtime")}>
        {dashboard.t("settings.runtime")}
      </button>
      <button class="btn-outline" class:active={section === "advanced"} onclick={() => (section = "advanced")}>
        {dashboard.t("settings.advanced")}
      </button>
    </nav>

    <section class="studio-panel">
      {#if section === "general"}
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("settings.general")}</h2>
          </div>
        </div>

        <div class="section-stack compact-settings-stack">
          <div class="settings-group">
            <h3>{dashboard.t("settings.applicationBehavior")}</h3>
            <div class="settings-check-grid">
              <label class="check-row">
                <input type="checkbox" bind:checked={draftSettings.behavior.start_minimized} />
                <span></span>
                {dashboard.t("settings.startMinimized")}
              </label>
              <label class="check-row">
                <input type="checkbox" bind:checked={draftSettings.behavior.close_will_hide} />
                <span></span>
                {dashboard.t("settings.closeMinimized")}
              </label>
              <label class="check-row">
                <input type="checkbox" bind:checked={draftSettings.behavior.launch_at_startup} />
                <span></span>
                {dashboard.t("settings.launchAtStartup")}
              </label>
            </div>
          </div>

          <div class="settings-group">
            <h3>{dashboard.t("settings.appearance")}</h3>
            <div class="input-group settings-narrow-field">
              <label for="localeSelect">{dashboard.t("settings.interfaceLanguage")}</label>
              <select id="localeSelect" value={dashboard.locale} onchange={(event) => dashboard.setLocale(event.currentTarget.value as Locale)}>
                <option value="fr">Français</option>
                <option value="en">English</option>
              </select>
            </div>

            <div class="theme-grid">
              {#each THEMES as theme}
                <button
                  type="button"
                  class="theme-option"
                  class:active={dashboard.currentTheme === theme.id}
                  aria-pressed={dashboard.currentTheme === theme.id}
                  onclick={() => dashboard.selectTheme(theme.id)}
                  style={`--theme-preview-bg:${theme.preview.background};--theme-preview-surface:${theme.preview.surface};--theme-preview-accent:${theme.preview.accent};--theme-preview-text:${theme.preview.text};`}
                >
                  <span class="theme-preview" aria-hidden="true">
                    <span class="theme-preview-panel"></span>
                    <span class="theme-preview-line"></span>
                    <span class="theme-preview-swatches">
                      <span class="theme-swatch accent"></span>
                      <span class="theme-swatch text"></span>
                    </span>
                  </span>
                  <span class="theme-copy">
                    <span class="theme-name">{theme.label}</span>
                  </span>
                </button>
              {/each}
            </div>
          </div>

          <div class="card-actions settings-actions">
            <button class="btn-secondary" onclick={cancelDraftSettings} disabled={dashboard.busy || !settingsDirty}>
              {dashboard.t("common.cancel")}
            </button>
            <button class="btn-primary" onclick={() => void saveDraftSettings()} disabled={dashboard.busy || !settingsDirty}>
              {dashboard.t("common.saveSettings")}
            </button>
          </div>
        </div>
      {:else if section === "runtime"}
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("settings.runtime")}</h2>
          </div>
        </div>

        <div class="section-stack compact-settings-stack">
          <div class="settings-group">
            <div class="settings-group-heading">
              <h3>{dashboard.t("settings.telemetry")}</h3>
              <div class="inline-actions">
                <button type="button" class="btn-secondary" onclick={() => dashboard.openTelemetryHelp()}>
                  {dashboard.t("settings.setupHelp")}
                </button>
                <span class="status-pill {dashboard.telemetryStatus?.state === 'connected' ? 'connected' : dashboard.telemetryStatus?.state === 'connecting' ? 'connecting' : 'disconnected'}">
                  <span class="status-dot"></span>
                  {dashboard.telemetryStatusLabel}
                </span>
              </div>
            </div>
            <div class="studio-grid settings-two-col">
              <div class="input-group">
                <label for="telemetryHost">{dashboard.t("settings.host")}</label>
                <input id="telemetryHost" bind:value={draftSettings.telemetry.rocket_league_host} />
              </div>
              <div class="input-group">
                <label for="telemetryPort">{dashboard.t("settings.port")}</label>
                <input id="telemetryPort" type="number" bind:value={draftSettings.telemetry.rocket_league_port} />
              </div>
              <div class="input-group">
                <label for="updateStateThrottle">{dashboard.t("settings.updateStateThrottle")}</label>
                <input id="updateStateThrottle" type="number" min="1" max="120" bind:value={draftSettings.overlay.update_state_throttle_fps} />
              </div>
            </div>
          </div>

          <div class="card-actions settings-actions">
            <button class="btn-secondary" onclick={cancelDraftSettings} disabled={dashboard.busy || !settingsDirty}>
              {dashboard.t("common.cancel")}
            </button>
            <button class="btn-primary" onclick={() => void saveDraftSettings()} disabled={dashboard.busy || !settingsDirty}>
              {dashboard.t("common.saveSettings")}
            </button>
          </div>
        </div>
      {:else}
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("settings.advanced")}</h2>
          </div>
        </div>

        <div class="section-stack compact-settings-stack">
          <div class="settings-group">
            <h3>{dashboard.t("settings.security")}</h3>
            <label class="check-row">
              <input type="checkbox" bind:checked={draftSettings.security.require_trusted_remote_packages} />
              <span></span>
              {dashboard.t("settings.requireTrustedRemotePackages")}
            </label>
            <label class="check-row">
              <input type="checkbox" bind:checked={draftSettings.security.plugins_safe_mode} />
              <span></span>
              {dashboard.t("settings.pluginsSafeMode")}
            </label>
            <label class="check-row">
              <input type="checkbox" bind:checked={draftSettings.security.disable_plugin_activation} />
              <span></span>
              {dashboard.t("settings.disablePluginActivation")}
            </label>
          </div>

          <div class="settings-group">
            <h3>{dashboard.t("settings.about")}</h3>
            <div class="runtime-version-card">
              <span class="runtime-api-copy">
                <small>{dashboard.t("developer.runtimeApiVersion")}</small>
                <strong>{dashboard.runtimeInfo?.runtimeApiVersion ?? "n/a"}</strong>
              </span>
              <span class="runtime-api-badge">{dashboard.runtimeInfo?.supportedRuntimeApi ?? "n/a"}</span>
            </div>
          </div>

          <div class="card-actions settings-actions">
            <button class="btn-secondary" onclick={cancelDraftSettings} disabled={dashboard.busy || !settingsDirty}>
              {dashboard.t("common.cancel")}
            </button>
            <button class="btn-primary" onclick={() => void saveDraftSettings()} disabled={dashboard.busy || !settingsDirty}>
              {dashboard.t("common.saveSettings")}
            </button>
          </div>
        </div>
      {/if}
    </section>
  </div>
{:else}
  <div class="empty-state">
    <p>{dashboard.t("common.loading")}</p>
  </div>
{/if}
