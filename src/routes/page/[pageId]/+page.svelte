<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow, LogicalSize, PhysicalSize } from "@tauri-apps/api/window";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";

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
  let resizedPageId = $state("");

  const page = $derived(pages?.pages.find((entry) => entry.id === data.pageId) ?? null);
  const MAIN_WINDOW_SIZE_KEY = "bakingrl.mainWindowBeforeInAppPage";
  const PAGE_TOOLBAR_HEIGHT = 48;
  const MAIN_MIN_WIDTH = 1024;
  const MAIN_MIN_HEIGHT = 700;

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
      await restoreMainWindowSize();
    } catch {
      // Browser/dev fallback navigates back to the dashboard.
    }
    window.location.href = "/";
  }

  async function editPage() {
    await restoreMainWindowSize();
    window.location.href = `/editor/page/${encodeURIComponent(data.pageId)}`;
  }

  async function fitMainWindowToPage(entry: PageLayout) {
    try {
      const current = getCurrentWindow();
      if (current.label !== "main") return;

      if (!sessionStorage.getItem(MAIN_WINDOW_SIZE_KEY)) {
        const size = await current.innerSize();
        sessionStorage.setItem(MAIN_WINDOW_SIZE_KEY, JSON.stringify({ width: size.width, height: size.height }));
      }

      const width = Math.max(320, Math.round(entry.width));
      const height = Math.max(240, Math.round(entry.height)) + PAGE_TOOLBAR_HEIGHT;
      await current.setMinSize(new LogicalSize(320, 260));
      await current.setSize(new LogicalSize(width, height));
      resizedPageId = entry.id;
    } catch {
      // Browser/dev fallback keeps the current window size.
    }
  }

  async function restoreMainWindowSize() {
    try {
      const current = getCurrentWindow();
      if (current.label !== "main") return;
      const raw = sessionStorage.getItem(MAIN_WINDOW_SIZE_KEY);
      if (!raw) return;
      const size = JSON.parse(raw) as { width?: number; height?: number };
      await current.setMinSize(new LogicalSize(MAIN_MIN_WIDTH, MAIN_MIN_HEIGHT));
      if (typeof size.width === "number" && typeof size.height === "number") {
        await current.setSize(new PhysicalSize(size.width, size.height));
      }
      sessionStorage.removeItem(MAIN_WINDOW_SIZE_KEY);
    } catch {
      // Browser/dev fallback has no native window to restore.
    }
  }

  $effect(() => {
    if (page && page.id !== resizedPageId) {
      void fitMainWindowToPage(page);
    }
  });

  onMount(() => {
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

  onDestroy(() => {
    void restoreMainWindowSize();
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
