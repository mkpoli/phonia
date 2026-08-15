<script lang="ts">
  import IconWaveform from '~icons/lucide/audio-waveform';
  import IconX from '~icons/lucide/x';
  import IconCheck from '~icons/lucide/check';

  interface Props {
    /** The recording's current sampling rate, shown for reference. */
    currentHz: number;
    busy?: boolean;
    onResample: (hz: number) => void;
    onClose: () => void;
  }

  let { currentHz, busy = false, onResample, onClose }: Props = $props();

  const PRESETS = [8000, 16000, 22050, 44100, 48000];

  let hz = $state(16000);

  const valid = $derived(Number.isFinite(hz) && hz >= 1);

  function apply() {
    if (!valid || busy) return;
    onResample(Math.round(hz));
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      onClose();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="backdrop" data-testid="resample-dialog">
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Resample recording">
    <header class="head">
      <h2><IconWaveform aria-hidden="true" />Resample</h2>
      <button type="button" class="close" data-testid="resample-close" aria-label="Close" onclick={onClose}>
        <IconX aria-hidden="true" />
      </button>
    </header>

    <div class="body">
      <fieldset>
        <legend>New sampling rate</legend>
        <label class="field">
          <input
            type="number"
            data-testid="resample-rate"
            min="1"
            step="100"
            value={hz}
            oninput={(event) => (hz = event.currentTarget.valueAsNumber)}
            onkeydown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                apply();
              }
            }}
          />
          <span class="unit">Hz</span>
        </label>
        <p class="current">Current: {currentHz.toLocaleString()} Hz</p>
        <div class="presets">
          {#each PRESETS as preset (preset)}
            <button
              type="button"
              class="preset"
              class:on={hz === preset}
              data-testid="resample-preset"
              data-hz={preset}
              onclick={() => (hz = preset)}
            >
              {preset / 1000} kHz
            </button>
          {/each}
        </div>
      </fieldset>

      <button
        type="button"
        class="apply"
        data-testid="resample-apply"
        disabled={!valid || busy}
        onclick={apply}
      >
        <IconCheck aria-hidden="true" /><span>{busy ? 'Resampling…' : 'Resample to new recording'}</span>
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in oklab, #000 52%, transparent);
    backdrop-filter: blur(2px);
    z-index: 25;
  }

  .dialog {
    width: min(22rem, calc(100vw - 2rem));
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.6rem 0.9rem;
    border-bottom: 1px solid var(--chrome-strong);
    background: var(--panel-soft);
  }

  .head h2 {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.95rem;
    font-weight: 600;
  }

  .head h2 :global(svg) {
    font-size: 1rem;
    color: var(--accent);
  }

  .close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    padding: 0.2rem;
    cursor: pointer;
  }

  .close:hover {
    background: var(--panel);
    color: var(--text);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    padding: 0.85rem 0.9rem 0.95rem;
  }

  fieldset {
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md);
    padding: 0.5rem 0.7rem 0.6rem;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  legend {
    padding: 0 0.35rem;
    font-size: 0.72rem;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .field input {
    flex: 1;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    min-height: 2rem;
    padding: 0.25rem 0.4rem;
    font-size: 0.9rem;
    font-variant-numeric: tabular-nums;
  }

  .field input:focus-visible {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }

  .unit {
    font-size: 0.8rem;
    color: var(--muted);
  }

  .current {
    margin: 0;
    font-size: 0.75rem;
    color: var(--muted);
  }

  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }

  .preset {
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    padding: 0.2rem 0.5rem;
    font-size: 0.76rem;
    cursor: pointer;
    white-space: nowrap;
    transition:
      border-color var(--t-fast),
      background var(--t-fast);
  }

  .preset:hover {
    border-color: var(--accent);
  }

  .preset.on {
    border-color: var(--accent);
    background: color-mix(in oklab, var(--accent) 16%, var(--panel-soft));
  }

  .apply {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    border: 1px solid var(--accent);
    border-radius: var(--radius-md);
    background: var(--accent);
    color: var(--on-accent);
    padding: 0.5rem 0.6rem;
    font-size: 0.88rem;
    font-weight: 600;
    cursor: pointer;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .apply :global(svg) {
    font-size: 1rem;
  }

  .apply:hover:not(:disabled) {
    background: var(--accent-strong);
    border-color: var(--accent-strong);
  }

  .apply:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
