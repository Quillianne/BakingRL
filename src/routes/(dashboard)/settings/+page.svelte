<script lang="ts">
  import { getDashboardContext } from "$lib/dashboard/context";
  import { THEMES } from "$lib/themes";
  import type { Locale } from "$lib/i18n";

  const dashboard = getDashboardContext();

  let section = $state<"appearance" | "telemetry" | "overlay">("appearance");
</script>

<div class="page-title">
  <div>
    <h1>{dashboard.t("nav.settings")}</h1>
    <p>{dashboard.t("settings.appearanceDesc")}</p>
  </div>
</div>

{#if dashboard.appSettings}
  <div class="settings-layout">
    <nav class="studio-panel settings-nav" aria-label={dashboard.t("nav.settings")}>
      <button class="btn-outline" class:active={section === "appearance"} onclick={() => (section = "appearance")}>
        {dashboard.t("settings.appearance")}
      </button>
      <button class="btn-outline" class:active={section === "telemetry"} onclick={() => (section = "telemetry")}>
        {dashboard.t("settings.telemetry")}
      </button>
      <button class="btn-outline" class:active={section === "overlay"} onclick={() => (section = "overlay")}>
        {dashboard.t("settings.overlay")}
      </button>
    </nav>

    <section class="studio-panel">
      {#if section === "appearance"}
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("settings.appearance")}</h2>
            <p>{dashboard.t("settings.appearanceDesc")}</p>
          </div>
        </div>

        <div class="section-stack">
          <div class="input-group">
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
                  <span class="theme-description">{theme.description}</span>
                </span>
              </button>
            {/each}
          </div>
        </div>
      {:else if section === "telemetry"}
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("settings.telemetry")}</h2>
            <p>{dashboard.t("settings.telemetryDesc")}</p>
          </div>
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

        <div class="studio-grid three-col">
          <div class="input-group">
            <label for="telemetryHost">{dashboard.t("settings.host")}</label>
            <input id="telemetryHost" bind:value={dashboard.appSettings.telemetry.rocket_league_host} />
          </div>
          <div class="input-group">
            <label for="telemetryPort">{dashboard.t("settings.port")}</label>
            <input id="telemetryPort" type="number" bind:value={dashboard.appSettings.telemetry.rocket_league_port} />
          </div>
        </div>

        <div class="card-actions">
          <button class="btn-primary" onclick={() => void dashboard.saveAppSettings()} disabled={dashboard.busy}>
            {dashboard.t("common.saveSettings")}
          </button>
        </div>
      {:else}
        <div class="panel-heading">
          <div>
            <h2>{dashboard.t("settings.overlay")}</h2>
            <p>{dashboard.t("settings.overlayDesc")}</p>
          </div>
        </div>

        <div class="section-stack">
          <div class="studio-grid three-col">
            <div class="input-group">
              <label for="overlayFps">{dashboard.t("settings.updateRate")}</label>
              <input id="overlayFps" type="number" min="1" max="120" bind:value={dashboard.appSettings.overlay.update_rate_fps} />
            </div>

            {#if dashboard.appSettings.overlay.use_monitor_size}
              <div class="input-group">
                <label for="overlayMonitor">{dashboard.t("settings.overlayMonitor")}</label>
                <select id="overlayMonitor" bind:value={dashboard.appSettings.overlay.monitor_id}>
                  <option value="">{dashboard.t("settings.currentPrimary")}</option>
                  {#each dashboard.overlayMonitors as monitor}
                    <option value={monitor.id}>
                      {monitor.name} · {monitor.width}x{monitor.height}{monitor.primary ? " · primary" : ""}{monitor.current ? " · current" : ""}
                    </option>
                  {/each}
                </select>
              </div>
            {:else}
              <div class="input-group">
                <span class="field-label">{dashboard.t("settings.overlaySize")}</span>
                <div class="form-row">
                  <input type="number" min="1" bind:value={dashboard.appSettings.overlay.screen_width} aria-label="Overlay width" />
                  <input type="number" min="1" bind:value={dashboard.appSettings.overlay.screen_height} aria-label="Overlay height" />
                </div>
              </div>
            {/if}
          </div>

          <label class="check-row">
            <input type="checkbox" bind:checked={dashboard.appSettings.overlay.use_monitor_size} />
            <span></span>
            {dashboard.t("settings.useFullMonitor")}
          </label>
          <label class="check-row">
            <input type="checkbox" bind:checked={dashboard.appSettings.overlay.hide_when_game_unfocused} />
            <span></span>
            {dashboard.t("settings.hideUnfocused")}
          </label>

          <div class="section-heading compact">
            <div>
              <h2>{dashboard.t("settings.applicationBehavior")}</h2>
            </div>
          </div>

          <label class="check-row">
            <input type="checkbox" bind:checked={dashboard.appSettings.behavior.start_minimized} />
            <span></span>
            {dashboard.t("settings.startMinimized")}
          </label>
          <label class="check-row">
            <input type="checkbox" bind:checked={dashboard.appSettings.behavior.close_will_hide} />
            <span></span>
            {dashboard.t("settings.closeMinimized")}
          </label>
          <label class="check-row">
            <input type="checkbox" bind:checked={dashboard.appSettings.behavior.launch_at_startup} />
            <span></span>
            {dashboard.t("settings.launchAtStartup")}
          </label>

          <div class="card-actions">
            <button class="btn-primary" onclick={() => void dashboard.saveAppSettings()} disabled={dashboard.busy}>
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
