<script lang="ts">
  import type { Selection, SelectionMode, ViewportState } from './types';

  interface Props {
    viewport: ViewportState;
    mode: SelectionMode;
    selection: Selection | null;
    onChange: (selection: Selection | null) => void;
    onSeek?: (time: number) => void;
    /** Applies an incremental two-finger time zoom and pan gesture. */
    onViewportGesture?: (factor: number, anchorRatio: number, panRatio: number) => void;
    /**
     * Double-click intent: `zoom` when the second click lands inside the live
     * selection, `fit` when it lands on empty pane space.
     */
    onDoubleZoom?: (intent: 'zoom' | 'fit') => void;
  }

  let { viewport, mode, selection, onChange, onSeek, onViewportGesture, onDoubleZoom }: Props =
    $props();

  let root = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);
  // Pixel origin of the drag, to tell a click apart from a box.
  let startX = 0;
  let startY = 0;
  let startT = 0;
  let startF = 0;
  // Timestamp and place of the last click, to recognise a double-click without
  // the DOM `dblclick` (which would arrive after the first click already cleared
  // the selection).
  let lastClickMs = 0;
  let lastClickX = 0;
  let lastClickY = 0;
  const touchPointers = new Map<number, { x: number; y: number }>();
  let gestureActive = false;
  let gestureDistance = 0;
  let gestureCenterX = 0;

  const CLICK_SLOP_PX = 3;
  const DOUBLE_CLICK_MS = 350;

  function capturePointer(pointerId: number) {
    try {
      root?.setPointerCapture(pointerId);
    } catch {
      // Synthetic pointer tests have no browser-managed pointer to capture.
    }
  }

  function releasePointer(pointerId: number) {
    try {
      if (root?.hasPointerCapture(pointerId)) root.releasePointerCapture(pointerId);
    } catch {
      // The browser may already have released a cancelled pointer.
    }
  }

  // Whether a signal point falls inside the current selection: time for a
  // waveform selection, time and frequency for a spectrogram box.
  function insideSelection(time: number, freq: number): boolean {
    if (!selection) return false;
    if (time < selection.t0 || time > selection.t1) return false;
    if (selection.mode === 'box') return freq >= selection.f0 && freq <= selection.f1;
    return true;
  }

  function ratios(event: PointerEvent) {
    const rect = root!.getBoundingClientRect();
    const rx = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    const ry = Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height));
    return { rx, ry };
  }

  function timeAt(rx: number) {
    return viewport.t0 + rx * (viewport.t1 - viewport.t0);
  }

  function freqAt(ry: number) {
    return viewport.f1 - ry * (viewport.f1 - viewport.f0);
  }

  function build(curT: number, curF: number): Selection {
    if (mode === 'time') {
      return {
        t0: Math.min(startT, curT),
        t1: Math.max(startT, curT),
        f0: viewport.f0,
        f1: viewport.f1,
        mode
      };
    }
    return {
      t0: Math.min(startT, curT),
      t1: Math.max(startT, curT),
      f0: Math.min(startF, curF),
      f1: Math.max(startF, curF),
      mode
    };
  }

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0 || !root) return;
    // Own the gesture: the timeline's click-to-seek must not also fire.
    event.stopPropagation();
    if (event.pointerType === 'touch') {
      event.preventDefault();
      touchPointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      capturePointer(event.pointerId);
      if (touchPointers.size >= 2) {
        dragging = false;
        gestureActive = true;
        rememberGesture();
        return;
      }
    }
    const { rx, ry } = ratios(event);
    startX = event.clientX;
    startY = event.clientY;
    startT = timeAt(rx);
    startF = freqAt(ry);
    dragging = true;
    capturePointer(event.pointerId);
  }

  function onPointerMove(event: PointerEvent) {
    if (event.pointerType === 'touch' && touchPointers.has(event.pointerId)) {
      touchPointers.set(event.pointerId, { x: event.clientX, y: event.clientY });
      if (gestureActive && touchPointers.size >= 2) {
        event.preventDefault();
        event.stopPropagation();
        applyGesture();
        return;
      }
    }
    if (!dragging) return;
    event.stopPropagation();
    const { rx, ry } = ratios(event);
    onChange(build(timeAt(rx), freqAt(ry)));
  }

  function onPointerUp(event: PointerEvent) {
    if (event.pointerType === 'touch' && touchPointers.has(event.pointerId)) {
      touchPointers.delete(event.pointerId);
      releasePointer(event.pointerId);
      if (gestureActive) {
        event.preventDefault();
        event.stopPropagation();
        dragging = false;
        gestureDistance = 0;
        if (touchPointers.size === 0) gestureActive = false;
        return;
      }
    }
    if (!dragging) return;
    event.stopPropagation();
    dragging = false;
    releasePointer(event.pointerId);
    const movedX = Math.abs(event.clientX - startX);
    const movedY = Math.abs(event.clientY - startY);
    const isClick = movedX < CLICK_SLOP_PX && (mode === 'time' || movedY < CLICK_SLOP_PX);
    if (isClick) {
      const now = performance.now();
      const isDouble =
        now - lastClickMs < DOUBLE_CLICK_MS &&
        Math.abs(event.clientX - lastClickX) < CLICK_SLOP_PX &&
        Math.abs(event.clientY - lastClickY) < CLICK_SLOP_PX;
      lastClickMs = now;
      lastClickX = event.clientX;
      lastClickY = event.clientY;
      const inside = insideSelection(startT, startF);
      if (isDouble) {
        // Inside the live selection zooms to it; empty space fits the file.
        onDoubleZoom?.(inside ? 'zoom' : 'fit');
        lastClickMs = 0;
        return;
      }
      // A click inside the selection seeks but keeps the box, so a following
      // second click can still zoom to it; a click on empty space clears.
      if (inside) onSeek?.(startT);
      else {
        onChange(null);
        onSeek?.(startT);
      }
      return;
    }
    const { rx, ry } = ratios(event);
    onChange(build(timeAt(rx), freqAt(ry)));
  }

  function gesturePoints(): [{ x: number; y: number }, { x: number; y: number }] | null {
    const points = [...touchPointers.values()];
    return points.length >= 2 ? [points[0], points[1]] : null;
  }

  function rememberGesture() {
    const points = gesturePoints();
    if (!points) return;
    const [a, b] = points;
    gestureDistance = Math.max(1, Math.hypot(b.x - a.x, b.y - a.y));
    gestureCenterX = (a.x + b.x) / 2;
  }

  function applyGesture() {
    const points = gesturePoints();
    if (!points || !root) return;
    const [a, b] = points;
    const distance = Math.max(1, Math.hypot(b.x - a.x, b.y - a.y));
    const centerX = (a.x + b.x) / 2;
    const rect = root.getBoundingClientRect();
    const anchorRatio = Math.min(1, Math.max(0, (centerX - rect.left) / rect.width));
    const factor = Math.min(2, Math.max(0.5, gestureDistance / distance));
    const panRatio = (gestureCenterX - centerX) / Math.max(1, rect.width);
    onViewportGesture?.(factor, anchorRatio, panRatio);
    gestureDistance = distance;
    gestureCenterX = centerX;
  }

  function onPointerCancel(event: PointerEvent) {
    touchPointers.delete(event.pointerId);
    dragging = false;
    gestureDistance = 0;
    if (touchPointers.size === 0) gestureActive = false;
  }

  // Selection geometry mapped to this pane's pixels, or null when the box has
  // slid entirely out of view.
  const rect = $derived.by(() => {
    if (!selection) return null;
    const span = viewport.t1 - viewport.t0;
    const left = ((selection.t0 - viewport.t0) / span) * 100;
    const right = ((selection.t1 - viewport.t0) / span) * 100;
    if (right < 0 || left > 100) return null;
    let top = 0;
    let bottom = 100;
    if (mode === 'box') {
      const fspan = Math.max(1, viewport.f1 - viewport.f0);
      top = (1 - (selection.f1 - viewport.f0) / fspan) * 100;
      bottom = (1 - (selection.f0 - viewport.f0) / fspan) * 100;
    }
    return {
      left: Math.max(0, left),
      width: Math.min(100, right) - Math.max(0, left),
      top: Math.max(0, top),
      height: Math.min(100, bottom) - Math.max(0, top)
    };
  });
</script>

<div
  bind:this={root}
  class="layer"
  role="application"
  aria-label={mode === 'box' ? 'Spectrogram selection' : 'Waveform selection'}
  data-testid="selection-layer-{mode}"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerCancel}
>
  {#if rect}
    <div
      class="box"
      data-testid="selection-box"
      data-sel-mode={mode}
      data-sel-t0={selection?.t0}
      data-sel-t1={selection?.t1}
      data-sel-f0={selection?.f0}
      data-sel-f1={selection?.f1}
      style="left:{rect.left}%; width:{rect.width}%; top:{rect.top}%; height:{rect.height}%;"
    ></div>
  {/if}
</div>

<style>
  .layer {
    position: absolute;
    inset: 0;
    z-index: 3;
    cursor: crosshair;
    touch-action: none;
  }

  .box {
    position: absolute;
    box-sizing: border-box;
    border: 1px solid var(--accent, #0f766e);
    background: color-mix(in oklab, var(--accent, #0f766e) 20%, transparent);
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--accent, #0f766e) 40%, transparent);
    pointer-events: none;
  }
</style>
