<script lang="ts">
  import IconArrowLeft from '~icons/lucide/arrow-left';
  import IconDownload from '~icons/lucide/download';
  import IconPlus from '~icons/lucide/plus';
  import IconEye from '~icons/lucide/eye';
  import IconEyeOff from '~icons/lucide/eye-off';
  import IconTrash from '~icons/lucide/trash-2';
  import IconMaximize from '~icons/lucide/maximize';
  import IconUndo from '~icons/lucide/undo-2';
  import IconRedo from '~icons/lucide/redo-2';
  import IconChevronUp from '~icons/lucide/chevron-up';
  import IconChevronDown from '~icons/lucide/chevron-down';
  import IconAlignLeft from '~icons/lucide/align-start-vertical';
  import IconAlignCenterX from '~icons/lucide/align-center-vertical';
  import IconAlignRight from '~icons/lucide/align-end-vertical';
  import IconAlignTop from '~icons/lucide/align-start-horizontal';
  import IconAlignMiddle from '~icons/lucide/align-center-horizontal';
  import IconAlignBottom from '~icons/lucide/align-end-horizontal';
  import ColourField from './ColourField.svelte';
  import InlineRename from './InlineRename.svelte';
  import { untrack } from 'svelte';
  import {
    makePlotObject,
    cloneObject,
    objectFigureSpec,
    namespaceSvgIds,
    stripOuterBackground,
    svgInner,
    svgViewBox,
    kindHasItemColor,
    defaultItemColor,
    PLOT_KINDS,
    plotKindLabel,
    type PlotKind,
    type PlotObject
  } from './plots';
  import type {
    AudioInfo,
    CoreClientLike,
    FigureColormapName,
    FigureThemeName
  } from './types';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    annotationId: bigint | null;
    theme: 'light' | 'dark';
    /** The editor's live time selection, so a new object or the selected one can
     *  be scoped to the span picked in Analyse. Null when nothing is selected. */
    selection?: { t0: number; t1: number } | null;
    projectName?: string;
    /** Renames the open project from the breadcrumb; absent leaves it read-only. */
    onRenameProject?: (name: string) => void;
    onExit?: () => void;
  }

  let {
    client,
    audio,
    annotationId,
    theme,
    selection = null,
    projectName,
    onRenameProject,
    onExit,
  }: Props = $props();

  function handleBackKeydown(event: KeyboardEvent) {
    // The crumb carries a rename field, which claims its own keys.
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onExit?.();
    }
  }

  const COLORMAPS: FigureColormapName[] = [
    'viridis',
    'magma',
    'inferno',
    'plasma',
    'cividis',
    'grayscale'
  ];

  // A representative CSS gradient per ramp, so the picker shows what each looks
  // like rather than naming it in a dropdown.
  const RAMP_CSS: Record<FigureColormapName, string> = {
    viridis: 'linear-gradient(90deg, #440154, #31688e, #35b779, #fde725)',
    magma: 'linear-gradient(90deg, #000004, #721f81, #f1605d, #fcfdbf)',
    inferno: 'linear-gradient(90deg, #000004, #932667, #f98e09, #fcffa4)',
    plasma: 'linear-gradient(90deg, #0d0887, #9c179e, #ed7953, #f0f921)',
    cividis: 'linear-gradient(90deg, #00224e, #575d6d, #a59c74, #fee838)',
    grayscale: 'linear-gradient(90deg, #000000, #808080, #ffffff)'
  };

  // The artboard: a fixed physical sheet the objects live on, in canvas pixels.
  let paperW = $state(760);
  let paperH = $state(520);
  // Figures default to light panels on white paper — the print default — no
  // matter the app's screen theme; the panel theme is switchable for screen.
  let paperTheme = $state<FigureThemeName>('light');
  // The paper (canvas) colour behind the plot panels; the panels keep their
  // own light/dark backgrounds set by the theme above.
  let paperBg = $state('#ffffff');
  // An optional figure title drawn at the top of the paper and into the export.
  let figTitle = $state('');
  // Axis-line/tick colour across every panel; `null` keeps the theme default.
  let axisColor = $state<string | null>(null);
  // Whether interior grid lines are drawn on data panels.
  let showGrid = $state(true);

  // Artboard size quick-picks. Widths track the common journal figure widths
  // (single ≈ 3.5 in, double ≈ 7 in) at ~110 px/in so the on-screen paper reads
  // at a comfortable working scale; heights are sensible starting canvases.
  const SIZE_PRESETS: { label: string; w: number; h: number }[] = [
    { label: 'Single column', w: 390, h: 300 },
    { label: 'Single column, tall', w: 390, h: 520 },
    { label: 'Double column', w: 770, h: 420 },
    { label: 'Double column, tall', w: 770, h: 620 },
    { label: 'Slide (16:9)', w: 960, h: 540 },
    { label: 'Square', w: 520, h: 520 }
  ];

  let objects = $state<PlotObject[]>([]);
  let selectedId = $state<string | null>(null);
  const selected = $derived(objects.find((o) => o.id === selectedId) ?? null);

  // Undo/redo over the whole editable figure state. Each mutating action snaps
  // the pre-change state onto the history; undo swaps it back. Praat offers only
  // one level of undo — a real editor keeps the whole trail.
  interface Snapshot {
    objects: PlotObject[];
    paperW: number;
    paperH: number;
    paperBg: string;
    paperTheme: FigureThemeName;
    figTitle: string;
    axisColor: string | null;
    showGrid: boolean;
  }
  let history = $state<Snapshot[]>([]);
  let future = $state<Snapshot[]>([]);

  function snapshot(): Snapshot {
    return {
      objects: objects.map((o) => ({ ...o })),
      paperW,
      paperH,
      paperBg,
      paperTheme,
      figTitle,
      axisColor,
      showGrid
    };
  }

  function pushHistory(s: Snapshot = snapshot()) {
    history = [...history.slice(-49), s];
    future = [];
  }

  // A drag captures its pre-move state up front and only commits it to history
  // on release if the object actually moved, so a plain click never floods the
  // undo trail with no-op steps.
  let dragBaseline: Snapshot | null = null;
  let dragChanged = false;

  // A run of arrow-key nudges coalesces into one undo step: the first nudge
  // snapshots the pre-nudge state, later nudges mutate in place, and any other
  // interaction ends the run so the next nudge starts a fresh step.
  let nudging = false;

  function restore(s: Snapshot) {
    objects = s.objects.map((o) => ({ ...o }));
    paperW = s.paperW;
    paperH = s.paperH;
    paperBg = s.paperBg;
    paperTheme = s.paperTheme;
    figTitle = s.figTitle;
    axisColor = s.axisColor;
    showGrid = s.showGrid;
    if (selectedId && !objects.some((o) => o.id === selectedId)) selectedId = null;
  }

  function undo() {
    if (history.length === 0) return;
    const cur = snapshot();
    const prev = history[history.length - 1];
    history = history.slice(0, -1);
    future = [cur, ...future];
    restore(prev);
  }

  function redo() {
    if (future.length === 0) return;
    const cur = snapshot();
    const next = future[0];
    future = future.slice(1);
    history = [...history, cur];
    restore(next);
  }

  // Each object's rendered SVG (namespaced), keyed by id, plus the render key
  // it was produced for so a render only re-runs when a visual input changes.
  let renders = $state<Record<string, { svg: string; key: string; vb: { w: number; h: number } }>>(
    {}
  );

  // Canvas viewport.
  let zoom = $state(1);
  let panX = $state(60);
  let panY = $state(40);

  let canvasEl = $state<HTMLDivElement | null>(null);
  let addOpen = $state(false);

  function renderKey(o: PlotObject): string {
    return [
      o.kind,
      o.t0,
      o.t1,
      o.freqCeiling,
      o.colormap,
      o.itemColor ?? '',
      Math.round(o.w),
      Math.round(o.h),
      paperTheme,
      axisColor ?? '',
      showGrid ? 'g1' : 'g0'
    ].join(':');
  }

  // Render (or re-render) any object whose visual inputs changed. Position and
  // z-order changes don't re-render — they only move existing pixels.
  const renderTokens = new Map<string, number>();
  $effect(() => {
    if (!client || !audio) return;
    // While a resize is dragging, the placed SVG stretches to the box via CSS;
    // re-rendering every pixel would fire a build_figure round-trip per frame.
    // Reading `drag` keeps this reactive, so the final size renders once the
    // drag ends. A move never changes size, so it needs no such guard.
    if (drag?.mode === 'resize' || drag?.mode === 'paper') return;
    for (const o of objects) {
      // Text labels are drawn directly, not composed from an engine figure.
      if (o.kind === 'text') continue;
      const key = renderKey(o);
      if (renders[o.id]?.key === key) continue;
      const token = (renderTokens.get(o.id) ?? 0) + 1;
      renderTokens.set(o.id, token);
      const spec = objectFigureSpec(o, audio, annotationId, paperTheme, {
        color: axisColor,
        showGrid
      });
      void (async () => {
        try {
          const json = await client.buildFigure(spec);
          const raw = await client.renderFigureSvg(json);
          if (renderTokens.get(o.id) !== token) return;
          const svg = stripOuterBackground(namespaceSvgIds(raw, o.id));
          renders = { ...renders, [o.id]: { svg, key, vb: svgViewBox(raw) } };
        } catch {
          // Leave the last good render in place on a transient failure.
        }
      })();
    }
  });

  const STACK_MARGIN = 24;
  const STACK_GAP = 12;

  function addObject(kind: PlotKind) {
    addOpen = false;
    pushHistory();
    // Stack new objects below the existing ones and align their left edges, so
    // adding waveform → spectrogram → tiers composes a clean figure straight
    // away; they stay fully draggable afterwards.
    const bottom = objects.reduce((b, o) => Math.max(b, o.y + o.h), STACK_MARGIN - STACK_GAP);
    const obj = makePlotObject(kind, STACK_MARGIN, bottom + STACK_GAP);
    // A live selection in Analyse scopes the new object to that span, so the
    // figure opens on the region the user was just looking at.
    if (selection) {
      obj.t0 = selection.t0;
      obj.t1 = selection.t1;
    }
    objects = [...objects, obj];
    selectedId = obj.id;
    // Grow the artboard to hold the new object with a margin.
    paperW = Math.max(paperW, obj.x + obj.w + STACK_MARGIN);
    paperH = Math.max(paperH, obj.y + obj.h + STACK_MARGIN);
    // Move focus to the canvas so the new, selected object nudges with the arrow
    // keys straight away, without a click to hand off focus from the toolbar.
    canvasEl?.focus();
  }

  function duplicateSelected() {
    if (!selected) return;
    pushHistory();
    const clone = cloneObject(selected);
    objects = [...objects, clone];
    selectedId = clone.id;
    canvasEl?.focus();
    paperW = Math.max(paperW, clone.x + clone.w + STACK_MARGIN);
    paperH = Math.max(paperH, clone.y + clone.h + STACK_MARGIN);
  }

  function deleteObject(id: string) {
    pushHistory();
    objects = objects.filter((o) => o.id !== id);
    delete renders[id];
    renders = { ...renders };
    if (selectedId === id) selectedId = null;
  }

  function toggleVisible(id: string) {
    pushHistory();
    objects = objects.map((o) => (o.id === id ? { ...o, visible: !o.visible } : o));
  }

  // Later in the array draws on top, so "forward" moves toward the end.
  function reorder(id: string, delta: 1 | -1) {
    const i = objects.findIndex((o) => o.id === id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= objects.length) return;
    pushHistory();
    const next = [...objects];
    [next[i], next[j]] = [next[j], next[i]];
    objects = next;
  }

  function reorderEnd(id: string, end: 'front' | 'back') {
    const i = objects.findIndex((o) => o.id === id);
    if (i < 0) return;
    if (end === 'front' && i === objects.length - 1) return;
    if (end === 'back' && i === 0) return;
    pushHistory();
    const next = objects.filter((o) => o.id !== id);
    if (end === 'front') next.push(objects[i]);
    else next.unshift(objects[i]);
    objects = next;
  }

  const isFront = (id: string) => objects.length > 0 && objects[objects.length - 1].id === id;
  const isBack = (id: string) => objects.length > 0 && objects[0].id === id;

  function patchSelected(patch: Partial<PlotObject>) {
    if (!selectedId) return;
    pushHistory();
    objects = objects.map((o) => (o.id === selectedId ? { ...o, ...patch } : o));
  }

  function renameObject(id: string, name: string) {
    pushHistory();
    objects = objects.map((o) => (o.id === id ? { ...o, name } : o));
  }

  type Align = 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom';

  // Snaps the selected object's box to a paper edge or centre line.
  function alignSelected(where: Align) {
    if (!selected) return;
    const o = selected;
    const patch: Partial<PlotObject> =
      where === 'left'
        ? { x: 0 }
        : where === 'center'
          ? { x: Math.round((paperW - o.w) / 2) }
          : where === 'right'
            ? { x: paperW - o.w }
            : where === 'top'
              ? { y: 0 }
              : where === 'middle'
                ? { y: Math.round((paperH - o.h) / 2) }
                : { y: paperH - o.h };
    patchSelected(patch);
  }

  // Applies a change without touching history — the colour popover snapshots
  // once on open, so its continuous edits stay one undo step.
  function setSelected(patch: Partial<PlotObject>) {
    if (!selectedId) return;
    objects = objects.map((o) => (o.id === selectedId ? { ...o, ...patch } : o));
  }

  // --- Direct manipulation: move and resize in artboard coordinates ---

  type Drag =
    | { mode: 'move'; id: string; startX: number; startY: number; ox: number; oy: number }
    | {
        mode: 'resize';
        id: string;
        handle: string;
        startX: number;
        startY: number;
        ox: number;
        oy: number;
        ow: number;
        oh: number;
      }
    | { mode: 'paper'; handle: string; startX: number; startY: number; ow: number; oh: number };
  let drag = $state<Drag | null>(null);

  // Panning the canvas by dragging empty space (objects stop propagation, so
  // this only starts on the bare workspace); a click with no travel deselects.
  let pan = $state<{ startX: number; startY: number; ox: number; oy: number; moved: boolean } | null>(
    null
  );

  function onCanvasPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    nudging = false;
    pan = { startX: event.clientX, startY: event.clientY, ox: panX, oy: panY, moved: false };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function clientToArtboard(clientX: number, clientY: number): { x: number; y: number } {
    const rect = canvasEl?.getBoundingClientRect();
    if (!rect) return { x: 0, y: 0 };
    return { x: (clientX - rect.left - panX) / zoom, y: (clientY - rect.top - panY) / zoom };
  }

  function startMove(event: PointerEvent, o: PlotObject) {
    if (event.button !== 0) return;
    event.stopPropagation();
    nudging = false;
    selectedId = o.id;
    dragBaseline = snapshot();
    dragChanged = false;
    const p = clientToArtboard(event.clientX, event.clientY);
    drag = { mode: 'move', id: o.id, startX: p.x, startY: p.y, ox: o.x, oy: o.y };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function startResize(event: PointerEvent, o: PlotObject, handle: string) {
    if (event.button !== 0) return;
    event.stopPropagation();
    nudging = false;
    selectedId = o.id;
    dragBaseline = snapshot();
    dragChanged = false;
    const p = clientToArtboard(event.clientX, event.clientY);
    drag = {
      mode: 'resize',
      id: o.id,
      handle,
      startX: p.x,
      startY: p.y,
      ox: o.x,
      oy: o.y,
      ow: o.w,
      oh: o.h
    };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  const MIN_SIZE = 80;
  const PAPER_MIN = 200;
  const SNAP_PX = 6;

  // Drag the paper's right, bottom, or bottom-right corner to resize the
  // artboard directly, the same gesture as the numeric Width/Height fields.
  function startPaperResize(event: PointerEvent, handle: string) {
    if (event.button !== 0) return;
    event.stopPropagation();
    nudging = false;
    selectedId = null;
    dragBaseline = snapshot();
    dragChanged = false;
    const p = clientToArtboard(event.clientX, event.clientY);
    drag = { mode: 'paper', handle, startX: p.x, startY: p.y, ow: paperW, oh: paperH };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  // Alignment guides shown while a snap is active, in artboard coordinates.
  let guideX = $state<number | null>(null);
  let guideY = $state<number | null>(null);

  // Snap the moving object's edges and centre to the other objects' edges and
  // to the artboard, so panels line up cleanly the way a design tool expects.
  function snapMove(id: string, x: number, y: number, w: number, h: number): { x: number; y: number } {
    const others = objects.filter((o) => o.id !== id && o.visible);
    const xTargets = [0, paperW / 2, paperW];
    const yTargets = [0, paperH / 2, paperH];
    for (const o of others) {
      xTargets.push(o.x, o.x + o.w / 2, o.x + o.w);
      yTargets.push(o.y, o.y + o.h / 2, o.y + o.h);
    }
    const threshold = SNAP_PX / zoom;

    const best = (edges: number[], targets: number[]) => {
      let delta = 0;
      let guide: number | null = null;
      let dist = threshold;
      for (const e of edges) {
        for (const t of targets) {
          const d = Math.abs(e - t);
          if (d < dist) {
            dist = d;
            delta = t - e;
            guide = t;
          }
        }
      }
      return { delta, guide };
    };

    const sx = best([x, x + w / 2, x + w], xTargets);
    const sy = best([y, y + h / 2, y + h], yTargets);
    guideX = sx.guide;
    guideY = sy.guide;
    return { x: x + sx.delta, y: y + sy.delta };
  }

  type Box = { ox: number; oy: number; ow: number; oh: number };

  // Snap the edge(s) being dragged to the other objects' edges and the artboard,
  // so a resized panel lines up flush the same way a moved one does.
  function snapResize(excludeId: string, handle: string, box: Box): Box {
    const others = objects.filter((o) => o.id !== excludeId && o.visible);
    const xTargets = [0, paperW / 2, paperW];
    const yTargets = [0, paperH / 2, paperH];
    for (const o of others) {
      xTargets.push(o.x, o.x + o.w / 2, o.x + o.w);
      yTargets.push(o.y, o.y + o.h / 2, o.y + o.h);
    }
    const threshold = SNAP_PX / zoom;
    const nearest = (value: number, targets: number[]): number | null => {
      let hit: number | null = null;
      let dist = threshold;
      for (const t of targets) {
        const d = Math.abs(value - t);
        if (d < dist) {
          dist = d;
          hit = t;
        }
      }
      return hit;
    };

    let { ox, oy, ow, oh } = box;
    let gx: number | null = null;
    let gy: number | null = null;

    if (handle.includes('e')) {
      const t = nearest(ox + ow, xTargets);
      if (t !== null) {
        ow = Math.max(MIN_SIZE, t - ox);
        gx = t;
      }
    } else if (handle.includes('w')) {
      const right = ox + ow;
      const t = nearest(ox, xTargets);
      if (t !== null) {
        ox = Math.min(t, right - MIN_SIZE);
        ow = right - ox;
        gx = t;
      }
    }
    if (handle.includes('s')) {
      const t = nearest(oy + oh, yTargets);
      if (t !== null) {
        oh = Math.max(MIN_SIZE, t - oy);
        gy = t;
      }
    } else if (handle.includes('n')) {
      const bottom = oy + oh;
      const t = nearest(oy, yTargets);
      if (t !== null) {
        oy = Math.min(t, bottom - MIN_SIZE);
        oh = bottom - oy;
        gy = t;
      }
    }

    guideX = gx;
    guideY = gy;
    return { ox, oy, ow, oh };
  }

  function onPointerMove(event: PointerEvent) {
    if (pan) {
      const dx = event.clientX - pan.startX;
      const dy = event.clientY - pan.startY;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) pan.moved = true;
      panX = pan.ox + dx;
      panY = pan.oy + dy;
      return;
    }
    if (!drag) return;
    dragChanged = true;
    const p = clientToArtboard(event.clientX, event.clientY);
    const dx = p.x - drag.startX;
    const dy = p.y - drag.startY;
    if (drag.mode === 'paper') {
      const d = drag;
      if (d.handle.includes('e')) paperW = Math.max(PAPER_MIN, Math.round(d.ow + dx));
      if (d.handle.includes('s')) paperH = Math.max(PAPER_MIN, Math.round(d.oh + dy));
    } else if (drag.mode === 'move') {
      const d = drag;
      const moving = objects.find((o) => o.id === d.id);
      const snapped = moving
        ? snapMove(d.id, d.ox + dx, d.oy + dy, moving.w, moving.h)
        : { x: d.ox + dx, y: d.oy + dy };
      objects = objects.map((o) => (o.id === d.id ? { ...o, x: snapped.x, y: snapped.y } : o));
    } else {
      const d = drag;
      let { ox, oy, ow, oh } = d;
      const west = d.handle.includes('w');
      const north = d.handle.includes('n');
      if (d.handle.includes('e')) ow = Math.max(MIN_SIZE, d.ow + dx);
      if (d.handle.includes('s')) oh = Math.max(MIN_SIZE, d.oh + dy);
      if (west) ow = Math.max(MIN_SIZE, d.ow - dx);
      if (north) oh = Math.max(MIN_SIZE, d.oh - dy);
      // Shift on a corner handle keeps the object's original proportions,
      // driven by whichever axis the pointer pulled further.
      const corner = (d.handle.includes('e') || west) && (d.handle.includes('s') || north);
      const lockAspect = event.shiftKey && corner && d.oh > 0;
      if (lockAspect) {
        const aspect = d.ow / d.oh;
        if (Math.abs(ow - d.ow) >= Math.abs(oh - d.oh) * aspect) {
          oh = Math.max(MIN_SIZE, Math.round(ow / aspect));
          ow = Math.round(oh * aspect);
        } else {
          ow = Math.max(MIN_SIZE, Math.round(oh * aspect));
          oh = Math.round(ow / aspect);
        }
      }
      // Re-anchor the west / north edges to the final size.
      if (west) ox = d.ox + (d.ow - ow);
      if (north) oy = d.oy + (d.oh - oh);
      // Edge snapping would fight a locked ratio, so only snap when free.
      if (!lockAspect) ({ ox, oy, ow, oh } = snapResize(d.id, d.handle, { ox, oy, ow, oh }));
      objects = objects.map((o) => (o.id === d.id ? { ...o, x: ox, y: oy, w: ow, h: oh } : o));
    }
  }

  function endDrag(event: PointerEvent) {
    if (drag || pan) {
      try {
        (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
      } catch {
        // capture may already be gone
      }
    }
    // A move or resize that actually changed the object commits its pre-drag
    // snapshot to the undo trail.
    if (drag && dragChanged && dragBaseline) pushHistory(dragBaseline);
    dragBaseline = null;
    dragChanged = false;
    // A bare click on the workspace (no pan travel) clears the selection.
    if (pan && !pan.moved) selectedId = null;
    pan = null;
    drag = null;
    guideX = null;
    guideY = null;
  }

  const NUDGE_KEYS: Record<string, { dx: number; dy: number }> = {
    ArrowLeft: { dx: -1, dy: 0 },
    ArrowRight: { dx: 1, dy: 0 },
    ArrowUp: { dx: 0, dy: -1 },
    ArrowDown: { dx: 0, dy: 1 }
  };

  function onCanvasKey(event: KeyboardEvent) {
    // A bare modifier press (Shift held to switch to a coarse nudge) must not
    // end the current nudge run before the arrow key arrives.
    if (event.key === 'Shift' || event.key === 'Control' || event.key === 'Alt' || event.key === 'Meta') {
      return;
    }
    const nudge = NUDGE_KEYS[event.key];
    if (nudge && selectedId) {
      event.preventDefault();
      const o = objects.find((x) => x.id === selectedId);
      if (!o) return;
      // Shift nudges by a coarse step, matching the arrange tools' feel.
      const step = event.shiftKey ? 10 : 1;
      if (!nudging) {
        pushHistory();
        nudging = true;
      }
      setSelected({ x: o.x + nudge.dx * step, y: o.y + nudge.dy * step });
      return;
    }
    nudging = false;
    if (event.key === 'Escape') {
      event.preventDefault();
      if (drag) {
        // Abort the drag: snap the object(s) back to where the drag began, so a
        // mis-drag doesn't have to be finished and then undone.
        if (dragBaseline) restore(dragBaseline);
        drag = null;
        dragBaseline = null;
        dragChanged = false;
        guideX = null;
        guideY = null;
      } else if (selectedId) {
        selectedId = null;
      }
      return;
    }
    if ((event.key === 'Delete' || event.key === 'Backspace') && selectedId) {
      event.preventDefault();
      deleteObject(selectedId);
      return;
    }
    // Bracket keys restack the selection: one step per press, or all the way
    // to the front / back with Shift, mirroring the arrange buttons.
    if ((event.key === '[' || event.key === ']') && selectedId) {
      event.preventDefault();
      if (event.shiftKey) reorderEnd(selectedId, event.key === ']' ? 'front' : 'back');
      else reorder(selectedId, event.key === ']' ? 1 : -1);
      return;
    }
    const mod = event.ctrlKey || event.metaKey;
    if (mod && event.key.toLowerCase() === 'z') {
      event.preventDefault();
      if (event.shiftKey) redo();
      else undo();
      return;
    }
    if (mod && event.key.toLowerCase() === 'y') {
      event.preventDefault();
      redo();
      return;
    }
    if (mod && event.key.toLowerCase() === 'd' && selectedId) {
      event.preventDefault();
      duplicateSelected();
    }
  }

  function onWheel(event: WheelEvent) {
    const rect = canvasEl?.getBoundingClientRect();
    if (!rect) return;
    event.preventDefault();
    if (event.ctrlKey || event.metaKey) {
      const mx = event.clientX - rect.left;
      const my = event.clientY - rect.top;
      const factor = event.deltaY < 0 ? 1.1 : 0.9;
      const next = Math.min(3, Math.max(0.25, zoom * factor));
      // Keep the point under the cursor fixed while zooming.
      panX = mx - ((mx - panX) * next) / zoom;
      panY = my - ((my - panY) * next) / zoom;
      zoom = next;
      return;
    }
    // Plain wheel / two-finger trackpad scroll pans; Shift makes a vertical-only
    // mouse wheel pan sideways.
    if (event.shiftKey) {
      panX -= event.deltaY || event.deltaX;
    } else {
      panX -= event.deltaX;
      panY -= event.deltaY;
    }
  }

  function fitView() {
    const rect = canvasEl?.getBoundingClientRect();
    if (!rect) return;
    const z = Math.min((rect.width - 80) / paperW, (rect.height - 80) / paperH, 1.5);
    zoom = Math.max(0.25, z);
    panX = (rect.width - paperW * zoom) / 2;
    panY = (rect.height - paperH * zoom) / 2;
  }

  // Shrink the paper to hug the objects with a uniform margin, shifting them so
  // the top-left inset matches — the one-click "tighten for export" that the
  // grow-only add flow can't reach.
  function fitPaperToContent() {
    const shown = objects.filter((o) => o.visible);
    if (shown.length === 0) return;
    const minX = Math.min(...shown.map((o) => o.x));
    const minY = Math.min(...shown.map((o) => o.y));
    const maxX = Math.max(...shown.map((o) => o.x + o.w));
    const maxY = Math.max(...shown.map((o) => o.y + o.h));
    const m = STACK_MARGIN;
    const dx = m - minX;
    const dy = m - minY;
    if (dx === 0 && dy === 0 && paperW === maxX + m && paperH === maxY + m) return;
    pushHistory();
    objects = objects.map((o) => ({ ...o, x: o.x + dx, y: o.y + dy }));
    paperW = Math.round(maxX - minX + m * 2);
    paperH = Math.round(maxY - minY + m * 2);
  }

  // --- Export: composite every visible object into one artboard-sized SVG ---

  function composeSvg(): string {
    const bg = paperBg;
    const parts = objects
      .filter((o) => o.visible && (o.kind === 'text' ? o.text.trim() !== '' : renders[o.id]))
      .map((o) => {
        if (o.kind === 'text') {
          const ink = o.itemColor ?? defaultItemColor('text');
          const spans = o.text
            .split('\n')
            .map(
              (line, i) =>
                `<tspan x="${o.x}" dy="${i === 0 ? o.fontSize : o.fontSize * 1.25}">${escapeXml(line)}</tspan>`
            )
            .join('');
          return `<text x="${o.x}" y="${o.y}" font-family="system-ui, -apple-system, 'Segoe UI', sans-serif" font-size="${o.fontSize}" fill="${ink}">${spans}</text>`;
        }
        const r = renders[o.id];
        return `<svg x="${o.x}" y="${o.y}" width="${o.w}" height="${o.h}" viewBox="0 0 ${r.vb.w} ${r.vb.h}" preserveAspectRatio="none" overflow="visible">${svgInner(r.svg)}</svg>`;
      })
      .join('');
    const titleInk = paperTheme === 'dark' ? '#e9efec' : '#1c1c1c';
    const title = figTitle
      ? `<text x="24" y="30" font-family="Georgia, serif" font-size="17" font-weight="600" fill="${titleInk}">${escapeXml(figTitle)}</text>`
      : '';
    return `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${paperW}" height="${paperH}" viewBox="0 0 ${paperW} ${paperH}"><rect width="${paperW}" height="${paperH}" fill="${bg}"/>${title}${parts}</svg>`;
  }

  function escapeXml(s: string): string {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function saveBlob(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = name;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  const hasContent = $derived(
    objects.some((o) => o.visible && (o.kind === 'text' ? o.text.trim() !== '' : renders[o.id]))
  );

  let exportOpen = $state(false);
  let copied = $state(false);

  // A file-safe name from the figure title, else the project, else "figure",
  // so an export lands as e.g. `danger-trail.svg` and needs no rename.
  function figureFilename(ext: string): string {
    const slug = (s: string) =>
      s
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '')
        .slice(0, 60);
    const base = slug(figTitle) || slug(projectName ?? '') || 'figure';
    return `${base}.${ext}`;
  }

  function exportSvg() {
    exportOpen = false;
    if (!hasContent) return;
    saveBlob(new Blob([composeSvg()], { type: 'image/svg+xml;charset=utf-8' }), figureFilename('svg'));
  }

  // Rasterize the composed figure to a PNG blob at 2× for crisp output, shared
  // by the download and the clipboard copy.
  async function renderPngBlob(): Promise<Blob | null> {
    const scale = 2;
    const svg = composeSvg();
    const url = URL.createObjectURL(new Blob([svg], { type: 'image/svg+xml;charset=utf-8' }));
    try {
      const image = new Image();
      await new Promise<void>((resolve, reject) => {
        image.onload = () => resolve();
        image.onerror = () => reject(new Error('SVG could not be rasterized'));
        image.src = url;
      });
      const canvas = document.createElement('canvas');
      canvas.width = paperW * scale;
      canvas.height = paperH * scale;
      const ctx = canvas.getContext('2d');
      if (!ctx) throw new Error('canvas 2D context unavailable');
      ctx.drawImage(image, 0, 0, canvas.width, canvas.height);
      return await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
    } finally {
      URL.revokeObjectURL(url);
    }
  }

  async function exportPng() {
    exportOpen = false;
    if (!hasContent) return;
    const png = await renderPngBlob();
    if (png) saveBlob(png, figureFilename('png'));
  }

  // Copy the figure as a PNG to the clipboard — paste it straight into a paper
  // draft, a slide, or a chat.
  async function copyImage() {
    exportOpen = false;
    if (!hasContent) return;
    try {
      const png = await renderPngBlob();
      if (png && 'clipboard' in navigator && 'write' in navigator.clipboard) {
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': png })]);
        copied = true;
        setTimeout(() => (copied = false), 1600);
      }
    } catch {
      // Clipboard image write is unsupported or blocked; the downloads remain.
    }
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (
      e.target instanceof Element &&
      !e.target.closest('.add-wrap') &&
      !e.target.closest('.export-wrap')
    ) {
      addOpen = false;
      exportOpen = false;
    }
  }}
/>

<div class="plots" data-testid="plots-view">
  <header class="bar">
    <nav class="crumb" aria-label="Location">
      {#if onExit}
        <span
          class="crumb-back"
          role="button"
          tabindex="0"
          data-testid="plots-back"
          onclick={() => onExit?.()}
          onkeydown={handleBackKeydown}
        >
          <IconArrowLeft aria-hidden="true" />
          {#if onRenameProject}
            <InlineRename
              name={projectName ?? 'Project'}
              class="crumb-back-name"
              label="Rename project"
              testId="rename-plots-project"
              onRename={(next) => onRenameProject?.(next)}
            />
          {:else}
            <span>{projectName ?? 'Project'}</span>
          {/if}
        </span>
        <span class="crumb-sep" aria-hidden="true">›</span>
      {/if}
      <span class="crumb-current">Figure — {audio?.name ?? ''}</span>
    </nav>

    <div class="add-wrap">
      <button
        type="button"
        class="add"
        data-testid="plots-add"
        aria-expanded={addOpen}
        onclick={() => (addOpen = !addOpen)}
      >
        <IconPlus aria-hidden="true" />
        <span>Add plot</span>
      </button>
      {#if addOpen}
        <div class="add-menu" role="menu">
          {#each PLOT_KINDS as { kind, label } (kind)}
            <button
              type="button"
              role="menuitem"
              data-testid="plots-add-{kind}"
              onclick={() => addObject(kind)}>{label}</button
            >
          {/each}
        </div>
      {/if}
    </div>

    <div class="spacer"></div>

    <button
      type="button"
      class="ghost"
      data-testid="plots-undo"
      title="Undo (Ctrl+Z)"
      disabled={history.length === 0}
      onclick={undo}
    >
      <IconUndo aria-hidden="true" />
    </button>
    <button
      type="button"
      class="ghost"
      data-testid="plots-redo"
      title="Redo (Ctrl+Shift+Z)"
      disabled={future.length === 0}
      onclick={redo}
    >
      <IconRedo aria-hidden="true" />
    </button>
    <button
      type="button"
      class="ghost"
      data-testid="plots-fit"
      title="Fit to view"
      onclick={fitView}
    >
      <IconMaximize aria-hidden="true" />
    </button>
    <div class="export-wrap">
      <button
        type="button"
        class="export"
        data-testid="plots-export"
        aria-haspopup="menu"
        aria-expanded={exportOpen}
        disabled={!hasContent}
        onclick={() => (exportOpen = !exportOpen)}
      >
        <IconDownload aria-hidden="true" />
        <span>{copied ? 'Copied' : 'Export'}</span>
      </button>
      {#if exportOpen}
        <div class="export-menu" role="menu">
          <button type="button" role="menuitem" data-testid="plots-export-svg" onclick={exportSvg}
            >Download SVG</button
          >
          <button type="button" role="menuitem" data-testid="plots-export-png" onclick={exportPng}
            >Download PNG</button
          >
          <button type="button" role="menuitem" data-testid="plots-copy-image" onclick={copyImage}
            >Copy image</button
          >
        </div>
      {/if}
    </div>
  </header>

  <div class="body">
    <!-- Layers panel: every placed object, front-most last (top of the list). -->
    <aside class="layers-panel" aria-label="Objects">
      <h2>Objects</h2>
      {#if objects.length === 0}
        <p class="layers-empty">Add a plot to begin.</p>
      {:else}
        <ul class="layers">
          {#each [...objects].reverse() as o (o.id)}
            <li>
              <div class="layer" class:sel={o.id === selectedId}>
                <button
                  type="button"
                  class="layer-eye"
                  aria-label={o.visible ? 'Hide' : 'Show'}
                  onclick={() => toggleVisible(o.id)}
                >
                  {#if o.visible}<IconEye aria-hidden="true" />{:else}<IconEyeOff aria-hidden="true" />{/if}
                </button>
                <InlineRename
                  name={o.name}
                  class="layer-name"
                  label="Rename plot object"
                  testId="plots-layer-item"
                  onActivate={() => (selectedId = o.id)}
                  onRename={(next) => renameObject(o.id, next)}
                />
                <button
                  type="button"
                  class="layer-move"
                  data-testid="plots-forward"
                  aria-label="Bring forward"
                  title="Bring forward ( ] )"
                  disabled={isFront(o.id)}
                  onclick={() => reorder(o.id, 1)}
                >
                  <IconChevronUp aria-hidden="true" />
                </button>
                <button
                  type="button"
                  class="layer-move"
                  aria-label="Send backward"
                  title="Send backward ( [ )"
                  disabled={isBack(o.id)}
                  onclick={() => reorder(o.id, -1)}
                >
                  <IconChevronDown aria-hidden="true" />
                </button>
                <button
                  type="button"
                  class="layer-del"
                  aria-label="Delete"
                  onclick={() => deleteObject(o.id)}
                >
                  <IconTrash aria-hidden="true" />
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </aside>

    <!-- The canvas: a pannable workspace holding the artboard. -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="canvas"
      bind:this={canvasEl}
      role="application"
      tabindex="0"
      data-testid="plots-canvas"
      class:panning={pan !== null}
      onpointerdown={onCanvasPointerDown}
      onpointermove={onPointerMove}
      onpointerup={endDrag}
      onkeydown={onCanvasKey}
      onwheel={onWheel}
    >
      <div
        class="artboard-wrap"
        style:transform="translate({panX}px, {panY}px) scale({zoom})"
        style:--z={zoom}
      >
        <div
          class="artboard"
          class:dark={paperTheme === 'dark'}
          data-testid="plots-artboard"
          style:width="{paperW}px"
          style:height="{paperH}px"
          style:background={paperBg}
        >
          {#if figTitle}
            <div class="fig-title" class:dark={paperTheme === 'dark'} data-testid="plots-title">
              {figTitle}
            </div>
          {/if}
          {#if objects.length === 0}
            <p class="art-hint">Add a plot from the toolbar, then drag to arrange it.</p>
          {/if}
          {#if guideX !== null}
            <span class="guide vert" style:left="{guideX}px"></span>
          {/if}
          {#if guideY !== null}
            <span class="guide horz" style:top="{guideY}px"></span>
          {/if}
          {#each objects as o (o.id)}
            {#if o.visible}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="obj"
                class:sel={o.id === selectedId}
                data-testid="plots-obj"
                data-kind={o.kind}
                style:left="{o.x}px"
                style:top="{o.y}px"
                style:width="{o.w}px"
                style:height="{o.h}px"
                onpointerdown={(e) => startMove(e, o)}
              >
                {#if o.kind === 'text'}
                  <div
                    class="obj-text"
                    style:font-size="{o.fontSize}px"
                    style:color={o.itemColor ?? defaultItemColor('text')}
                  >{o.text}</div>
                {:else if renders[o.id]}
                  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                  <div class="obj-svg">{@html renders[o.id].svg}</div>
                {:else}
                  <div class="obj-loading">{plotKindLabel(o.kind)}…</div>
                {/if}
                {#if o.id === selectedId}
                  {#each ['n', 'e', 's', 'w', 'nw', 'ne', 'sw', 'se'] as h (h)}
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <span
                      class="handle {h}"
                      data-testid="plots-handle-{h}"
                      title={h.length === 2 ? 'Drag to resize · Shift keeps ratio' : 'Drag to resize'}
                      onpointerdown={(e) => startResize(e, o, h)}
                    ></span>
                  {/each}
                  {#if drag && drag.mode !== 'paper' && drag.id === o.id}
                    <span class="size-badge" data-testid="plots-size-badge"
                      >{Math.round(o.w)} × {Math.round(o.h)}</span
                    >
                  {/if}
                {/if}
              </div>
            {/if}
          {/each}

          {#each ['e', 's', 'se'] as h (h)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <span
              class="paper-handle {h}"
              class:active={drag?.mode === 'paper'}
              data-testid="plots-paper-handle-{h}"
              title="Drag to resize the paper"
              onpointerdown={(e) => startPaperResize(e, h)}
            ></span>
          {/each}
          {#if drag?.mode === 'paper'}
            <span class="paper-size-badge" data-testid="plots-paper-size-badge"
              >{paperW} × {paperH}</span
            >
          {/if}
        </div>
      </div>
    </div>

    <!-- Properties: the selected object, or the artboard when nothing is picked. -->
    <aside class="props" aria-label="Properties">
      {#if selected}
        <div class="props-head">
          <h2>{plotKindLabel(selected.kind)}</h2>
          <button
            type="button"
            class="reset"
            data-testid="plots-duplicate"
            title="Duplicate (Ctrl+D)"
            onclick={duplicateSelected}>Duplicate</button
          >
        </div>
        <label class="field">
          <span>Name</span>
          <input
            type="text"
            data-testid="plots-obj-name"
            value={selected.name}
            oninput={(e) => patchSelected({ name: e.currentTarget.value })}
          />
        </label>
        <div class="frame">
          <span class="frame-h">Frame</span>
          <div class="frame-grid">
            <label class="field mini">
              <span>X</span>
              <input
                type="number"
                data-testid="plots-frame-x"
                value={Math.round(selected.x)}
                oninput={(e) => patchSelected({ x: Number(e.currentTarget.value) })}
              />
            </label>
            <label class="field mini">
              <span>Y</span>
              <input
                type="number"
                value={Math.round(selected.y)}
                oninput={(e) => patchSelected({ y: Number(e.currentTarget.value) })}
              />
            </label>
            <label class="field mini">
              <span>W</span>
              <input
                type="number"
                min="80"
                data-testid="plots-frame-w"
                value={Math.round(selected.w)}
                oninput={(e) => patchSelected({ w: Math.max(80, Number(e.currentTarget.value)) })}
              />
            </label>
            <label class="field mini">
              <span>H</span>
              <input
                type="number"
                min="80"
                data-testid="plots-frame-h"
                value={Math.round(selected.h)}
                oninput={(e) => patchSelected({ h: Math.max(80, Number(e.currentTarget.value)) })}
              />
            </label>
          </div>
        </div>
        <div class="align" role="group" aria-label="Align to paper">
          <span class="frame-h">Align</span>
          <div class="align-grid">
            <button type="button" data-testid="plots-align-left" title="Align left" onclick={() => alignSelected('left')}>
              <IconAlignLeft aria-hidden="true" />
            </button>
            <button type="button" data-testid="plots-align-center" title="Centre horizontally" onclick={() => alignSelected('center')}>
              <IconAlignCenterX aria-hidden="true" />
            </button>
            <button type="button" data-testid="plots-align-right" title="Align right" onclick={() => alignSelected('right')}>
              <IconAlignRight aria-hidden="true" />
            </button>
            <button type="button" data-testid="plots-align-top" title="Align top" onclick={() => alignSelected('top')}>
              <IconAlignTop aria-hidden="true" />
            </button>
            <button type="button" data-testid="plots-align-middle" title="Centre vertically" onclick={() => alignSelected('middle')}>
              <IconAlignMiddle aria-hidden="true" />
            </button>
            <button type="button" data-testid="plots-align-bottom" title="Align bottom" onclick={() => alignSelected('bottom')}>
              <IconAlignBottom aria-hidden="true" />
            </button>
          </div>
        </div>
        {#if selected.kind === 'text'}
          <label class="field">
            <span>Text</span>
            <textarea
              class="text-input"
              rows="2"
              data-testid="plots-text-input"
              value={selected.text}
              oninput={(e) => patchSelected({ text: e.currentTarget.value })}
            ></textarea>
          </label>
          <label class="field">
            <span>Type size (px)</span>
            <input
              type="number"
              min="8"
              max="200"
              step="1"
              data-testid="plots-font-size"
              value={selected.fontSize}
              oninput={(e) =>
                patchSelected({ fontSize: Math.max(8, Number(e.currentTarget.value)) })}
            />
          </label>
        {/if}
        {#if selected.kind !== 'text'}
        <div class="field-row">
          <label class="field">
            <span>Start (s)</span>
            <input
              type="number"
              min="0"
              step="0.05"
              placeholder="0"
              data-testid="plots-window-start"
              value={selected.t0 ?? ''}
              oninput={(e) =>
                patchSelected({ t0: e.currentTarget.value === '' ? null : Number(e.currentTarget.value) })}
            />
          </label>
          <label class="field">
            <span>End (s)</span>
            <input
              type="number"
              min="0"
              step="0.05"
              placeholder={audio ? audio.duration.toFixed(2) : ''}
              data-testid="plots-window-end"
              value={selected.t1 ?? ''}
              oninput={(e) =>
                patchSelected({ t1: e.currentTarget.value === '' ? null : Number(e.currentTarget.value) })}
            />
          </label>
        </div>
        <div class="window-actions">
          {#if selection}
            <button
              type="button"
              class="mini"
              data-testid="plots-use-selection"
              onclick={() => patchSelected({ t0: selection.t0, t1: selection.t1 })}
            >
              Use selection ({selection.t0.toFixed(2)}–{selection.t1.toFixed(2)} s)
            </button>
          {/if}
          {#if selected.t0 !== null || selected.t1 !== null}
            <button
              type="button"
              class="mini"
              data-testid="plots-full-recording"
              onclick={() => patchSelected({ t0: null, t1: null })}
            >
              Full recording
            </button>
          {/if}
        </div>
        {/if}
        {#if selected.kind === 'spectrogram' || selected.kind === 'formant' || selected.kind === 'spectralSlice' || selected.kind === 'ltas'}
          <label class="field">
            <span>Frequency ceiling (Hz)</span>
            <input
              type="number"
              min="500"
              step="500"
              value={selected.freqCeiling}
              oninput={(e) => patchSelected({ freqCeiling: Number(e.currentTarget.value) })}
            />
          </label>
        {/if}
        {#if selected.kind === 'spectrogram'}
          <div class="field">
            <span>Colour ramp</span>
            <div class="ramp-swatches" role="radiogroup" aria-label="Spectrogram colour ramp">
              {#each COLORMAPS as c (c)}
                <button
                  type="button"
                  class="ramp"
                  class:on={selected.colormap === c}
                  role="radio"
                  aria-checked={selected.colormap === c}
                  title={c[0].toUpperCase() + c.slice(1)}
                  data-testid="plots-ramp-{c}"
                  style:background={RAMP_CSS[c]}
                  onclick={() => patchSelected({ colormap: c })}
                ></button>
              {/each}
            </div>
          </div>
        {/if}
        {#if kindHasItemColor(selected.kind)}
          <div class="field">
            <span>Item colour</span>
            <span class="colour-row">
              <ColourField
                testId="plots-item-color"
                value={selected.itemColor ?? defaultItemColor(selected.kind)}
                onOpen={() => pushHistory()}
                onChange={(hex) => setSelected({ itemColor: hex })}
              />
              {#if selected.itemColor}
                <button
                  type="button"
                  class="reset"
                  title="Reset to default"
                  onclick={() => patchSelected({ itemColor: null })}>Reset</button
                >
              {/if}
            </span>
          </div>
        {/if}
      {:else}
        <h2>Artboard</h2>
        <label class="field">
          <span>Figure title</span>
          <input
            type="text"
            data-testid="plots-title-input"
            placeholder="e.g. Figure 1 — Danger trail"
            bind:value={figTitle}
            onfocus={() => pushHistory()}
          />
        </label>
        <label class="field">
          <span>Preset size</span>
          <select
            data-testid="plots-size-preset"
            aria-label="Artboard size preset"
            onchange={(e) => {
              const p = SIZE_PRESETS.find((s) => s.label === e.currentTarget.value);
              if (p) {
                pushHistory();
                paperW = p.w;
                paperH = p.h;
              }
              e.currentTarget.selectedIndex = 0;
            }}
          >
            <option value="">Choose a size…</option>
            {#each SIZE_PRESETS as p (p.label)}
              <option value={p.label}>{p.label} · {p.w}×{p.h}</option>
            {/each}
          </select>
        </label>
        <div class="field-row">
          <label class="field">
            <span>Width (px)</span>
            <input type="number" min="200" step="20" bind:value={paperW} />
          </label>
          <label class="field">
            <span>Height (px)</span>
            <input type="number" min="200" step="20" bind:value={paperH} />
          </label>
        </div>
        <p class="phys" data-testid="plots-phys-size">
          Exports at {(paperW / 96).toFixed(1)} × {(paperH / 96).toFixed(1)} in ({Math.round(
            (paperW / 96) * 25.4
          )} × {Math.round((paperH / 96) * 25.4)} mm)
        </p>
        <button
          type="button"
          class="fit-paper"
          data-testid="plots-fit-paper"
          disabled={objects.length === 0}
          onclick={fitPaperToContent}
        >
          Fit paper to content
        </button>
        <div class="field-row">
          <div class="field">
            <span>Background</span>
            <span class="colour-row">
              <ColourField
                testId="plots-bg-color"
                value={paperBg}
                onOpen={() => pushHistory()}
                onChange={(hex) => (paperBg = hex)}
              />
              {#if paperBg.toLowerCase() !== '#ffffff'}
                <button type="button" class="reset" title="White" onclick={() => (paperBg = '#ffffff')}
                  >White</button
                >
              {/if}
            </span>
          </div>
          <label class="field">
            <span>Panel theme</span>
            <select bind:value={paperTheme} data-testid="plots-theme">
              <option value="light">Light</option>
              <option value="dark">Dark</option>
            </select>
          </label>
        </div>
        <div class="field-row">
          <div class="field">
            <span>Axis colour</span>
            <span class="colour-row">
              <ColourField
                testId="plots-axis-color"
                value={axisColor ?? '#666666'}
                onOpen={() => pushHistory()}
                onChange={(hex) => (axisColor = hex)}
              />
              {#if axisColor !== null}
                <button type="button" class="reset" title="Theme default" onclick={() => (axisColor = null)}
                  >Default</button
                >
              {/if}
            </span>
          </div>
          <label class="field check-field">
            <span>Grid lines</span>
            <input
              type="checkbox"
              data-testid="plots-grid"
              checked={showGrid}
              onchange={(e) => {
                pushHistory();
                showGrid = e.currentTarget.checked;
              }}
            />
          </label>
        </div>
        <p class="props-hint">Select an object to edit its plot, or add one from the toolbar.</p>
      {/if}
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
    cursor: pointer;
  }

  .crumb-back:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .crumb-back:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .phys {
    margin: 0;
    color: var(--muted);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
  }

  .fit-paper {
    width: 100%;
    padding: 0.4rem 0.6rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    font-size: 0.78rem;
    cursor: pointer;
  }

  .fit-paper:hover:not(:disabled) {
    border-color: color-mix(in oklab, var(--accent) 40%, var(--chrome-strong));
    background: var(--panel);
  }

  .fit-paper:disabled {
    opacity: 0.5;
    cursor: default;
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
    max-width: 14rem;
  }

  .add-wrap {
    position: relative;
    margin-left: 0.4rem;
  }

  .add {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    min-height: 1.9rem;
    padding: 0 0.7rem;
    border: 1px solid color-mix(in oklab, var(--accent) 40%, var(--chrome-strong));
    border-radius: var(--radius-sm);
    background: var(--accent-tint);
    color: var(--accent-strong);
    font: inherit;
    font-size: 0.8rem;
    font-weight: 600;
  }

  .add:hover {
    background: color-mix(in oklab, var(--accent) 22%, var(--panel));
  }

  .add-menu {
    position: absolute;
    top: calc(100% + 0.3rem);
    left: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 9rem;
    padding: 0.25rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md, 8px);
    background: var(--panel);
    box-shadow: var(--shadow-lg, 0 8px 30px rgba(0, 0, 0, 0.4));
  }

  .add-menu button {
    text-align: left;
    padding: 0.4rem 0.5rem;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 0.82rem;
  }

  .add-menu button:hover {
    background: var(--accent-tint);
    color: var(--accent-strong);
  }

  .spacer {
    flex: 1 1 auto;
  }

  .ghost {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.9rem;
    height: 1.9rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--muted);
  }

  .ghost:hover:not(:disabled) {
    background: var(--panel);
    color: var(--text);
  }

  .ghost:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .export {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    min-height: 1.9rem;
    padding: 0 0.7rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
    font-weight: 600;
  }

  .export:hover:not(:disabled) {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .export :global(svg) {
    font-size: 0.9rem;
  }

  .export:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .export-wrap {
    position: relative;
  }

  .export-menu {
    position: absolute;
    top: calc(100% + 0.3rem);
    right: 0;
    z-index: 20;
    display: flex;
    flex-direction: column;
    min-width: 9rem;
    padding: 0.25rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md, 8px);
    background: var(--panel);
    box-shadow: var(--shadow-lg, 0 8px 30px rgba(0, 0, 0, 0.4));
  }

  .export-menu button {
    text-align: left;
    padding: 0.4rem 0.5rem;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 0.82rem;
  }

  .export-menu button:hover {
    background: var(--accent-tint);
    color: var(--accent-strong);
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }

  .layers-panel {
    flex: none;
    width: 12rem;
    min-width: 12rem;
    border-right: 1px solid var(--chrome-strong);
    background: var(--panel);
    overflow-y: auto;
    padding: 0.6rem 0.5rem;
  }

  .layers-panel h2,
  .props h2 {
    margin: 0 0 0.5rem;
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .layers-empty,
  .props-hint,
  .art-hint {
    color: var(--muted);
    font-size: 0.78rem;
    line-height: 1.5;
  }

  .layers {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .layer {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    border-radius: var(--radius-sm);
    padding: 0.1rem 0.15rem;
  }

  .layer.sel {
    background: var(--accent-tint);
  }

  .layer-eye,
  .layer-del,
  .layer-move {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.4rem;
    height: 1.5rem;
    flex: none;
    border: none;
    background: transparent;
    color: var(--muted);
    border-radius: var(--radius-sm);
  }

  .layer-move {
    width: 1.15rem;
    opacity: 0;
  }

  .layer:hover .layer-move,
  .layer.sel .layer-move {
    opacity: 1;
  }

  .layer-eye:hover,
  .layer-del:hover,
  .layer-move:hover:not(:disabled) {
    background: var(--panel-soft);
    color: var(--text);
  }

  .layer-move:disabled {
    opacity: 0.2 !important;
    cursor: default;
  }

  .layer-move :global(svg) {
    font-size: 0.8rem;
  }

  .layer-del:hover {
    color: var(--danger, #d66);
  }

  .layer-eye :global(svg),
  .layer-del :global(svg) {
    font-size: 0.85rem;
  }

  .layer :global(.inline-rename) {
    flex: 1;
    min-width: 0;
  }

  .layer :global(.layer-name) {
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.2rem;
  }

  .layer.sel :global(.layer-name) {
    color: var(--accent-strong);
    font-weight: 600;
  }

  .canvas {
    flex: 1 1 auto;
    min-width: 0;
    position: relative;
    overflow: hidden;
    outline: none;
    /* Dragging objects or panning must never sweep-select the figure text. */
    user-select: none;
    -webkit-user-select: none;
    background:
      radial-gradient(120% 90% at 50% 0%, color-mix(in oklab, var(--accent) 5%, transparent), transparent 60%),
      var(--canvas);
    background-image:
      radial-gradient(circle at 1px 1px, color-mix(in oklab, var(--muted) 18%, transparent) 1px, transparent 0);
    background-size: 22px 22px;
    cursor: grab;
  }

  .canvas.panning {
    cursor: grabbing;
  }

  .artboard-wrap {
    position: absolute;
    top: 0;
    left: 0;
    transform-origin: 0 0;
    /* The whole artboard is scaled by --z, so chrome sizes are divided by it to
       stay a constant number of screen pixels at any zoom. --z is set inline. */
    --hs: calc(10px / var(--z, 1)); /* handle size */
    --hb: calc(1.5px / var(--z, 1)); /* handle border / selection outline */
    --ho: calc(-6px / var(--z, 1)); /* handle inset from the edge */
    --gl: calc(1px / var(--z, 1)); /* hairline: guides, hover outline */
  }

  .artboard {
    position: relative;
    background: #fff;
    box-shadow:
      0 1px 2px rgba(0, 0, 0, 0.3),
      0 16px 50px rgba(0, 0, 0, 0.4);
  }

  .artboard.dark {
    background: #0c1211;
  }

  .art-hint {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    text-align: center;
    padding: 2rem;
    pointer-events: none;
  }

  .artboard.dark .art-hint {
    color: color-mix(in oklab, #fff 55%, transparent);
  }

  .fig-title {
    position: absolute;
    top: 0.6rem;
    left: 1.5rem;
    right: 1.5rem;
    font-family: Georgia, 'Times New Roman', serif;
    font-size: 1.05rem;
    font-weight: 600;
    line-height: 1.3;
    color: #1c1c1c;
    pointer-events: none;
    z-index: 3;
  }

  .fig-title.dark {
    color: #e9efec;
  }

  /* Alignment guides while snapping — thin gold rules across the artboard. */
  .guide {
    position: absolute;
    background: var(--warn);
    z-index: 5;
    pointer-events: none;
  }

  .guide.vert {
    top: 0;
    bottom: 0;
    width: var(--gl);
    margin-left: calc(var(--gl) / -2);
  }

  .guide.horz {
    left: 0;
    right: 0;
    height: var(--gl);
    margin-top: calc(var(--gl) / -2);
  }

  .obj {
    position: absolute;
    line-height: 0;
    cursor: move;
  }

  /* A faint outline on hover tells you an unselected panel is grabbable. */
  .obj:hover:not(.sel) {
    outline: var(--gl) solid color-mix(in oklab, var(--accent) 50%, transparent);
    outline-offset: var(--gl);
  }

  .obj.sel {
    outline: var(--hb) solid var(--accent);
    outline-offset: var(--gl);
  }

  .obj-svg,
  .obj-svg :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
  }

  .obj-loading {
    width: 100%;
    height: 100%;
    display: grid;
    place-items: center;
    font-size: 0.75rem;
    color: var(--muted);
    background: color-mix(in oklab, var(--muted) 8%, transparent);
    line-height: 1.2;
  }

  /* A text label draws from the box's top-left, matching the exported <text>. */
  .obj-text {
    width: 100%;
    height: 100%;
    font-family: system-ui, -apple-system, 'Segoe UI', sans-serif;
    line-height: 1.25;
    white-space: pre-wrap;
    overflow: hidden;
  }

  .handle {
    position: absolute;
    width: var(--hs);
    height: var(--hs);
    background: var(--panel);
    border: var(--hb) solid var(--accent);
    border-radius: calc(2px / var(--z, 1));
  }

  /* Live dimensions while moving or resizing, Figma-style. */
  .size-badge {
    position: absolute;
    left: 50%;
    bottom: -1.6rem;
    transform: translateX(-50%);
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    background: var(--accent);
    color: var(--ink, #03211b);
    font-size: 0.68rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
  }

  .handle.nw {
    top: var(--ho);
    left: var(--ho);
    cursor: nwse-resize;
  }

  .handle.ne {
    top: var(--ho);
    right: var(--ho);
    cursor: nesw-resize;
  }

  .handle.sw {
    bottom: var(--ho);
    left: var(--ho);
    cursor: nesw-resize;
  }

  .handle.se {
    bottom: var(--ho);
    right: var(--ho);
    cursor: nwse-resize;
  }

  /* Mid-edge handles: resize one dimension without touching the other. */
  .handle.n {
    top: var(--ho);
    left: 50%;
    transform: translateX(-50%);
    cursor: ns-resize;
  }

  .handle.s {
    bottom: var(--ho);
    left: 50%;
    transform: translateX(-50%);
    cursor: ns-resize;
  }

  .handle.w {
    top: 50%;
    left: var(--ho);
    transform: translateY(-50%);
    cursor: ew-resize;
  }

  .handle.e {
    top: 50%;
    right: var(--ho);
    transform: translateY(-50%);
    cursor: ew-resize;
  }

  /* Grips on the paper's own edges: drag to resize the artboard directly. */
  .paper-handle {
    position: absolute;
    z-index: 5;
    background: color-mix(in oklab, var(--accent) 45%, transparent);
    border-radius: 999px;
    opacity: 0.55;
    transition: opacity 0.12s ease, background 0.12s ease;
  }

  .paper-handle:hover,
  .paper-handle.active {
    opacity: 1;
    background: var(--accent);
  }

  .paper-handle.e {
    top: 50%;
    right: calc(-4px / var(--z, 1));
    width: calc(6px / var(--z, 1));
    height: calc(44px / var(--z, 1));
    transform: translateY(-50%);
    cursor: ew-resize;
  }

  .paper-handle.s {
    left: 50%;
    bottom: calc(-4px / var(--z, 1));
    width: calc(44px / var(--z, 1));
    height: calc(6px / var(--z, 1));
    transform: translateX(-50%);
    cursor: ns-resize;
  }

  .paper-handle.se {
    right: var(--ho);
    bottom: var(--ho);
    width: calc(14px / var(--z, 1));
    height: calc(14px / var(--z, 1));
    border-radius: calc(3px / var(--z, 1));
    cursor: nwse-resize;
  }

  .paper-size-badge {
    position: absolute;
    right: 0;
    bottom: -1.7rem;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    background: var(--accent);
    color: var(--ink, #03211b);
    font-size: 0.68rem;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    pointer-events: none;
    z-index: 5;
  }

  .props {
    flex: none;
    width: 15rem;
    min-width: 15rem;
    border-left: 1px solid var(--chrome-strong);
    background: var(--panel);
    overflow-y: auto;
    padding: 0.7rem 0.75rem;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    font-size: 0.72rem;
    color: var(--muted);
    min-width: 0;
  }

  .field-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.5rem;
  }

  .check-field {
    flex-direction: row;
    align-items: center;
    gap: 0.45rem;
    align-self: end;
    min-height: 1.8rem;
  }

  .check-field input[type='checkbox'] {
    flex: none;
    width: 1rem;
    height: 1rem;
    min-height: 0;
    padding: 0;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .field input,
  .field select {
    min-height: 1.8rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
    padding: 0 0.4rem;
    min-width: 0;
  }

  .props-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .frame-h {
    display: block;
    font-size: 0.66rem;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 0.3rem;
  }

  .frame-grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr 1fr;
    gap: 0.3rem;
  }

  .align-grid {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 0.3rem;
  }

  .align-grid button {
    display: grid;
    place-items: center;
    aspect-ratio: 1;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--muted);
    cursor: pointer;
  }

  .align-grid button:hover {
    background: var(--panel);
    color: var(--text);
    border-color: color-mix(in oklab, var(--accent) 40%, var(--chrome-strong));
  }

  .align-grid :global(svg) {
    width: 1rem;
    height: 1rem;
  }

  .field.mini {
    gap: 0.15rem;
  }

  .field.mini span {
    font-size: 0.66rem;
    text-align: center;
  }

  .field.mini input {
    text-align: center;
    padding: 0 0.2rem;
  }

  .ramp-swatches {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 0.3rem;
  }

  .ramp {
    height: 1.4rem;
    border: 1.5px solid var(--chrome-strong);
    border-radius: 4px;
    padding: 0;
    cursor: pointer;
    transition: border-color var(--t-fast);
  }

  .ramp:hover {
    border-color: color-mix(in oklab, var(--accent) 45%, var(--chrome-strong));
  }

  .ramp.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .colour-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .reset {
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--muted);
    font: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.45rem;
    cursor: pointer;
  }

  .reset:hover {
    background: var(--panel);
    color: var(--text);
  }

  .text-input {
    resize: vertical;
    min-height: 2.4rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    padding: 0.35rem 0.45rem;
  }

  .window-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .window-actions .mini {
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--muted);
    font: inherit;
    font-size: 0.72rem;
    padding: 0.2rem 0.45rem;
    cursor: pointer;
  }

  .window-actions .mini:hover {
    background: var(--panel);
    color: var(--text);
  }

  @media (max-width: 720px) {
    .plots {
      inset: var(--safe-top) var(--safe-right)
        calc(var(--mobile-rail-height) + var(--safe-bottom)) var(--safe-left);
    }

    .bar {
      gap: 0.4rem;
      min-height: 3.25rem;
      padding: 0.25rem 0.4rem;
      overflow-x: auto;
      scrollbar-width: none;
    }

    .bar::-webkit-scrollbar {
      display: none;
    }

    .crumb-current,
    .crumb-sep {
      display: none;
    }

    .crumb {
      flex: none;
    }

    .crumb-back :global(.inline-rename) {
      display: none;
    }

    .crumb-back,
    .add,
    .ghost,
    .export {
      min-height: 2.75rem;
    }

    .crumb-back span {
      display: none;
    }

    .crumb-back,
    .ghost {
      width: 2.75rem;
      min-width: 2.75rem;
      justify-content: center;
      padding: 0;
    }

    .add-menu,
    .export-menu {
      position: fixed;
      top: calc(var(--safe-top) + 3.75rem);
      right: calc(var(--safe-right) + 0.5rem);
      left: calc(var(--safe-left) + 0.5rem);
      min-width: 0;
    }

    .add-menu button,
    .export-menu button {
      min-height: 2.75rem;
    }

    .body {
      flex-direction: column;
    }

    .canvas {
      order: 1;
      min-height: 40%;
      touch-action: none;
    }

    .layers-panel {
      order: 2;
      width: 100%;
      min-width: 0;
      max-height: 5rem;
      padding: 0.35rem;
      border-top: 1px solid var(--chrome-strong);
      border-right: none;
      overflow-x: auto;
      overflow-y: hidden;
    }

    .layers-panel h2 {
      display: none;
    }

    .layers {
      width: max-content;
      min-width: 100%;
      flex-direction: row;
    }

    .layer {
      min-width: 10rem;
      min-height: 3.5rem;
    }

    .layer-eye,
    .layer-del,
    .layer-move {
      min-width: 2.75rem;
      min-height: 2.75rem;
      opacity: 1;
    }

    .props {
      order: 3;
      width: 100%;
      min-width: 0;
      max-height: 38%;
      padding: 0.65rem;
      border-top: 1px solid var(--chrome-strong);
      border-left: none;
    }

    .field input,
    .field select,
    .fit-paper,
    .reset,
    .window-actions .mini {
      min-height: 2.75rem;
    }

    .artboard-wrap {
      --hs: calc(18px / var(--z, 1));
      --ho: calc(-10px / var(--z, 1));
    }
  }
</style>
