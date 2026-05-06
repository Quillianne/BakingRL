<script lang="ts">
  import "$lib/theme.css";
  import { onMount } from "svelte";
  import { applyTheme, getStoredTheme } from "$lib/themes";

  let { children } = $props();

  onMount(() => {
    applyTheme(getStoredTheme());
  });
</script>

{@render children()}
