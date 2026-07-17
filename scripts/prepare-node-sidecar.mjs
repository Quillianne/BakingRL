import { constants } from "node:fs";
import { access, chmod, copyFile, mkdir, realpath, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import path from "node:path";

const explicitSource = process.env.BAKINGRL_NODE_BINARY;
const source = await realpath(explicitSource || process.execPath);
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || process.env.TARGET || defaultTriple();
const suffix = process.platform === "win32" ? ".exe" : "";
const outDir = path.resolve("src-tauri", "bin");
const destination = path.join(outDir, `node-${triple}${suffix}`);

await access(source, constants.X_OK);
await mkdir(outDir, { recursive: true });
await copyFile(source, destination);
await chmod(destination, 0o755);

const probeEnv = { ...process.env };
delete probeEnv.NODE_OPTIONS;
delete probeEnv.NODE_PATH;
const probe = spawnSync(destination, ["--version"], {
  encoding: "utf8",
  env: probeEnv,
  timeout: 10_000,
  windowsHide: true
});
if (probe.error || probe.status !== 0) {
  await rm(destination, { force: true });
  const details = [
    probe.error?.message,
    probe.signal ? `signal ${probe.signal}` : undefined,
    probe.stderr?.trim(),
    probe.stdout?.trim()
  ].filter(Boolean).join("\n");
  throw new Error(
    `Copied Node binary '${source}' cannot run from the Tauri sidecar location. ` +
      "Use a self-contained Node distribution or set BAKINGRL_NODE_BINARY to one before building." +
      (details ? `\n${details}` : "")
  );
}

console.log(`Prepared Node sidecar: ${destination} (${probe.stdout.trim()})`);

function defaultTriple() {
  const key = `${process.platform}:${process.arch}`;
  const triples = {
    "darwin:arm64": "aarch64-apple-darwin",
    "darwin:x64": "x86_64-apple-darwin",
    "linux:arm64": "aarch64-unknown-linux-gnu",
    "linux:x64": "x86_64-unknown-linux-gnu",
    "win32:arm64": "aarch64-pc-windows-msvc",
    "win32:x64": "x86_64-pc-windows-msvc"
  };
  const triple = triples[key];
  if (!triple) {
    throw new Error(`Unsupported Node sidecar target '${key}'. Set TAURI_ENV_TARGET_TRIPLE.`);
  }
  return triple;
}
