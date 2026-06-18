type WebviewMessage = {
  source?: string;
  type?: string;
  id?: string | number;
  payload?: Record<string, unknown>;
};

type WebviewHostMessage = {
  source: "bakingrl-host";
  type: string;
  id?: string | number;
  payload?: Record<string, unknown>;
};

export type PluginWebviewItem = {
  id: string;
  name: string;
  width: number;
  height: number;
  settings: Record<string, unknown>;
};

export type PluginWebviewMountOptions = {
  root: HTMLElement;
  src: string;
  packageId: string;
  exportName: string;
  item: PluginWebviewItem;
  settings: Record<string, unknown>;
  mode: "runtime";
  runtimeApi?: string | null;
  assetUrl(ref: string): string;
  subscribeTelemetry(callback: (event: unknown) => void): () => void;
  getTelemetrySnapshot?(): unknown | Promise<unknown>;
  publishTelemetry?(eventName: string, payload?: unknown): unknown | Promise<unknown>;
  setActive?(active: boolean): void;
  configuration?: {
    packageId: string;
    settings: {
      get(): Promise<Record<string, unknown>>;
      update(values: Record<string, unknown>): Promise<Record<string, unknown>>;
      save(values: Record<string, unknown>): Promise<Record<string, unknown>>;
      reset(): Promise<Record<string, unknown>>;
      subscribe(callback: (settings: Record<string, unknown>) => void | Promise<void>): () => void;
    };
    secrets: {
      configured(key: string): Promise<boolean>;
      set(key: string, value: string): Promise<unknown>;
      clear(key: string): Promise<unknown>;
    };
  };
};

export type PluginWebviewHandle = {
  update(item: PluginWebviewItem, settings: Record<string, unknown>): void;
  cleanup(): void;
};

const HOST_SOURCE = "bakingrl-host";
const WEBVIEW_SOURCE = "bakingrl-webview";

