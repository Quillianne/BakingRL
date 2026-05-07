<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
  import {
    consumePendingRouteReturn,
    returnStateQuery,
    routeReturnFromParams,
    storeRouteScrollRestore,
    type RouteReturnState
  } from "$lib/returnState";

  const { data } = $props();

  type PageLayout = {
    id: string;
    name: string;
    favorite: boolean;
    width: number;
    height: number;
  };

  type PagesFile = {
    pages: PageLayout[];
  };

  let pages = $state<PagesFile | null>(null);
  let message = $state("");
  let pageReturnState = $state<RouteReturnState>({ returnTo: "/", scrollY: 0 });
  let pageReturnInitialized = $state(false);

  const page = $derived(pages?.pages.find((entry) => entry.id === data.pageId) ?? null);
  const routePageReturnState = $derived(routeReturnFromParams(data.returnTo, data.scrollY, "/"));

  async function refresh() {
    pages = await invoke<PagesFile>("get_pages");
  }

  async function closePage() {
    try {
      const current = getCurrentWindow();
      if (current.label.startsWith("page-")) {
        await current.close();
        return;
      }
    } catch {
      // Browser/dev fallback navigates back to the dashboard.
    }
    storeRouteScrollRestore(pageReturnState);
    await navigateTo(pageReturnState.returnTo);
  }

  async function editPage() {
    const pageUrl = `/page/${encodeURIComponent(data.pageId)}${returnStateQuery(pageReturnState)}`;
    await navigateTo(`/editor/page/${encodeURIComponent(data.pageId)}${returnStateQuery({ returnTo: pageUrl, scrollY: 0 })}`);
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
    void listen<PagesFile>("bakingrl-pages-changed", (event) => {
      pages = event.payload;
    }).then((unlisten) => {
      unlistenPages = unlisten;
    });
    return () => {
      unlistenPages?.();
    };
  });
</script>

<main>
  <header class="page-toolbar">
    <div class="page-title">
      <button type="button" class="icon-btn" onclick={() => void closePage()} title="Close page">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
      </button>
      <strong>{page?.name ?? "Page"}</strong>
    </div>
    <button type="button" class="btn-primary" onclick={() => void editPage()}>Edit</button>
  </header>

  {#if page}
    <section class="page-stage" aria-label={page.name}>
      <OverlayRenderer source="page" mode="page" layoutId={page.id} />
    </section>
  {:else}
    <section class="empty-state">
      <p>{message || "Loading page..."}</p>
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
    height: 100vh;
    display: grid;
    grid-template-rows: 48px minmax(0, 1fr);
    color: var(--text-primary);
    background: var(--editor-bg-dark);
  }

  .page-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-color);
    background: var(--editor-bg-panel);
  }

  .page-title {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .page-title strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
  }

  .page-stage {
    min-width: 0;
    min-height: 0;
    position: relative;
  }

  .icon-btn,
  .btn-primary {
    border: 1px solid var(--border-color);
    color: var(--text-primary);
    cursor: pointer;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    padding: 0;
    border-radius: 6px;
    background: var(--bg-panel-hover);
  }

  .btn-primary {
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--accent);
    font-weight: 700;
  }

  .empty-state {
    display: grid;
    place-items: center;
    color: var(--text-secondary);
  }
</style>
