import {
  defaultOverlayParams,
  type AudioInfo,
  type FigureColormapName,
  type FigureLayerToggles,
  type FigureSpec,
  type FigureThemeName
} from './types';

/** A kind of plot object that can be placed on the figure canvas. */
export type PlotKind =
  | 'waveform'
  | 'spectrogram'
  | 'pitch'
  | 'formant'
  | 'intensity'
  | 'harmonicity'
  | 'spectralSlice'
  | 'ltas'
  | 'tiers'
  | 'text';

/** The order objects offer themselves in the Add menu and layers list. */
export const PLOT_KINDS: { kind: PlotKind; label: string }[] = [
  { kind: 'waveform', label: 'Waveform' },
  { kind: 'spectrogram', label: 'Spectrogram' },
  { kind: 'pitch', label: 'Pitch' },
  { kind: 'formant', label: 'Formants' },
  { kind: 'intensity', label: 'Intensity' },
  { kind: 'harmonicity', label: 'Harmonicity' },
  { kind: 'spectralSlice', label: 'Spectral slice' },
  { kind: 'ltas', label: 'LTAS' },
  { kind: 'tiers', label: 'Tiers' },
  { kind: 'text', label: 'Text' }
];

export function plotKindLabel(kind: PlotKind): string {
  return PLOT_KINDS.find((k) => k.kind === kind)?.label ?? kind;
}

/**
 * One placed plot on the canvas — a single-layer figure at a position and
 * size in artboard pixels. Every object is independent and additive: adding,
 * moving, restyling, or deleting one never disturbs the others.
 */
export interface PlotObject {
  id: string;
  kind: PlotKind;
  name: string;
  /** Position and size in artboard pixels. */
  x: number;
  y: number;
  w: number;
  h: number;
  visible: boolean;
  /** Time window in seconds; `null` means the whole recording. */
  t0: number | null;
  t1: number | null;
  /** Frequency ceiling in hertz (spectrogram / formant). */
  freqCeiling: number;
  /** Spectrogram colormap. */
  colormap: FigureColormapName;
  /** Item (trace / speckle) colour as a hex string; `null` uses the default. */
  itemColor: string | null;
  /** Text-object content; empty for every plot kind. */
  text: string;
  /** Text-object type size in pixels. */
  fontSize: number;
}

/** Kinds whose ink colour is a single settable stroke/speckle. */
export function kindHasItemColor(kind: PlotKind): boolean {
  return (
    kind === 'waveform' ||
    kind === 'pitch' ||
    kind === 'formant' ||
    kind === 'intensity' ||
    kind === 'text'
  );
}

/** The default ink colour a kind draws with, as a hex string. */
export function defaultItemColor(kind: PlotKind): string {
  return kind === 'formant' ? '#c80000' : '#111111';
}

let objectCounter = 0;

/** A fresh object of `kind`, positioned by the caller. */
export function makePlotObject(kind: PlotKind, x: number, y: number): PlotObject {
  objectCounter += 1;
  const tall = kind === 'spectrogram';
  const text = kind === 'text';
  return {
    id: `obj-${objectCounter}`,
    kind,
    name: text ? 'Label' : plotKindLabel(kind),
    x,
    y,
    w: text ? 220 : 420,
    h: tall ? 220 : kind === 'tiers' ? 120 : text ? 44 : 160,
    visible: true,
    t0: null,
    t1: null,
    freqCeiling: 5000,
    colormap: 'viridis',
    itemColor: null,
    text: text ? 'Label' : '',
    fontSize: 18
  };
}

/** A copy of `o` with a fresh id, offset so it lands clear of the original. */
export function cloneObject(o: PlotObject, dx = 20, dy = 20): PlotObject {
  objectCounter += 1;
  return { ...o, id: `obj-${objectCounter}`, name: `${o.name} copy`, x: o.x + dx, y: o.y + dy };
}

