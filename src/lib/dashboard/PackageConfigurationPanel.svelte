<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
  import type { DashboardState } from "$lib/dashboard/state.svelte";
  import type { PackageDescriptor, PageLayout } from "$lib/dashboard/types";

  const {
    state: dashboard,
    pkg
  }: {
    state: DashboardState;
    pkg: PackageDescriptor;
  } = $props();

  let page = $state<PageLayout | null>(null);
  let error = $state("");
  let loading = $state(false);
  let requestId = 0;

  $effect(() => {
    const packageId = pkg.id;
    const canLoad = Boolean(pkg.exports.configuration && dashboard.isPackageEnabled(pkg) && dashboard.isPackageCompatible(pkg));
    if (!canLoad) {
      page = null;
      error = "";
      loading = false;
      return;
    }
    void loadConfigurationPage(packageId);
  });

  async function loadConfigurationPage(packageId: string) {
    const currentRequest = ++requestId;
    loading = true;
    error = "";
    try {
      const loadedPage = await invoke<PageLayout>("get_package_configuration_page", { packageId });
      if (currentRequest === requestId) page = loadedPage;
    } catch (loadError) {
      if (currentRequest === requestId) {
        error = dashboard.errorMessage(loadError);
        page = null;
      }
    } finally {
      if (currentRequest === requestId) loading = false;
    }
  }
</script>

{#if !pkg.exports.configuration}
  <div class="empty-state">
    <p>{dashboard.t("packages.configurationUnavailable")}</p>
  </div>
{:else if !dashboard.isPackageCompatible(pkg)}
  <div class="callout">
    <strong>{dashboard.packageCompatibilityLabel(pkg)}</strong>
    <span>{pkg.compatibility.message ?? dashboard.t("packages.incompatiblePackage")}</span>
  </div>
{:else if !dashboard.isPackageEnabled(pkg)}
  <div class="empty-state">
    <p>{dashboard.t("packages.configurationDisabled")}</p>
  </div>
{:else if loading}
  <div class="empty-state">
    <p>{dashboard.t("common.loading")}</p>
  </div>
{:else if error}
  <div class="callout">
    <strong>{dashboard.t("common.error")}</strong>
    <span>{error}</span>
  </div>
{:else if page}
  <div class="plugin-config-frame">
    <OverlayRenderer
      mode="page"
      source="configuration"
      layoutOverride={page}
      layoutRevision={page.updated_at_ms}
      preview
    />
  </div>
{/if}
