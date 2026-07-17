import { constants } from "node:fs";
import { access, mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

const DEFAULT_TIMEOUT_MS = 10_000;
const MAX_STDERR_CHARS = 32 * 1024;
const packageId = "dev.bakingrl.extension-host-smoke";
const defaultBootstrap = fileURLToPath(
  new URL("../src-tauri/gen/extension-host/bootstrap.mjs", import.meta.url)
);
const options = parseArgs(process.argv.slice(2));

let temporaryRoot;
let child;
let childClose;
let output;
let failure;

try {
  const nodeBinary = path.resolve(options.node ?? process.execPath);
  const bootstrap = path.resolve(options.bootstrap ?? defaultBootstrap);
  await requireFile(nodeBinary, "Node runtime", true);
  await requireFile(bootstrap, "extension-host bootstrap");

  temporaryRoot = await mkdtemp(path.join(tmpdir(), "bakingrl-extension-host-smoke-"));
  const pluginRoot = path.join(temporaryRoot, "plugin");
  const markerPath = path.join(temporaryRoot, "lifecycle.log");
  const entryPath = path.join(pluginRoot, "index.mjs");
  await mkdir(pluginRoot);
  await writeFile(path.join(pluginRoot, "package.json"), JSON.stringify({ type: "module" }));
  await writeFile(entryPath, pluginSource(), "utf8");

  const spec = {
    packageId,
    packageRoot: pluginRoot,
    entryUrl: pathToFileURL(entryPath).href,
    runtimeApi: "2.3",
    settings: {},
    serviceImports: [],
    serviceMethods: {},
    sidecars: [],
    webviews: {}
  };

  output = createOutputCapture();
  child = spawn(nodeBinary, [bootstrap], {
    env: {
      ...process.env,
      BAKINGRL_EXTENSION_HOST_SPEC: JSON.stringify(spec),
      BAKINGRL_EXTENSION_HOST_SMOKE_MARKER: markerPath
    },
    stdio: ["pipe", "pipe", "pipe"],
    windowsHide: true
  });

  const rpc = superviseRpc(child, output);
  childClose = superviseClose(child);
  const [registration] = await Promise.all([
    waitForRuntime(rpc.serviceRegistered, childClose, "service registration"),
    waitForRuntime(rpc.runtimeReady, childClose, "runtime/ready")
  ]);

  assertServiceRegistration(registration);

  const pingInput = { source: "packaged-runtime-smoke" };
  const ping = await waitForRuntime(
    rpc.request("services/callRegistered", {
      serviceRef: `${packageId}/health`,
      method: "ping",
      input: pingInput
    }),
    childClose,
    "registered service call"
  );
  if (ping?.ok !== true || ping?.packageId !== packageId || ping?.input?.source !== pingInput.source) {
    throw new Error(`Registered service returned an unexpected result: ${JSON.stringify(ping)}`);
  }

  rpc.notify("bakingrl/shutdown", {});
  const result = await withTimeout(childClose, DEFAULT_TIMEOUT_MS, "clean extension-host shutdown");
  if (result.signal || result.code !== 0) {
    throw new Error(
      `Extension host exited with code ${String(result.code)} and signal ${String(result.signal)}.`
    );
  }

  const lifecycle = (await readFile(markerPath, "utf8"))
    .split(/\r?\n/u)
    .filter(Boolean);
  if (lifecycle.join(",") !== "activate,deactivate") {
    throw new Error(`Unexpected plugin lifecycle: ${JSON.stringify(lifecycle)}`);
  }
  if (output.stdout.trim()) {
    throw new Error("Extension host wrote non-JSON data to its JSON-RPC stdout stream.");
  }

  console.log(`Extension-host smoke passed with ${nodeBinary}.`);
} catch (error) {
  failure = error;
  process.exitCode = 1;
} finally {
  if (child) {
    if (child.exitCode === null && child.signalCode === null) child.kill();
    if (childClose) {
      const closed = await withTimeout(childClose, 2_000, "extension-host termination")
        .then(() => true)
        .catch(() => false);
      if (!closed && child.exitCode === null && child.signalCode === null) {
        child.kill("SIGKILL");
      }
      await withTimeout(childClose, 2_000, "forced extension-host termination").catch(() => {});
    }
  }
  output?.close();
  if (failure) {
    const details = failure instanceof Error ? failure.stack ?? failure.message : String(failure);
    const stderr = output?.stderr.trim();
    const stdout = output?.stdout.trim();
    const diagnostics = [
      details,
      stderr ? `Extension-host stderr:\n${stderr}` : "",
      stdout ? `Non-JSON extension-host stdout:\n${stdout}` : ""
    ].filter(Boolean);
    console.error(diagnostics.join("\n"));
  }
  if (temporaryRoot) {
    await rm(temporaryRoot, {
      recursive: true,
      force: true,
      maxRetries: 3,
      retryDelay: 100
    }).catch((error) => {
      console.error(`Unable to remove smoke directory '${temporaryRoot}':`, error);
      process.exitCode = 1;
    });
  }
}

function superviseRpc(runtime, capturedOutput) {
  let nextRequestId = 1_000_000;
  const pending = new Map();
  const serviceRegistered = deferred();
  const runtimeReady = deferred();
  let serviceRegistrationSeen = false;
  const lines = createInterface({ input: runtime.stdout, terminal: false });
  capturedOutput.lines = lines;

  lines.on("line", (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      capturedOutput.stdout = appendBounded(capturedOutput.stdout, `${line}\n`);
      return;
    }

    if (message?.jsonrpc !== "2.0") {
      capturedOutput.stdout = appendBounded(capturedOutput.stdout, `${line}\n`);
      return;
    }

    if (typeof message.method === "string") {
      if (message.method === "services/registerService") {
        serviceRegistrationSeen = true;
        serviceRegistered.resolve(message.params);
      }

      if (message.method === "runtime/ready") {
        if (!("id" in message)) {
          runtimeReady.reject(new Error("runtime/ready must be a JSON-RPC request."));
          return;
        }
        send({ jsonrpc: "2.0", id: message.id, result: { ok: true } });
        if (!serviceRegistrationSeen) {
          runtimeReady.reject(new Error("runtime/ready was sent before the plugin registered its service."));
          return;
        }
        runtimeReady.resolve(message.params);
        return;
      }

      if ("id" in message) {
        if (message.method === "diagnostics/log") {
          send({ jsonrpc: "2.0", id: message.id, result: { ok: true } });
        } else {
          send({
            jsonrpc: "2.0",
            id: message.id,
            error: { code: -32601, message: `Smoke host does not implement '${message.method}'.` }
          });
        }
      }
      return;
    }

    if (!("id" in message)) return;
    const request = pending.get(message.id);
    if (!request) return;
    pending.delete(message.id);
    if (message.error) {
      request.reject(new Error(String(message.error.message ?? "JSON-RPC request failed")));
    } else {
      request.resolve(message.result);
    }
  });

  runtime.stderr.setEncoding("utf8");
  runtime.stderr.on("data", (chunk) => {
    capturedOutput.stderr = appendBounded(capturedOutput.stderr, String(chunk));
  });
  runtime.stdin.on("error", (error) => {
    capturedOutput.stderr = appendBounded(
      capturedOutput.stderr,
      `[smoke host] Extension-host stdin failed: ${String(error)}\n`
    );
  });

  function send(message) {
    if (!runtime.stdin.write(`${JSON.stringify(message)}\n`)) {
      runtime.stdin.once("drain", () => {});
    }
  }

  return {
    serviceRegistered: serviceRegistered.promise,
    runtimeReady: runtimeReady.promise,
    request(method, params) {
      const id = nextRequestId++;
      const response = deferred();
      pending.set(id, response);
      send({ jsonrpc: "2.0", id, method, params });
      return response.promise;
    },
    notify(method, params) {
      send({ jsonrpc: "2.0", method, params });
    }
  };
}

