import { constants } from "node:fs";
import { access, chmod, copyFile, mkdir, realpath } from "node:fs/promises";
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

console.log(`Prepared Node sidecar: ${destination}`);

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
