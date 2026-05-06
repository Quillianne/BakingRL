<script lang="ts">
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
</script>

<!-- The Stream Overlay is accessed via OBS Browser Source -->
<OverlayRenderer layoutType="stream" />

<svelte:head>
  <style>
    html,
    body {
      background: transparent !important;
    }
  </style>
</svelte:head>
