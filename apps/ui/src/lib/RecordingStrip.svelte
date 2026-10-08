<script lang="ts">
  import IconSquare from '~icons/lucide/square';
  import IconX from '~icons/lucide/x';
  import InlineRename from './InlineRename.svelte';
  import { amplitudeToMeterFill } from './meter';
  import { formatSampleRate, formatTime } from './types';

  interface RecorderDevice {
    deviceId: string;
    label: string;
  }

  interface Props {
    devices: RecorderDevice[];
    selectedDeviceId: string;
    /** Meter reading, RMS and peak in [0, 1]. */
    level: { rms: number; peak: number; clipped: boolean };
    /** Whether the take has clipped at any point (a latched warning). */
    clipLatched: boolean;
    elapsedSeconds: number;
    /** True capture rate in hertz, or 0 before the device reports it. */
    sampleRate: number;
    /** The project this take lands in; the strip names it so the destination is never a surprise. */
    destinationName?: string;
    /** True when the project was created for this take (recording started with none open). */
    destinationIsNew?: boolean;
    /** Renames the destination project in place; absent when it cannot be renamed here. */
    onRenameDestination?: (name: string) => void;
    onSelectDevice: (deviceId: string) => void;
    onStop: () => void;
    onCancel: () => void;
  }

  let {
    devices,
    selectedDeviceId,
    level,
    clipLatched,
    elapsedSeconds,
    sampleRate,
    destinationName,
    destinationIsNew = false,
    onRenameDestination,
    onSelectDevice,
    onStop,
    onCancel
  }: Props = $props();

  const rmsFill = $derived(amplitudeToMeterFill(level.rms));
  const peakFill = $derived(amplitudeToMeterFill(level.peak));
</script>

