<script lang="ts">
  import IconRotateCcw from '~icons/lucide/rotate-ccw';

  interface Props {
    /** Name of the removed recording, shown in the message. */
    name: string;
    /** True once a later journaled change has displaced the delete's entry;
     *  the button stops offering the undo. */
    stale?: boolean;
    onUndo: () => void;
  }

  let { name, stale = false, onUndo }: Props = $props();
</script>

<div class="undo-banner" role="status" data-testid="removal-undo">
  {#if stale}
    <span
      >Recording “{name}” removed. Another change happened since — restore it from the undo history
      (Ctrl+Z) instead.</span
    >
    <button type="button" class="undo" data-testid="removal-undo-action" disabled>
      <IconRotateCcw aria-hidden="true" />
      <span>Undo</span>
    </button>
  {:else}
    <span>Recording “{name}” removed.</span>
    <button type="button" class="undo" data-testid="removal-undo-action" onclick={onUndo}>
      <IconRotateCcw aria-hidden="true" />
      <span>Undo</span>
    </button>
  {/if}
</div>

<style>
  .undo-banner {
    position: fixed;
    left: 50%;
    bottom: 1.25rem;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 0.9rem;
    padding: 0.5rem 0.6rem 0.5rem 0.95rem;
    border-radius: var(--radius-lg);
    border: 1px solid var(--chrome-strong);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-lg);
    font-size: 0.85rem;
    z-index: 15;
  }

  .undo {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md);
    background: var(--panel-soft);
    color: var(--accent-strong);
    padding: 0.35rem 0.65rem;
    box-shadow: var(--shadow-sm);
    transition:
      background var(--t-fast),
      border-color var(--t-fast),
      color var(--t-fast);
  }

  .undo :global(svg) {
    font-size: 1rem;
  }

  .undo:hover {
    background: var(--accent-tint);
    border-color: color-mix(in oklab, var(--accent) 30%, transparent);
  }

  .undo:disabled {
    color: var(--muted);
    cursor: default;
  }

  .undo:disabled:hover {
    background: var(--panel-soft);
    border-color: var(--chrome-strong);
  }
</style>