/** `[r, g, b]` parsed from a `#rrggbb` string, or null if unparseable. */
export function hexToRgb(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** The single-layer figure spec that renders one object. */
export function objectFigureSpec(
  obj: PlotObject,
  audio: AudioInfo,
  annotationId: bigint | null,
  theme: FigureThemeName,
  axis: { color: string | null; showGrid: boolean } = { color: null, showGrid: true }
): FigureSpec {
  const defaults = defaultOverlayParams();
  const layers: FigureLayerToggles = {
    waveform: obj.kind === 'waveform',
    spectrogram: obj.kind === 'spectrogram',
    pitch: obj.kind === 'pitch',
    formant: obj.kind === 'formant',
    intensity: obj.kind === 'intensity',
    harmonicity: obj.kind === 'harmonicity',
    spectral_slice: obj.kind === 'spectralSlice',
    ltas: obj.kind === 'ltas',
    tiers: obj.kind === 'tiers'
  };
  const ink = obj.itemColor ? hexToRgb(obj.itemColor) : null;
  const axisInk = axis.color ? hexToRgb(axis.color) : null;
  return {
    audio: Number(audio.id),
    annotation: annotationId !== null ? Number(annotationId) : null,
    t0: obj.t0 ?? 0,
    t1: obj.t1 ?? audio.duration,
    f0: 0,
    f1: obj.freqCeiling,
    layers,
    // Physical size follows the object's pixel box (1px ≈ 0.75pt) so axis
    // labels stay proportionate to the box the figure is drawn into.
    width: Math.max(1, obj.w * 0.75),
    height: Math.max(1, obj.h * 0.75),
    unit: 'pt',
    theme,
    colormap: obj.colormap,
    dynamic_range_db: 70,
    max_db: null,
    spectrogram_width_px: 1000,
    spectrogram_height_px: 300,
    window_length: 0.005,
    pitch_floor_hz: defaults.pitch.floorHz,
    pitch_ceiling_hz: defaults.pitch.ceilingHz,
    pitch_unit: 'hertz',
    formant_ceiling_hz: obj.freqCeiling,
    formant_max: defaults.formant.maxFormants,
    formant_smoothed: defaults.formant.smoothed,
    intensity_floor_hz: defaults.intensity.floorHz,
    waveform_color: obj.kind === 'waveform' ? ink : null,
    pitch_color: obj.kind === 'pitch' ? ink : null,
    formant_color: obj.kind === 'formant' ? ink : null,
    intensity_color: obj.kind === 'intensity' ? ink : null,
    harmonicity_color: obj.kind === 'harmonicity' ? ink : null,
    axis_color: axisInk,
    show_grid: axis.showGrid
  };
}

/** viewBox width/height parsed from a rendered SVG string, for compositing. */
export function svgViewBox(svg: string): { w: number; h: number } {
  const vb = svg.match(/viewBox="([\d.\s-]+)"/);
  if (vb) {
    const parts = vb[1].trim().split(/\s+/).map(Number);
    if (parts.length === 4 && parts[2] > 0 && parts[3] > 0) return { w: parts[2], h: parts[3] };
  }
  const w = svg.match(/\bwidth="([\d.]+)/);
  const h = svg.match(/\bheight="([\d.]+)/);
  return { w: w ? Number(w[1]) : 1, h: h ? Number(h[1]) : 1 };
}

/** Inner markup of an SVG string (everything between the outer svg tags). */
export function svgInner(svg: string): string {
  const open = svg.indexOf('>', svg.indexOf('<svg'));
  const close = svg.lastIndexOf('</svg>');
  return open >= 0 && close > open ? svg.slice(open + 1, close) : svg;
}

/**
 * Removes the figure's opaque full-bleed background rect — always the first
 * `<rect>` the SVG backend draws — so the artboard's paper colour shows through
 * the figure margins; the plot panels keep their own backgrounds.
 */
export function stripOuterBackground(svg: string): string {
  return svg.replace(/<rect\b[^>]*\/>/, '');
}

/**
 * Prefixes every `id` and its `url(#…)`/`href="#…"` references so several
 * rendered figures can coexist in one document — on the canvas and in the
 * composited export — without their clip-path and gradient ids colliding.
 */
export function namespaceSvgIds(svg: string, prefix: string): string {
  return svg
    .replace(/\bid="([^"]+)"/g, `id="${prefix}-$1"`)
    .replace(/\burl\(#([^)]+)\)/g, `url(#${prefix}-$1)`)
    .replace(/(\bxlink:href|\bhref)="#([^"]+)"/g, `$1="#${prefix}-$2"`);
}
