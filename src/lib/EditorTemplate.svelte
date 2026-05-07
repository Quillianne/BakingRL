<script lang="ts">
  import { tick, type Snippet } from "svelte";

  type CanvasSize = {
    width: number;
    height: number;
  };

  type PanPoint = {
    x: number;
    y: number;
  };

  let {
    title,
    message = "",
    canvas,
    centerKey,
    canvasAriaLabel,
    dragging = false,
    checkerboard = true,
    zoom = $bindable(0.48),
    panX = $bindable(0),
    panY = $bindable(0),
    stage = $bindable<HTMLElement | undefined>(),
    stageWrap = $bindable<HTMLElement | undefined>(),
    onPointerMove,
    onPointerUp,
    onStagePointerDown,
    onClose,
    stageContent,
    leftPanel,
    rightPanel,
    overlays
  }: {
    title: string;
    message?: string;
    canvas: CanvasSize;
    centerKey: string;
    canvasAriaLabel: string;
    dragging?: boolean;
    checkerboard?: boolean;
    zoom?: number;
    panX?: number;
    panY?: number;
    stage?: HTMLElement;
    stageWrap?: HTMLElement;
    onPointerMove: (event: PointerEvent) => void;
    onPointerUp: (event: PointerEvent) => void;
    onStagePointerDown: (event: PointerEvent) => void;
    onClose: () => void | Promise<void>;
    stageContent: Snippet;
    leftPanel: Snippet;
    rightPanel: Snippet;
    overlays?: Snippet;
  } = $props();

  let appliedCenterKey = $state("");

  const stageStyle = $derived(
    `width:${canvas.width}px;height:${canvas.height}px;transform:translate(${panX}px, ${panY}px) scale(${zoom});`
  );

  $effect(() => {
    if (!stageWrap || !stage || appliedCenterKey === centerKey) return;
    appliedCenterKey = centerKey;
    void centerViewport(centerKey);
  });

  function setZoom(nextZoom: number) {
    zoom = Math.max(0.12, Math.min(1.5, nextZoom));
  }

  function resetViewport() {
    void centerViewport(centerKey);
  }

  async function centerViewport(key: string) {
    zoom = fittedZoom();
    panX = 0;
    panY = 0;
    await tick();
    await nextFrame();
    if (key !== centerKey || !stageWrap || !stage) return;
    applyMeasuredCenter();
    await tick();
    await nextFrame();
    applyMeasuredCenter();
  }

  function fittedZoom() {
    if (!stageWrap) return zoom;
    const bounds = viewportBounds();
    const fitWidth = bounds.width / canvas.width;
    const fitHeight = bounds.height / canvas.height;
    return Math.max(0.12, Math.min(1.5, Math.min(fitWidth, fitHeight) * 0.92));
  }

  function viewportBounds() {
    if (!stageWrap) return { left: 0, top: 0, width: 0, height: 0 };
    const rect = stageWrap.getBoundingClientRect();
    const styles = getComputedStyle(stageWrap);
    const left = Number.parseFloat(styles.paddingLeft) || 0;
    const right = Number.parseFloat(styles.paddingRight) || 0;
    const top = Number.parseFloat(styles.paddingTop) || 0;
    const bottom = Number.parseFloat(styles.paddingBottom) || 0;
    return {
      left: rect.left + left,
      top: rect.top + top,
      width: Math.max(1, rect.width - left - right),
      height: Math.max(1, rect.height - top - bottom)
    };
  }

  function applyMeasuredCenter() {
    const nextPan = measuredCenterPan({ x: panX, y: panY });
    panX = nextPan.x;
    panY = nextPan.y;
  }

  function measuredCenterPan(currentPan: PanPoint): PanPoint {
    if (!stageWrap || !stage) return currentPan;
    const bounds = viewportBounds();
    const stageRect = stage.getBoundingClientRect();
    const targetX = bounds.left + bounds.width / 2;
    const targetY = bounds.top + bounds.height / 2;
    const stageX = stageRect.left + stageRect.width / 2;
    const stageY = stageRect.top + stageRect.height / 2;
    return {
      x: currentPan.x + Math.round(targetX - stageX),
      y: currentPan.y + Math.round(targetY - stageY)
    };
  }

  function nextFrame() {
    return new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  }

  function handleWheel(event: WheelEvent) {
    event.preventDefault();
    setZoom(zoom + (event.deltaY > 0 ? -0.05 : 0.05));
  }
</script>

