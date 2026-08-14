<script lang="ts">
  import IconScatterChart from '~icons/lucide/scatter-chart';
  import IconX from '~icons/lucide/x';
  import type { AudioInfo, CoreClientLike, AnnotationId, TierInfo } from './types';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    annotationId: AnnotationId | null;
    /** Formants Burg fits; F1/F2 are the first two. */
    maxFormants: number;
    ceilingHz: number;
    onClose: () => void;
  }

  let { client, audio, annotationId, maxFormants, ceilingHz, onClose }: Props = $props();

  interface Point {
    label: string;
    f1: number;
    f2: number;
  }

  let tiers = $state<TierInfo[]>([]);
  let selectedTierId = $state<bigint | null>(null);
  let points = $state<Point[]>([]);
  let loading = $state(true);
  let canvas = $state<HTMLCanvasElement | null>(null);

  function cssVar(name: string, fallback: string): string {
    if (typeof window === 'undefined' || !canvas) return fallback;
    return getComputedStyle(canvas).getPropertyValue(name).trim() || fallback;
  }

  async function collect() {
    if (!client || !audio || annotationId === null) {
      loading = false;
      return;
    }
    loading = true;
    const all = await client.annotationTiers(annotationId);
    tiers = all.filter((t) => t.kind === 'interval');
    if (tiers.length === 0) {
      points = [];
      loading = false;
      return;
    }
    const tierId = selectedTierId ?? tiers[0].id;
    selectedTierId = tierId;
    const intervals = await client.intervalsInRange(annotationId, tierId, -1, 1e12);
    const labelled = intervals.filter((iv) => iv.label.trim() !== '');
    const rows = await Promise.all(
      labelled.map(async (iv): Promise<Point | null> => {
        const means = await client!.formantSpanMeans(
          audio!.id,
          ceilingHz,
          maxFormants,
          true,
          iv.xmin,
          iv.xmax
        );
        const f1 = means[0];
        const f2 = means[1];
        if (!Number.isFinite(f1) || !Number.isFinite(f2)) return null;
        return { label: iv.label, f1, f2 };
      })
    );
    points = rows.filter((p): p is Point => p !== null);
    loading = false;
  }

  $effect(() => {
    void annotationId;
    void selectedTierId;
    void collect();
  });

  // Axis extents padded around the data, clamped to a sane vowel space.
  const bounds = $derived.by(() => {
    if (points.length === 0) return null;
    let f1lo = Infinity;
    let f1hi = -Infinity;
    let f2lo = Infinity;
    let f2hi = -Infinity;
    for (const p of points) {
      f1lo = Math.min(f1lo, p.f1);
      f1hi = Math.max(f1hi, p.f1);
      f2lo = Math.min(f2lo, p.f2);
      f2hi = Math.max(f2hi, p.f2);
    }
    const f1pad = Math.max(50, (f1hi - f1lo) * 0.15);
    const f2pad = Math.max(100, (f2hi - f2lo) * 0.15);
    return {
      f1lo: f1lo - f1pad,
      f1hi: f1hi + f1pad,
      f2lo: f2lo - f2pad,
      f2hi: f2hi + f2pad
    };
  });

  function draw() {
    const ctx = canvas?.getContext('2d');
    if (!ctx || !canvas || !bounds) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = cssVar('--canvas', '#0b0f14');
    ctx.fillRect(0, 0, w, h);

    const padL = 52;
    const padT = 30;
    const plotW = w - padL - 16;
    const plotH = h - padT - 16;
    // F2 high → left, F1 low → top (Praat vowel-space convention).
    const xOf = (f2: number) =>
      padL + ((bounds.f2hi - f2) / (bounds.f2hi - bounds.f2lo)) * plotW;
    const yOf = (f1: number) =>
      padT + ((f1 - bounds.f1lo) / (bounds.f1hi - bounds.f1lo)) * plotH;

    // Frame + axis labels (F2 across the top, F1 down the left).
    ctx.strokeStyle = cssVar('--chrome-strong', '#233');
    ctx.fillStyle = cssVar('--muted', '#8aa');
    ctx.font = '10px system-ui, sans-serif';
    ctx.lineWidth = 1;
    ctx.strokeRect(padL, padT, plotW, plotH);
    ctx.textAlign = 'center';
    ctx.fillText('F2 (Hz)', padL + plotW / 2, 12);
    for (const f2 of [500, 1000, 1500, 2000, 2500]) {
      if (f2 < bounds.f2lo || f2 > bounds.f2hi) continue;
      const x = xOf(f2);
      ctx.fillText(String(f2), x, padT - 3);
    }
    ctx.save();
    ctx.translate(12, padT + plotH / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.fillText('F1 (Hz)', 0, 0);
    ctx.restore();
    ctx.textAlign = 'right';
    for (const f1 of [300, 500, 700, 900]) {
      if (f1 < bounds.f1lo || f1 > bounds.f1hi) continue;
      const y = yOf(f1);
      ctx.fillText(String(f1), padL - 5, y + 3);
    }

    // Points + labels.
    ctx.fillStyle = cssVar('--accent', '#2dd4bf');
    ctx.textAlign = 'left';
    for (const p of points) {
      const x = xOf(p.f2);
      const y = yOf(p.f1);
      ctx.beginPath();
      ctx.arc(x, y, 3.5, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = cssVar('--text', '#e8eef2');
      ctx.font = '11px system-ui, sans-serif';
      ctx.fillText(p.label, x + 6, y + 3);
      ctx.fillStyle = cssVar('--accent', '#2dd4bf');
    }
  }

  $effect(() => {
    void points;
    void bounds;
    if (canvas) draw();
  });
</script>

<div class="backdrop" data-testid="vowel-chart-card">
  <div class="card" role="dialog" aria-modal="true" aria-label="Vowel chart">
    <header>
      <h2><IconScatterChart aria-hidden="true" />Vowel chart</h2>
      {#if tiers.length > 1}
        <select data-testid="vowel-tier" bind:value={selectedTierId} aria-label="Tier">
          {#each tiers as tier (tier.id)}
            <option value={tier.id}>{tier.name}</option>
          {/each}
        </select>
      {:else if tiers.length === 1}
        <span class="span">{tiers[0].name}</span>
      {/if}
      <button type="button" class="close" data-testid="vowel-chart-close" onclick={onClose} aria-label="Close">
        <IconX aria-hidden="true" />
      </button>
    </header>
    {#if loading}
      <p class="status" data-testid="vowel-chart-loading">Measuring…</p>
    {:else if tiers.length === 0}
      <p class="status">No interval tier to chart. Add one and label its vowels.</p>
    {:else if points.length === 0}
      <p class="status">No labelled intervals with measurable formants on this tier.</p>
    {:else}
      <canvas bind:this={canvas} class="plot" data-testid="vowel-chart-canvas"></canvas>
      <p class="note" data-testid="vowel-chart-count">
        {points.length} vowel{points.length === 1 ? '' : 's'} · F2 high→low across, F1 low→high down —
        front vowels left, open vowels down.
      </p>
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
    width: min(40rem, calc(100vw - 2rem));
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

  header select {
    font: inherit;
    font-size: 0.8rem;
    background: var(--panel-soft);
    color: var(--text);
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    padding: 0.15rem 0.35rem;
  }

  .close {
    margin-left: auto;
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
    height: 22rem;
    background: var(--canvas);
  }

  .note {
    margin: 0;
    padding: 0.6rem 0.9rem;
    font-size: 0.74rem;
    color: var(--muted);
    border-top: 1px solid var(--chrome-strong);
  }
</style>
