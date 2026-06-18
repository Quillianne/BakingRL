<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { adapter } from "$lib/adapter/index";
  import { importPluginModule } from "$lib/pluginModuleLoader";
  import { mountPluginWebview, type PluginWebviewHandle } from "$lib/pluginWebview";
  import type { GameEventFrame } from "$lib/rlTelemetry";

  const { data } = $props();

  let root = $state<HTMLElement | null>(null);
  let message = $state("");

  type TelemetryCallback = (event: GameEventFrame) => void | Promise<void>;
  type TelemetrySubscription = {
    eventName: string;
    callback: TelemetryCallback;
  };

  function publishTelemetry(callbacks: Set<TelemetryCallback>, event: unknown) {
    for (const callback of callbacks) void callback(event as GameEventFrame);
  }

  async function packageSettings() {
    try {
      return await invoke<Record<string, unknown>>("get_package_settings", { packageId: data.packageId });
    } catch {
      return {};
    }
  }

  async function savePackageSettings(values: Record<string, unknown>) {
    return await invoke<Record<string, unknown>>("save_package_settings", {
      packageId: data.packageId,
      values
    });
  }

  function subscribePackageSettings(callback: (settings: Record<string, unknown>) => void | Promise<void>) {
    let active = true;
    let unlisten: (() => void) | null = null;
    void listen<string>("bakingrl-package-settings-changed", (event) => {
      if (!active || event.payload !== data.packageId) return;
      void packageSettings().then((settings) => {
        if (active) void callback(settings);
      });
    }).then((cleanup) => {
      if (active) unlisten = cleanup;
      else cleanup();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  }

  onMount(() => {
    let disposed = false;
    let webviewHandle: PluginWebviewHandle | null = null;
    let unlistenTelemetry: (() => void) | undefined;
    let moduleCleanup: (() => void) | undefined;
    const telemetryCallbacks = new Set<TelemetryCallback>();
    const telemetrySubscriptions = new Set<TelemetrySubscription>();
    let latestTelemetryFrame: GameEventFrame | null = null;

    async function readTelemetrySnapshot() {
      try {
        const snapshot = await invoke<GameEventFrame | null>("get_telemetry_snapshot");
        if (snapshot) latestTelemetryFrame = snapshot;
        return snapshot ?? latestTelemetryFrame;
      } catch {
        return latestTelemetryFrame;
      }
    }

    function publishTelemetryFrame(eventName: string, payload?: unknown) {
      return invoke("emit_developer_telemetry", {
        frame: {
          Event: eventName,
          Data: payload ?? null
        }
      });
    }

    function createTelemetryHub() {
      return {
        subscribe(eventName: string, callback: TelemetryCallback) {
          const subscription = { eventName, callback };
          telemetrySubscriptions.add(subscription);
          return () => telemetrySubscriptions.delete(subscription);
        },
        publish(eventName: string, payload?: unknown) {
          return publishTelemetryFrame(eventName, payload);
        },
        snapshot: readTelemetrySnapshot,
        getSnapshot: readTelemetrySnapshot
      };
    }

    const telemetryHub = createTelemetryHub();

    async function mount() {
      if (!root) return;
      const settings = await packageSettings();
      const dimensions = {
        width: Math.max(1, root.clientWidth || window.innerWidth),
        height: Math.max(1, root.clientHeight || window.innerHeight)
      };

      if (data.entry) {
        const module = await importPluginModule(data.packageId, data.entry, Date.now());
        const exported = module.default ?? module;
        if (typeof exported?.mount === "function") {
          const cleanup = await exported.mount({
            root,
            packageId: data.packageId,
            webviewId: data.webviewId,
            settings: {
              get: packageSettings,
              save: savePackageSettings,
              subscribe: subscribePackageSettings
            },
            telemetryHub,
            dimensions,
            mode: "runtime"
          });
          if (typeof cleanup === "function") {
            if (disposed) cleanup();
            else moduleCleanup = cleanup;
          }
        }
        return;
      }

      if (data.path) {
        webviewHandle = mountPluginWebview({
          root,
          src: adapter.packageHtmlUrl(data.packageId, data.path, Date.now()),
          packageId: data.packageId,
          exportName: data.webviewId,
          runtimeApi: data.runtimeApi,
          item: {
            id: data.webviewId,
            name: data.webviewId,
            width: dimensions.width,
            height: dimensions.height,
            settings: {}
          },
          settings,
          mode: "runtime",
          assetUrl(ref) {
            return adapter.packageFileUrl(data.packageId, ref);
          },
          subscribeTelemetry(callback) {
            telemetryCallbacks.add(callback);
            return () => telemetryCallbacks.delete(callback);
          },
          getTelemetrySnapshot: readTelemetrySnapshot,
          publishTelemetry: publishTelemetryFrame
        });
        return;
      }

      throw new Error("Missing plugin webview entry.");
    }

    void listen<GameEventFrame>("bakingrl-telemetry", (event) => {
      latestTelemetryFrame = event.payload;
      publishTelemetry(telemetryCallbacks, event.payload);
      for (const subscription of telemetrySubscriptions) {
        if (subscription.eventName === event.payload.Event) void subscription.callback(event.payload);
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenTelemetry = unlisten;
    });

    void mount().catch((error) => {
      if (!disposed) message = error instanceof Error ? error.message : String(error);
    });

    return () => {
      disposed = true;
      moduleCleanup?.();
      webviewHandle?.cleanup();
      unlistenTelemetry?.();
    };
  });
</script>

<svelte:head>
  <title>{data.webviewId}</title>
</svelte:head>

<main class="plugin-webview-host" bind:this={root}>
  {#if message}
    <div class="plugin-webview-error">{message}</div>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
  }

  .plugin-webview-host {
    min-height: calc(100vh - 48px);
    width: 100%;
    background: var(--bg-primary);
    color: var(--text-primary);
    overflow: hidden;
  }

  .plugin-webview-error {
    display: grid;
    min-height: calc(100vh - 48px);
    place-items: center;
    padding: 24px;
    color: var(--danger);
    text-align: center;
  }
</style>
