<script lang="ts">
  import IconLayers from '~icons/lucide/layers';
  import IconEye from '~icons/lucide/eye';
  import IconEyeOff from '~icons/lucide/eye-off';
  import IconChevronRight from '~icons/lucide/chevron-right';
  import IconClipboard from '~icons/lucide/clipboard-copy';
  import IconCheck from '~icons/lucide/check';
  import IconX from '~icons/lucide/x';
  import type { OverlayParams, OverlayStats, SelectionReadout } from './types';
  import type { TrackSample } from './tracks';
  import { formatPitch, PITCH_UNITS } from './pitch-units';

  interface Props {
    params: OverlayParams;
    stats: OverlayStats;
    /** Selection readout, for each layer's live value. Null with no selection. */
    readout?: SelectionReadout | null;
    /** Provisional tracked-formant means over the selection (F1, F2, …). */
    formantMeans?: number[] | null;
    /** Mean formant bandwidths over the selection (B1, B2, …), paired with
     *  `formantMeans` by slot. */
    formantBandwidths?: number[] | null;
    /** Per-slot formant frequency range over the selection, or null when the
     *  layer is off or no selection is active. */
    formantStats?: { slot: number; minHz: number; maxHz: number }[] | null;
    /** Intensity extrema over the selection, or null with no selection. */
    intensityStats?: {
      maxDb: number;
      maxTime: number;
      minDb: number;
      minTime: number;
    } | null;
    /** Harmonicity (HNR) extrema over the selection, or null when the layer is
     *  off or no selection is active. */
    harmonicityStats?: {
      maxDb: number;
      maxTime: number;
      minDb: number;
      minTime: number;
    } | null;
    /** CPP extrema over the selection, or null when the layer is off or no
     *  selection is active. */
    cppStats?: {
      maxDb: number;
      maxTime: number;
      minDb: number;
      minTime: number;
    } | null;
    /** Pitch (F0) extrema over the selection, or null when the layer is off or
     *  no selection is active. */
    pitchStats?: {
      maxHz: number;
      maxTime: number;
      minHz: number;
      minTime: number;
    } | null;
    /** Track values at the playhead; each layer's live value when no
     *  selection readout supplies one. */
    cursor?: TrackSample | null;
    /** Playhead time in seconds, for the copy-at-cursor row. */
    cursorTime?: number;
    onClose?: () => void;
  }

  let {
    params,
    stats,
    readout = null,
    formantMeans = null,
    formantBandwidths = null,
    formantStats = null,
    intensityStats = null,
    harmonicityStats = null,
    cppStats = null,
    pitchStats = null,
    cursor = null,
    cursorTime = 0,
    onClose
  }: Props = $props();

  let cursorCopied = $state(false);

  function tsvCell(value: number | null | undefined, digits: number): string {
    return value === null || value === undefined || !Number.isFinite(value) ? '' : value.toFixed(digits);
  }

  // A tab-separated point measurement at the playhead — time, F0, F1–F4 with
  // their bandwidths, and intensity — so rows accumulate cleanly in a sheet.
  async function copyCursor() {
    const f = cursor?.formants ?? [];
    const cols = [tsvCell(cursorTime, 4), tsvCell(cursor?.f0Hz, 1)];
    for (let i = 0; i < 4; i += 1) {
      cols.push(tsvCell(f[i]?.frequencyHz, 0), tsvCell(f[i]?.bandwidthHz, 0));
    }
    cols.push(tsvCell(cursor?.intensityDb, 1));
    try {
      await navigator.clipboard.writeText(cols.join('\t'));
      cursorCopied = true;
      setTimeout(() => (cursorCopied = false), 1500);
    } catch {
      cursorCopied = false;
    }
  }

  // A ceiling clips the data once a tracked value crowds it. Tracked maxima
  // sit a little under the ceiling even when the true value is cut off, so the
  // badge fires within 5% of the ceiling (ux.md: values crowding the ceiling
  // get a warning badge).
  let pitchClipped = $derived(
    stats.pitchMaxHz > 0 && stats.pitchMaxHz >= params.pitch.ceilingHz * 0.95
  );
  let formantClipped = $derived(
    stats.formantMaxHz > 0 && stats.formantMaxHz >= params.formant.ceilingHz * 0.95
  );

  // A connected track asserts that consecutive candidates are the same
  // formant. Only the Viterbi-smoothed track carries that assignment; raw
  // Burg candidates are just "N loudest resonances this frame" with no
  // cross-frame identity, so drawing a line through them would claim a
  // tracking decision nothing made. Turning smoothing off while a track is
  // selected falls back to speckles rather than silently rendering a track
  // the data no longer supports.
  $effect(() => {
    if (!params.formant.smoothed && params.formant.mark === 'track') {
      params.formant.mark = 'speckle';
    }
  });

  let expanded = $state({
    pitch: true,
    formant: true,
    intensity: true,
    harmonicity: false,
    cpp: false,
    spectrogram: false
  });

  let visibleCount = $derived(
    Number(params.pitch.show) +
      Number(params.formant.show) +
      Number(params.intensity.show) +
      Number(params.harmonicity.show) +
      Number(params.cpp.show)
  );

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

  // Raw Burg candidates carry no cross-frame identity (that is exactly why
  // speckles, not a line, are the default mark), so there is no single
  // current F1/F2 to report unless the smoothed track supplies one. At the
  // playhead the two lowest candidates of that frame stand in.
  let formantLive = $derived.by(() => {
    if (formantMeans && formantMeans.length >= 2) {
      return `${formantMeans[0].toFixed(0)} · ${formantMeans[1].toFixed(0)} Hz`;
    }
    if (!readout && cursor && cursor.formantsHz.length >= 2) {
      return `${cursor.formantsHz[0].toFixed(0)} · ${cursor.formantsHz[1].toFixed(0)} Hz`;
    }
    return '—';
  });

  // Per-slot mean formant frequency and bandwidth over the selection.
  const formantMeanRows = $derived(
    (formantMeans ?? [])
      .map((freq, i) => ({ slot: i + 1, freq, bw: formantBandwidths?.[i] ?? Number.NaN }))
      .filter((row) => Number.isFinite(row.freq))
  );
