import type {
  PackageDescriptor,
  WebviewContributionDescriptor
} from "$lib/dashboard/types";

export type ResolvedPackagePrimaryAction =
  | {
      kind: "webview";
      target: string;
      webview: WebviewContributionDescriptor;
      configuration: boolean;
    }
  | {
      kind: "settings";
      target: null;
      webview: null;
      configuration: true;
    };

export function packageCategories(pkg: PackageDescriptor) {
  return pkg.presentation?.categories ?? [];
}

export function resolvePackagePrimaryAction(
  pkg: PackageDescriptor
): ResolvedPackagePrimaryAction | null {
  const declared = pkg.presentation?.primaryAction;
  if (declared) {
    if (declared.kind === "webview" && declared.target) {
      const webview = pkg.contributions.webviews.find((candidate) => candidate.name === declared.target);
      if (webview) {
        if (webview.kind === "settings" && pkg.has_public_settings) {
          return { kind: "settings", target: null, webview: null, configuration: true };
        }
        return {
          kind: "webview",
          target: webview.name,
          webview,
          configuration: webview.kind === "settings"
        };
      }
    }
    if (declared.kind === "settings" && pkg.has_public_settings) {
      return { kind: "settings", target: null, webview: null, configuration: true };
    }
    return null;
  }

  const tool = pkg.contributions.webviews.find(
    (webview) => webview.kind === "tool" || webview.kind === "panel"
  );
  if (tool) {
    return {
      kind: "webview",
      target: tool.name,
      webview: tool,
      configuration: false
    };
  }

  const settingsWebview = pkg.contributions.webviews.find(
    (webview) => webview.kind === "settings"
  );
  if (settingsWebview) {
    if (pkg.has_public_settings) {
      return { kind: "settings", target: null, webview: null, configuration: true };
    }
    return {
      kind: "webview",
      target: settingsWebview.name,
      webview: settingsWebview,
      configuration: true
    };
  }

  if (pkg.has_public_settings) {
    return { kind: "settings", target: null, webview: null, configuration: true };
  }
  return null;
}

export function packageConfigurationIsPrimary(pkg: PackageDescriptor) {
  return resolvePackagePrimaryAction(pkg)?.configuration ?? false;
}
