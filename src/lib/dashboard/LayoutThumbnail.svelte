<script lang="ts">
  import { createLayoutThumbnail, type ThumbnailLayout } from "$lib/layoutThumbnail";

  const {
    thumbnail = null,
    layout = null,
    kind = "overlay",
    themeKey = ""
  }: {
    thumbnail?: string | null;
    layout?: ThumbnailLayout | null;
    kind?: "overlay" | "page";
    themeKey?: string;
  } = $props();

  const source = $derived(layout ? thumbnailForTheme(layout, kind, themeKey) : thumbnail);

  function thumbnailForTheme(layout: ThumbnailLayout, kind: "overlay" | "page", _themeKey: string) {
    return createLayoutThumbnail(layout, { kind });
  }
</script>

{#if source}
  <img class="thumb-image" src={source} alt="" draggable="false" />
{:else}
  <div class="thumb-placeholder"></div>
{/if}