<div class="editor-template" class:dragging>
  <section
    bind:this={stageWrap}
    class="stage-wrap"
    role="application"
    aria-label={canvasAriaLabel}
    onwheel={handleWheel}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointerdown={onStagePointerDown}
  >
    <div class="stage" class:checkerboard bind:this={stage} style={stageStyle}>
      {@render stageContent()}
    </div>
  </section>

  <aside class="editor-panel">
    <header class="editor-header">
      <div class="header-title">
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"></path><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"></path></svg>
        <strong>{title}</strong>
      </div>
      <div class="header-actions">
        {#if message}
          <span class="status-msg">{message}</span>
        {/if}
        <button class="icon-btn" onclick={() => setZoom(zoom - 0.08)} title="Zoom out">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"></circle><path d="M8 11h6"></path><path d="m21 21-4.3-4.3"></path></svg>
        </button>
        <button class="zoom-readout" onclick={resetViewport}>{Math.round(zoom * 100)}%</button>
        <button class="icon-btn" onclick={() => setZoom(zoom + 0.08)} title="Zoom in">
          <svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="11" cy="11" r="8"></circle><path d="M8 11h6"></path><path d="M11 8v6"></path><path d="m21 21-4.3-4.3"></path></svg>
        </button>
        <button class="icon-btn close-btn" onclick={() => void onClose()} title="Close Editor">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
        </button>
      </div>
    </header>

    <div class="panel-content">
      <div class="side-panel left">
        {@render leftPanel()}
      </div>
      <div class="side-panel right properties-side">
        {@render rightPanel()}
      </div>
    </div>
  </aside>

  {#if overlays}
    {@render overlays()}
  {/if}
</div>

<style>
  .editor-template {
    position: relative;
    width: 100vw;
    min-width: 0;
    height: var(--app-content-height, 100vh);
    min-height: 0;
    overflow: hidden;
    color: var(--text-primary);
    background: var(--editor-bg-dark);
    font-family: var(--font-family);
  }

  .editor-template.dragging {
    user-select: none;
    cursor: grabbing !important;
  }

  .stage-wrap {
    position: absolute;
    inset: 48px 0 0 0;
    display: grid;
    place-items: center;
    overflow: hidden;
    padding: 24px 348px 24px 292px;
    background:
      radial-gradient(circle at 1px 1px, color-mix(in srgb, var(--text-muted) 28%, transparent) 1px, transparent 0),
      var(--editor-bg-dark);
    background-size: 24px 24px;
  }

  .stage {
    position: relative;
    flex: none;
    overflow: visible;
    transform-origin: center center;
    outline: 1px solid var(--border-color-focus);
    background-color: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    box-shadow: 0 24px 80px color-mix(in srgb, var(--bg-dark) 70%, transparent);
  }

  .stage.checkerboard::before {
    content: "";
    position: absolute;
    inset: 0;
    z-index: 0;
    background-image:
      linear-gradient(color-mix(in srgb, var(--text-muted) 18%, transparent) 1px, transparent 1px),
      linear-gradient(90deg, color-mix(in srgb, var(--text-muted) 18%, transparent) 1px, transparent 1px);
    background-size: 40px 40px;
    pointer-events: none;
  }

  .editor-panel {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: 0;
    background: transparent;
    pointer-events: none;
  }

  .editor-header {
    position: absolute;
    inset: 0 0 auto 0;
    z-index: 700;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 48px;
    min-height: 48px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border-color);
    background: var(--editor-bg-panel);
    pointer-events: auto;
  }

  .header-title {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 8px;
    color: var(--text-primary);
  }

  .header-title svg {
    flex: none;
    color: var(--accent);
  }

  .header-title strong {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 650;
  }

  .header-actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    margin-left: 12px;
  }

  .status-msg {
    padding: 2px 6px;
    border-radius: 4px;
    background: var(--success-bg);
    color: var(--success);
    font-size: 11px;
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    transition: var(--transition);
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--editor-bg-panel-hover);
    color: var(--text-primary);
  }

  .zoom-readout {
    min-width: 54px;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    color: var(--text-secondary);
    font-size: 12px;
    cursor: pointer;
  }

  .panel-content {
    position: absolute;
    inset: 48px 0 0 0;
    z-index: 600;
    display: block;
    box-sizing: border-box;
    overflow: hidden;
    padding: 12px;
    pointer-events: none;
  }

  .side-panel {
    display: flex;
    width: 280px;
    height: auto;
    min-height: 0;
    max-height: calc(100% - 24px);
    flex-direction: column;
    gap: 12px;
    overflow: hidden;
    pointer-events: none;
  }

  .side-panel.left {
    position: absolute;
    top: 12px;
    left: 12px;
    overflow-y: auto;
    overscroll-behavior: contain;
    pointer-events: auto;
  }

  .side-panel.right {
    width: 336px;
  }

  .side-panel.properties-side {
    position: absolute;
    right: 12px;
    bottom: 12px;
    overflow: visible;
    pointer-events: none;
  }

  .side-panel.properties-side :global(.accordion) {
    flex: 0 0 auto;
    max-height: 100%;
    min-height: 0;
  }

  .side-panel.properties-side :global(.accordion > .accordion-body) {
    min-height: 0;
    max-height: calc(var(--app-content-height, 100vh) - 132px);
  }

  .side-panel :global(.accordion) {
    pointer-events: auto;
  }

  .stage :global(.snap-guide) {
    position: absolute;
    z-index: 130;
    background: var(--accent);
    box-shadow: 0 0 12px var(--accent);
    pointer-events: none;
  }

  .stage :global(.snap-guide.vertical) {
    top: 0;
    bottom: 0;
    width: 1px;
  }

  .stage :global(.snap-guide.horizontal) {
    right: 0;
    left: 0;
    height: 1px;
  }

  @media (max-width: 980px) {
    .stage-wrap {
      padding: 24px;
    }

    .panel-content {
      padding: 10px;
    }

    .side-panel {
      width: min(280px, calc(50vw - 16px)) !important;
      max-height: calc(100% - 20px);
    }

    .side-panel.left {
      top: 10px;
      left: 10px;
    }

    .side-panel.properties-side {
      right: 10px;
      bottom: 10px;
      width: min(336px, calc(50vw - 16px)) !important;
    }
  }
</style>
