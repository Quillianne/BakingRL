<script lang="ts">
  import type { Snippet } from "svelte";

  type BadgeTone = "route" | "stream" | "muted" | "warn";
  type Badge = {
    label: string;
    tone?: BadgeTone;
  };

  const {
    name,
    ariaLabel,
    summary,
    actionLabel = "",
    variant = "default",
    badges = [],
    deleteTitle = "",
    deleteDisabled = false,
    onNameBlur,
    onDelete,
    preview,
    tools,
    actions,
    onOpen
  }: {
    name: string;
    ariaLabel: string;
    summary: string;
    actionLabel?: string;
    variant?: "default" | "overlay" | "page";
    badges?: Badge[];
    deleteTitle?: string;
    deleteDisabled?: boolean;
    onNameBlur: (event: FocusEvent) => void | Promise<void>;
    onDelete?: () => void | Promise<void>;
    preview?: Snippet;
    tools?: Snippet;
    actions: Snippet;
    onOpen?: () => void | Promise<void>;
  } = $props();

  const roleClass = $derived(
    [
      badges.some((badge) => badge.tone === "route") ? "role-route" : "",
      badges.some((badge) => badge.tone === "stream") ? "role-stream" : "",
      badges.some((badge) => badge.tone === "warn") ? "role-warn" : ""
    ]
      .filter(Boolean)
      .join(" ")
  );

  const variantClass = $derived(variant === "default" ? "" : `layout-card-${variant}`);
</script>

<article
  class={`studio-card layout-card ${variantClass} ${roleClass} ${onOpen ? "clickable-card" : ""}`}
  title={summary}
>
  <div class="preview-click-zone">
    <div class="thumb-preview overlay-layout-preview layout-card-preview" aria-hidden="true" data-action-label={actionLabel}>
      {@render preview?.()}
    </div>
    {#if onOpen}
      <button type="button" class="preview-hit-target" aria-label={actionLabel ? `${actionLabel} ${name}` : name} onclick={() => void onOpen()}></button>
    {/if}
  </div>

  <div class="card-heading layout-card-heading">
    <div class="package-meta layout-card-title">
      <input
        aria-label={ariaLabel}
        value={name}
        onblur={onNameBlur}
        onkeydown={(event) => event.key === "Enter" && (event.currentTarget as HTMLInputElement).blur()}
      />
    </div>
    <div class="layout-card-tools">
      {#if onDelete}
        <button class="icon-button danger" onclick={() => void onDelete()} disabled={deleteDisabled} title={deleteTitle}>
          <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M3 6h18"></path>
            <path d="M8 6V4h8v2"></path>
            <path d="M19 6v14H5V6"></path>
          </svg>
        </button>
      {/if}
      {@render tools?.()}
    </div>
  </div>

  <div class="card-actions">
    {@render actions()}
  </div>
</article>
