import { inspect } from "node:util";
import { createInterface } from "node:readline";

type JsonRpcId = number | string | null;

type RpcHandler = (params: unknown) => unknown | Promise<unknown>;

interface HostSpec {
  packageId: string;
  packageRoot: string;
  entryUrl: string;
  runtimeApi: string | null;
  storageRoot: string;
  settings: unknown;
  serviceImports: string[];
  serviceMethods: Record<string, string[]>;
  sidecars: string[];
  webviews: Record<string, {
    title?: string;
    entry?: string;
    path?: string;
    route?: string;
  }>;
}

interface Disposable {
  dispose(): unknown | Promise<unknown>;
}

const originalConsole = globalThis.console;

function formatLogArg(value: unknown): string {
  if (typeof value === "string") return value;
  return inspect(value, { depth: 8, colors: false, breakLength: 120 });
}

function writeLog(level: string, args: unknown[]) {
  process.stderr.write(`[${level}] ${args.map(formatLogArg).join(" ")}\n`);
}

globalThis.console = {
  ...originalConsole,
  debug: (...args: unknown[]) => writeLog("debug", args),
  error: (...args: unknown[]) => writeLog("error", args),
  info: (...args: unknown[]) => writeLog("info", args),
  log: (...args: unknown[]) => writeLog("info", args),
  warn: (...args: unknown[]) => writeLog("warn", args)
};

class RpcPeer {
  #nextId = 1;
  #pending = new Map<JsonRpcId, { resolve(value: unknown): void; reject(error: Error): void }>();
  #handlers = new Map<string, RpcHandler>();

  constructor() {
    const input = createInterface({ input: process.stdin, terminal: false });
    input.on("line", (line) => {
      void this.#handleLine(line);
    });
    input.on("close", () => {
      void shutdown(0);
    });
  }

  on(method: string, handler: RpcHandler) {
    this.#handlers.set(method, handler);
  }

  request(method: string, params?: unknown): Promise<unknown> {
    const id = this.#nextId++;
    this.#send({ jsonrpc: "2.0", id, method, params });
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
    });
  }

  notify(method: string, params?: unknown) {
    this.#send({ jsonrpc: "2.0", method, params });
  }

  async #handleLine(line: string) {
    let message: any;
    try {
      message = JSON.parse(line);
    } catch {
      console.warn("Host sent non-JSON input to extension host.");
      return;
    }

    if (message?.jsonrpc !== "2.0") return;

    if ("id" in message && ("result" in message || "error" in message)) {
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      this.#pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(String(message.error.message ?? "JSON-RPC request failed")));
      } else {
        pending.resolve(message.result);
      }
      return;
    }

    if (typeof message.method !== "string") return;
    const handler = this.#handlers.get(message.method);
    if (!handler) {
      if ("id" in message) {
        this.#send({
          jsonrpc: "2.0",
          id: message.id,
          error: { code: -32601, message: `Unsupported host method '${message.method}'.` }
        });
      }
      return;
    }

    try {
      const result = await handler(message.params);
      if ("id" in message) {
        this.#send({ jsonrpc: "2.0", id: message.id, result });
      }
    } catch (error) {
      if ("id" in message) {
        this.#send({
          jsonrpc: "2.0",
          id: message.id,
          error: { code: -32000, message: errorMessage(error) }
        });
      } else {
        console.error(error);
      }
    }
  }

  #send(message: unknown) {
    process.stdout.write(`${JSON.stringify(message)}\n`);
  }
}

const rawSpec = process.env.BAKINGRL_EXTENSION_HOST_SPEC;
if (!rawSpec) {
  throw new Error("BAKINGRL_EXTENSION_HOST_SPEC is required.");
}

const spec = JSON.parse(rawSpec) as HostSpec;
const rpc = new RpcPeer();
const subscriptions: Disposable[] = [];
const localCommands = new Map<string, (...args: unknown[]) => unknown | Promise<unknown>>();
const localServices = new Map<string, Record<string, (input: unknown) => unknown | Promise<unknown>>>();
const busListeners = new Map<string, Set<(event: unknown) => unknown | Promise<unknown>>>();

let pluginModule: any = null;
let activationResult: unknown;
let shuttingDown = false;
let latestTelemetryEvent: unknown = null;

