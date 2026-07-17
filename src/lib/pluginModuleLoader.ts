import { convertFileSrc } from "@tauri-apps/api/core";
import { adapter } from "$lib/adapter/index";

const pluginModuleProtocol = "bakingrl-plugin";
const pluginModuleAppPath = "__bakingrl-plugin";

export async function importPluginModule(packageId: string, webviewId: string, entry: string, version: string | number) {
  if (!adapter.isTauri) {
    return import(/* @vite-ignore */ adapter.packageModuleUrl(packageId, entry, version));
  }
  return import(/* @vite-ignore */ pluginModuleUrl(packageId, webviewId, entry, version));
}

export function pluginModuleUrl(packageId: string, webviewId: string, entry: string, version: string | number) {
  if (import.meta.env.DEV) {
    return pluginModuleProtocolUrl(packageId, webviewId, entry, version);
  }
  const path = pluginModulePath(packageId, webviewId, entry);
  return new URL(`/${pluginModuleAppPath}/${path}?v=${encodeURIComponent(String(version))}`, window.location.href).href;
}

function pluginModuleProtocolUrl(packageId: string, webviewId: string, entry: string, version: string | number) {
  const path = pluginModulePath(packageId, webviewId, entry);
  const base = convertFileSrc("", pluginModuleProtocol);
  return `${base}${path}?v=${encodeURIComponent(String(version))}`;
}

function pluginModulePath(packageId: string, webviewId: string, entry: string) {
  const normalizedEntry = normalizePackagePath(entry);
  return ["modules", packageId, webviewId, ...normalizedEntry.split("/")]
    .map((segment) => encodeURIComponent(segment))
    .join("/");
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
