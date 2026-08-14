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
    /** `spectrum` is one FFT over the span; `ltas` averages frame spectra. */
    mode?: 'spectrum' | 'ltas';
    onClose: () => void;
  }

  let { client, audio, t0, t1, mode = 'spectrum', onClose }: Props = $props();

  const title = $derived(mode === 'ltas' ? 'LTAS' : 'Spectrum');

  let data = $state<SpectrumSliceData | null>(null);
  let loading = $state(true);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let hover = $state<{ hz: number; db: number } | null>(null);

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
    const request =
      mode === 'ltas' ? client.ltas(audio.id, t0, t1) : client.spectrumSlice(audio.id, t0, t1);
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

  // dB extent, padded, for the vertical scale.
  const range = $derived.by(() => {
    if (!data || data.db.length === 0) return null;
    let lo = Infinity;
    let hi = -Infinity;
    for (const v of data.db) {
      if (!Number.isFinite(v)) continue;
      if (v < lo) lo = v;
      if (v > hi) hi = v;
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

    // Grid + frequency ticks every 1 kHz.
    ctx.strokeStyle = cssVar('--chrome-strong', '#233');
    ctx.fillStyle = cssVar('--muted', '#8aa');
    ctx.lineWidth = 1;
    ctx.font = '10px system-ui, sans-serif';
    ctx.textAlign = 'center';
    for (let hz = 0; hz <= range.maxHz; hz += 1000) {
      const x = xOf(hz);
      ctx.globalAlpha = 0.25;
      ctx.beginPath();
      ctx.moveTo(x, 8);
      ctx.lineTo(x, 8 + plotH);
      ctx.stroke();
      ctx.globalAlpha = 1;
      ctx.fillText(`${hz / 1000}k`, x, h - 8);
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
    // Redraw when data, range, hover, or the element changes.
    void data;
    void range;
    void hover;
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
  <div class="card" role="dialog" aria-modal="true" aria-label="Spectrum">
    <header>
      <h2><IconAudioWaveform aria-hidden="true" />{title}</h2>
      <span class="span">{t0.toFixed(3)}–{t1.toFixed(3)} s</span>
      <span class="readout" data-testid="spectrum-readout">
        {#if hover}{Math.round(hover.hz)} Hz · {hover.db.toFixed(1)} dB{:else}hover to read{/if}
      </span>
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