rpc.on("bakingrl/shutdown", async () => {
  await shutdown(0);
  return { ok: true };
});

rpc.on("services/callRegistered", async (params: any) => {
  const serviceRef = String(params?.serviceRef ?? "");
  const method = String(params?.method ?? "");
  return await invokeLocalService(serviceRef, method, params?.input ?? null);
});

rpc.on("commands/executeRegistered", async (params: any) => {
  const command = String(params?.command ?? "");
  const args = Array.isArray(params?.args) ? params.args : [];
  return await invokeLocalCommand(command, args);
});

rpc.on("bus/event", async (params: any) => {
  const event = params?.event;
  const eventName = String(event?.Event ?? event?.event ?? params?.eventName ?? "");
  if (!eventName) return { ok: true };
  latestTelemetryEvent = event;
  const callbacks = [...busListeners.entries()]
    .filter(([pattern]) => matchesEventPattern(pattern, eventName))
    .flatMap(([, listeners]) => [...listeners]);
  for (const callback of callbacks) {
    try {
      await callback(event);
    } catch (error) {
      console.error("Plugin bus listener failed.", error);
    }
  }
  return { ok: true };
});

function localServiceRef(name: string) {
  return name.includes("/") ? name : `${spec.packageId}/${name}`;
}

function localCommandName(command: string) {
  const prefix = `${spec.packageId}/`;
  return command.startsWith(prefix) ? command.slice(prefix.length) : command;
}

function matchesEventPattern(pattern: string, value: string) {
  return pattern === "*" || pattern === value || (pattern.endsWith(".*") && value.startsWith(pattern.slice(0, -1)));
}

async function invokeLocalService(serviceRef: string, method: string, input: unknown) {
  const service = localServices.get(localServiceRef(serviceRef)) ?? localServices.get(serviceRef);
  const fn = service?.[method];
  if (typeof fn !== "function") {
    throw new Error(`Service '${serviceRef}' does not expose method '${method}'.`);
  }
  return await fn(input ?? null);
}

async function invokeLocalCommand(command: string, args: unknown[]) {
  const fn = localCommands.get(command) ?? localCommands.get(localCommandName(command));
  if (typeof fn !== "function") {
    throw new Error(`Command '${command}' is not registered.`);
  }
  return await fn(...args);
}

function disposable(dispose: () => unknown | Promise<unknown>): Disposable {
  return { dispose };
}

function settingsReader(settings: unknown) {
  const values = settings && typeof settings === "object" && !Array.isArray(settings)
    ? { ...(settings as Record<string, unknown>) }
    : {};

  return Object.assign({}, values, {
    get(key: string) {
      return values[key];
    },
    all() {
      return { ...values };
    }
  });
}

function resourceBytes(payload: any): Buffer {
  const encoded = typeof payload?.contentsBase64 === "string" ? payload.contentsBase64 : "";
  return Buffer.from(encoded, "base64");
}

