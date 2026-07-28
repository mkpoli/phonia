<script lang="ts">
  import { untrack } from 'svelte';
  import IconArrowLeft from '~icons/lucide/arrow-left';
  import IconDownload from '~icons/lucide/download';
  import IconEye from '~icons/lucide/eye';
  import IconEyeOff from '~icons/lucide/eye-off';
  import { zipStore } from './zip';
  import {
    defaultOverlayParams,
    type AudioInfo,
    type CoreClientLike,
    type FigureColormapName,
    type FigureExportFormat,
    type FigureLayerToggles,
    type FigureLengthUnit,
    type FigureSpec,
    type FigureThemeName
  } from './types';
  import type { PaletteSelection } from './palette';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    annotationId: bigint | null;
    theme: 'light' | 'dark';
    palette: PaletteSelection;
    /** Name of the project the figure's recording belongs to. */
    projectName?: string;
    /** Returns to the project view. */
    onExit?: () => void;
  }

  let { client, audio, annotationId, theme, palette, projectName, onExit }: Props = $props();

  const FIGURE_COLORMAPS: FigureColormapName[] = [
    'viridis',
    'magma',
    'inferno',
    'plasma',
    'cividis',
    'grayscale'
  ];

  // A figure seeds from the on-screen palette when that palette is one a reader
  // can trust under colour-vision deficiency; the brand and custom ramps fall
  // back to viridis.
  function seedFigurePalette(sel: PaletteSelection): FigureColormapName {
    if (sel.kind === 'builtin') {
      const lower = sel.name.toLowerCase() as FigureColormapName;
      if (FIGURE_COLORMAPS.includes(lower)) return lower;
    }
    return 'viridis';
  }

  const LAYER_LABELS: Array<{ key: keyof FigureLayerToggles; label: string }> = [
    { key: 'waveform', label: 'Waveform' },
    { key: 'spectrogram', label: 'Spectrogram' },
    { key: 'pitch', label: 'Pitch' },
    { key: 'formant', label: 'Formants' },
    { key: 'intensity', label: 'Intensity' },
    { key: 'tiers', label: 'Tiers' }
  ];

  const FORMATS: Array<{ value: FigureExportFormat; label: string; nativeOnly?: boolean }> = [
    { value: 'svg', label: 'SVG' },
    { value: 'png', label: 'PNG' },
    { value: 'pdf', label: 'PDF', nativeOnly: true },
    { value: 'vega', label: 'Vega-Lite JSON' },
    { value: 'tikz', label: 'TikZ (PGFPlots)' },
    { value: 'typst', label: 'Typst / CeTZ' },
    { value: 'python', label: 'Python (matplotlib)' },
    { value: 'r', label: 'R (ggplot2)' },
    { value: 'julia', label: 'Julia (Makie)' }
  ];

  const defaults = defaultOverlayParams();

  // The view opens seeded from the editor state, then owns these controls
  // independently; untrack keeps the seed from re-binding to the props.
  let layers = $state<FigureLayerToggles>({
    waveform: true,
    spectrogram: true,
    pitch: true,
    formant: false,
    intensity: false,
    tiers: untrack(() => annotationId) !== null
  });
  let width = $state(16);
  let height = $state(12);
  let unit = $state<FigureLengthUnit>('cm');
  let figTheme = $state<FigureThemeName>(untrack(() => theme));
  let colormap = $state<FigureColormapName>(seedFigurePalette(untrack(() => palette)));
  let format = $state<FigureExportFormat>('svg');

  let svg = $state('');
  let figureJson = $state('');
  let busy = $state(false);
  let error = $state('');
  let previewToken = 0;

  const noLayers = $derived(!LAYER_LABELS.some(({ key }) => layers[key]));

  // Physical aspect ratio of the paper, so the preview reads as a real sheet.
  const aspect = $derived(height > 0 ? width / height : 1);

  function buildSpec(): FigureSpec | null {
    if (!audio) return null;
    return {
      audio: Number(audio.id),
      annotation: annotationId !== null ? Number(annotationId) : null,
      t0: 0,
      t1: audio.duration,
      f0: 0,
      f1: 5000,
      layers: { ...layers },
      width,
      height,
      unit,
      theme: figTheme,
      colormap,
      dynamic_range_db: 70,
      max_db: null,
      spectrogram_width_px: 1000,
      spectrogram_height_px: 300,
      window_length: 0.005,
      pitch_floor_hz: defaults.pitch.floorHz,
      pitch_ceiling_hz: defaults.pitch.ceilingHz,
      pitch_unit: 'hertz',
      formant_ceiling_hz: defaults.formant.ceilingHz,
      formant_max: defaults.formant.maxFormants,
      formant_smoothed: defaults.formant.smoothed,
      intensity_floor_hz: defaults.intensity.floorHz
    };
  }

  async function refresh() {
    const spec = buildSpec();
    if (!client || !spec || noLayers) {
      svg = '';
      figureJson = '';
      if (noLayers) error = '';
      return;
    }
    const token = ++previewToken;
    busy = true;
    error = '';
    try {
      const json = await client.buildFigure(spec);
      if (token !== previewToken) return;
      figureJson = json;
      const rendered = await client.renderFigureSvg(json);
      if (token !== previewToken) return;
      svg = rendered;
    } catch (caught) {
      if (token !== previewToken) return;
      error = caught instanceof Error ? caught.message : String(caught);
      svg = '';
      figureJson = '';
    } finally {
      if (token === previewToken) busy = false;
    }
  }

  // The preview goes through the same SVG backend the export uses, so what the
  // paper shows is exactly what saves.
  $effect(() => {
    void [
      client,
      audio,
      annotationId,
      layers.waveform,
      layers.spectrogram,
      layers.pitch,
      layers.formant,
      layers.intensity,
      layers.tiers,
      width,
      height,
      unit,
      figTheme,
      colormap
    ];
    void refresh();
  });

  function inchesPerUnit(u: FigureLengthUnit): number {
    if (u === 'cm') return 1 / 2.54;
    if (u === 'pt') return 1 / 72;
    return 1;
  }

  function saveBlob(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = name;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  function baseName(name: string): string {
    const dot = name.indexOf('.');
    return dot > 0 ? name.slice(0, dot) : name;
  }

  async function downloadPng() {
    if (!svg) return;
    const dpi = 192;
    const w = Math.max(1, Math.round(width * inchesPerUnit(unit) * dpi));
    const h = Math.max(1, Math.round(height * inchesPerUnit(unit) * dpi));
    const blob = new Blob([svg], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    try {
      const image = new Image();
      image.width = w;
      image.height = h;
      await new Promise<void>((resolve, reject) => {
        image.onload = () => resolve();
        image.onerror = () => reject(new Error('SVG could not be rasterized'));
        image.src = url;
      });
      const canvas = document.createElement('canvas');
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext('2d');
      if (!ctx) throw new Error('canvas 2D context unavailable');
      ctx.drawImage(image, 0, 0, w, h);
      const png = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
      if (png) saveBlob(png, 'figure.png');
    } finally {
      URL.revokeObjectURL(url);
    }
  }

  async function download() {
    if (!client || !figureJson) return;
    error = '';
    try {
      if (format === 'png') {
        await downloadPng();
        return;
      }
      const bundle = await client.exportFigure(figureJson, format);
      if (bundle.sidecars.length > 0) {
        const entries = [{ name: bundle.mainName, bytes: bundle.mainBytes }, ...bundle.sidecars];
        const zip = zipStore(entries);
        saveBlob(
          new Blob([zip as BlobPart], { type: 'application/zip' }),
          `${baseName(bundle.mainName)}.zip`
        );
      } else {
        saveBlob(new Blob([bundle.mainBytes as BlobPart], { type: bundle.mime }), bundle.mainName);
      }
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    }
  }
</script>

<div class="plots" data-testid="plots-view">
  <header class="bar">
    <nav class="crumb" aria-label="Location">
      {#if onExit}
        <button type="button" class="crumb-back" data-testid="plots-back" onclick={() => onExit?.()}>
          <IconArrowLeft aria-hidden="true" />
          <span>{projectName ?? 'Project'}</span>
        </button>
        <span class="crumb-sep" aria-hidden="true">›</span>
      {/if}
      <span class="crumb-current">Figure — {audio?.name ?? ''}</span>
    </nav>

    <div class="spacer"></div>

    <select
      class="format"
      data-testid="plots-format"
      aria-label="Export format"
      bind:value={format}
    >
      {#each FORMATS as f (f.value)}
        <option value={f.value}>{f.label}{f.nativeOnly ? ' (desktop)' : ''}</option>
      {/each}
    </select>
    <button
      type="button"
      class="export"
      data-testid="plots-export"
      disabled={!figureJson || busy}
      onclick={download}
    >
      <IconDownload aria-hidden="true" />
      <span>Export</span>
    </button>
  </header>

  <div class="stage">
    <div class="desk">
      {#if noLayers}
        <p class="hint" data-testid="plots-empty">Turn on a layer to compose the figure.</p>
      {:else if error}
        <p class="hint err" data-testid="plots-error">{error}</p>
      {:else}
        <!-- The paper carries the exact SVG the export writes, sized to its
             physical aspect ratio so it reads as a real sheet on the desk. -->
        <div
          class="paper"
          data-testid="plots-paper"
          style:aspect-ratio={aspect}
          class:busy
        >
          {#if svg}
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html svg}
          {/if}
        </div>
      {/if}
    </div>

    <aside class="inspector" aria-label="Figure options">
      <section class="group">
        <h2>Paper</h2>
        <div class="size-row">
          <label>
            <span>Width</span>
            <input type="number" min="1" step="0.5" data-testid="plots-width" bind:value={width} />
          </label>
          <label>
            <span>Height</span>
            <input type="number" min="1" step="0.5" data-testid="plots-height" bind:value={height} />
          </label>
        </div>
        <div class="size-row">
          <label>
            <span>Unit</span>
            <select bind:value={unit} data-testid="plots-unit">
              <option value="cm">cm</option>
              <option value="in">in</option>
              <option value="pt">pt</option>
            </select>
          </label>
          <label>
            <span>Theme</span>
            <select bind:value={figTheme} data-testid="plots-theme">
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </label>
        </div>
      </section>

      <section class="group">
        <h2>Layers</h2>
        <ul class="layers">
          {#each LAYER_LABELS as { key, label } (key)}
            <li>
              <button
                type="button"
                class="layer"
                class:on={layers[key]}
                data-testid="plots-layer-{key}"
                aria-pressed={layers[key]}
                onclick={() => (layers[key] = !layers[key])}
              >
                {#if layers[key]}<IconEye aria-hidden="true" />{:else}<IconEyeOff aria-hidden="true" />{/if}
                <span>{label}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>

      <section class="group">
        <h2>Spectrogram colour</h2>
        <select bind:value={colormap} data-testid="plots-colormap" aria-label="Spectrogram colormap">
          {#each FIGURE_COLORMAPS as c (c)}
            <option value={c}>{c[0].toUpperCase() + c.slice(1)}</option>
          {/each}
        </select>
      </section>
    </aside>
  </div>
</div>

<style>
  .plots {
    position: fixed;
    inset: 0 0 0 4.75rem;
    display: flex;
    flex-direction: column;
    background: var(--chrome);
    color: var(--text);
  }

  .bar {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-height: 2.6rem;
    padding: 0.3rem 0.9rem;
    border-bottom: 1px solid var(--chrome-strong);
    background: var(--panel);
    font-size: 0.82rem;
  }

  .crumb {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    min-width: 0;
  }

  .crumb-back {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    min-height: 1.7rem;
    padding: 0.15rem 0.55rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .crumb-back:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .crumb-back :global(svg) {
    font-size: 0.95rem;
  }

  .crumb-sep {
    color: var(--muted);
    opacity: 0.6;
  }

  .crumb-current {
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spacer {
    flex: 1 1 auto;
  }

  .format {
    min-height: 1.9rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
    padding: 0 0.4rem;
  }

  .export {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    min-height: 1.9rem;
    padding: 0 0.8rem;
    border: 1px solid color-mix(in oklab, var(--accent) 40%, var(--chrome-strong));
    border-radius: var(--radius-sm);
    background: var(--accent-tint);
    color: var(--accent-strong);
    font: inherit;
    font-size: 0.8rem;
    font-weight: 600;
    transition: background var(--t-fast);
  }

  .export:hover:not(:disabled) {
    background: color-mix(in oklab, var(--accent) 22%, var(--panel));
  }

  .export:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .stage {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }

  .desk {
    flex: 1 1 auto;
    min-width: 0;
    display: grid;
    place-items: center;
    padding: 2rem;
    overflow: auto;
    /* A quiet neutral desk with a faint bathymetric wash, so the paper reads
       as a lit document resting on it. */
    background:
      radial-gradient(120% 90% at 50% 0%, color-mix(in oklab, var(--accent) 6%, transparent), transparent 60%),
      var(--canvas);
  }

  .paper {
    max-width: min(100%, 900px);
    max-height: 100%;
    width: auto;
    height: auto;
    background: #fff;
    border-radius: 2px;
    box-shadow:
      0 1px 2px rgba(0, 0, 0, 0.3),
      0 12px 40px rgba(0, 0, 0, 0.35);
    transition: opacity var(--t-fast);
    line-height: 0;
  }

  .paper.busy {
    opacity: 0.6;
  }

  .paper :global(svg) {
    display: block;
    width: 100%;
    height: 100%;
  }

  .hint {
    color: var(--muted);
    font-size: 0.9rem;
    max-width: 24rem;
    text-align: center;
  }

  .hint.err {
    color: var(--danger, #d66);
  }

  .inspector {
    flex: none;
    width: 15rem;
    min-width: 15rem;
    border-left: 1px solid var(--chrome-strong);
    background: var(--panel);
    overflow-y: auto;
    padding: 0.5rem 0;
  }

  .group {
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid var(--chrome-strong);
  }

  .group h2 {
    margin: 0 0 0.5rem;
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .size-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.4rem;
  }

  .size-row + .size-row {
    margin-top: 0.5rem;
  }

  .size-row label {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    font-size: 0.72rem;
    color: var(--muted);
    min-width: 0;
  }

  .size-row input,
  .size-row select {
    min-height: 1.7rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    font-size: 0.78rem;
    padding: 0 0.35rem;
  }

  .layers {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .layer {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.35rem 0.4rem;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 0.8rem;
    text-align: left;
    transition:
      background var(--t-fast),
      color var(--t-fast);
  }

  .layer:hover {
    background: var(--panel-soft);
  }

  .layer.on {
    color: var(--text);
  }

  .layer :global(svg) {
    flex: none;
    font-size: 0.9rem;
  }

  .group select[data-testid='plots-colormap'] {
    width: 100%;
    min-height: 1.8rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
    padding: 0 0.35rem;
  }
</style>
