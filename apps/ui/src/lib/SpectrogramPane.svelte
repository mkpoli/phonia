<script lang="ts">
  import type {
    AudioInfo,
    CoreClientLike,
    OverlayParams,
    OverlayStats,
    Selection,
    SelectionReadout,
    SpectrogramTileRequest,
    ViewportState
  } from './types';
  import { paletteKey, rampToLut, type PaletteSelection } from './palette';
  import {
    applyCanvasSize,
    cssVar,
    FrameTimeMonitor,
    hexToRgb01,
    makeProgram,
    measureCanvasTarget,
    slippyTransform,
    type DrawnViewport
  } from './rendering';
  import { emptyOverlayTracks, sampleTracks, type OverlayTracks } from './tracks';
  import FrequencyRuler from './FrequencyRuler.svelte';
  import GhostWaveform from './GhostWaveform.svelte';
  import SelectionLayer from './SelectionLayer.svelte';
  import TrackOverlay from './TrackOverlay.svelte';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    viewport: ViewportState;
    theme: 'light' | 'dark';
    palette: PaletteSelection;
    /** Whether the active ramp renders reversed (floor in the ceiling color). */
    paletteInvert: boolean;
    overlayParams: OverlayParams;
    onOverlayStats?: (stats: OverlayStats) => void;
    selection?: Selection | null;
    onSelectionChange?: (selection: Selection | null) => void;
    onSeek?: (time: number) => void;
    /** Multiplies the frequency ceiling by `factor` (frequency-ruler drag). */
    onScaleFrequency?: (factor: number) => void;
    /** Restores the frequency ceiling to its default. */
    onResetFrequency?: () => void;
    /** Double-click intent: zoom to the active box, or fit the whole file. */
    onDoubleZoom?: (intent: 'zoom' | 'fit') => void;
    /** Reports the fetched overlay tracks, so the host can sample them too. */
    onTracks?: (tracks: OverlayTracks) => void;
    /** Selection aggregates; hovering inside the selection shows these. */
    readout?: SelectionReadout | null;
    /** Tracked-formant means over the selection (F1, F2, …). */
    formantMeans?: number[] | null;
    /** Traces the waveform envelope over the spectrogram (waveform pane hidden). */
    ghostWaveform?: boolean;
  }

  let {
    client,
    audio,
    viewport,
    theme,
    palette,
    paletteInvert,
    overlayParams,
    onOverlayStats,
    selection = null,
    onSelectionChange,
    onSeek,
    onScaleFrequency,
    onResetFrequency,
    onDoubleZoom,
    onTracks,
    readout = null,
    formantMeans = null,
    ghostWaveform = false
  }: Props = $props();

  // The pointer's pane position, for the hover readout. Cleared on leave and
  // suppressed while a drag is in flight so it never rides a selection.
  let hover = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  let hoverHeld = $state(false);
  let tracks = $state<OverlayTracks>(emptyOverlayTracks());

  function hoverMove(event: PointerEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    hover = {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
      w: rect.width,
      h: rect.height
    };
  }

  const hoverTime = $derived(
    hover ? viewport.t0 + (hover.x / Math.max(1, hover.w)) * (viewport.t1 - viewport.t0) : 0
  );
  const hoverHz = $derived(
    hover ? viewport.f1 - (hover.y / Math.max(1, hover.h)) * (viewport.f1 - viewport.f0) : 0
  );
  const hoverSample = $derived(sampleTracks(tracks, hoverTime));

  // Hovering inside the active selection's time span reads out the range's
  // aggregates rather than the instant under the pointer.
  const hoverInSelection = $derived(
    !!selection &&
      !!readout &&
      hover !== null &&
      hoverTime >= selection.t0 &&
      hoverTime <= selection.t1
  );

  function fmtHz(value: number): string {
    if (value >= 1000) {
      const khz = value / 1000;
      return `${Number.isInteger(khz) ? khz : khz.toFixed(1)} kHz`;
    }
    return `${Math.round(value)} Hz`;
  }

  // A custom ramp resolves to its 768-byte LUT; a built-in resolves to its name.
  // The id keys the tile cache so a palette change — a live edit of a custom
  // ramp, or an invert flip — recolors, and re-selecting the same palette hits
  // the cache.
  const paletteId = $derived(`${paletteKey(palette)}${paletteInvert ? ':inv' : ''}`);
  const paletteLut = $derived(
    palette.kind === 'custom' ? rampToLut(palette.ramp.stops) : undefined
  );

  let canvas = $state<HTMLCanvasElement | null>(null);
  let notice = $state('');
  let usingCanvas2d = $state(false);
  let renderToken = $state(0);
  // Advances on every transform draw (instant CSS remap or fresh raster). Stamped
  // straight onto the canvas rather than through reactive state: the viewport
  // effect calls it synchronously, and a tracked read-modify-write here would
  // retrigger the effect. The e2e reads these to assert the pane tracks the
  // viewport within the frame budget and stays in step with the other panes.
  let drawGen = 0;

  const cache = new Map<string, ImageBitmap>();
  const monitor = new FrameTimeMonitor();

  // The viewport the current canvas pixels were rasterized for. Null until the
  // first tile lands, when the canvas carries no transform.
  let base: DrawnViewport | null = null;
  // Generation of the most recent fetch; a tile resolved from an older
  // generation is dropped so a superseded pan never overwrites fresh imagery.
  let reqGen = 0;
  let fetchScheduled = false;

  function liveViewport(): DrawnViewport {
    return { t0: viewport.t0, t1: viewport.t1, vLo: viewport.f0, vHi: viewport.f1 };
  }

  // Redraw the existing pixels immediately by remapping them with a CSS
  // transform: waveform, spectrogram, and overlays all follow the one shared
  // viewport, so they move as a single rigid sheet with no worker round-trip.
  function applyTransform() {
    if (!canvas) return;
    canvas.style.transform = base ? slippyTransform(base, liveViewport(), 'freq') : 'none';
    drawGen += 1;
    canvas.setAttribute('data-draw-generation', String(drawGen));
    canvas.setAttribute('data-draw-time', performance.now().toFixed(2));
  }

  function scheduleFetch() {
    if (fetchScheduled) return;
    fetchScheduled = true;
    requestAnimationFrame(() => {
      fetchScheduled = false;
      void fetchFreshTile();
    });
  }

  // A new recording invalidates the transform anchor: the old bitmap belongs to
  // a different signal and must not be stretched over the new viewport.
  $effect(() => {
    audio?.id;
    base = null;
    if (canvas) canvas.style.transform = 'none';
  });

  $effect(() => {
    if (!canvas) return;
    const observer = new ResizeObserver(() => scheduleFetch());
    observer.observe(canvas);
    scheduleFetch();
    return () => observer.disconnect();
  });

  $effect(() => {
    viewport.t0;
    viewport.t1;
    viewport.f0;
    viewport.f1;
    paletteId;
    overlayParams.spectrogram.windowLength;
    overlayParams.spectrogram.dynamicRangeDb;
    overlayParams.spectrogram.preemphasis;
    applyTransform();
    scheduleFetch();
  });

  async function getTile(width: number, height: number) {
    if (!client || !audio) return null;
    const cssWidth = Math.max(32, Math.floor(width / Math.max(1, window.devicePixelRatio || 1)));
    const cssHeight = Math.max(32, Math.floor(height / Math.max(1, window.devicePixelRatio || 1)));
    // Key by the exact rendered range: the tile covers [t0, t1) precisely, so
    // two different viewports must never share a cache entry — a coarser key
    // (e.g. a fixed time bucket) hands back a bitmap drawn for another range,
    // which then rides a stale transform anchor and desyncs from the ruler.
    const paramsHash = [
      viewport.t0.toFixed(4),
      viewport.t1.toFixed(4),
      viewport.f0.toFixed(1),
      viewport.f1.toFixed(1),
      cssWidth,
      cssHeight,
      paletteId,
      overlayParams.spectrogram.windowLength,
      overlayParams.spectrogram.dynamicRangeDb,
      overlayParams.spectrogram.preemphasis
    ].join(':');
    const key = `${String(audio.id)}:spec:${paramsHash}`;
    const cached = cache.get(key);
    if (cached) return cached;
    const req: SpectrogramTileRequest = {
      t0: viewport.t0,
      t1: viewport.t1,
      f0: viewport.f0,
      f1: viewport.f1,
      widthPx: cssWidth,
      heightPx: cssHeight,
      windowLength: overlayParams.spectrogram.windowLength,
      maxFrequency: 5000,
      timeStep: 0.002,
      frequencyStep: 20,
      dynamicRangeDb: overlayParams.spectrogram.dynamicRangeDb,
      colormap: palette.kind === 'builtin' ? palette.name : 'Phonia',
      invert: paletteInvert,
      preemphasis: overlayParams.spectrogram.preemphasis,
      lut: paletteLut
    };
    const bitmap = await client.spectrogramTile(audio.id, req);
    cache.set(key, bitmap);
    return bitmap;
  }

  // The bitmap currently on the canvas and the pixel size it was drawn at, so a
  // pan that reuses the same tile skips the re-raster and lets the CSS transform
  // carry the motion — the crisp draw runs only when the tile itself changes.
  let displayed: ImageBitmap | null = null;
  let displayedW = 0;
  let displayedH = 0;

  async function fetchFreshTile() {
    if (!canvas) return;
    const gen = ++reqGen;
    const requested = liveViewport();
    // Measure the target size without touching the backing store yet: a
    // resize's new CSS box already stretches the still-displayed bitmap (like
    // an image), so nothing goes blank while this awaits a fresh tile.
    const { width, height, dpr } = measureCanvasTarget(canvas);
    const bitmap = await getTile(width, height);
    // Dropped: a newer pan or zoom already superseded this request.
    if (gen !== reqGen || !canvas) return;
    if (!bitmap) {
      applyCanvasSize(canvas, width, height);
      drawEmpty(width, height);
      displayed = null;
      displayedW = width;
      displayedH = height;
      return;
    }
    // Same tile and canvas size: the current pixels are already correct for
    // `base`, and the transform maps them onto the live viewport. Nothing to do.
    if (bitmap === displayed && width === displayedW && height === displayedH) return;
    // Resize the backing store only now, in the same tick as the redraw, so
    // the canvas is never cleared without fresh pixels ready to fill it.
    applyCanvasSize(canvas, width, height);
    drawBitmap(width, height, dpr, bitmap);
    displayed = bitmap;
    displayedW = width;
    displayedH = height;
    // The fresh pixels represent the viewport at request time; re-apply the
    // transform so any motion since then still shows without a flash.
    base = requested;
    applyTransform();
    renderToken += 1;
  }

  function drawBitmap(width: number, height: number, dpr: number, bitmap: ImageBitmap) {
    if (usingCanvas2d) {
      drawCanvas2d(width, height, dpr, bitmap);
      return;
    }
    const start = performance.now();
    try {
      drawWebgl(width, height, bitmap);
    } catch {
      usingCanvas2d = true;
      notice = 'Canvas fallback active';
      drawCanvas2d(width, height, dpr, bitmap);
    }
    const elapsed = performance.now() - start;
    if (!usingCanvas2d && monitor.record(elapsed)) {
      usingCanvas2d = true;
      notice = 'Canvas fallback active';
    }
  }

  function drawEmpty(width: number, height: number) {
    const ctx = canvas?.getContext('2d');
    if (!ctx) return;
    ctx.fillStyle = cssVar('--canvas', '#f8fafc');
    ctx.fillRect(0, 0, width, height);
  }

  function drawCanvas2d(width: number, height: number, dpr: number, bitmap: ImageBitmap) {
    const ctx = canvas?.getContext('2d');
    if (!ctx) return;
    ctx.fillStyle = cssVar('--canvas', '#f8fafc');
    ctx.fillRect(0, 0, width, height);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(bitmap, 0, 0, width, height);
  }

  function drawWebgl(width: number, height: number, bitmap: ImageBitmap) {
    const gl = canvas?.getContext('webgl2', { antialias: false, preserveDrawingBuffer: true });
    if (!gl) throw new Error('WebGL2 unavailable');
    const vertex = `#version 300 es
      in vec2 a_pos;
      out vec2 v_uv;
      void main() {
        v_uv = vec2((a_pos.x + 1.0) * 0.5, 1.0 - (a_pos.y + 1.0) * 0.5);
        gl_Position = vec4(a_pos, 0.0, 1.0);
      }`;
    const fragment = `#version 300 es
      precision mediump float;
      uniform sampler2D u_tile;
      in vec2 v_uv;
      out vec4 out_color;
      void main() {
        out_color = texture(u_tile, v_uv);
      }`;
    const program = makeProgram(gl, vertex, fragment);
    const background = hexToRgb01(cssVar('--canvas', '#f8fafc'));
    gl.viewport(0, 0, width, height);
    gl.clearColor(background[0], background[1], background[2], 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(program);
    const texture = gl.createTexture();
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, bitmap);
    const vertices = new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]);
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
    const posLoc = gl.getAttribLocation(program, 'a_pos');
    gl.enableVertexAttribArray(posLoc);
    gl.vertexAttribPointer(posLoc, 2, gl.FLOAT, false, 0, 0);
    gl.uniform1i(gl.getUniformLocation(program, 'u_tile'), 0);
    gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    gl.deleteBuffer(buffer);
    gl.deleteTexture(texture);
    gl.deleteProgram(program);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section
  class="pane"
  onpointermove={hoverMove}
  onpointerleave={() => (hover = null)}
  onpointerdown={() => (hoverHeld = true)}
  onpointerup={() => (hoverHeld = false)}