function createContext() {
  const settings = settingsReader(spec.settings);
  function subscribeEvents(
    eventName: string,
    callback: (event: unknown) => unknown | Promise<unknown>,
    subscribeMethod: "bus/subscribe" | "telemetryHub/subscribe",
    unsubscribeMethod: "bus/unsubscribe" | "telemetryHub/unsubscribe"
  ) {
    if (!eventName || typeof callback !== "function") {
      throw new Error("bus.subscribe requires an event name and callback.");
    }
    let listeners = busListeners.get(eventName);
    if (!listeners) {
      listeners = new Set();
      busListeners.set(eventName, listeners);
      void rpc.request(subscribeMethod, { eventName }).catch((error) => {
        console.error(`Unable to subscribe to event '${eventName}'.`, error);
      });
    }
    listeners.add(callback);
    return () => {
      const current = busListeners.get(eventName);
      current?.delete(callback);
      if (current && current.size === 0) {
        busListeners.delete(eventName);
        void rpc.request(unsubscribeMethod, { eventName }).catch((error) => {
          console.error(`Unable to unsubscribe from event '${eventName}'.`, error);
        });
      }
    };
  }

  const diagnostics = {
    log: (message: string, details?: unknown) =>
      rpc.request("diagnostics/log", { severity: "info", phase: "runtime", message, details }),
    info: (message: string, details?: unknown) =>
      rpc.request("diagnostics/log", { severity: "info", phase: "runtime", message, details }),
    warn: (message: string, details?: unknown) =>
      rpc.request("diagnostics/log", { severity: "warning", phase: "runtime", message, details }),
    error: (message: string, details?: unknown) =>
      rpc.request("diagnostics/log", { severity: "error", phase: "runtime", message, details })
  };
  const logger = {
    trace: diagnostics.log,
    debug: diagnostics.log,
    log: diagnostics.log,
    info: diagnostics.info,
    warn: diagnostics.warn,
    error: diagnostics.error
  };

  return {
    id: spec.packageId,
    packageId: spec.packageId,
    extensionPath: spec.packageRoot,
    storagePath: spec.storageRoot,
    settings,
    subscriptions,
    logger,
    commands: {
      registerCommand(command: string, handler: (...args: unknown[]) => unknown | Promise<unknown>) {
        if (!command || typeof handler !== "function") {
          throw new Error("commands.registerCommand requires a command and handler.");
        }
        localCommands.set(command, handler);
        rpc.notify("commands/registerCommand", { command });
        const item = disposable(() => {
          localCommands.delete(command);
          rpc.notify("commands/unregisterCommand", { command });
        });
        subscriptions.push(item);
        return item;
      },
      async executeCommand(command: string, ...args: unknown[]) {
        const local = localCommands.get(command);
        if (local) return await local(...args);
        return await rpc.request("commands/executeCommand", { command, args });
      }
    },
    services: {
      register(name: string, service: Record<string, (input: unknown) => unknown | Promise<unknown>>) {
        return this.registerService(name, service);
      },
      registerService(name: string, service: Record<string, (input: unknown) => unknown | Promise<unknown>>) {
        const serviceRef = localServiceRef(name);
        localServices.set(serviceRef, service);
        rpc.notify("services/registerService", { serviceRef, methods: Object.keys(service ?? {}) });
        const item = disposable(() => {
          localServices.delete(serviceRef);
          rpc.notify("services/unregisterService", { serviceRef });
        });
        subscriptions.push(item);
        return item;
      },
      async call(serviceRef: string, method: string, input?: unknown) {
        const local = localServices.get(localServiceRef(serviceRef)) ?? localServices.get(serviceRef);
        if (local?.[method]) return await invokeLocalService(serviceRef, method, input ?? null);
        return await rpc.request("services/call", { serviceRef, method, input: input ?? null });
      }
    },
    plugins: {
      list: () => rpc.request("plugins/list", {})
    },
    extensions: {
      points: (filter?: { packageId?: string }) =>
        rpc.request("extensions/listPoints", { packageId: filter?.packageId ?? null }),
      contributions: (target?: string | { target?: string }) =>
        rpc.request("extensions/listContributions", {
          target: typeof target === "string" ? target : target?.target ?? null
        })
    },
    resources: {
      list: (filter?: { packageId?: string; type?: string; visibility?: string }) =>
        rpc.request("resources/list", {
          packageId: filter?.packageId ?? null,
          type: filter?.type ?? null,
          visibility: filter?.visibility ?? null
        }),
      async read(ref: string, path?: string) {
        const payload = await rpc.request("resources/read", { ref, path: path ?? null });
        return resourceBytes(payload);
      },
      async readText(ref: string, path?: string) {
        const payload = await rpc.request("resources/read", { ref, path: path ?? null });
        return resourceBytes(payload).toString("utf8");
      },
      async readJson<T = unknown>(ref: string, path?: string): Promise<T> {
        const payload = await rpc.request("resources/read", { ref, path: path ?? null });
        return JSON.parse(resourceBytes(payload).toString("utf8")) as T;
      }
    },
    bus: {
      subscribe(eventName: string, callback: (event: unknown) => unknown | Promise<unknown>) {
        return subscribeEvents(
          eventName,
          callback,
          "bus/subscribe",
          "bus/unsubscribe"
        );
      },
      emit(eventName: string, payload?: unknown) {
        void rpc.request("bus/emit", { eventName, payload: payload ?? null }).catch((error) => {
          console.error(`Unable to emit bus event '${eventName}'.`, error);
        });
      }
    },
    telemetryHub: {
      subscribe(eventName: string, callback: (event: unknown) => unknown | Promise<unknown>) {
        return subscribeEvents(
          eventName,
          callback,
          "telemetryHub/subscribe",
          "telemetryHub/unsubscribe"
        );
      },
      publish(eventName: string, payload?: unknown) {
        latestTelemetryEvent = { Event: String(eventName ?? ""), Data: payload ?? null };
        return rpc.request("telemetryHub/publish", { eventName, payload: payload ?? null });
      },
      snapshot() {
        return rpc.request("telemetryHub/snapshot", {}).then((snapshot) => snapshot ?? latestTelemetryEvent);
      },
      getSnapshot() {
        return rpc.request("telemetryHub/getSnapshot", {}).then((snapshot) => snapshot ?? latestTelemetryEvent);
      }
    },
    registry: {
      get: (key: string) => rpc.request("registry/get", { key }),
      set: (key: string, value: unknown) => rpc.request("registry/set", { key, value }),
      entries: () => rpc.request("registry/entries", {})
    },
    storage: {
      readText: (uri: string) => rpc.request("storage/readText", { uri }),
      writeText: (uri: string, contents: unknown) =>
        rpc.request("storage/writeText", { uri, contents: String(contents ?? "") })
    },
    secrets: {
      get: async (key: string) => (await rpc.request("secrets/get", { key })) ?? undefined,
      configured: (key: string) => rpc.request("secrets/configured", { key })
    },
    diagnostics,
    sidecars: {
      declared: [...spec.sidecars],
      start: (name: string) => rpc.request("sidecars/start", { name }),
      stop: (name: string) => rpc.request("sidecars/stop", { name }),
      restart: (name: string) => rpc.request("sidecars/restart", { name }),
      call: (name: string, method: string, params?: unknown) =>
        rpc.request("sidecars/call", { name, method, params: params ?? null })
    },
    telemetry: {
      event: (name: string, properties?: unknown) => rpc.request("telemetry/event", { name, properties })
    },
    stateHub: {
      read(key: string) {
        return rpc.request("stateHub/read", { key });
      },
      write(key: string, value: unknown) {
        return rpc.request("stateHub/write", { key, value });
      },
      snapshot() {
        return rpc.request("stateHub/snapshot", {});
      },
      getSnapshot() {
        return rpc.request("stateHub/getSnapshot", {});
      }
    },
    runtime: {
      packageId: spec.packageId,
      api: spec.runtimeApi
    },
    webviews: {
      declared: spec.webviews,
      open: (id: string, options?: unknown) => rpc.request("webviews/open", { id, options }),
      close: (id: string) => rpc.request("webviews/close", { id })
    }
  };
}