<div class="strip" data-testid="recording-strip" role="region" aria-label="Recording">
  <div class="pulse" aria-hidden="true"></div>
  {#if destinationName}
    <span class="announce" data-testid="recording-destination">
      <span class="label">Recording into{destinationIsNew ? ' new project' : ''}</span>
      {#if onRenameDestination}
        <InlineRename
          name={destinationName}
          class="dest-name"
          label="Rename project"
          testId="recording-destination-name"
          onRename={onRenameDestination}
        />
      {:else}
        <span class="dest-name" data-testid="recording-destination-name">{destinationName}</span>
      {/if}
    </span>
  {:else}
    <span class="label">Recording</span>
  {/if}

  <label class="device">
    <span class="device-label">Input</span>
    <select
      data-testid="recording-device"
      aria-label="Input device"
      value={selectedDeviceId}
      onchange={(event) => onSelectDevice(event.currentTarget.value)}
    >
      {#if devices.length === 0}
        <option value="">Default microphone</option>
      {/if}
      {#each devices as device (device.deviceId)}
        <option value={device.deviceId}>{device.label}</option>
      {/each}
    </select>
  </label>

  <div class="meter" data-testid="recording-level" title="Input level">
    <div class="meter-track">
      <div class="meter-rms" style={`width: ${(rmsFill * 100).toFixed(1)}%`}></div>
      <div class="meter-peak" style={`left: ${(peakFill * 100).toFixed(1)}%`}></div>
    </div>
    <span
      class="clip"
      class:on={clipLatched}
      data-testid="recording-clip"
      data-clipped={clipLatched}
      aria-hidden={!clipLatched}
    >
      CLIP
    </span>
  </div>

  <span class="elapsed" data-testid="recording-elapsed">{formatTime(elapsedSeconds)}</span>
  <span class="rate" data-testid="recording-samplerate">{formatSampleRate(sampleRate)}</span>

  <div class="actions">
    <button type="button" class="cancel" data-testid="recording-cancel" onclick={onCancel}>
      <IconX aria-hidden="true" /><span>Cancel</span>
    </button>
    <button type="button" class="stop" data-testid="recording-stop" onclick={onStop}>
      <IconSquare aria-hidden="true" /><span>Stop</span>
    </button>
  </div>
</div>

<style>
  /* Centred over the content area, clear of the 4.75rem side rail. */
  .strip {
    position: fixed;
    left: calc(50% + 4.75rem / 2);
    bottom: 1rem;
    transform: translateX(-50%);
    z-index: 15;
    display: flex;
    align-items: center;
    gap: 0.9rem;
    width: max-content;
    max-width: calc(100vw - 4.75rem - 2rem);
    padding: 0.55rem 0.9rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-xl);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-lg);
    font-size: 0.85rem;
  }

  .pulse {
    width: 0.7rem;
    height: 0.7rem;
    border-radius: 50%;
    background: #dc2626;
    animation: blink 1.1s ease-in-out infinite;
  }

  @keyframes blink {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.35;
    }
  }

  .label {
    font-weight: 600;
    letter-spacing: 0.01em;
    white-space: nowrap;
  }

  .announce {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
    max-width: 30rem;
  }

  .announce :global(.inline-rename) {
    min-width: 0;
  }

  .strip > :not(.announce) {
    flex: none;
  }

  :global(.dest-name) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
    color: var(--accent-strong);
  }

  .device {
    display: flex;
    align-items: center;
    gap: 0.35rem;
  }

  .device-label {
    color: var(--muted);
    font-size: 0.78rem;
  }

  .device select {
    max-width: 12rem;
    border: 1px solid var(--chrome-strong);
    border-radius: 6px;
    background: var(--panel-soft);
    color: var(--text);
    padding: 0.2rem 0.4rem;
  }

  .meter {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .meter-track {
    position: relative;
    width: 9rem;
    height: 0.7rem;
    border-radius: 4px;
    background: var(--panel-soft);
    border: 1px solid var(--chrome-strong);
    overflow: hidden;
  }

  .meter-rms {
    position: absolute;
    inset: 0 auto 0 0;
    background: linear-gradient(90deg, var(--accent), var(--accent-strong));
    transition: width 60ms linear;
  }

  .meter-peak {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--text);
    transition: left 60ms linear;
  }

  .clip {
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--muted);
    opacity: 0.35;
  }

  .clip.on {
    color: #fff;
    opacity: 1;
    background: #dc2626;
    padding: 0.05rem 0.3rem;
    border-radius: 4px;
    animation: flash 0.5s ease-in-out 3;
  }

  @keyframes flash {
    0%,
    100% {
      background: #dc2626;
    }
    50% {
      background: #f87171;
    }
  }

  .elapsed {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
    min-width: 3.5rem;
    text-align: right;
  }

  .rate {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .actions {
    display: flex;
    gap: 0.4rem;
  }

  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md);
    padding: 0.32rem 0.75rem;
    background: var(--panel-soft);
    color: var(--text);
    font-size: 0.82rem;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .actions button :global(svg) {
    font-size: 0.85rem;
  }

  .actions .stop {
    border-color: var(--accent);
    background: var(--accent-tint);
    color: var(--accent-strong);
    font-weight: 600;
  }

  .actions .stop :global(svg) {
    fill: currentColor;
  }

  .actions button:hover {
    filter: brightness(1.04);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  /* Narrow screens stack the strip into three rows: destination and time,
     level, then input and actions. */
  @media (max-width: 1200px) {
    .strip {
      width: min(34rem, calc(100vw - 4.75rem - 2rem));
      display: grid;
      grid-template-columns: auto minmax(0, 1fr) auto auto;
      grid-template-areas:
        'pulse dest dest elapsed'
        'meter meter meter rate'
        'device device actions actions';
      gap: 0.55rem 0.6rem;
      padding: 0.7rem 0.8rem;
    }

    .pulse {
      grid-area: pulse;
    }

    .strip > .label,
    .announce {
      grid-area: dest;
      min-width: 0;
      max-width: none;
    }

    .announce .label {
      flex: none;
    }

    .elapsed {
      grid-area: elapsed;
      min-width: 0;
    }

    .meter {
      grid-area: meter;
      min-width: 0;
    }

    .meter-track {
      flex: 1;
      width: auto;
    }

    .rate {
      grid-area: rate;
      text-align: right;
    }

    .device {
      grid-area: device;
      min-width: 0;
    }

    .device select {
      flex: 1;
      min-width: 0;
      max-width: none;
      min-height: 2.25rem;
    }

    .actions {
      grid-area: actions;
      justify-content: flex-end;
    }

    .actions button {
      min-height: 2.25rem;
    }
  }

  /* On phones the rail becomes a bottom dock; the strip spans the width above it. */
  @media (max-width: 720px) {
    .strip {
      left: calc(0.5rem + var(--safe-left));
      right: calc(0.5rem + var(--safe-right));
      bottom: calc(var(--mobile-rail-height) + var(--safe-bottom) + 0.5rem);
      width: auto;
      max-width: none;
      transform: none;
    }

    .announce {
      flex-direction: column;
      align-items: flex-start;
      gap: 0.1rem;
    }

    .announce .label {
      color: var(--muted);
      font-size: 0.75rem;
      font-weight: 500;
    }

    .announce :global(.inline-rename) {
      max-width: 100%;
    }

    /* Centre the name in its touch-sized hit area. */
    .announce :global(.inline-rename .display) {
      align-content: center;
    }
  }
</style>
