<script lang="ts">
  import IconTable from '~icons/lucide/table';
  import IconX from '~icons/lucide/x';
  import IconClipboard from '~icons/lucide/clipboard-copy';
  import IconCheck from '~icons/lucide/check';
  import IconDownload from '~icons/lucide/download';
  import IconScissors from '~icons/lucide/scissors';
  import type { AudioInfo, CoreClientLike, OverlayParams, TierInfo, AnnotationId } from './types';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    annotationId: AnnotationId | null;
    params: OverlayParams;
    /** Recording name, for the exported file name. */
    name?: string;
    /** Extracts each labelled interval to its own recording; absent hides the
     *  affordance. */
    onExtractIntervals?: (spans: { t0: number; t1: number; label: string }[]) => Promise<void>;
    onClose: () => void;
  }

  let {
    client,
    audio,
    annotationId,
    params,
    name = 'measurements',
    onExtractIntervals,
    onClose
  }: Props = $props();

  interface Row {
    label: string;
    tmin: number;
    tmax: number;
    f0Hz: number | null;
    formantsHz: number[];
    intensityDb: number | null;
    cogHz: number | null;
  }

  let tiers = $state<TierInfo[]>([]);
  let selectedTierId = $state<bigint | null>(null);
  let rows = $state<Row[]>([]);
  let loading = $state(false);
  let copied = $state(false);
  let extracting = $state(false);

  async function extract() {
    if (!onExtractIntervals || rows.length === 0) return;
    extracting = true;
    try {
      await onExtractIntervals(rows.map((row) => ({ t0: row.tmin, t1: row.tmax, label: row.label })));
    } finally {
      extracting = false;
    }
  }

  // The columns, in order: label, times, then the measures harvested per
  // interval from the same engine queries the readout uses.
  const COLUMNS = ['Label', 'tmin (s)', 'tmax (s)', 'Dur (s)', 'F0 (Hz)', 'F1 (Hz)', 'F2 (Hz)', 'F3 (Hz)', 'F4 (Hz)', 'Intensity (dB)', 'CoG (Hz)'];

  function n(value: number | null | undefined, digits: number): string {
    return value === null || value === undefined || !Number.isFinite(value) ? '' : value.toFixed(digits);
  }

  function cells(row: Row): string[] {
    return [
      row.label,
      n(row.tmin, 4),
      n(row.tmax, 4),
      n(row.tmax - row.tmin, 4),
      n(row.f0Hz, 1),
      n(row.formantsHz[0], 0),
      n(row.formantsHz[1], 0),
      n(row.formantsHz[2], 0),
      n(row.formantsHz[3], 0),
      n(row.intensityDb, 1),
      n(row.cogHz, 0)
    ];
  }

  async function measure() {
    if (!client || !audio || annotationId === null) {
      rows = [];
      tiers = [];
      return;
    }
    loading = true;
    try {
      const all = await client.annotationTiers(annotationId);
      tiers = all.filter((t) => t.kind === 'interval');
      if (tiers.length === 0) {
        rows = [];
        return;
      }
      const tierId = selectedTierId ?? tiers[0].id;
      selectedTierId = tierId;
      const intervals = await client.intervalsInRange(annotationId, tierId, -1, 1e12);
      const labelled = intervals.filter((iv) => iv.label.trim() !== '');
      rows = await Promise.all(
        labelled.map(async (iv): Promise<Row> => {
          const [readout, formants] = await Promise.all([
            client.selectionReadout(
              audio.id,
              iv.xmin,
              iv.xmax,
              0,
              0,
              params.pitch.floorHz,
              params.pitch.ceilingHz,
              params.intensity.floorHz
            ),
            client.formantSpanMeans(
              audio.id,
              params.formant.ceilingHz,
              params.formant.maxFormants,
              params.formant.smoothed,
              iv.xmin,
              iv.xmax
            )
          ]);
          return {
            label: iv.label,
            tmin: iv.xmin,
            tmax: iv.xmax,
            f0Hz: readout.f0MeanHz,
            formantsHz: Array.from(formants),
            intensityDb: readout.intensityMeanDb,
            cogHz: readout.spectralCogHz
          };
        })
      );
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Re-measure when the recording, annotation, or chosen tier changes.
    void annotationId;
    void selectedTierId;
    void measure();
  });

  function table(sep: string): string {
    return [COLUMNS.join(sep), ...rows.map((r) => cells(r).join(sep))].join('\n');
  }

  async function copyTsv() {
    try {
      await navigator.clipboard.writeText(table('\t'));
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      copied = false;
    }
  }

  function slug(s: string): string {
    return (
      s
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '') || 'measurements'
    );
  }

  function downloadCsv() {
    const blob = new Blob([table(',')], { type: 'text/csv;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = `${slug(name)}.csv`;
    anchor.click();
    URL.revokeObjectURL(url);
  }
</script>

<div class="backdrop" data-testid="measurement-table">
  <div class="card" role="dialog" aria-modal="true" aria-label="Measurements">
    <header>
      <h2><IconTable aria-hidden="true" />Measurements</h2>
      {#if tiers.length > 1}
        <select data-testid="measure-tier" bind:value={selectedTierId} aria-label="Tier">
          {#each tiers as tier (tier.id)}
            <option value={tier.id}>{tier.name}</option>
          {/each}
        </select>
      {:else if tiers.length === 1}
        <span class="span">{tiers[0].name}</span>
      {/if}
      <button type="button" class="close" data-testid="measure-close" onclick={onClose} aria-label="Close">
        <IconX aria-hidden="true" />
      </button>
    </header>

    {#if loading}
      <p class="status" data-testid="measure-loading">Measuring…</p>
    {:else if tiers.length === 0}
      <p class="status">This recording has no interval tier to measure. Add one and label its intervals.</p>
    {:else if rows.length === 0}
      <p class="status">No labelled intervals on this tier yet.</p>
    {:else}
      <div class="scroll">
        <table data-testid="measure-grid">
          <thead>
            <tr>{#each COLUMNS as col (col)}<th>{col}</th>{/each}</tr>
          </thead>
          <tbody>
            {#each rows as row, i (i)}
              <tr data-testid="measure-row">
                {#each cells(row) as cell, c (c)}
                  <td class:label={c === 0}>{cell}</td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <footer>
        <span class="count">{rows.length} interval{rows.length === 1 ? '' : 's'}</span>
        <button type="button" class="copy" data-testid="measure-copy" onclick={copyTsv}>
          {#if copied}<IconCheck aria-hidden="true" /><span>Copied</span>
          {:else}<IconClipboard aria-hidden="true" /><span>Copy TSV</span>{/if}
        </button>
        <button type="button" class="copy" data-testid="measure-csv" onclick={downloadCsv}>
          <IconDownload aria-hidden="true" /><span>Download CSV</span>
        </button>
        {#if onExtractIntervals}
          <button
            type="button"
            class="copy"
            data-testid="measure-extract"
            disabled={extracting}
            onclick={extract}
          >
            <IconScissors aria-hidden="true" />
            <span>{extracting ? 'Extracting…' : 'Extract to recordings'}</span>
          </button>
        {/if}
      </footer>
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
    width: min(56rem, calc(100vw - 2rem));
    max-height: calc(100vh - 3rem);
    display: flex;
    flex-direction: column;
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md, 8px);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    padding: 0.7rem 0.9rem;
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
    padding: 1.4rem;
    color: var(--muted);
    text-align: center;
  }

  .scroll {
    overflow: auto;
    min-height: 0;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
  }

  th {
    position: sticky;
    top: 0;
    background: var(--panel);
    text-align: right;
    font-weight: 600;
    color: var(--muted);
    padding: 0.35rem 0.6rem;
    border-bottom: 1px solid var(--chrome-strong);
    white-space: nowrap;
  }

  th:first-child {
    text-align: left;
  }

  td {
    text-align: right;
    padding: 0.3rem 0.6rem;
    border-bottom: 1px solid color-mix(in oklab, var(--chrome-strong) 45%, transparent);
    white-space: nowrap;
  }

  td.label {
    text-align: left;
    color: var(--text);
    font-weight: 500;
  }

  tbody tr:hover td {
    background: var(--panel-soft);
  }

  footer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.6rem 0.9rem;
    border-top: 1px solid var(--chrome-strong);
  }

  .count {
    color: var(--muted);
    font-size: 0.78rem;
    margin-right: auto;
  }

  .copy {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    padding: 0.28rem 0.6rem;
    font-size: 0.78rem;
    cursor: pointer;
  }

  .copy:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }
</style>