async function activate() {
  pluginModule = await import(spec.entryUrl);
  const activateFn = pluginModule?.activate ?? pluginModule?.default?.activate;
  if (typeof activateFn !== "function") {
    console.warn(`Plugin '${spec.packageId}' has no activate(context) export.`);
    return;
  }
  activationResult = await activateFn(createContext());
  if (isDisposable(activationResult)) {
    subscriptions.push(activationResult);
  }
}

async function deactivate() {
  const deactivateFn = pluginModule?.deactivate ?? pluginModule?.default?.deactivate;
  if (typeof deactivateFn === "function") {
    await deactivateFn();
  }
  for (const item of [...subscriptions].reverse()) {
    try {
      await item.dispose();
    } catch (error) {
      console.warn("Disposable failed during extension host shutdown.", error);
    }
  }
  subscriptions.length = 0;
}

async function shutdown(code: number) {
  if (shuttingDown) return;
  shuttingDown = true;
  try {
    await deactivate();
  } catch (error) {
    console.error("Plugin deactivate() failed.", error);
    code = code || 1;
  } finally {
    process.exitCode = code;
    setTimeout(() => process.exit(code), 0);
  }
}

function isDisposable(value: unknown): value is Disposable {
  return !!value && typeof (value as Disposable).dispose === "function";
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

process.on("SIGINT", () => {
  void shutdown(0);
});

process.on("SIGTERM", () => {
  void shutdown(0);
});

activate().catch(async (error) => {
  console.error("Plugin activate(context) failed.", error);
  try {
    await rpc.request("diagnostics/log", {
      severity: "fatal",
      phase: "activation",
      message: errorMessage(error)
    });
  } catch {
    // The host may already be gone; exiting non-zero lets the Rust supervisor diagnose the crash.
  }
  await shutdown(1);
});