export function mountPluginWebview(options: PluginWebviewMountOptions): PluginWebviewHandle {
  const iframe = document.createElement("iframe");
  iframe.className = "plugin-webview-frame";
  iframe.src = options.src;
  iframe.title = `${options.packageId}/${options.exportName}`;
  iframe.setAttribute("loading", "eager");
  iframe.setAttribute("referrerpolicy", "no-referrer");
  iframe.setAttribute("sandbox", "allow-scripts allow-same-origin allow-forms");
  iframe.style.width = "100%";
  iframe.style.height = "100%";
  iframe.style.display = "block";
  iframe.style.border = "0";
  iframe.style.background = "transparent";
  options.root.replaceChildren(iframe);

  let currentItem = options.item;
  let currentSettings = options.settings;
  let disposed = false;
  let latestTelemetryEvent: unknown = null;
  const targetOrigin = frameTargetOrigin(options.src);

  const post = (type: string, payload?: Record<string, unknown>, id?: string | number) => {
    if (disposed) return;
    const message: WebviewHostMessage = { source: HOST_SOURCE, type, payload };
    if (id !== undefined) message.id = id;
    iframe.contentWindow?.postMessage(message, targetOrigin);
  };

  const hostPayload = () => ({
    packageId: options.packageId,
    exportName: options.exportName,
    mode: options.mode,
    runtime: {
      packageId: options.packageId,
      api: options.runtimeApi ?? null
    },
    apis: {
      telemetryHub: { subscribe: true, publish: true, snapshot: true, getSnapshot: true },
      stateHub: { read: true, write: true, snapshot: true, getSnapshot: true },
      configuration: options.configuration
        ? {
            settings: { get: true, update: true, save: true, reset: true, subscribe: true },
            secrets: { configured: true, set: true, clear: true }
          }
        : false
    },
    configuration: options.configuration
      ? {
          packageId: options.configuration.packageId
        }
      : undefined,
    item: currentItem,
    settings: currentSettings,
    dimensions: {
      width: currentItem.width,
      height: currentItem.height
    },
    assets: {
      url: "__postMessage:bakingrl:asset-url"
    }
  });

  const sendInit = () => post("bakingrl:webview:init", hostPayload());
  const sendUpdate = () => post("bakingrl:webview:update", hostPayload());

  const telemetryCleanup = options.subscribeTelemetry((event) => {
    latestTelemetryEvent = event;
    post("bakingrl:webview:telemetry", { event });
  });
  const configurationCleanup = options.configuration?.settings.subscribe((settings) => {
    post("bakingrl:configuration-settings-changed", { settings });
  });

  const onMessage = (event: MessageEvent<WebviewMessage>) => {
    if (disposed || event.source !== iframe.contentWindow) return;
    const message = event.data;
    if (!message || message.source !== WEBVIEW_SOURCE) return;

    if (message.type === "bakingrl:webview:ready") {
      sendInit();
      return;
    }

    if (message.type === "bakingrl:asset-url") {
      const ref = typeof message.payload?.ref === "string" ? message.payload.ref : "";
      post("bakingrl:asset-url:result", { ref, url: options.assetUrl(ref) }, message.id);
      return;
    }

    if (message.type === "bakingrl:telemetry-snapshot") {
      const fallback = latestTelemetryEvent;
      void Promise.resolve(options.getTelemetrySnapshot?.())
        .then((snapshot) => post("bakingrl:telemetry-snapshot:result", { snapshot: snapshot ?? fallback }, message.id))
        .catch((error) =>
          post("bakingrl:telemetry-snapshot:result", { snapshot: fallback, error: String(error) }, message.id)
        );
      return;
    }

    if (message.type === "bakingrl:telemetry-publish" && options.publishTelemetry) {
      const eventName = typeof message.payload?.eventName === "string" ? message.payload.eventName : "";
      void Promise.resolve(options.publishTelemetry(eventName, message.payload?.payload))
        .then(() => post("bakingrl:telemetry-publish:result", { ok: true }, message.id))
        .catch((error) => post("bakingrl:telemetry-publish:result", { ok: false, error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:state-snapshot") {
      post("bakingrl:state-snapshot:result", { snapshot: hostPayload() }, message.id);
      return;
    }

    if (message.type === "bakingrl:configuration-settings-get" && options.configuration) {
      void options.configuration.settings
        .get()
        .then((settings) => post("bakingrl:configuration-settings-get:result", { settings }, message.id))
        .catch((error) => post("bakingrl:configuration-settings-get:result", { error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:configuration-settings-update" && options.configuration) {
      const values = recordPayloadValue(message.payload?.values);
      void options.configuration.settings
        .update(values)
        .then((settings) => post("bakingrl:configuration-settings-update:result", { settings }, message.id))
        .catch((error) => post("bakingrl:configuration-settings-update:result", { error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:configuration-settings-save" && options.configuration) {
      const values = recordPayloadValue(message.payload?.values);
      void options.configuration.settings
        .save(values)
        .then((settings) => post("bakingrl:configuration-settings-save:result", { settings }, message.id))
        .catch((error) => post("bakingrl:configuration-settings-save:result", { error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:configuration-settings-reset" && options.configuration) {
      void options.configuration.settings
        .reset()
        .then((settings) => post("bakingrl:configuration-settings-reset:result", { settings }, message.id))
        .catch((error) => post("bakingrl:configuration-settings-reset:result", { error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:configuration-secret-configured" && options.configuration) {
      const key = typeof message.payload?.key === "string" ? message.payload.key : "";
      void options.configuration.secrets
        .configured(key)
        .then((configured) => post("bakingrl:configuration-secret-configured:result", { configured }, message.id))
        .catch((error) => post("bakingrl:configuration-secret-configured:result", { error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:configuration-secret-set" && options.configuration) {
      const key = typeof message.payload?.key === "string" ? message.payload.key : "";
      const value = typeof message.payload?.value === "string" ? message.payload.value : "";
      void options.configuration.secrets
        .set(key, value)
        .then((state) => post("bakingrl:configuration-secret-set:result", { state: recordPayloadValue(state) }, message.id))
        .catch((error) => post("bakingrl:configuration-secret-set:result", { error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:configuration-secret-clear" && options.configuration) {
      const key = typeof message.payload?.key === "string" ? message.payload.key : "";
      void options.configuration.secrets
        .clear(key)
        .then((state) => post("bakingrl:configuration-secret-clear:result", { state: recordPayloadValue(state) }, message.id))
        .catch((error) => post("bakingrl:configuration-secret-clear:result", { error: String(error) }, message.id));
      return;
    }

    if (message.type === "bakingrl:set-active" && options.setActive) {
      options.setActive(Boolean(message.payload?.active));
    }
  };

  iframe.addEventListener("load", sendInit);
  window.addEventListener("message", onMessage);

  return {
    update(item, settings) {
      currentItem = item;
      currentSettings = settings;
      sendUpdate();
    },
    cleanup() {
      disposed = true;
      telemetryCleanup();
      configurationCleanup?.();
      iframe.removeEventListener("load", sendInit);
      window.removeEventListener("message", onMessage);
      iframe.remove();
    }
  };
}

function recordPayloadValue(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function frameTargetOrigin(src: string) {
  try {
    return new URL(src, window.location.href).origin;
  } catch {
    return "*";
  }
}
