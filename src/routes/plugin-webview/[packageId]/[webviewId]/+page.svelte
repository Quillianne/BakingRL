<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { importPluginModule } from "$lib/pluginModuleLoader";
  import type { PackageConfigurationState } from "$lib/dashboard/types";
  import type { GameEventFrame } from "$lib/rlTelemetry";

  const { data } = $props();

  let root = $state<HTMLElement | null>(null);
  let message = $state("");
  let isSurface = $state(false);

  type TelemetryCallback = (event: GameEventFrame) => void | Promise<void>;
  type TelemetrySubscription = {
    eventName: string;
    callback: TelemetryCallback;
  };

  type PackageWebviewRuntimeDescriptor = {
    packageId: string;
    webviewId: string;
    entry: string;
    kind?: string | null;
    runtimeApi: string;
  };

  type DiagnosticSeverity = "info" | "warning" | "error";

  type PackageWebviewAssetPayload = {
    contentsBase64: string;
    contentType: string;
    path: string;
  };

  type ModuleSettings = Record<string, unknown> & {
    get(): Promise<Record<string, unknown>>;
    save(values: Record<string, unknown>): Promise<Record<string, unknown>>;
    subscribe(callback: (settings: Record<string, unknown>) => void | Promise<void>): () => void;
  };

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

  async function updatePackageSettings(values: Record<string, unknown>) {
    return await savePackageSettings({
      ...(await packageSettings()),
      ...values
    });
  }

  async function resetPackageSettings() {
    return await savePackageSettings({});
  }

  async function packageConfigurationState() {
    return await invoke<PackageConfigurationState>("get_package_configuration_state", {
      packageId: data.packageId
    });
  }

  async function packageSecretConfigured(key: string) {
    const state = await packageConfigurationState();
    return state.secrets.some((secret) => secret.key === key && secret.configured);
  }

  async function setPackageSecret(key: string, value: string) {
    return await invoke<PackageConfigurationState>("set_package_secret", {
      packageId: data.packageId,
      key,
      value
    });
  }

  async function deletePackageSecret(key: string) {
    return await invoke<PackageConfigurationState>("delete_package_secret", {
      packageId: data.packageId,
      key
    });
  }

  async function callPackageService(serviceRef: string, method: string, input?: unknown) {
    return await invoke("call_service_export", {
      callerPackageId: data.packageId,
      serviceRef,
      method,
      input: input ?? null
    });
  }

  async function packageAssetUrl(packageId: string, webviewId: string, relativePath: string) {
    const asset = await invoke<PackageWebviewAssetPayload>("read_package_webview_asset", {
      packageId,
      webviewId,
      relativePath
    });
    const contentType = asset.contentType.replace(/\s*;\s*/g, ";");
    return `data:${contentType};base64,${asset.contentsBase64}`;
  }

  async function readPackageRegistry(key: string) {
    return await invoke("plugin_registry_get", {
      packageId: data.packageId,
      key
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

  function createModuleSettings(settings: Record<string, unknown>, canSave: boolean): ModuleSettings {
    return {
      ...settings,
      get: packageSettings,
      save(values) {
        if (!canSave) {
          throw new Error("Only settings webviews can save package settings.");
        }
        return savePackageSettings(values);
      },
      subscribe: subscribePackageSettings
    };
  }

  function diagnosticDetails(details: unknown) {
    if (details === undefined) return "";
    if (typeof details === "string") return details;
    try {
      return JSON.stringify(details);
    } catch {
      return String(details);
    }
  }

  function diagnosticMessage(message: string, details?: unknown) {
    const suffix = diagnosticDetails(details);
    return suffix ? `${message} ${suffix}` : message;
  }

  function reportWebviewDiagnostic(severity: DiagnosticSeverity, message: string, details?: unknown) {
    void invoke("push_package_webview_diagnostic", {
      packageId: data.packageId,
      webviewId: data.webviewId,
      severity,
      phase: "webview",
      message: diagnosticMessage(message, details)
    }).catch((error) => {
      console.warn(`[${data.packageId}/${data.webviewId}] diagnostic report failed`, error);
    });
  }

  onMount(() => {
    let disposed = false;
    let unlistenTelemetry: (() => void) | undefined;
    let moduleCleanup: (() => void) | undefined;
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
      return invoke("emit_package_webview_event", {
        packageId: data.packageId,
        webviewId: data.webviewId,
        eventName,
        payload: payload ?? null
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
    const configuration = {
      packageId: data.packageId,
      settings: {
        get: packageSettings,
        update: updatePackageSettings,
        save: savePackageSettings,
        reset: resetPackageSettings,
        subscribe: subscribePackageSettings
      },
      secrets: {
        configured: packageSecretConfigured,
        set: setPackageSecret,
        clear: deletePackageSecret
      }
    };
    const diagnostics = {
      log(message: string, details?: unknown) {
        console.log(`[${data.packageId}/${data.webviewId}] ${message}`, details ?? "");
        reportWebviewDiagnostic("info", message, details);
      },
      info(message: string, details?: unknown) {
        console.info(`[${data.packageId}/${data.webviewId}] ${message}`, details ?? "");
        reportWebviewDiagnostic("info", message, details);
      },
      warn(message: string, details?: unknown) {
        console.warn(`[${data.packageId}/${data.webviewId}] ${message}`, details ?? "");
        reportWebviewDiagnostic("warning", message, details);
      },
      error(message: string, details?: unknown) {
        console.error(`[${data.packageId}/${data.webviewId}] ${message}`, details ?? "");
        reportWebviewDiagnostic("error", message, details);
      },
      report(diagnostic: unknown) {
        console.warn(`[${data.packageId}/${data.webviewId}] diagnostic`, diagnostic);
        reportWebviewDiagnostic("warning", "webview diagnostic", diagnostic);
      },
      clear() {
        // Webview diagnostics are append-only in the host session.
      }
    };

    async function mount() {
      if (!root) return;
      const descriptor = await invoke<PackageWebviewRuntimeDescriptor>(
        "get_package_webview_runtime_descriptor",
        {
          packageId: data.packageId,
          webviewId: data.webviewId
        }
      );
      isSurface = descriptor.kind === "surface";
      const settings = await packageSettings();
      const isSettingsWebview = descriptor.kind === "settings";
      const dimensions = {
        width: Math.max(1, root.clientWidth || window.innerWidth),
        height: Math.max(1, root.clientHeight || window.innerHeight)
      };

      const module = await importPluginModule(
        descriptor.packageId,
        descriptor.webviewId,
        descriptor.entry,
        Date.now()
      );
      const exported = module.default ?? module;
      if (typeof exported?.mount === "function") {
        const moduleSettings = createModuleSettings(settings, isSettingsWebview);
        const item = {
          id: descriptor.webviewId,
          package_id: descriptor.packageId,
          export_name: descriptor.webviewId,
          name: descriptor.webviewId,
          x: 0,
          y: 0,
          width: dimensions.width,
          height: dimensions.height,
          z_index: 0,
          visible: true,
          locked: false,
          opacity: 1,
          settings
        };
        const cleanup = await exported.mount({
          root,
          packageId: descriptor.packageId,
          webviewId: descriptor.webviewId,
          exportName: descriptor.webviewId,
          package: {
            id: descriptor.packageId,
            name: descriptor.packageId,
            enabled: true
          },
          runtime: {
            packageId: descriptor.packageId,
            api: descriptor.runtimeApi
          },
          item,
          settings: moduleSettings,
          configuration: isSettingsWebview ? configuration : undefined,
          telemetryHub,
          bus: telemetryHub,
          registry: {
            get: readPackageRegistry
          },
          state: {
            get: readPackageRegistry,
            async set() {
              throw new Error("Module webview state writes are not supported by the host.");
            }
          },
          services: {
            call: callPackageService
          },
          assets: {
            url(ref: string) {
              return packageAssetUrl(descriptor.packageId, descriptor.webviewId, ref);
            }
          },
          diagnostics,
          telemetry: {
            event: publishTelemetryFrame
          },
          secrets: {
            async get() {
              return undefined;
            },
            configured: packageSecretConfigured
          },
          setActive() {
            // Standalone module webviews stay visible while their window is open.
          },
          dimensions,
          mode: "runtime"
        });
        if (typeof cleanup === "function") {
          if (disposed) cleanup();
          else moduleCleanup = cleanup;
        }
      }
    }

    void listen<GameEventFrame>("bakingrl-telemetry", (event) => {
      latestTelemetryFrame = event.payload;
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
      unlistenTelemetry?.();
    };
  });
</script>

<svelte:head>
  <title>{data.webviewId}</title>
</svelte:head>

<main class="plugin-webview-host" class:surface={isSurface} bind:this={root}>
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

  .plugin-webview-host.surface {
    min-height: 100vh;
    background: transparent;
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
