<script lang="ts">
  const {
    thumbnail = null,
    name = ""
  }: {
    thumbnail?: string | null;
    name?: string;
  } = $props();
</script>

{#if thumbnail}
  <img class="thumb-image" src={thumbnail} alt="" draggable="false" />
{:else}
  <div class="thumb-placeholder">
    <span>{name}</span>
  </div>
{/if}
