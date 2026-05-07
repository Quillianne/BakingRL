<script lang="ts">
  import type { DashboardState } from "$lib/dashboard/state.svelte";

  const {
    state
  }: {
    state: DashboardState;
  } = $props();
</script>

{#if state.telemetryHelpOpen}
  <div class="modal-layer">
    <button
      type="button"
      class="modal-scrim"
      aria-label={state.t("telemetry.helpClose")}
      onclick={() => state.closeTelemetryHelp()}
    ></button>
    <div
      class="studio-modal telemetry-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="telemetry-help-title"
      tabindex="-1"
    >
      <div class="modal-heading">
        <span class="badge route">Rocket League Telemetry</span>
        <h2 id="telemetry-help-title">{state.t("telemetry.helpTitle")}</h2>
        <p>{state.t("telemetry.helpDesc")}</p>
      </div>

      <ol class="setup-steps">
        <li>{state.t("telemetry.stepClose")}</li>
        <li>{state.t("telemetry.stepOpen")} <code>&lt;Rocket League install&gt;\TAGame\Config\DefaultStatsAPI.ini</code>.</li>
        <li>{state.t("telemetry.stepPacket")}</li>
        <li>{state.t("telemetry.stepPort")} <code>{state.appSettings?.telemetry.rocket_league_port ?? state.telemetryStatus?.port ?? 49123}</code>.</li>
        <li>{state.t("telemetry.stepRestart")}</li>
      </ol>

      <div class="callout">
        <strong>{state.t("telemetry.expected")}</strong>
        <span>
          {state.t("telemetry.listensOn")}
          <code>{state.appSettings?.telemetry.rocket_league_host ?? state.telemetryStatus?.host ?? "127.0.0.1"}:{state.appSettings?.telemetry.rocket_league_port ?? state.telemetryStatus?.port ?? 49123}</code>.
          {state.t("telemetry.keepAligned")}
        </span>
      </div>

      <label class="check-row">
        <input type="checkbox" bind:checked={state.telemetryHelpDontShow} />
        <span></span>
        {state.t("telemetry.dontShow")}
      </label>

      <div class="modal-actions">
        <button class="btn-primary" onclick={() => state.closeTelemetryHelp()}>OK</button>
      </div>
    </div>
  </div>
{/if}
