<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { importPluginModule } from "$lib/pluginModuleLoader";
  import { getInitialLocale, translations } from "$lib/i18n";
  import type { PackageConfigurationState } from "$lib/dashboard/types";
  import type { GameEventFrame } from "$lib/rlTelemetry";

  const { data } = $props();
  const t = translations[getInitialLocale()];

  let root = $state<HTMLElement | null>(null);
  let moduleRoot = $state<HTMLElement | null>(null);
  let message = $state("");
  let loading = $state(true);
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
    const moduleObserver = new MutationObserver(() => {
      if (!disposed && moduleRoot?.hasChildNodes()) loading = false;
    });
    if (moduleRoot) moduleObserver.observe(moduleRoot, { childList: true, subtree: true });
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
      if (!root || !moduleRoot) return;
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
          root: moduleRoot,
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
      } else {
        throw new Error("The plugin webview module does not export a mount function.");
      }
      if (!disposed) loading = false;
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`[${data.packageId}/${data.webviewId}] webview mount failed`, error);
      reportWebviewDiagnostic("error", "Plugin webview mount failed.", errorMessage);
      if (!disposed) {
        moduleRoot?.replaceChildren();
        message = errorMessage;
        loading = false;
      }
    });

    return () => {
      disposed = true;
      moduleObserver.disconnect();
      moduleCleanup?.();
      unlistenTelemetry?.();
    };
  });
</script>

<svelte:head>
  <title>{data.webviewId}</title>
</svelte:head>

<main class="plugin-webview-host" class:surface={isSurface} bind:this={root}>
  <div class="plugin-webview-mount" bind:this={moduleRoot}></div>
  {#if loading}
    <div class="plugin-webview-status" role="status" aria-live="polite">
      <span class="plugin-webview-spinner" aria-hidden="true"></span>
      <strong>{t["webview.loading"]}</strong>
    </div>
  {/if}
  {#if message}
    <div class="plugin-webview-error" role="alert">
      <span>{t["webview.errorEyebrow"]}</span>
      <strong>{t["webview.loadError"]}</strong>
      <p>{message}</p>
      <small>{t["webview.loadErrorHint"]}</small>
    </div>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
  }

  .plugin-webview-host {
    position: relative;
    min-height: calc(100vh - 48px);
    width: 100%;
    background: var(--bg-primary, #111315);
    color: var(--text-primary, #f2eee7);
    overflow: hidden;
  }

  .plugin-webview-host.surface {
    min-height: 100vh;
    background: transparent;
  }

  .plugin-webview-mount {
    min-height: inherit;
  }

  .plugin-webview-status,
  .plugin-webview-error {
    position: absolute;
    z-index: 10;
    inset: 0;
    display: grid;
    align-content: center;
    justify-items: center;
    gap: 12px;
    min-height: calc(100vh - 48px);
    padding: 32px;
    background: var(--bg-primary, #111315);
    text-align: center;
  }

  .plugin-webview-status {
    color: var(--text-muted, #aaa59c);
  }

  .plugin-webview-spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--border-color, #30353a);
    border-top-color: var(--accent, #e2a64b);
    border-radius: 50%;
    animation: plugin-webview-spin 0.8s linear infinite;
  }

  .plugin-webview-error {
    color: var(--text-primary, #f2eee7);
  }

  .plugin-webview-error > span {
    color: var(--danger, #e18478);
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .plugin-webview-error > strong {
    font-size: 22px;
  }

  .plugin-webview-error > p {
    max-width: 720px;
    margin: 0;
    color: var(--danger, #e18478);
    font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .plugin-webview-error > small {
    max-width: 620px;
    color: var(--text-muted, #aaa59c);
  }

  :global(html[data-bakingrl-window="surface"]) .plugin-webview-status,
  :global(html[data-bakingrl-window="surface"]) .plugin-webview-error,
  .plugin-webview-host.surface .plugin-webview-status,
  .plugin-webview-host.surface .plugin-webview-error {
    display: none;
  }

  @keyframes plugin-webview-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
