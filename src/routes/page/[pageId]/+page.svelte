<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";

  const { data } = $props();

  type PageLayout = {
    id: string;
    name: string;
    width: number;
    height: number;
  };

  type PagesFile = {
    pages: PageLayout[];
  };

  let pages = $state<PagesFile | null>(null);
  let message = $state("");

  const page = $derived(pages?.pages.find((entry) => entry.id === data.pageId) ?? null);

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
    window.location.href = "/";
  }

  function editPage() {
    window.location.href = `/editor/page/${encodeURIComponent(data.pageId)}`;
  }

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
</script>

<main>
  <header class="page-toolbar">
    <div class="page-title">
      <button type="button" class="icon-btn" onclick={() => void closePage()} title="Close page">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
      </button>
      <strong>{page?.name ?? "Page"}</strong>
    </div>
    <button type="button" class="btn-primary" onclick={editPage}>Edit</button>
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
    background: #070b12;
    font-family: Inter, Arial, sans-serif;
  }

  main {
    width: 100vw;
    height: 100vh;
    display: grid;
    grid-template-rows: 48px minmax(0, 1fr);
    color: #f8fafc;
    background: #070b12;
  }

  .page-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 14px;
    border-bottom: 1px solid rgba(148, 163, 184, 0.18);
    background: rgba(8, 13, 22, 0.94);
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
    border: 1px solid rgba(148, 163, 184, 0.24);
    color: #f8fafc;
    cursor: pointer;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: grid;
    place-items: center;
    padding: 0;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.04);
  }

  .btn-primary {
    padding: 8px 12px;
    border-radius: 6px;
    background: #2563eb;
    font-weight: 700;
  }

  .empty-state {
    display: grid;
    place-items: center;
    color: rgba(248, 250, 252, 0.7);
  }
</style>
