<script lang="ts">
  import IconPlay from '~icons/lucide/play';
  import IconZoomIn from '~icons/lucide/zoom-in';
  import IconActivity from '~icons/lucide/activity';
  import IconAudioWaveform from '~icons/lucide/audio-waveform';
  import IconFilter from '~icons/lucide/filter';
  import IconScissors from '~icons/lucide/scissors';
  import IconFlipHorizontal2 from '~icons/lucide/flip-horizontal-2';
  import IconX from '~icons/lucide/x';
  import { formatTime, type Selection, type SelectionReadout } from './types';
  import { formatPitch, type PitchUnit } from './pitch-units';

  interface Props {
    selection: Selection;
    readout: SelectionReadout | null;
    formantMeans: number[] | null;
    showFormants: boolean;
    /** Display unit for the F0 statistics. */
    pitchUnit?: PitchUnit;
    /** A band-filtered preview of the box selection is sounding right now. */
    filteredPlaying?: boolean;
    onPlay: () => void;
    onZoom: () => void;
    onVoiceReport: () => void;
    onSpectrum: () => void;
    /** Copies the selection's time span into a new library recording; absent
     *  hides the extract button. */
    onExtract?: () => void;
    /** Filters a box selection to its frequency band into a new recording;
     *  absent, or on a time selection, hides the filter button. */
    onFilter?: () => void;
    /** Reverses the selection in time into a new recording; absent hides the
     *  reverse button. */
    onReverse?: () => void;
    onClear: () => void;
  }

  let {
    selection,
    readout,
    formantMeans,
    showFormants,
    pitchUnit = 'hertz',
    filteredPlaying = false,
    onPlay,
    onZoom,
    onVoiceReport,
    onSpectrum,
    onExtract,
    onFilter,
    onReverse,
    onClear
  }: Props = $props();

  // A box selection plays through the engine's band filter, so its Play button
  // states the band it will render rather than the plain transport.
  const banded = $derived(selection.mode === 'box');

  function hz(value: number | null | undefined, digits = 0): string {
    return value === null || value === undefined || !Number.isFinite(value)
      ? '—'
      : `${value.toFixed(digits)} Hz`;
  }

  function db(value: number | null | undefined): string {
    return value === null || value === undefined || !Number.isFinite(value)
      ? '—'
      : `${value.toFixed(1)} dB`;
  }

  function num(value: number | null | undefined, digits = 2): string {
    return value === null || value === undefined || !Number.isFinite(value)
      ? '—'
      : value.toFixed(digits);
  }

  function amp(value: number | null | undefined): string {
    return value === null || value === undefined || !Number.isFinite(value)
      ? '—'
      : value.toFixed(3);
  }

  // A near-full-scale peak reads as clipping in normalized float audio.
  const clipping = $derived((readout?.peak ?? 0) >= 0.99);

  const visibleFormants = $derived(
    (formantMeans ?? []).slice(0, 3).map((value, index) => ({ index: index + 1, value }))
  );
</script>

<div
  class="readout"
  data-testid="readout-bar"
  data-t0={selection.t0}
  data-t1={selection.t1}
  data-f0={selection.f0}
  data-f1={selection.f1}
  data-band-energy={readout?.bandEnergyDb ?? ''}
