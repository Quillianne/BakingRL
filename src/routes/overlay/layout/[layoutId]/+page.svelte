<script lang="ts">
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";

  const { data } = $props();
</script>

<OverlayRenderer layoutId={data.layoutId} />

<svelte:head>
  <style>
    html,
    body {
      background: transparent !important;
    }
  </style>
</svelte:head>
