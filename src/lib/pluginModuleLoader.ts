import { invoke } from "@tauri-apps/api/core";
import { adapter } from "$lib/adapter/index";

const relativeImportPattern = /\b(import|export)([\s\S]*?)(["'])(\.{1,2}\/[^"']+)\3/g;
const objectUrls = new Map<string, string>();
const pendingUrls = new Map<string, Promise<string>>();

export async function importPluginModule(packageId: string, entry: string, version: string | number) {
  if (!adapter.isTauri) {
    return import(/* @vite-ignore */ adapter.packageModuleUrl(packageId, entry, version));
  }
  const url = await pluginModuleUrl(packageId, entry, version);
  return import(/* @vite-ignore */ url);
}

async function pluginModuleUrl(packageId: string, entry: string, version: string | number): Promise<string> {
  const normalizedEntry = normalizePackagePath(entry);
  const key = `${packageId}\0${normalizedEntry}\0${version}`;
  const cached = objectUrls.get(key);
  if (cached) return cached;
  const pending = pendingUrls.get(key);
  if (pending) return pending;

  const next = buildModuleUrl(packageId, normalizedEntry, version, key);
  pendingUrls.set(key, next);
  return next;
}

async function buildModuleUrl(packageId: string, entry: string, version: string | number, key: string) {
  try {
    const source = await invoke<string>("read_package_file_text", {
      packageId,
      relativePath: entry
    });
    const rewritten = await rewriteRelativeImports(packageId, entry, version, source);
    const debugSource = `\n//# sourceURL=bakingrl-plugin://${packageId}/${entry}`;
    const blob = new Blob([rewritten, debugSource], { type: "text/javascript" });
    const url = URL.createObjectURL(blob);
    objectUrls.set(key, url);
    return url;
  } finally {
    pendingUrls.delete(key);
  }
}

async function rewriteRelativeImports(packageId: string, entry: string, version: string | number, source: string) {
  const replacements: Array<{ start: number; end: number; value: string }> = [];
  for (const match of source.matchAll(relativeImportPattern)) {
    const rawSpecifier = match[4];
    if (!rawSpecifier || match.index === undefined) continue;
    const resolved = resolveRelativePackagePath(entry, rawSpecifier);
    const rewrittenUrl = await pluginModuleUrl(packageId, resolved, version);
    const specifierStart = match.index + match[0].lastIndexOf(rawSpecifier);
    replacements.push({
      start: specifierStart,
      end: specifierStart + rawSpecifier.length,
      value: rewrittenUrl
    });
  }

  if (!replacements.length) return source;

  let output = "";
  let cursor = 0;
  for (const replacement of replacements) {
    output += source.slice(cursor, replacement.start);
    output += replacement.value;
    cursor = replacement.end;
  }
  return output + source.slice(cursor);
}

function resolveRelativePackagePath(from: string, specifier: string) {
  const base = from.split("/").slice(0, -1);
  for (const segment of specifier.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      base.pop();
    } else {
      base.push(segment);
    }
  }
  return normalizePackagePath(base.join("/"));
}

function normalizePackagePath(path: string) {
  const parts: string[] = [];
  for (const segment of path.split("/")) {
    if (!segment || segment === ".") continue;
    if (segment === "..") {
      parts.pop();
    } else {
      parts.push(segment);
    }
  }
  return parts.join("/");
}
