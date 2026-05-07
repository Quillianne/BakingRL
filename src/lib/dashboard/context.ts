import { getContext, setContext } from "svelte";
import type { DashboardState } from "$lib/dashboard/state.svelte";

const DASHBOARD_CONTEXT = Symbol("bakingrl-dashboard");

export function setDashboardContext(state: DashboardState) {
  setContext(DASHBOARD_CONTEXT, state);
}

export function getDashboardContext() {
  return getContext<DashboardState>(DASHBOARD_CONTEXT);
}
