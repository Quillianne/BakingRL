<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import PackageSettingsForm from "$lib/dashboard/PackageSettingsForm.svelte";
  import { getInitialLocale, translations } from "$lib/i18n";
  import type { PackageConfigurationState } from "$lib/dashboard/types";
  import {
    consumePendingRouteReturn,
    routeReturnFromParams,
    storeRouteScrollRestore,
    type RouteReturnState
  } from "$lib/returnState";

  const { data } = $props();
  const t = translations[getInitialLocale()];

  let configurationState = $state<PackageConfigurationState | null>(null);
  let message = $state("");
  let pageReturnState = $state<RouteReturnState>({ returnTo: "/", scrollY: 0 });
  let pageReturnInitialized = $state(false);

  const configurationPrefix = "configuration-";
  const secretsPrefix = "secrets-";
  const configurationPackageId = $derived(
    data.pageId.startsWith(configurationPrefix) ? data.pageId.slice(configurationPrefix.length) : null
  );
  const secretsPackageId = $derived(
    data.pageId.startsWith(secretsPrefix) ? data.pageId.slice(secretsPrefix.length) : null
  );
  const isConfigurationPage = $derived(configurationPackageId !== null);
  const isSecretsPage = $derived(secretsPackageId !== null);
  const packageConfigurationId = $derived(configurationPackageId ?? secretsPackageId);
  const pageTitle = $derived(
    isSecretsPage && configurationState
      ? `${configurationState.title} · ${t["packages.secrets"]}`
      : configurationState?.title ?? t["packages.configuration"]
  );
  const routePageReturnState = $derived(routeReturnFromParams(data.returnTo, data.scrollY, "/"));

  async function refresh() {
    message = "";
    if (packageConfigurationId) {
      configurationState = await invoke<PackageConfigurationState>("get_package_configuration_state", { packageId: packageConfigurationId });
      return;
    }
    configurationState = null;
    message = t["packages.configurationUnavailable"];
  }

  async function closePage() {
    storeRouteScrollRestore(pageReturnState);
    await navigateTo(pageReturnState.returnTo);
  }

  async function navigateTo(path: string) {
    try {
      await goto(path);
    } catch {
      window.location.href = path;
    }
  }

  $effect(() => {
    if (pageReturnInitialized) return;
    pageReturnState = routePageReturnState;
    pageReturnInitialized = true;
  });

  onMount(() => {
    const pendingReturn = consumePendingRouteReturn();
    if (!data.returnTo && pendingReturn) {
      pageReturnState = routeReturnFromParams(pendingReturn.returnTo, pendingReturn.scrollY, "/");
    }
    void refresh().catch((error) => {
      message = String(error);
    });
    let unlistenPackages: (() => void) | undefined;
    void listen("bakingrl-packages-changed", () => {
      if (packageConfigurationId) {
        void refresh().catch((error) => {
          configurationState = null;
          message = String(error);
        });
      }
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    return () => {
      unlistenPackages?.();
    };
  });
</script>

<main>
  <header class="page-toolbar">
    <button type="button" class="icon-btn back-btn" onclick={() => void closePage()} title={t["common.back"]} aria-label={t["common.back"]}>
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2">
        <path d="m12 19-7-7 7-7"></path>
        <path d="M19 12H5"></path>
      </svg>
    </button>
    <strong class="page-heading">{pageTitle}</strong>
  </header>

  {#if isSecretsPage && configurationState}
    <section class="generated-configuration-stage secrets-stage">
      <PackageSettingsForm packageId={secretsPackageId ?? ""} configuration={configurationState} secretOnly />
    </section>
  {:else if isConfigurationPage && configurationState}
    <section class="generated-configuration-stage">
      <PackageSettingsForm packageId={configurationPackageId ?? ""} configuration={configurationState} />
    </section>
  {:else if packageConfigurationId && message}
    <section class="generated-configuration-stage">
      {#if configurationState}
        <PackageSettingsForm packageId={packageConfigurationId ?? ""} configuration={configurationState} secretOnly={isSecretsPage} />
      {:else}
        <p>{message}</p>
      {/if}
    </section>
  {:else}
    <section class="empty-state">
      <p>{message || t["common.loading"]}</p>
    </section>
  {/if}
</main>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--editor-bg-dark);
    font-family: var(--font-family);
  }

  main {
    width: 100vw;
    height: var(--app-content-height, 100vh);
    display: grid;
    grid-template-rows: 48px minmax(0, 1fr);
    color: var(--text-primary);
    background: var(--editor-bg-dark);
  }

  .page-toolbar {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 8px;
    padding: 0 112px 0 10px;
    border-bottom: 1px solid var(--border-color);
    background: var(--editor-bg-panel);
  }

  .page-heading {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 650;
  }

  .icon-btn {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: var(--transition);
  }

  .icon-btn:hover {
    background: var(--editor-bg-panel-hover);
    color: var(--text-primary);
  }

  .back-btn {
    color: var(--text-primary);
  }

  .empty-state {
    display: grid;
    place-items: center;
    color: var(--text-secondary);
  }

  .generated-configuration-stage {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .generated-configuration-stage {
    display: grid;
    place-items: center;
    padding: 18px;
  }

  .secrets-stage :global(.package-settings-form) {
    width: min(560px, calc(100vw - 48px));
  }

</style>