>
  <div class="fields">
    <span class="field" data-testid="readout-duration">
      <span class="k">Δt</span><span class="v">{formatTime(selection.t1 - selection.t0)} s</span>
    </span>
    <span class="field">
      <span class="k">t</span><span class="v">{formatTime(selection.t0)}–{formatTime(selection.t1)}</span>
    </span>
    <span class="field">
      <span class="k">f</span>
      <span class="v">
        {#if selection.mode === 'box'}
          {selection.f0.toFixed(0)}–{selection.f1.toFixed(0)} Hz
        {:else}
          full band
        {/if}
      </span>
    </span>
    <span class="field" data-testid="readout-f0-mean" data-value={readout?.f0MeanHz ?? ''}>
      <span class="k">F0 mean</span><span class="v">{formatPitch(readout?.f0MeanHz, pitchUnit)}</span>
    </span>
    <span class="field" data-testid="readout-f0-range">
      <span class="k">F0 min/max</span><span class="v">{formatPitch(readout?.f0MinHz, pitchUnit)} / {formatPitch(readout?.f0MaxHz, pitchUnit)}</span>
    </span>
    <span class="field" data-testid="readout-f0-sd" data-value={readout?.f0SdHz ?? ''}>
      <span class="k">F0 SD</span><span class="v">{hz(readout?.f0SdHz, 1)}</span>
    </span>
    <span class="field" data-testid="readout-f0-quantile" data-value={readout?.f0P5Hz ?? ''}>
      <span class="k">F0 5–95%</span
      ><span class="v">{formatPitch(readout?.f0P5Hz, pitchUnit)} / {formatPitch(
          readout?.f0P95Hz,
          pitchUnit
        )}</span>
    </span>
    <span class="field" data-testid="readout-band-energy" data-value={readout?.bandEnergyDb ?? ''}>
      <span class="k">Band energy</span><span class="v">{db(readout?.bandEnergyDb)}</span>
    </span>
    <span class="field" data-testid="readout-rms" data-value={readout?.rms ?? ''}>
      <span class="k">RMS</span><span class="v">{amp(readout?.rms)}</span>
    </span>
    <span
      class="field"
      class:provisional={clipping}
      data-testid="readout-peak"
      data-value={readout?.peak ?? ''}
      title={clipping ? 'Peak near full scale — the selection may be clipping' : 'Absolute peak amplitude'}
    >
      <span class="k">Peak</span><span class="v">{amp(readout?.peak)}</span>
    </span>
    <span class="field" data-testid="readout-intensity">
      <span class="k">Intensity</span><span class="v">{db(readout?.intensityMeanDb)}</span>
    </span>
    <span class="field" data-testid="readout-intensity-sd" data-value={readout?.intensitySdDb ?? ''}>
      <span class="k">Int. SD</span><span class="v">{db(readout?.intensitySdDb)}</span>
    </span>
    <span class="field" data-testid="readout-hnr">
      <span class="k">HNR</span><span class="v">{db(readout?.hnrMeanDb)}</span>
    </span>
    <span
      class="field"
      data-testid="readout-cog"
      data-value={readout?.spectralCogHz ?? ''}
      title={`Spectral centre of gravity. SD ${hz(readout?.spectralSdHz, 0)}, skewness ${num(readout?.spectralSkewness)}, kurtosis ${num(readout?.spectralKurtosis)}`}
    >
      <span class="k">CoG</span><span class="v">{hz(readout?.spectralCogHz, 0)}</span>
    </span>
    {#if showFormants}
      <span class="field provisional" data-testid="readout-formants" title="Formant tracking weights are provisional (T2.6 open)">
        <span class="k">F1–F3<sup>*</sup></span>
        <span class="v">
          {#each visibleFormants as formant (formant.index)}
            {hz(Number.isFinite(formant.value) ? formant.value : null, 0)}{formant.index < visibleFormants.length ? ' / ' : ''}
          {/each}
        </span>
      </span>
    {/if}
  </div>

  <div class="actions">
    {#if showFormants}
      <span class="marker" data-testid="provisional-marker">* provisional tracking</span>
    {/if}
    {#if filteredPlaying}
      <span class="filtered" data-testid="filtered-indicator">
        <IconFilter aria-hidden="true" />
        <span>filtered {selection.f0.toFixed(0)}–{selection.f1.toFixed(0)} Hz</span>
      </span>
    {/if}
    <button type="button" data-testid="selection-play" onclick={onPlay}>
      {#if banded}
        <IconFilter aria-hidden="true" /><span>Play band</span>
      {:else}
        <IconPlay aria-hidden="true" /><span>Play</span>
      {/if}
    </button>
    <button type="button" data-testid="selection-zoom" onclick={onZoom}>
      <IconZoomIn aria-hidden="true" /><span>Zoom to</span>
    </button>
    <button type="button" data-testid="selection-spectrum" onclick={onSpectrum}>
      <IconAudioWaveform aria-hidden="true" /><span>Spectrum</span>
    </button>
    <button type="button" data-testid="selection-voice-report" onclick={onVoiceReport}>
      <IconActivity aria-hidden="true" /><span>Voice report</span>
    </button>
    {#if onFilter && banded}
      <button type="button" data-testid="selection-filter" onclick={onFilter}>
        <IconFilter aria-hidden="true" /><span>Filter</span>
      </button>
    {/if}
    {#if onExtract}
      <button type="button" data-testid="selection-extract" onclick={onExtract}>
        <IconScissors aria-hidden="true" /><span>Extract</span>
      </button>
    {/if}
    {#if onReverse}
      <button type="button" data-testid="selection-reverse" onclick={onReverse}>
        <IconFlipHorizontal2 aria-hidden="true" /><span>Reverse</span>
      </button>
    {/if}
    <button type="button" class="clear" data-testid="selection-clear" onclick={onClear}>
      <IconX aria-hidden="true" /><span>Clear</span>
    </button>
  </div>
</div>

<style>
  .readout {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    min-height: 2.1rem;
    padding: 0.3rem 0.75rem;
    border-bottom: 1px solid var(--chrome-strong);
    background: var(--panel);
    color: var(--text);
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }

  .fields {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    flex-wrap: wrap;
    min-width: 0;
  }

  .field {
    display: inline-flex;
    align-items: baseline;
    gap: 0.35rem;
  }

  .field .k {
    color: var(--muted);
    font-size: 0.72rem;
  }

  .field .v {
    color: var(--text);
  }

  .provisional .v {
    color: var(--warn);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .marker {
    color: var(--warn);
    font-size: 0.72rem;
    margin-right: 0.3rem;
  }

  .filtered {
    display: inline-flex;
    align-items: center;
    gap: 0.25rem;
    color: var(--accent-strong);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
    margin-right: 0.3rem;
  }

  .filtered :global(svg) {
    font-size: 0.8rem;
  }

  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    min-height: 1.6rem;
    padding: 0.22rem 0.55rem;
    font-size: 0.76rem;
    transition:
      background var(--t-fast),
      border-color var(--t-fast),
      color var(--t-fast);
  }

  .actions button :global(svg) {
    font-size: 0.85rem;
  }

  .actions button:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .actions .clear:hover {
    color: var(--danger);
    border-color: color-mix(in oklab, var(--danger) 40%, var(--chrome-strong));
  }
</style>
