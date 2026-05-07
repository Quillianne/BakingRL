<script lang="ts">
  import type { ToastMessage } from "$lib/dashboard/types";

  const {
    toasts,
    ondismiss
  }: {
    toasts: ToastMessage[];
    ondismiss: (id: string) => void;
  } = $props();
</script>

<div class="toast-viewport" aria-live="polite" aria-relevant="additions">
  {#each toasts as toast (toast.id)}
    <article class="toast {toast.tone}">
      <span class="toast-dot" aria-hidden="true"></span>
      <p>{toast.message}</p>
      <button type="button" aria-label="Dismiss notification" onclick={() => ondismiss(toast.id)}>
        <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6 6 18"></path>
          <path d="m6 6 12 12"></path>
        </svg>
      </button>
    </article>
  {/each}
</div>
