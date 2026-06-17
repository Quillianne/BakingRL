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
  mode: "runtime" | "editor";
  runtimeApi?: string | null;
  assetUrl(ref: string): string;
  subscribeTelemetry(callback: (event: unknown) => void): () => void;
  emitEditorEvent?(eventName: string, payload?: unknown): void;
  setActive?(active: boolean): void;
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
      telemetryHub: { subscribe: true, publish: true },
      stateHub: { read: true, write: true }
    },
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
    post("bakingrl:webview:telemetry", { event });
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

    if (message.type === "bakingrl:editor-event" && options.emitEditorEvent) {
      const eventName = typeof message.payload?.eventName === "string" ? message.payload.eventName : "";
      options.emitEditorEvent(eventName, message.payload?.payload);
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
      iframe.removeEventListener("load", sendInit);
      window.removeEventListener("message", onMessage);
      iframe.remove();
    }
  };
}

function frameTargetOrigin(src: string) {
  try {
    return new URL(src, window.location.href).origin;
  } catch {
    return "*";
  }
}
