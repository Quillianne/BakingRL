<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
  import PackageSettingsForm from "$lib/dashboard/PackageSettingsForm.svelte";
  import { getInitialLocale, translations } from "$lib/i18n";
  import type { PackageConfigurationState, PageLayout, PagesFile } from "$lib/dashboard/types";
  import {
    consumePendingRouteReturn,
    returnStateQuery,
    routeReturnFromParams,
    storeRouteScrollRestore,
    type RouteReturnState
  } from "$lib/returnState";

  const { data } = $props();
  const t = translations[getInitialLocale()];

  let pages = $state<PagesFile | null>(null);
  let configurationPage = $state<PageLayout | null>(null);
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
  const isPackageConfigurationRoute = $derived(packageConfigurationId !== null);
  const page = $derived(configurationPage ?? pages?.pages.find((entry) => entry.id === data.pageId) ?? null);
  const pageTitle = $derived(
    page?.name ??
      (isSecretsPage && configurationState
        ? `${configurationState.title} · ${t["packages.secrets"]}`
        : configurationState?.title ?? (isPackageConfigurationRoute ? t["packages.configuration"] : t["nav.pages"]))
  );
  const routePageReturnState = $derived(routeReturnFromParams(data.returnTo, data.scrollY, "/"));

  async function refresh() {
    message = "";
    if (packageConfigurationId) {
      configurationState = await invoke<PackageConfigurationState>("get_package_configuration_state", { packageId: packageConfigurationId });
      if (configurationPackageId && configurationState.hasCustomPage) {
        try {
          configurationPage = await invoke<PageLayout>("get_package_configuration_page", { packageId: configurationPackageId });
        } catch (error) {
          configurationPage = null;
          message = String(error);
        }
      } else {
        configurationPage = null;
      }
      pages = null;
      return;
    }
    configurationPage = null;
    configurationState = null;
    pages = await invoke<PagesFile>("get_pages");
  }

  async function editPage() {
    if (isPackageConfigurationRoute) return;
    const pageUrl = `/page/${encodeURIComponent(data.pageId)}${returnStateQuery(pageReturnState)}`;
    await navigateTo(`/editor/page/${encodeURIComponent(data.pageId)}${returnStateQuery({ returnTo: pageUrl, scrollY: 0 })}`);
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
    let unlistenPages: (() => void) | undefined;
    let unlistenPackages: (() => void) | undefined;
    void listen<PagesFile>("bakingrl-pages-changed", (event) => {
      if (!packageConfigurationId) pages = event.payload;
    }).then((unlisten) => {
      unlistenPages = unlisten;
    });
    void listen("bakingrl-packages-changed", () => {
      if (packageConfigurationId) {
        void refresh().catch((error) => {
          configurationPage = null;
          configurationState = null;
          message = String(error);
        });
      }
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    return () => {
      unlistenPages?.();
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
    {#if !isPackageConfigurationRoute}
      <button type="button" class="btn-primary" onclick={() => void editPage()}>{t["common.edit"]}</button>
    {/if}
  </header>

  {#if isSecretsPage && configurationState}
    <section class="generated-configuration-stage secrets-stage">
      <PackageSettingsForm packageId={secretsPackageId ?? ""} configuration={configurationState} secretOnly />
    </section>
  {:else if isConfigurationPage && configurationState}
    {#if page}
      <section class="page-stage configuration-stage" aria-label={page.name}>
        <div class="configuration-preview">
          <OverlayRenderer
            source="configuration"
            mode="page"
            layoutOverride={page}
            layoutRevision={page.updated_at_ms}
            preview
          />
        </div>
      </section>
    {:else}
      <section class="generated-configuration-stage">
        {#if configurationState.hasCustomPage && message}
          <div class="configuration-error">
            <p>{message}</p>
          </div>
        {:else}
          <PackageSettingsForm packageId={configurationPackageId ?? ""} configuration={configurationState} />
        {/if}
      </section>
    {/if}
  {:else if page}
    <section class="page-stage" aria-label={page.name}>
      <OverlayRenderer source="page" mode="page" layoutId={page.id} />
    </section>
  {:else if isPackageConfigurationRoute && message}
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

  .page-stage {
    min-width: 0;
    min-height: 0;
    position: relative;
  }

  .btn-primary {
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    cursor: pointer;
  }

  .btn-primary {
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--accent);
    font-weight: 700;
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

  .configuration-stage {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 0;
  }

  .configuration-preview,
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

  .configuration-error {
    display: flex;
    width: min(720px, calc(100vw - 48px));
    flex-direction: column;
    gap: 14px;
    color: var(--danger);
    font-size: 13px;
    font-weight: 700;
  }

</style>