>
  {#key usingCanvas2d}
    <canvas
      bind:this={canvas}
      class="canvas"
      data-testid="spectrogram-canvas"
      data-render-token={renderToken}
      data-draw-generation="0"
      data-draw-time="0"
    ></canvas>
  {/key}
  {#if ghostWaveform}
    <GhostWaveform {client} {audio} {viewport} {theme} />
  {/if}
  <TrackOverlay
    {client}
    {audio}
    {viewport}
    {theme}
    params={overlayParams}
    onStats={onOverlayStats}
    onTracks={(next) => {
      tracks = next;
      onTracks?.(next);
    }}
  />
  <FrequencyRuler {viewport} onScale={onScaleFrequency} onReset={onResetFrequency} />
  {#if audio && onSelectionChange}
    <SelectionLayer
      {viewport}
      mode="box"
      {selection}
      onChange={onSelectionChange}
      {onSeek}
      {onDoubleZoom}
    />
  {/if}
  {#if audio && hover && !hoverHeld}
    <div
      class="hover-readout"
      data-testid="hover-readout"
      data-in-selection={hoverInSelection}
      style:left="{Math.min(hover.x + 14, hover.w - 170)}px"
      style:top="{hover.y > hover.h - 104 ? hover.y - 92 : hover.y + 16}px"
    >
      {#if hoverInSelection && selection && readout}
        <span class="hr-pos">
          Selection {selection.t0.toFixed(3)}–{selection.t1.toFixed(3)} s · {Math.round(
            readout.duration * 1000
          )} ms
        </span>
        {#if readout.f0MeanHz !== null}
          <span class="hr-row pitch">F0 {readout.f0MeanHz.toFixed(1)} Hz</span>
        {/if}
        {#if formantMeans && formantMeans.length >= 2}
          <span class="hr-row formant">
            {formantMeans.slice(0, 3).map((f, i) => `F${i + 1} ${Math.round(f)}`).join(' · ')}
          </span>
        {/if}
        {#if readout.intensityMeanDb !== null}
          <span class="hr-row intensity">{readout.intensityMeanDb.toFixed(1)} dB</span>
        {/if}
      {:else}
        <span class="hr-pos">{hoverTime.toFixed(3)} s · {Math.round(hoverHz)} Hz</span>
        {#if hoverSample.f0Hz !== null}
          <span class="hr-row pitch">F0 {hoverSample.f0Hz.toFixed(1)} Hz</span>
        {/if}
        {#if hoverSample.formantsHz.length > 0}
          <span class="hr-row formant">
            {hoverSample.formantsHz.map((f, i) => `F${i + 1} ${Math.round(f)}`).join(' · ')}
          </span>
        {/if}
        {#if hoverSample.intensityDb !== null}
          <span class="hr-row intensity">{hoverSample.intensityDb.toFixed(1)} dB</span>
        {/if}
      {/if}
    </div>
  {/if}
  <div class="pane-label">
    Spectrogram · {fmtHz(viewport.f0)}–{fmtHz(viewport.f1)} · {Math.round(
      overlayParams.spectrogram.windowLength * 1000
    )} ms · {Math.round(overlayParams.spectrogram.dynamicRangeDb)} dB
  </div>
  {#if notice}
    <div class="notice">{notice}</div>
  {/if}
</section>

<style>
  .pane {
    position: relative;
    /* The timeline grid track owns the height; a floor here would push the
       pane past its track and over the tier pane on short windows. */
    height: 100%;
    min-height: 0;
    border-bottom: 1px solid var(--chrome-strong);
    background: var(--canvas);
    overflow: hidden;
  }

  .canvas {
    display: block;
    width: 100%;
    height: 100%;
    transform-origin: 0 0;
    will-change: transform;
  }

  .hover-readout {
    position: absolute;
    z-index: 4;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    padding: 0.3rem 0.5rem;
    border-radius: 6px;
    background: var(--chip-bg);
    border: 1px solid var(--chrome-strong);
    font-size: 0.68rem;
    font-variant-numeric: tabular-nums;
    line-height: 1.35;
    pointer-events: none;
    white-space: nowrap;
  }

  .hover-readout .hr-pos {
    color: var(--text);
    font-weight: 600;
  }

  .hover-readout .hr-row.pitch {
    color: #9cc4ff;
  }

  .hover-readout .hr-row.formant {
    color: #ff5a52;
  }

  .hover-readout .hr-row.intensity {
    color: #ffcc33;
  }

  .pane-label,
  .notice {
    position: absolute;
    z-index: 2;
    top: 0.4rem;
    font-size: 0.75rem;
    pointer-events: none;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    background: var(--chip-bg);
    color: var(--chip-fg);
    box-shadow: 0 0 0 1px var(--chip-ring);
  }

  .pane-label {
    left: 0.6rem;
  }

  .notice {
    right: 0.6rem;
    color: var(--warn);
  }
</style>
