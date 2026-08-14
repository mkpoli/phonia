<script lang="ts">
  import IconChartSpline from '~icons/lucide/chart-spline';
  import IconX from '~icons/lucide/x';
  import type { AudioInfo, CoreClientLike } from './types';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    t0: number;
    t1: number;
    /** Number of formants Burg fits; F1–F4 are read from the first four. */
    maxFormants: number;
    onClose: () => void;
  }

  let { client, audio, t0, t1, maxFormants, onClose }: Props = $props();

  const CEILINGS = [4500, 5000, 5500, 6000];

  interface Row {
    ceiling: number;
    formants: number[];
  }

  let rows = $state<Row[]>([]);
  let loading = $state(true);

  $effect(() => {
    if (!client || !audio) {
      loading = false;
      return;
    }
    let cancelled = false;
    loading = true;
    Promise.all(
      CEILINGS.map(async (ceiling): Promise<Row> => {
        const means = await client!.formantSpanMeans(audio!.id, ceiling, maxFormants, true, t0, t1);
        return { ceiling, formants: Array.from(means) };
      })
    )
      .then((result) => {
        if (!cancelled) {
          rows = result;
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

  function hz(value: number | undefined): string {
    return value === undefined || !Number.isFinite(value) ? '—' : `${Math.round(value)}`;
  }

  // The steadiest ceiling: the one whose F1–F4 move least from its neighbours,
  // a cheap read on where the Burg order stops swapping poles.
  const steadiest = $derived.by(() => {
    if (rows.length < 2) return null;
    let best = rows[0].ceiling;
    let bestDrift = Infinity;
    for (let i = 0; i < rows.length; i += 1) {
      let drift = 0;
      let count = 0;
      for (const j of [i - 1, i + 1]) {
        if (j < 0 || j >= rows.length) continue;
        for (let k = 0; k < 4; k += 1) {
          const a = rows[i].formants[k];
          const b = rows[j].formants[k];
          if (Number.isFinite(a) && Number.isFinite(b)) {
            drift += Math.abs(a - b);
            count += 1;
          }
        }
      }
      const mean = count > 0 ? drift / count : Infinity;
      if (mean < bestDrift) {
        bestDrift = mean;
        best = rows[i].ceiling;
      }
    }
    return best;
  });
</script>

<div class="backdrop" data-testid="formant-sweep-card">
  <div class="card" role="dialog" aria-modal="true" aria-label="Formant ceiling sweep">
    <header>
      <h2><IconChartSpline aria-hidden="true" />Formant ceiling sweep</h2>
      <span class="span">{t0.toFixed(3)}–{t1.toFixed(3)} s</span>
      <button type="button" class="close" data-testid="formant-sweep-close" onclick={onClose} aria-label="Close">
        <IconX aria-hidden="true" />
      </button>
    </header>
    {#if loading}
      <p class="status" data-testid="formant-sweep-loading">Analysing…</p>
    {:else if rows.length === 0}
      <p class="status">No formants for this selection.</p>
    {:else}
      <table data-testid="formant-sweep-grid">
        <thead>
          <tr><th>Ceiling (Hz)</th><th>F1</th><th>F2</th><th>F3</th><th>F4</th></tr>
        </thead>
        <tbody>
          {#each rows as row (row.ceiling)}
            <tr data-testid="formant-sweep-row" class:steady={row.ceiling === steadiest}>
              <td class="ceil">{row.ceiling}{#if row.ceiling === steadiest}<span class="tag">steadiest</span>{/if}</td>
              <td>{hz(row.formants[0])}</td>
              <td>{hz(row.formants[1])}</td>
              <td>{hz(row.formants[2])}</td>
              <td>{hz(row.formants[3])}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="note">
        Formant means at each LPC ceiling. A vowel is analysed cleanly where F1–F4 hold steady across
        ceilings; a value that jumps signals the Burg order swapping poles — pick the ceiling that
        keeps the tracks stable.
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
    width: min(32rem, calc(100vw - 2rem));
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
    padding: 1.6rem;
    text-align: center;
    color: var(--muted);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.82rem;
    font-variant-numeric: tabular-nums;
  }

  th {
    text-align: right;
    font-weight: 600;
    color: var(--muted);
    padding: 0.4rem 0.9rem;
    border-bottom: 1px solid var(--chrome-strong);
  }

  th:first-child {
    text-align: left;
  }

  td {
    text-align: right;
    padding: 0.4rem 0.9rem;
    border-bottom: 1px solid color-mix(in oklab, var(--chrome-strong) 45%, transparent);
  }

  td.ceil {
    text-align: left;
    color: var(--text);
    font-weight: 500;
  }

  tr.steady td {
    background: color-mix(in oklab, var(--accent) 12%, transparent);
  }

  .tag {
    margin-left: 0.4rem;
    font-size: 0.66rem;
    font-weight: 600;
    color: var(--accent-strong, var(--accent));
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .note {
    margin: 0;
    padding: 0.7rem 0.9rem;
    font-size: 0.74rem;
    color: var(--muted);
    border-top: 1px solid var(--chrome-strong);
  }
</style>
