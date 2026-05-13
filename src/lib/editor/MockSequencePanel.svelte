<script lang="ts">
  import {
    MOCK_TELEMETRY_SEQUENCES,
    mockSequenceDurationMs,
    type MockTelemetryPerspective,
    type MockTelemetrySequence
  } from "$lib/rlTelemetrySequences";

  const PERSPECTIVE_GROUPS: { perspective: MockTelemetryPerspective; label: string }[] = [
    { perspective: "caster", label: "Caster" },
    { perspective: "player", label: "Player POV" }
  ];

  let {
    sequences = MOCK_TELEMETRY_SEQUENCES,
    activeSequenceId = "",
    onplay,
    onstop
  }: {
    sequences?: MockTelemetrySequence[];
    activeSequenceId?: string;
    onplay: (sequence: MockTelemetrySequence) => void;
    onstop: () => void;
  } = $props();

  let selectedPerspective = $state<MockTelemetryPerspective>("caster");

  const activeSequence = $derived(
    sequences.find((sequence) => sequence.id === activeSequenceId) ?? null
  );
  const availablePerspectives = $derived(
    PERSPECTIVE_GROUPS.map((group) => ({
      ...group,
      sequences: sequences.filter((sequence) => sequence.perspective === group.perspective)
    })).filter((group) => group.sequences.length > 0)
  );
  const selectedSequences = $derived(
    sequences.filter((sequence) => sequence.perspective === selectedPerspective)
  );

  $effect(() => {
    if (availablePerspectives.some((group) => group.perspective === selectedPerspective)) return;
    selectedPerspective = availablePerspectives[0]?.perspective ?? "player";
  });

  function durationLabel(sequence: MockTelemetrySequence) {
    const seconds = Math.max(0.1, mockSequenceDurationMs(sequence) / 1000);
    return seconds >= 10 ? `${Math.round(seconds)}s` : `${seconds.toFixed(1)}s`;
  }

  function frameLabel(sequence: MockTelemetrySequence) {
    const count = sequence.frames.length;
    return `${count} ${count === 1 ? "frame" : "frames"}`;
  }
</script>

<div class="mock-sequence-panel">
  {#if activeSequence}
    <div class="active-sequence">
      <span>
        <small>Playing</small>
        <strong>{activeSequence.name}</strong>
      </span>
      <button type="button" class="stop-btn" onclick={onstop}>Stop</button>
    </div>
  {/if}

  {#if availablePerspectives.length > 1}
    <div class="perspective-switch" role="group" aria-label="Mock sequence type">
      {#each availablePerspectives as group (group.perspective)}
        <button
          type="button"
          class:active={selectedPerspective === group.perspective}
          aria-pressed={selectedPerspective === group.perspective}
          onclick={() => (selectedPerspective = group.perspective)}
        >
          {group.label}
        </button>
      {/each}
    </div>
  {/if}

  <div class="sequence-list">
    {#each selectedSequences as sequence (sequence.id)}
      <article class="sequence-card" class:active={activeSequenceId === sequence.id}>
        <div class="sequence-main">
          <strong>{sequence.name}</strong>
          <p>{sequence.description}</p>
          <span>{frameLabel(sequence)} · {durationLabel(sequence)}</span>
        </div>
        <button type="button" class="play-btn" onclick={() => onplay(sequence)}>
          {activeSequenceId === sequence.id ? "Restart" : "Play"}
        </button>
      </article>
    {/each}
  </div>
</div>

<style>
  .mock-sequence-panel {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 10px;
  }

  .active-sequence,
  .sequence-card {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-panel-hover) 52%, transparent);
  }

  .active-sequence {
    justify-content: space-between;
    padding: 8px;
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border-color));
    background: color-mix(in srgb, var(--accent) 12%, var(--bg-panel));
  }

  .active-sequence span {
    display: grid;
    min-width: 0;
    gap: 2px;
  }

  .active-sequence small,
  .sequence-main span {
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 650;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .active-sequence strong,
  .sequence-main strong {
    min-width: 0;
    overflow: hidden;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sequence-list {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 6px;
  }

  .perspective-switch {
    display: grid;
    min-width: 0;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px;
    padding: 3px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-dark) 22%, transparent);
  }

  .perspective-switch button {
    height: 26px;
    border: 0;
    border-radius: calc(var(--radius-sm) - 2px);
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    transition: var(--transition);
  }

  .perspective-switch button:hover,
  .perspective-switch button.active {
    background: var(--editor-bg-panel-hover);
    color: var(--text-primary);
  }

  .sequence-card {
    padding: 8px;
    transition: border-color 0.16s, background 0.16s;
  }

  .sequence-card:hover,
  .sequence-card.active {
    border-color: var(--border-color-focus);
    background: var(--editor-bg-panel-hover);
  }

  .sequence-main {
    display: grid;
    min-width: 0;
    flex: 1;
    gap: 3px;
  }

  .sequence-main p {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    color: var(--text-secondary);
    font-size: 11px;
    line-height: 1.35;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .play-btn,
  .stop-btn {
    display: inline-flex;
    flex: none;
    align-items: center;
    justify-content: center;
    min-width: 52px;
    height: 26px;
    padding: 0 9px;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--bg-dark) 35%, transparent);
    color: var(--text-primary);
    font: inherit;
    font-size: 11px;
    font-weight: 650;
    cursor: pointer;
    transition: var(--transition);
  }

  .play-btn:hover,
  .stop-btn:hover {
    border-color: var(--border-color-focus);
    background: color-mix(in srgb, var(--accent) 14%, var(--bg-panel-hover));
  }

  .stop-btn {
    color: var(--danger);
  }
</style>
