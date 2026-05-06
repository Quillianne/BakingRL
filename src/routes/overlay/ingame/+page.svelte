<script lang="ts">
  import OverlayRenderer from "$lib/OverlayRenderer.svelte";
</script>

<!-- The In-Game Overlay has window-level Tauri configurations (click-through, frameless) 
     but the rendering logic is perfectly shared. -->
<OverlayRenderer />

<svelte:head>
  <style>
    html,
    body {
      background: transparent !important;
    }
  </style>
</svelte:head>
