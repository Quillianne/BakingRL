<script lang="ts">
  import "$lib/theme.css";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { Minus, Square, X } from "@lucide/svelte";
  import { applyTheme, getStoredTheme } from "$lib/themes";

  let { children } = $props();
  let windowLabel = $state(detectWindowLabel());

  const dragZoneHeight = 96;
  const packageFileOpenedEvent = "bakingrl-package-files-opened";
  const isSurfaceWindow = $derived(windowLabel?.startsWith("plugin-surface-") ?? false);
  const showWindowFrame = $derived(windowLabel !== null && !isSurfaceWindow);

  $effect(() => {
    if (typeof document === "undefined") return;
    if (isSurfaceWindow) document.documentElement.dataset.bakingrlWindow = "surface";
    else delete document.documentElement.dataset.bakingrlWindow;
  });

  onMount(() => {
    applyTheme(getStoredTheme());
    windowLabel = detectWindowLabel();
    let unlistenPackageFiles: (() => void) | undefined;
    if (windowLabel === "main") {
      void listen(packageFileOpenedEvent, () => {
        void navigateToPlugins();
      }).then((unlisten) => {
        unlistenPackageFiles = unlisten;
      });
    }
    return () => {
      unlistenPackageFiles?.();
    };
  });

  function detectWindowLabel() {
    try {
      return getCurrentWindow().label;
    } catch {
      return null;
    }
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
        <Minus size={13} strokeWidth={1.8} aria-hidden="true" />
      </button>
      <button type="button" class="app-window-control" aria-label="Agrandir" title="Agrandir" onclick={toggleMaximizeWindow}>
        <Square size={11} strokeWidth={1.8} aria-hidden="true" />
      </button>
      <button type="button" class="app-window-control close" aria-label="Fermer" title="Fermer" onclick={closeWindow}>
        <X size={13} strokeWidth={1.8} aria-hidden="true" />
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

  :global(html[data-bakingrl-window="surface"]),
  :global(html[data-bakingrl-window="surface"] body) {
    background: transparent !important;
  }

  .app-window-controls {
    position: fixed;
    z-index: 10001;
    top: 0;
    right: 0;
    display: flex;
    gap: 0;
    align-items: center;
  }

  .app-window-control {
    display: grid;
    place-items: center;
    width: 36px;
    height: 32px;
    border: 0;
    border-left: 1px solid var(--border-color);
    border-radius: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    opacity: 0.82;
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

</style>
