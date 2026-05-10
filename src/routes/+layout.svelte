<script lang="ts">
  import "$lib/theme.css";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { packageRuntime } from "$lib/packageRuntime.svelte";
  import { applyTheme, getStoredTheme } from "$lib/themes";

  let { children } = $props();
  let windowLabel = $state(detectWindowLabel());

  const dragZoneHeight = 96;
  const packageFileOpenedEvent = "bakingrl-package-files-opened";
  const showWindowFrame = $derived(windowLabel !== null && !isOverlayRuntimeRoute($page.url.pathname));

  onMount(() => {
    applyTheme(getStoredTheme());
    windowLabel = detectWindowLabel();
    let unlistenPackageFiles: (() => void) | undefined;
    let unlistenPackages: (() => void) | undefined;
    void listen("bakingrl-packages-changed", () => {
      packageRuntime.markPackagesChanged();
    }).then((unlisten) => {
      unlistenPackages = unlisten;
    });
    if (windowLabel === "main") {
      void listen(packageFileOpenedEvent, () => {
        void navigateToPlugins();
      }).then((unlisten) => {
        unlistenPackageFiles = unlisten;
      });
    }
    return () => {
      unlistenPackageFiles?.();
      unlistenPackages?.();
    };
  });

  function detectWindowLabel() {
    try {
      return getCurrentWindow().label;
    } catch {
      return null;
    }
  }

  function isOverlayRuntimeRoute(pathname: string) {
    return pathname === "/overlay" || pathname.startsWith("/overlay/");
  }

  async function navigateToPlugins() {
    try {
      await goto("/plugins");
    } catch {
      window.location.href = "/plugins";
    }
  }

  type AppWindow = ReturnType<typeof getCurrentWindow>;

  function runWindowAction(action: (appWindow: AppWindow) => Promise<void>) {
    try {
      void action(getCurrentWindow()).catch(() => {});
    } catch {
      // Browser fallback: window controls are only active in Tauri.
    }
  }

  function minimizeWindow() {
    runWindowAction((appWindow) => appWindow.minimize());
  }

  function toggleMaximizeWindow() {
    runWindowAction((appWindow) => appWindow.toggleMaximize());
  }

  function closeWindow() {
    runWindowAction((appWindow) => appWindow.close());
  }

  function startWindowDrag(event: PointerEvent) {
    if (event.button !== 0) return;
    if (event.clientY > dragZoneHeight) return;
    const target = event.target as HTMLElement | null;
    const isDraggableScrim = Boolean(target?.closest(".modal-scrim"));
    if (!isDraggableScrim && target?.closest("button, a, input, textarea, select")) return;
    runWindowAction((appWindow) => appWindow.startDragging());
  }
</script>

{#if showWindowFrame}
  <div class="app-window-frame" role="presentation" onpointerdown={startWindowDrag}>
    <div class="app-window-controls">
      <button type="button" class="app-window-control" aria-label="Réduire" title="Réduire" onclick={minimizeWindow}>
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
          <path d="M2 6h8" />
        </svg>
      </button>
      <button type="button" class="app-window-control" aria-label="Agrandir" title="Agrandir" onclick={toggleMaximizeWindow}>
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
          <rect x="3" y="3" width="6" height="6" rx="1" />
        </svg>
      </button>
      <button type="button" class="app-window-control close" aria-label="Fermer" title="Fermer" onclick={closeWindow}>
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
          <path d="m3 3 6 6M9 3 3 9" />
        </svg>
      </button>
    </div>
    {@render children()}
  </div>
{:else}
  {@render children()}
{/if}

<style>
  .app-window-frame {
    --app-content-height: 100vh;

    position: relative;
    width: 100vw;
    min-height: 100vh;
    background: var(--bg-dark);
    color: var(--text-primary);
  }

  .app-window-controls {
    position: fixed;
    z-index: 10001;
    top: 6px;
    right: 6px;
    display: flex;
    gap: 2px;
    align-items: center;
  }

  .app-window-control {
    display: grid;
    place-items: center;
    width: 28px;
    height: 24px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0.7;
  }

  .app-window-control:hover {
    background: color-mix(in srgb, var(--bg-panel-hover) 85%, transparent);
    color: var(--text-primary);
    opacity: 1;
  }

  .app-window-control.close:hover {
    background: color-mix(in srgb, var(--danger) 18%, transparent);
    color: var(--danger);
  }

  .app-window-control svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