</script>

<aside class="inspector" data-testid="inspector" aria-label="Analysis layers">
  <header class="head">
    <h2><IconLayers aria-hidden="true" />Layers</h2>
    <span class="count">{visibleCount}/5 visible</span>
    <button
      type="button"
      class="copy-cursor"
      data-testid="copy-cursor"
      title="Copy values at cursor: time, F0, F1–F4 + bandwidths, intensity (tab-separated)"
      aria-label="Copy values at cursor"
      onclick={copyCursor}
    >
      {#if cursorCopied}<IconCheck aria-hidden="true" />{:else}<IconClipboard aria-hidden="true" />{/if}
    </button>
    {#if onClose}
      <button type="button" class="close" aria-label="Close inspector" onclick={onClose}>
        <IconX aria-hidden="true" />
      </button>
    {/if}
  </header>

  <section class="layer" data-testid="inspector-spectrogram">
    <div class="layer-head">
      <button
        type="button"
        class="layer-name"
        aria-expanded={expanded.spectrogram}
        onclick={() => (expanded.spectrogram = !expanded.spectrogram)}
      >
        <span class="chev" class:open={expanded.spectrogram}
          ><IconChevronRight aria-hidden="true" /></span
        >
        Spectrogram
      </button>
      <span class="live-value"
        >{params.spectrogram.windowLength <= 0.01 ? 'wideband' : 'narrowband'}</span
      >
    </div>
    {#if expanded.spectrogram}
      <div class="params">
        <div class="field">
          <div class="label-row"><span>Window</span><span class="unit">s</span></div>
          <input
            type="number"
            min="0.001"
            max="0.05"
            step="0.001"
            data-testid="spectrogram-window"
            bind:value={params.spectrogram.windowLength}
          />
          <p class="note">
            Gaussian analysis window. Short (~0.005 s) resolves time (wideband); long
            (~0.03 s) resolves frequency (narrowband).
          </p>
        </div>
        <div class="field">
          <div class="label-row"><span>Dynamic range</span><span class="unit">dB</span></div>
          <input
            type="number"
            min="20"
            max="120"
            step="5"
            data-testid="spectrogram-dynamic-range"
            bind:value={params.spectrogram.dynamicRangeDb}
          />
          <p class="note">Levels this far below the peak render as the floor colour.</p>
        </div>
        <div class="field">
          <label class="toggle inline">
            <input
              type="checkbox"
              data-testid="spectrogram-preemphasis"
              bind:checked={params.spectrogram.preemphasis}
            />
            <span>Pre-emphasis</span>
          </label>
          <p class="note">
            Lifts the display +6 dB/octave above 1 kHz so the upper formants read
            more clearly. The raw dB and cursor readouts are unchanged.
          </p>
        </div>
        <div class="field">
          <div class="label-row"><span>Window shape</span></div>
          <select data-testid="spectrogram-window-shape" bind:value={params.spectrogram.windowShape}>
            <option value="gaussian">Gaussian</option>
            <option value="hanning">Hanning</option>
          </select>
          <p class="note">
            Gaussian is Praat's default. Hanning trades a little frequency
            resolution for sharper time detail.
          </p>
        </div>
      </div>
    {/if}
  </section>

  <section class="layer" class:off={!params.pitch.show} data-testid="inspector-pitch">
    <div class="layer-head">
      <button
        type="button"
        class="eye"
        data-testid="toggle-pitch"
        aria-pressed={params.pitch.show}
        aria-label={params.pitch.show ? 'Hide pitch' : 'Show pitch'}
        title={params.pitch.show ? 'Hide pitch' : 'Show pitch'}
        onclick={() => (params.pitch.show = !params.pitch.show)}
      >
        {#if params.pitch.show}<IconEye aria-hidden="true" />{:else}<IconEyeOff aria-hidden="true" />{/if}
      </button>
      <span class="swatch pitch"></span>
      <button
        type="button"
        class="layer-name"
        aria-expanded={expanded.pitch}
        onclick={() => (expanded.pitch = !expanded.pitch)}
      >
        <span class="chev" class:open={expanded.pitch}><IconChevronRight aria-hidden="true" /></span>
        Pitch
      </button>
      <span
        class="live-value"
        title={readout ? 'Mean F0 over the selection' : 'F0 interpolated at the cursor time'}
        >{formatPitch(readout ? readout.f0MeanHz : cursor?.f0Hz, params.pitch.unit)}</span
      >
    </div>
    {#if pitchStats}
      <div class="extrema" data-testid="pitch-extrema">
        <span data-testid="pitch-max"
          >Max {formatPitch(pitchStats.maxHz, params.pitch.unit)} at {pitchStats.maxTime.toFixed(
            3
          )} s</span
        >
        <span data-testid="pitch-min"
          >Min {formatPitch(pitchStats.minHz, params.pitch.unit)} at {pitchStats.minTime.toFixed(
            3
          )} s</span
        >
      </div>
    {/if}
    {#if expanded.pitch}
      <div class="params">
        <div class="field">
          <div class="label-row"><span>Floor</span><span class="unit">Hz</span></div>
          <input
            type="number"
            min="20"
            max="600"
            step="5"
            data-testid="pitch-floor"
            bind:value={params.pitch.floorHz}
          />
          <p class="note">Default 75 Hz — Praat raw-autocorrelation floor.</p>
        </div>
        <div class="field">
          <div class="label-row">
            <span>Ceiling</span>
            {#if pitchClipped}
              <span class="badge" data-testid="pitch-clip-badge" title="Tracked pitch reaches the ceiling"
                >clips ~{Math.round(stats.pitchMaxHz)} Hz</span
              >
            {:else}
              <span class="unit">Hz</span>
            {/if}
          </div>
          <input
            type="number"
            min="100"
            max="2000"
            step="10"
            data-testid="pitch-ceiling"
            bind:value={params.pitch.ceilingHz}
          />
          <p class="note">Default 600 Hz — Praat. Lower toward 300 Hz for male speech.</p>
        </div>
        <div class="field">
          <div class="label-row"><span>Voicing threshold</span></div>
          <input
            type="number"
            min="0"
            max="1"
            step="0.05"
            data-testid="pitch-voicing-threshold"
            bind:value={params.pitch.voicingThreshold}
          />
          <p class="note">
            Default 0.45 — Praat. Raise it to mark more frames unvoiced (fewer
            octave errors on creaky or breathy voice); lower it to keep faint pitch.
          </p>
        </div>
        <div class="field">
          <div class="label-row"><span>Unit</span></div>
          <select data-testid="pitch-unit" bind:value={params.pitch.unit}>
            {#each PITCH_UNITS as u (u.value)}
              <option value={u.value}>{u.label}</option>
            {/each}
          </select>
          <p class="note">Display unit for F0 readouts. Semitones are re 100 Hz, Praat's default.</p>
        </div>
        <div class="field">
          <label class="toggle inline">
            <input type="checkbox" data-testid="pitch-pulses" bind:checked={params.pulses.show} />
            <span>Glottal pulses</span>
          </label>
          <p class="note">
            Marks each glottal pulse on the waveform — the point process jitter, shimmer, and HNR
            are measured from.
          </p>
        </div>
      </div>
    {/if}
  </section>

  <section class="layer" class:off={!params.formant.show} data-testid="inspector-formant">
    <div class="layer-head">
      <button
        type="button"
        class="eye"
        data-testid="toggle-formant"
        aria-pressed={params.formant.show}
        aria-label={params.formant.show ? 'Hide formants' : 'Show formants'}
        title={params.formant.show ? 'Hide formants' : 'Show formants'}
        onclick={() => (params.formant.show = !params.formant.show)}
      >
        {#if params.formant.show}<IconEye aria-hidden="true" />{:else}<IconEyeOff aria-hidden="true" />{/if}
      </button>
      <span class="swatch formant"></span>
      <button
        type="button"
        class="layer-name"
        aria-expanded={expanded.formant}
        onclick={() => (expanded.formant = !expanded.formant)}
      >
        <span class="chev" class:open={expanded.formant}><IconChevronRight aria-hidden="true" /></span>
        Formants
      </button>
      <span class="live-value">{formantLive}</span>
    </div>
    {#if formantStats}
      <div class="extrema" data-testid="formant-extrema">
        {#each formantStats as slot (slot.slot)}
          <span data-testid="formant-extrema-row"
            >F{slot.slot} {Math.round(slot.minHz)}–{Math.round(slot.maxHz)} Hz</span
          >
        {/each}
      </div>
    {/if}
    {#if expanded.formant}
      <div class="params">
        {#if formantMeanRows.length > 0}
          <table class="formant-table" data-testid="formant-mean-table">
            <thead>
              <tr
                ><th scope="col">over selection</th><th scope="col">Freq</th><th scope="col">Bw</th
                ></tr
              >
            </thead>
            <tbody>
              {#each formantMeanRows as row (row.slot)}
                <tr data-testid="formant-mean-row">
                  <td class="fk">F{row.slot}</td>
                  <td>{hz(row.freq)}</td>
                  <td>{hz(row.bw)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else if cursor && cursor.formants.length > 0}
          <table class="formant-table" data-testid="formant-table">
            <thead>
              <tr><th scope="col">at cursor</th><th scope="col">Freq</th><th scope="col">Bw</th></tr>
            </thead>
            <tbody>
              {#each cursor.formants as formant, i (i)}
                <tr data-testid="formant-row">
                  <td class="fk">F{i + 1}</td>
                  <td>{hz(formant.frequencyHz)}</td>
                  <td>{hz(formant.bandwidthHz)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
        <div class="field">
          <div class="label-row">
            <span>Ceiling</span>
            {#if formantClipped}
              <span class="badge" data-testid="formant-clip-badge" title="A tracked formant reaches the ceiling"
                >clips ~{Math.round(stats.formantMaxHz)} Hz</span
              >
            {:else}
              <span class="unit">Hz</span>
            {/if}
          </div>
          <input
            type="number"
            min="1000"
            max="12000"
            step="100"
            data-testid="formant-ceiling"
            bind:value={params.formant.ceilingHz}
          />
          <p class="note">Default 5500 Hz — Praat adult-female. Use 5000 Hz for adult male.</p>
        </div>
        <div class="field">
          <div class="label-row"><span>Max formants</span></div>
          <input
            type="number"
            min="1"
            max="7"
            step="1"
            data-testid="formant-count"
            bind:value={params.formant.maxFormants}
          />
          <p class="note">Default 5 — Praat.</p>
        </div>
        <div class="field">
          <label class="toggle inline">
            <input type="checkbox" data-testid="formant-smoothed" bind:checked={params.formant.smoothed} />
            <span>Tracked (provisional)</span>
          </label>
          <p class="note">
            Off shows raw Burg candidates. On runs Xia–Espy-Wilson smoothing, whose weights stay
            provisional until gate review.
          </p>
        </div>
        <div class="field">
          <div class="label-row"><span>Mark</span></div>
          <select data-testid="formant-mark" bind:value={params.formant.mark}>
            <option value="speckle">Speckles</option>
            <option value="track" disabled={!params.formant.smoothed}>Connected tracks</option>
          </select>
          {#if params.formant.smoothed}
            <p class="note">
              Speckles are the Praat-familiar dot-per-candidate view, sized by bandwidth. Tracks
              connect each formant across time and break wherever a frame has no candidate for it,
              rather than drawing through the gap.
            </p>
          {:else}
            <p class="note">
              Connected tracks needs Tracked (provisional) on: raw candidates carry no identity
              from one frame to the next, so a line through them would join points that were never
              the same formant.
            </p>
          {/if}
        </div>
      </div>
    {/if}
  </section>

  <section class="layer" class:off={!params.intensity.show} data-testid="inspector-intensity">
    <div class="layer-head">
      <button
        type="button"
        class="eye"
        data-testid="toggle-intensity"
        aria-pressed={params.intensity.show}
        aria-label={params.intensity.show ? 'Hide intensity' : 'Show intensity'}
        title={params.intensity.show ? 'Hide intensity' : 'Show intensity'}
        onclick={() => (params.intensity.show = !params.intensity.show)}
      >
        {#if params.intensity.show}<IconEye aria-hidden="true" />{:else}<IconEyeOff aria-hidden="true" />{/if}
      </button>
      <span class="swatch intensity"></span>
      <button
        type="button"
        class="layer-name"
        aria-expanded={expanded.intensity}
        onclick={() => (expanded.intensity = !expanded.intensity)}
      >
        <span class="chev" class:open={expanded.intensity}><IconChevronRight aria-hidden="true" /></span>
        Intensity
      </button>
      <span
        class="live-value"
        title={readout ? 'Mean intensity over the selection' : 'Intensity interpolated at the cursor time'}
        >{db(readout ? readout.intensityMeanDb : cursor?.intensityDb)}</span
      >
    </div>
    {#if intensityStats}
      <div class="extrema" data-testid="intensity-extrema">
        <span data-testid="intensity-max"
          >Max {db(intensityStats.maxDb)} at {intensityStats.maxTime.toFixed(3)} s</span
        >
        <span data-testid="intensity-min"
          >Min {db(intensityStats.minDb)} at {intensityStats.minTime.toFixed(3)} s</span
        >
      </div>
    {/if}
    {#if expanded.intensity}
      <div class="params">
        <div class="field">
          <div class="label-row"><span>Floor</span><span class="unit">Hz</span></div>
          <input
            type="number"
            min="20"
            max="400"
            step="10"
            data-testid="intensity-floor"
            bind:value={params.intensity.floorHz}
          />
          <p class="note">Default 100 Hz — Praat pitch floor sets the intensity window length.</p>
        </div>
      </div>
    {/if}
  </section>

  <section
    class="layer"
    class:off={!params.harmonicity.show}
    data-testid="inspector-harmonicity"
  >
    <div class="layer-head">
      <button
        type="button"
        class="eye"
        data-testid="toggle-harmonicity"
        aria-pressed={params.harmonicity.show}
        aria-label={params.harmonicity.show ? 'Hide harmonicity' : 'Show harmonicity'}
        title={params.harmonicity.show ? 'Hide harmonicity' : 'Show harmonicity'}
        onclick={() => (params.harmonicity.show = !params.harmonicity.show)}
      >
        {#if params.harmonicity.show}<IconEye aria-hidden="true" />{:else}<IconEyeOff
            aria-hidden="true"
          />{/if}
      </button>
      <span class="swatch harmonicity"></span>
      <button
        type="button"
        class="layer-name"
        aria-expanded={expanded.harmonicity}
        onclick={() => (expanded.harmonicity = !expanded.harmonicity)}
      >
        <span class="chev" class:open={expanded.harmonicity}
          ><IconChevronRight aria-hidden="true" /></span
        >
        Harmonicity
      </button>
      <span
        class="live-value"
        title={readout ? 'Mean HNR over the selection' : 'HNR at the cursor'}
        >{db(readout ? readout.hnrMeanDb : cursor?.hnrDb)}</span
      >
    </div>
    {#if harmonicityStats}
      <div class="extrema" data-testid="harmonicity-extrema">
        <span data-testid="harmonicity-max"
          >Max {db(harmonicityStats.maxDb)} at {harmonicityStats.maxTime.toFixed(3)} s</span
        >
        <span data-testid="harmonicity-min"
          >Min {db(harmonicityStats.minDb)} at {harmonicityStats.minTime.toFixed(3)} s</span
        >
      </div>
    {/if}
    {#if expanded.harmonicity}
      <div class="params">
        <div class="field">
          <div class="label-row"><span>Floor</span><span class="unit">Hz</span></div>
          <input
            type="number"
            min="40"
            max="400"
            step="5"
            data-testid="harmonicity-floor"
            bind:value={params.harmonicity.floorHz}
          />
          <p class="note">Lowest searched F0 — Praat's harmonicity floor, default 75 Hz.</p>
        </div>
      </div>
    {/if}
  </section>

  <section class="layer" class:off={!params.cpp.show} data-testid="inspector-cpp">
    <div class="layer-head">
      <button
        type="button"
        class="eye"
        data-testid="toggle-cpp"
        aria-pressed={params.cpp.show}
        aria-label={params.cpp.show ? 'Hide CPP' : 'Show CPP'}
        title={params.cpp.show ? 'Hide CPP' : 'Show CPP'}
        onclick={() => (params.cpp.show = !params.cpp.show)}
      >
        {#if params.cpp.show}<IconEye aria-hidden="true" />{:else}<IconEyeOff
            aria-hidden="true"
          />{/if}
      </button>
      <span class="swatch cpp"></span>
      <button
        type="button"
        class="layer-name"
        aria-expanded={expanded.cpp}
        onclick={() => (expanded.cpp = !expanded.cpp)}
      >
        <span class="chev" class:open={expanded.cpp}><IconChevronRight aria-hidden="true" /></span>
        CPP
      </button>
      <span class="live-value" title="Cepstral peak prominence at the cursor"
        >{db(cursor?.cppDb)}</span
      >
    </div>
    {#if cppStats}
      <div class="extrema" data-testid="cpp-extrema">
        <span data-testid="cpp-max"
          >Max {db(cppStats.maxDb)} at {cppStats.maxTime.toFixed(3)} s</span
        >
        <span data-testid="cpp-min"
          >Min {db(cppStats.minDb)} at {cppStats.minTime.toFixed(3)} s</span
        >
      </div>
    {/if}
    {#if expanded.cpp}
      <div class="params">
        <p class="note">
          Cepstral peak prominence — the harmonic peak's height over the cepstral
          baseline. Higher marks clearer voicing; lower marks breathiness.
        </p>
      </div>
    {/if}
  </section>
</aside>

<style>
  .inspector {
    width: 17rem;
    min-width: 17rem;
    height: 100%;
    overflow-y: auto;
    border-left: 1px solid var(--chrome-strong);
    background: var(--panel);
    color: var(--text);
    padding: 0.75rem 0.85rem 1.5rem;
    font-size: 0.85rem;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: -0.75rem -0.85rem 0.75rem;
    padding: 0.6rem 0.85rem;
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
    color: var(--muted);
  }

  .head .count {
    color: var(--muted);
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
  }

  .close,
  .copy-cursor {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    font-size: 1rem;
    line-height: 1;
    padding: 0.2rem;
    transition:
      background var(--t-fast),
      color var(--t-fast);
  }

  .copy-cursor {
    margin-left: auto;
    font-size: 0.9rem;
  }

  .close:hover,
  .copy-cursor:hover {
    background: var(--panel-soft);
    color: var(--text);
  }

  .layer {
    padding: 0.5rem 0;
    border-top: 1px solid var(--chrome-strong);
  }

  .layer-head {
    display: flex;
    align-items: center;
    gap: 0.45rem;
  }

  .eye {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 1.6rem;
    height: 1.6rem;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--accent);
    transition:
      background var(--t-fast),
      color var(--t-fast);
  }

  .eye:hover {
    background: var(--accent-tint);
  }

  .layer.off .eye {
    color: var(--muted);
  }

  .swatch {
    flex: none;
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 2px;
    box-shadow: 0 0 0 1px rgba(4, 8, 16, 0.5);
  }

  .layer.off .swatch {
    opacity: 0.35;
  }

  .swatch.pitch {
    background: var(--overlay-pitch);
  }

  .swatch.formant {
    background: var(--overlay-formant);
  }

  .swatch.intensity {
    background: var(--overlay-intensity);
  }

  .swatch.harmonicity {
    background: var(--overlay-harmonicity);
  }

  .swatch.cpp {
    background: var(--overlay-cpp);
  }

  .layer-name {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text);
    font-weight: 600;
    text-align: left;
    padding: 0;
  }

  .layer.off .layer-name {
    color: var(--muted);
  }

  .chev {
    display: inline-flex;
    flex: none;
    color: var(--muted);
    transition: transform var(--t-fast);
  }

  .chev.open {
    transform: rotate(90deg);
  }

  .live-value {
    flex: none;
    color: var(--text);
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }

  .layer.off .live-value {
    color: var(--muted);
  }

  .params {
    padding-left: 2.05rem;
  }

  .formant-table {
    width: 100%;
    border-collapse: collapse;
    margin-bottom: 0.6rem;
    font-variant-numeric: tabular-nums;
    font-size: 0.76rem;
  }

  .formant-table th {
    text-align: right;
    font-weight: 500;
    color: var(--muted);
    padding: 0.1rem 0.35rem;
  }

  .formant-table th:first-child {
    text-align: left;
  }

  .formant-table td {
    text-align: right;
    padding: 0.1rem 0.35rem;
    color: var(--text);
  }

  .formant-table .fk {
    text-align: left;
    color: var(--muted);
  }

  .toggle.inline {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    font-weight: 500;
  }

  .field {
    margin: 0.5rem 0;
  }

  .label-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.2rem;
  }

  .unit {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  input[type='number'] {
    width: 100%;
    min-height: 2rem;
    padding: 0.32rem 0.45rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font-variant-numeric: tabular-nums;
    transition:
      border-color var(--t-fast),
      box-shadow var(--t-fast);
  }

  input[type='number']:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 20%, transparent);
  }

  select {
    width: 100%;
    min-height: 2rem;
    padding: 0.32rem 0.45rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    transition:
      border-color var(--t-fast),
      box-shadow var(--t-fast);
  }

  select:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 20%, transparent);
  }

  .note {
    margin: 0.28rem 0 0;
    color: var(--muted);
    font-size: 0.72rem;
    line-height: 1.35;
  }

  .extrema {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    padding: 0.05rem 0 0.3rem;
    color: var(--muted);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
  }

  .badge {
    padding: 0.05rem 0.4rem;
    border-radius: 999px;
    background: color-mix(in oklab, var(--warn), transparent 78%);
    color: var(--warn);
    font-size: 0.7rem;
    font-weight: 600;
  }
</style>
