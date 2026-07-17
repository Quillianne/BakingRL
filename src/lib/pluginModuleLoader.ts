import { convertFileSrc } from "@tauri-apps/api/core";
import { adapter } from "$lib/adapter/index";

const pluginModuleProtocol = "bakingrl-plugin";

export async function importPluginModule(packageId: string, webviewId: string, entry: string, version: string | number) {
  if (!adapter.isTauri) {
    return import(/* @vite-ignore */ adapter.packageModuleUrl(packageId, entry, version));
  }
  return import(/* @vite-ignore */ pluginModuleUrl(packageId, webviewId, entry, version));
}

export function pluginModuleUrl(packageId: string, webviewId: string, entry: string, version: string | number) {
  const normalizedEntry = normalizePackagePath(entry);
  const path = ["modules", packageId, webviewId, ...normalizedEntry.split("/")]
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  const base = convertFileSrc("", pluginModuleProtocol);
  return `${base}${path}?v=${encodeURIComponent(String(version))}`;
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
