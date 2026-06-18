export type RouteReturnState = {
  returnTo: string;
  scrollY: number;
};

const PENDING_ROUTE_RETURN_KEY = "bakingrl.pendingRouteReturn";
const ROUTE_SCROLL_RESTORE_KEY = "bakingrl.routeReturn";

export function captureRouteReturnState(): RouteReturnState {
  if (typeof window === "undefined") return { returnTo: "/", scrollY: 0 };
  const scrollHost = document.querySelector(".studio-main") as HTMLElement | null;
  return {
    returnTo: `${window.location.pathname}${window.location.search}${window.location.hash}`,
    scrollY: Math.max(0, Math.round(scrollHost?.scrollTop ?? window.scrollY ?? 0))
  };
}

export function returnStateQuery(state = captureRouteReturnState()) {
  const params = new URLSearchParams();
  params.set("returnTo", normalizeReturnTo(state.returnTo, "/"));
  params.set("scrollY", String(Math.max(0, Math.round(Number(state.scrollY) || 0))));
  return `?${params.toString()}`;
}

export function routeReturnFromParams(returnTo: unknown, scrollY: unknown, fallback = "/"): RouteReturnState {
  return {
    returnTo: normalizeReturnTo(returnTo, fallback),
    scrollY: Math.max(0, Math.round(Number(scrollY) || 0))
  };
}

export function storePendingRouteReturn(state = captureRouteReturnState()) {
  writeSessionJson(PENDING_ROUTE_RETURN_KEY, {
    returnTo: normalizeReturnTo(state.returnTo, "/"),
    scrollY: Math.max(0, Math.round(Number(state.scrollY) || 0))
  });
}

export function consumePendingRouteReturn() {
  return consumeSessionJson<RouteReturnState>(PENDING_ROUTE_RETURN_KEY);
}

export function storeRouteScrollRestore(state: RouteReturnState) {
  writeSessionJson(ROUTE_SCROLL_RESTORE_KEY, {
    scrollY: Math.max(0, Math.round(Number(state.scrollY) || 0))
  });
}

export function consumeRouteScrollRestore() {
  return consumeSessionJson<{ scrollY?: number }>(ROUTE_SCROLL_RESTORE_KEY);
}

export function normalizeReturnTo(value: unknown, fallback = "/") {
  if (typeof value !== "string") return fallback;
  const trimmed = value.trim();
  return trimmed.startsWith("/") && !trimmed.startsWith("//") ? trimmed : fallback;
}

function writeSessionJson(key: string, value: unknown) {
  try {
    sessionStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Session state is a convenience only.
  }
}

function consumeSessionJson<T>(key: string): T | null {
  try {
    const raw = sessionStorage.getItem(key);
    if (!raw) return null;
    sessionStorage.removeItem(key);
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}