function superviseClose(runtime) {
  return new Promise((resolve, reject) => {
    runtime.once("error", reject);
    runtime.once("close", (code, signal) => resolve({ code, signal }));
  });
}

function createOutputCapture() {
  return {
    stderr: "",
    stdout: "",
    lines: undefined,
    close() {
      this.lines?.close();
    }
  };
}

function appendBounded(current, chunk) {
  const combined = `${current}${chunk}`;
  return combined.length <= MAX_STDERR_CHARS
    ? combined
    : combined.slice(combined.length - MAX_STDERR_CHARS);
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function withTimeout(promise, timeoutMs, label) {
  let timeout;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timeout = setTimeout(() => reject(new Error(`Timed out waiting for ${label}.`)), timeoutMs);
      })
    ]);
  } finally {
    clearTimeout(timeout);
  }
}

function waitForRuntime(promise, close, label) {
  const prematureExit = close.then(({ code, signal }) => {
    throw new Error(
      `Extension host exited before ${label} (code ${String(code)}, signal ${String(signal)}).`
    );
  });
  return withTimeout(Promise.race([promise, prematureExit]), DEFAULT_TIMEOUT_MS, label);
}

async function requireFile(filePath, label, executable = false) {
  const file = await stat(filePath).catch(() => null);
  if (!file?.isFile()) {
    throw new Error(`${label} is missing at '${filePath}'.`);
  }
  if (executable && process.platform !== "win32") {
    await access(filePath, constants.X_OK);
  }
}

function assertServiceRegistration(registration) {
  const serviceRef = String(registration?.serviceRef ?? "");
  const methods = Array.isArray(registration?.methods) ? registration.methods : [];
  if (serviceRef !== `${packageId}/health` || !methods.includes("ping")) {
    throw new Error(`Unexpected service registration: ${JSON.stringify(registration)}`);
  }
}

function parseArgs(args) {
  const parsed = {};
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument !== "--node" && argument !== "--bootstrap") {
      throw new Error(`Unknown argument '${argument}'.`);
    }
    const value = args[index + 1];
    if (!value || value.startsWith("--")) {
      throw new Error(`Missing value for '${argument}'.`);
    }
    parsed[argument.slice(2)] = value;
    index += 1;
  }
  return parsed;
}

function pluginSource() {
  return `
import { appendFile } from "node:fs/promises";

const markerPath = process.env.BAKINGRL_EXTENSION_HOST_SMOKE_MARKER;

async function mark(phase) {
  if (!markerPath) throw new Error("Missing lifecycle marker path.");
  await appendFile(markerPath, \`${"${phase}"}\\n\`, "utf8");
}

export async function activate(context) {
  await mark("activate");
  context.services.registerService("health", {
    ping(input) {
      return { ok: true, packageId: context.packageId, input };
    }
  });
}

export async function deactivate() {
  await mark("deactivate");
}
`.trimStart();
}
