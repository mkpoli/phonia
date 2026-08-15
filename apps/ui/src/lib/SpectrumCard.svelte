<script lang="ts">
  import IconAudioWaveform from '~icons/lucide/audio-waveform';
  import IconX from '~icons/lucide/x';
  import type { AudioInfo, CoreClientLike, SpectrumSliceData } from './types';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    /** Selection span in seconds to take the spectrum over. */
    t0: number;
    t1: number;
    /**
     * `spectrum` is one FFT over the span; `ltas` averages frame spectra;
     * `cepstrum` is the averaged quefrency-domain cepstrum.
     */
    mode?: 'spectrum' | 'ltas' | 'cepstrum';
    /**
     * Resolves the LPC-smoothed envelope over the span. When supplied and
     * `mode` is `spectrum`, the envelope overlays the FFT so its resonance
     * peaks read as formants.
     */
    onLpcEnvelope?: ((t0: number, t1: number) => Promise<SpectrumSliceData>) | null;
    onClose: () => void;
  }

  let { client, audio, t0, t1, mode = 'spectrum', onLpcEnvelope = null, onClose }: Props = $props();

  const isCepstrum = $derived(mode === 'cepstrum');
  const title = $derived(mode === 'ltas' ? 'LTAS' : mode === 'cepstrum' ? 'Cepstrum' : 'Spectrum');
  const LPC_COLOR = '#f2a33c';

  let data = $state<SpectrumSliceData | null>(null);
  let lpc = $state<SpectrumSliceData | null>(null);
  let showLpc = $state(true);
  let loading = $state(true);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let hover = $state<{ hz: number; db: number } | null>(null);

  const lpcActive = $derived(mode === 'spectrum' && onLpcEnvelope !== null);

  function cssVar(name: string, fallback: string): string {
    if (typeof window === 'undefined' || !canvas) return fallback;
    const v = getComputedStyle(canvas).getPropertyValue(name).trim();
    return v || fallback;
  }

  $effect(() => {
    if (!client || !audio) {
      loading = false;
      return;
    }
    let cancelled = false;
    loading = true;
    const request = isCepstrum
      ? client.cepstrumSlice(audio.id, t0, t1)
      : mode === 'ltas'
        ? client.ltas(audio.id, t0, t1)
        : client.spectrumSlice(audio.id, t0, t1);
    request
      .then((result) => {
        if (!cancelled) {
          data = result;
          loading = false;
        }
      })
      .catch(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    if (!lpcActive || !audio || !onLpcEnvelope) {
      lpc = null;
      return;
    }
    let cancelled = false;
    onLpcEnvelope(t0, t1)
      .then((result) => {
        if (!cancelled) lpc = result;
      })
      .catch(() => {
        if (!cancelled) lpc = null;
      });
    return () => {
      cancelled = true;
    };
  });

  function peakDb(values: number[]): number | null {
    let hi = -Infinity;
    for (const v of values) {
      if (Number.isFinite(v) && v > hi) hi = v;
    }
    return Number.isFinite(hi) ? hi : null;
  }

  // The envelope level-matched to the FFT so peaks sit on the raw spectrum: a
  // single offset aligns their maxima, since the LPC gain carries no absolute
  // reference the FFT shares.
  const alignedLpc = $derived.by(() => {
    if (!showLpc || !lpcActive || !lpc || !data || lpc.db.length === 0) return null;
    const fftPeak = peakDb(data.db);
    const lpcPeak = peakDb(lpc.db);
    if (fftPeak === null || lpcPeak === null) return null;
    const offset = fftPeak - lpcPeak;
    return { freqs: lpc.freqs, db: lpc.db.map((v) => v + offset) };
  });

  // dB extent, padded, for the vertical scale — folds in the aligned envelope so
  // its valleys are not clipped off the bottom of the plot.
  const range = $derived.by(() => {
    if (!data || data.db.length === 0) return null;
    let lo = Infinity;
    let hi = -Infinity;
    for (const v of data.db) {
      if (!Number.isFinite(v)) continue;
      if (v < lo) lo = v;
      if (v > hi) hi = v;
    }
    if (alignedLpc) {
      for (const v of alignedLpc.db) {
        if (!Number.isFinite(v)) continue;
        if (v < lo) lo = v;
        if (v > hi) hi = v;
      }
    }
    if (!Number.isFinite(lo) || !Number.isFinite(hi)) return null;
    const maxHz = data.freqs.length ? data.freqs[data.freqs.length - 1] : 1;
    return { lo: lo - 2, hi: hi + 2, maxHz };
  });

  function draw() {
    const ctx = canvas?.getContext('2d');
    if (!ctx || !canvas || !data || !range) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    ctx.fillStyle = cssVar('--canvas', '#0b0f14');
    ctx.fillRect(0, 0, w, h);

    const padL = 44;
    const padB = 22;
    const plotW = w - padL - 8;
    const plotH = h - padB - 8;
    const xOf = (hz: number) => padL + (hz / range.maxHz) * plotW;
    const yOf = (db: number) => 8 + (1 - (db - range.lo) / (range.hi - range.lo)) * plotH;

    // Grid + axis ticks: quefrency in ms for the cepstrum, else every 1 kHz.
    ctx.strokeStyle = cssVar('--chrome-strong', '#233');
    ctx.fillStyle = cssVar('--muted', '#8aa');
    ctx.lineWidth = 1;
    ctx.font = '10px system-ui, sans-serif';
    ctx.textAlign = 'center';
    const gridline = (x: number, label: string) => {
      ctx.globalAlpha = 0.25;
      ctx.beginPath();
      ctx.moveTo(x, 8);
      ctx.lineTo(x, 8 + plotH);
      ctx.stroke();
      ctx.globalAlpha = 1;
      ctx.fillText(label, x, h - 8);
    };
    if (isCepstrum) {
      const maxMs = range.maxHz * 1000;
      for (let ms = 0; ms <= maxMs; ms += 5) gridline(xOf(ms / 1000), `${ms} ms`);
    } else {
      for (let hz = 0; hz <= range.maxHz; hz += 1000) gridline(xOf(hz), `${hz / 1000}k`);
    }

    // Spectrum curve.
    ctx.strokeStyle = cssVar('--accent', '#2dd4bf');
    ctx.lineWidth = 1.25;
    ctx.beginPath();
    for (let i = 0; i < data.freqs.length; i += 1) {
      const x = xOf(data.freqs[i]);
      const y = yOf(data.db[i]);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    }
    ctx.stroke();

    // LPC envelope, level-matched, traced over the raw spectrum.
    if (alignedLpc) {
      ctx.strokeStyle = LPC_COLOR;
      ctx.lineWidth = 2;
      ctx.beginPath();
      for (let i = 0; i < alignedLpc.freqs.length; i += 1) {
        const x = xOf(alignedLpc.freqs[i]);
        const y = yOf(alignedLpc.db[i]);
        if (i === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
    }

    // Hover cursor.
    if (hover) {
      const x = xOf(hover.hz);
      ctx.strokeStyle = cssVar('--accent-strong', '#0ea5a0');
      ctx.globalAlpha = 0.7;
      ctx.beginPath();
      ctx.moveTo(x, 8);
      ctx.lineTo(x, 8 + plotH);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }
  }

  $effect(() => {
    // Redraw when data, range, hover, the envelope, or the element changes.
    void data;
    void range;
    void hover;
    void alignedLpc;
    if (canvas) draw();
  });

  function onMove(event: MouseEvent) {
    if (!canvas || !data || !range) return;
    const rect = canvas.getBoundingClientRect();
    const padL = 44;
    const plotW = rect.width - padL - 8;
    const rel = (event.clientX - rect.left - padL) / plotW;
    if (rel < 0 || rel > 1) {
      hover = null;
      return;
    }
    const hz = rel * range.maxHz;
    // Nearest bin for the dB readout.
    const bin = Math.min(data.freqs.length - 1, Math.round((hz / range.maxHz) * (data.freqs.length - 1)));
    hover = { hz: data.freqs[bin], db: data.db[bin] };
  }
</script>

<div class="backdrop" data-testid="spectrum-card">
  <div class="card" role="dialog" aria-modal="true" aria-label={title}>
    <header>
      <h2><IconAudioWaveform aria-hidden="true" />{title}</h2>
      <span class="span">{t0.toFixed(3)}–{t1.toFixed(3)} s</span>
      <span class="readout" data-testid="spectrum-readout">
        {#if hover}
          {#if isCepstrum}{(hover.hz * 1000).toFixed(2)} ms · {hover.db.toFixed(3)}{:else}{Math.round(
              hover.hz
            )} Hz · {hover.db.toFixed(1)} dB{/if}
        {:else}hover to read{/if}
      </span>
      {#if lpcActive}
        <button
          type="button"
          class="lpc-toggle"
          class:on={showLpc}
          data-testid="spectrum-lpc-toggle"
          onclick={() => (showLpc = !showLpc)}
          aria-pressed={showLpc}
          title="Show or hide the LPC-smoothed envelope"
        >
          <span class="dot" style:background={LPC_COLOR}></span>LPC
        </button>
      {/if}
      <button type="button" class="close" data-testid="spectrum-close" onclick={onClose} aria-label="Close">
        <IconX aria-hidden="true" />
      </button>
    </header>
    {#if loading}
      <p class="status" data-testid="spectrum-loading">Computing…</p>
    {:else if !data || data.db.length === 0}
      <p class="status">No spectrum for this selection.</p>
    {:else}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <canvas
        bind:this={canvas}
        class="plot"
        data-testid="spectrum-canvas"
        onmousemove={onMove}
        onmouseleave={() => (hover = null)}
      ></canvas>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: rgba(15, 23, 42, 0.42);
    backdrop-filter: blur(3px);
    -webkit-backdrop-filter: blur(3px);
    z-index: 30;
  }

  .card {
    width: min(44rem, calc(100vw - 2rem));
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md, 8px);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    overflow: hidden;
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.6rem 0.9rem;
    border-bottom: 1px solid var(--chrome-strong);
  }

  header h2 {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    margin: 0;
    font-size: 0.95rem;
  }

  .span {
    color: var(--muted);
    font-size: 0.8rem;
  }

  .readout {
    margin-left: auto;
    color: var(--accent-strong, var(--accent));
    font-size: 0.8rem;
    font-variant-numeric: tabular-nums;
  }

  .lpc-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.15rem 0.5rem;
    border: 1px solid var(--chrome-strong);
    border-radius: 999px;
    background: transparent;
    color: var(--muted);
    font-size: 0.72rem;
    letter-spacing: 0.02em;
    cursor: pointer;
  }

  .lpc-toggle.on {
    color: var(--text);
    border-color: color-mix(in srgb, #f2a33c 60%, var(--chrome-strong));
  }

  .lpc-toggle .dot {
    width: 0.55rem;
    height: 0.55rem;
    border-radius: 50%;
    opacity: 0.35;
  }

  .lpc-toggle.on .dot {
    opacity: 1;
  }

  .close {
    display: grid;
    place-items: center;
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .close:hover {
    color: var(--text);
  }

  .status {
    padding: 2rem;
    text-align: center;
    color: var(--muted);
  }

  .plot {
    display: block;
    width: 100%;
    height: 18rem;
    background: var(--canvas);
    cursor: crosshair;
  }
</style>
