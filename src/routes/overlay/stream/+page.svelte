<script lang="ts">
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
</script>

<OverlayRenderer layoutType="stream" />

<svelte:head>
  <style>
    html,
    body {
      background: transparent !important;
    }
  </style>
</svelte:head>
