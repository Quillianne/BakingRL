<script lang="ts">
  const {
    open = false,
    title = "Confirm action",
    message = "",
    confirmLabel = "Confirm",
    cancelLabel = "Cancel",
    danger = false,
    onconfirm = () => {},
    oncancel = () => {}
  }: {
    open?: boolean;
    title?: string;
    message?: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    onconfirm?: () => void | Promise<void>;
    oncancel?: () => void;
  } = $props();

  function handleBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) {
      oncancel();
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      oncancel();
    }
  }
</script>

{#if open}
  <div class="confirm-backdrop" role="presentation" onclick={handleBackdropClick} onkeydown={handleKeydown}>
    <div
      class="confirm-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
      tabindex="-1"
    >
      <div class="confirm-copy">
        <h2 id="confirm-title">{title}</h2>
        <p id="confirm-message">{message}</p>
      </div>
      <div class="confirm-actions">
        <button type="button" class="cancel-btn" onclick={oncancel}>{cancelLabel}</button>
        <button type="button" class="confirm-btn" class:danger onclick={onconfirm}>{confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .confirm-backdrop {
    position: fixed;
    inset: 0;
    z-index: 2000;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgba(0, 0, 0, 0.56);
    backdrop-filter: blur(6px);
  }

  .confirm-dialog {
    width: min(420px, 100%);
    padding: 20px;
    background: var(--editor-bg-panel, var(--bg-panel));
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    box-shadow: 0 24px 72px rgba(0, 0, 0, 0.5);
  }

  .confirm-copy {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  h2 {
    margin: 0;
    color: var(--text-primary);
    font-size: 16px;
    font-weight: 650;
  }

  p {
    margin: 0;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.45;
  }

  .confirm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin-top: 20px;
  }

  button {
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 8px 14px;
    color: var(--text-primary);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: var(--transition);
  }

  .cancel-btn {
    background: rgba(255, 255, 255, 0.05);
  }

  .cancel-btn:hover {
    background: rgba(255, 255, 255, 0.1);
  }

  .confirm-btn {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }

  .confirm-btn:hover {
    background: var(--accent-hover);
  }

  .confirm-btn.danger {
    background: var(--danger);
    border-color: var(--danger);
  }

  .confirm-btn.danger:hover {
    background: var(--danger-hover);
    border-color: var(--danger-hover);
  }
</style>
