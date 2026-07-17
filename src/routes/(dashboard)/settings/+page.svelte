<script lang="ts">
  import { Activity, Save, Undo2 } from "@lucide/svelte";
  import { getDashboardContext } from "$lib/dashboard/context";
  import { THEMES } from "$lib/themes";
  import type { Locale } from "$lib/i18n";
  import type { AppSettings } from "$lib/dashboard/types";

  const dashboard = getDashboardContext();

  let section = $state<"general" | "runtime" | "advanced">("general");
  let draftSettings = $state<AppSettings | null>(null);
  let syncedSettingsSignature = "";
  const settingsDirty = $derived(
    Boolean(
      draftSettings &&
        dashboard.appSettings &&
        settingsSignature(draftSettings) !== settingsSignature(dashboard.appSettings)
    )
  );
  const telemetryStateClass = $derived(
    dashboard.telemetryStatus?.state === "connected"
      ? "connected"
      : dashboard.telemetryStatus?.state === "connecting"
        ? "connecting"
        : "disconnected"
  );

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

<header class="page-title control-page-title">
  <div>
    <span class="page-index">04 / BakingRL</span>
    <h1>{dashboard.t("nav.settings")}</h1>
  </div>
</header>

{#if draftSettings}
  <div class="settings-layout">
    <nav class="settings-nav" aria-label={dashboard.t("nav.settings")}>
      <button type="button" class:active={section === "general"} aria-pressed={section === "general"} onclick={() => (section = "general")}>
        <span>01</span>
        <strong>{dashboard.t("settings.general")}</strong>
      </button>
      <button type="button" class:active={section === "runtime"} aria-pressed={section === "runtime"} onclick={() => (section = "runtime")}>
        <span>02</span>
        <strong>{dashboard.t("settings.runtime")}</strong>
      </button>
      <button type="button" class:active={section === "advanced"} aria-pressed={section === "advanced"} onclick={() => (section = "advanced")}>
        <span>03</span>
        <strong>{dashboard.t("settings.advanced")}</strong>
      </button>
    </nav>

    <section class="settings-content">
      {#if section === "general"}
        <header class="settings-section-header">
          <span>01</span>
          <h2>{dashboard.t("settings.general")}</h2>
        </header>

        <div class="setting-block">
          <h3>{dashboard.t("settings.applicationBehavior")}</h3>
          <div class="setting-lines">
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

        <div class="setting-block">
          <h3>{dashboard.t("settings.appearance")}</h3>
          <label class="setting-field-row" for="localeSelect">
            <span>{dashboard.t("settings.interfaceLanguage")}</span>
            <select id="localeSelect" value={dashboard.locale} onchange={(event) => dashboard.setLocale(event.currentTarget.value as Locale)}>
              <option value="fr">Français</option>
              <option value="en">English</option>
            </select>
          </label>

          <div class="theme-switcher" role="group" aria-label={dashboard.t("settings.appearance")}>
            {#each THEMES as theme, index}
              <button
                type="button"
                class:active={dashboard.currentTheme === theme.id}
                aria-pressed={dashboard.currentTheme === theme.id}
                onclick={() => dashboard.selectTheme(theme.id)}
              >
                <span>{String(index + 1).padStart(2, "0")}</span>
                <i style={`--swatch:${theme.preview.background}`}></i>
                <i style={`--swatch:${theme.preview.surface}`}></i>
                <i style={`--swatch:${theme.preview.accent}`}></i>
                <strong>{dashboard.t(theme.labelKey)}</strong>
              </button>
            {/each}
          </div>
          <p class="setting-immediate-note">{dashboard.t("settings.appearanceImmediate")}</p>
        </div>
      {:else if section === "runtime"}
        <header class="settings-section-header">
          <span>02</span>
          <h2>{dashboard.t("settings.runtime")}</h2>
        </header>

        <div class="setting-block">
          <div class="setting-block-heading">
            <h3>{dashboard.t("settings.telemetry")}</h3>
            <button type="button" class="settings-runtime-state" onclick={() => dashboard.openTelemetryHelp()}>
              <Activity size={15} strokeWidth={1.8} />
              <i class={telemetryStateClass}></i>
              {dashboard.telemetryStatusLabel}
            </button>
          </div>

          <div class="setting-input-grid">
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
              <input id="updateStateThrottle" type="number" min="1" max="120" bind:value={draftSettings.telemetry.update_state_throttle_fps} />
            </div>
          </div>

          <button type="button" class="section-command inline-command" onclick={() => dashboard.openTelemetryHelp()}>
            {dashboard.t("settings.setupHelp")}
          </button>
        </div>
      {:else}
        <header class="settings-section-header">
          <span>03</span>
          <h2>{dashboard.t("settings.advanced")}</h2>
        </header>

        <div class="setting-block">
          <h3>{dashboard.t("settings.security")}</h3>
          <div class="setting-lines">
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
        </div>

        <div class="setting-block">
          <h3>{dashboard.t("settings.about")}</h3>
          <dl class="version-readout">
            <div>
              <dt>{dashboard.t("settings.appVersion")}</dt>
              <dd>{dashboard.runtimeInfo?.appVersion ?? "n/a"}</dd>
            </div>
            <div>
              <dt>{dashboard.t("developer.runtimeApiVersion")}</dt>
              <dd>{dashboard.runtimeInfo?.runtimeApiVersion ?? "n/a"}</dd>
            </div>
            <div>
              <dt>{dashboard.t("developer.supportedRuntimeApi")}</dt>
              <dd>{dashboard.runtimeInfo?.supportedRuntimeApi ?? "n/a"}</dd>
            </div>
          </dl>
        </div>
      {/if}

      <footer class="settings-savebar">
        <button class="btn-secondary" onclick={cancelDraftSettings} disabled={dashboard.busy || !settingsDirty}>
          <Undo2 size={15} strokeWidth={1.8} />
          {dashboard.t("common.cancel")}
        </button>
        <button class="btn-primary" onclick={() => void saveDraftSettings()} disabled={dashboard.busy || !settingsDirty}>
          <Save size={15} strokeWidth={1.8} />
          {dashboard.t("common.saveSettings")}
        </button>
      </footer>
    </section>
  </div>
{:else}
  <div class="empty-state">
    <p>{dashboard.t("common.loading")}</p>
  </div>
{/if}
