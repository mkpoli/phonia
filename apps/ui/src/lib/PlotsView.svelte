<script lang="ts">
  import IconArrowLeft from '~icons/lucide/arrow-left';
  import IconDownload from '~icons/lucide/download';
  import IconPlus from '~icons/lucide/plus';
  import IconEye from '~icons/lucide/eye';
  import IconEyeOff from '~icons/lucide/eye-off';
  import IconTrash from '~icons/lucide/trash-2';
  import IconMaximize from '~icons/lucide/maximize';
  import { untrack } from 'svelte';
  import {
    makePlotObject,
    objectFigureSpec,
    namespaceSvgIds,
    svgInner,
    svgViewBox,
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
    projectName?: string;
    onExit?: () => void;
  }

  let { client, audio, annotationId, theme, projectName, onExit }: Props = $props();

  const COLORMAPS: FigureColormapName[] = [
    'viridis',
    'magma',
    'inferno',
    'plasma',
    'cividis',
    'grayscale'
  ];

  // The artboard: a fixed physical sheet the objects live on, in canvas pixels.
  let paperW = $state(760);
  let paperH = $state(520);
  let paperTheme = $state<FigureThemeName>(untrack(() => theme));

  let objects = $state<PlotObject[]>([]);
  let selectedId = $state<string | null>(null);
  const selected = $derived(objects.find((o) => o.id === selectedId) ?? null);

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
    return [o.kind, o.t0, o.t1, o.freqCeiling, o.colormap, Math.round(o.w), Math.round(o.h), paperTheme].join(
      ':'
    );
  }

  // Render (or re-render) any object whose visual inputs changed. Position and
  // z-order changes don't re-render — they only move existing pixels.
  const renderTokens = new Map<string, number>();
  $effect(() => {
    if (!client || !audio) return;
    for (const o of objects) {
      const key = renderKey(o);
      if (renders[o.id]?.key === key) continue;
      const token = (renderTokens.get(o.id) ?? 0) + 1;
      renderTokens.set(o.id, token);
      const spec = objectFigureSpec(o, audio, annotationId, paperTheme);
      void (async () => {
        try {
          const json = await client.buildFigure(spec);
          const raw = await client.renderFigureSvg(json);
          if (renderTokens.get(o.id) !== token) return;
          const svg = namespaceSvgIds(raw, o.id);
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
    // Stack new objects below the existing ones and align their left edges, so
    // adding waveform → spectrogram → tiers composes a clean figure straight
    // away; they stay fully draggable afterwards.
    const bottom = objects.reduce((b, o) => Math.max(b, o.y + o.h), STACK_MARGIN - STACK_GAP);
    const obj = makePlotObject(kind, STACK_MARGIN, bottom + STACK_GAP);
    objects = [...objects, obj];
    selectedId = obj.id;
    // Grow the artboard to hold the new object with a margin.
    paperW = Math.max(paperW, obj.x + obj.w + STACK_MARGIN);
    paperH = Math.max(paperH, obj.y + obj.h + STACK_MARGIN);
  }

  function deleteObject(id: string) {
    objects = objects.filter((o) => o.id !== id);
    delete renders[id];
    renders = { ...renders };
    if (selectedId === id) selectedId = null;
  }

  function toggleVisible(id: string) {
    objects = objects.map((o) => (o.id === id ? { ...o, visible: !o.visible } : o));
  }

  function patchSelected(patch: Partial<PlotObject>) {
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
      };
  let drag: Drag | null = null;

  // Panning the canvas by dragging empty space (objects stop propagation, so
  // this only starts on the bare workspace); a click with no travel deselects.
  let pan = $state<{ startX: number; startY: number; ox: number; oy: number; moved: boolean } | null>(
    null
  );

  function onCanvasPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
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
    selectedId = o.id;
    const p = clientToArtboard(event.clientX, event.clientY);
    drag = { mode: 'move', id: o.id, startX: p.x, startY: p.y, ox: o.x, oy: o.y };
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function startResize(event: PointerEvent, o: PlotObject, handle: string) {
    if (event.button !== 0) return;
    event.stopPropagation();
    selectedId = o.id;
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
    const p = clientToArtboard(event.clientX, event.clientY);
    const dx = p.x - drag.startX;
    const dy = p.y - drag.startY;
    if (drag.mode === 'move') {
      const d = drag;
      objects = objects.map((o) => (o.id === d.id ? { ...o, x: d.ox + dx, y: d.oy + dy } : o));
    } else {
      const d = drag;
      let { ox, oy, ow, oh } = d;
      if (d.handle.includes('e')) ow = Math.max(MIN_SIZE, d.ow + dx);
      if (d.handle.includes('s')) oh = Math.max(MIN_SIZE, d.oh + dy);
      if (d.handle.includes('w')) {
        ow = Math.max(MIN_SIZE, d.ow - dx);
        ox = d.ox + (d.ow - ow);
      }
      if (d.handle.includes('n')) {
        oh = Math.max(MIN_SIZE, d.oh - dy);
        oy = d.oy + (d.oh - oh);
      }
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
    // A bare click on the workspace (no pan travel) clears the selection.
    if (pan && !pan.moved) selectedId = null;
    pan = null;
    drag = null;
  }

  function onCanvasKey(event: KeyboardEvent) {
    if ((event.key === 'Delete' || event.key === 'Backspace') && selectedId) {
      event.preventDefault();
      deleteObject(selectedId);
    }
  }

  function onWheel(event: WheelEvent) {
    if (!event.ctrlKey && !event.metaKey) return;
    event.preventDefault();
    const rect = canvasEl?.getBoundingClientRect();
    if (!rect) return;
    const mx = event.clientX - rect.left;
    const my = event.clientY - rect.top;
    const factor = event.deltaY < 0 ? 1.1 : 0.9;
    const next = Math.min(3, Math.max(0.25, zoom * factor));
    // Keep the point under the cursor fixed while zooming.
    panX = mx - ((mx - panX) * next) / zoom;
    panY = my - ((my - panY) * next) / zoom;
    zoom = next;
  }

  function fitView() {
    const rect = canvasEl?.getBoundingClientRect();
    if (!rect) return;
    const z = Math.min((rect.width - 80) / paperW, (rect.height - 80) / paperH, 1.5);
    zoom = Math.max(0.25, z);
    panX = (rect.width - paperW * zoom) / 2;
    panY = (rect.height - paperH * zoom) / 2;
  }

  // --- Export: composite every visible object into one artboard-sized SVG ---

  function composeSvg(): string {
    const bg = paperTheme === 'dark' ? '#0c1211' : '#ffffff';
    const parts = objects
      .filter((o) => o.visible && renders[o.id])
      .map((o) => {
        const r = renders[o.id];
        return `<svg x="${o.x}" y="${o.y}" width="${o.w}" height="${o.h}" viewBox="0 0 ${r.vb.w} ${r.vb.h}" preserveAspectRatio="none" overflow="visible">${svgInner(r.svg)}</svg>`;
      })
      .join('');
    return `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${paperW}" height="${paperH}" viewBox="0 0 ${paperW} ${paperH}"><rect width="${paperW}" height="${paperH}" fill="${bg}"/>${parts}</svg>`;
  }

  function saveBlob(blob: Blob, name: string) {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = name;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  const hasContent = $derived(objects.some((o) => o.visible && renders[o.id]));

  function exportSvg() {
    if (!hasContent) return;
    saveBlob(new Blob([composeSvg()], { type: 'image/svg+xml;charset=utf-8' }), 'figure.svg');
  }

  async function exportPng() {
    if (!hasContent) return;
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
      const png = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, 'image/png'));
      if (png) saveBlob(png, 'figure.png');
    } finally {
      URL.revokeObjectURL(url);
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
      data-testid="plots-fit"
      title="Fit to view"
      onclick={fitView}
    >
      <IconMaximize aria-hidden="true" />
    </button>
    <button
      type="button"
      class="export"
      data-testid="plots-export-svg"
      disabled={!hasContent}
      onclick={exportSvg}
    >
      <IconDownload aria-hidden="true" />
      <span>SVG</span>
    </button>
    <button
      type="button"
      class="export"
      data-testid="plots-export-png"
      disabled={!hasContent}
      onclick={exportPng}
    >
      <IconDownload aria-hidden="true" />
      <span>PNG</span>
    </button>
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
                <button
                  type="button"
                  class="layer-name"
                  data-testid="plots-layer-item"
                  onclick={() => (selectedId = o.id)}>{o.name}</button
                >
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
      <div class="artboard-wrap" style:transform="translate({panX}px, {panY}px) scale({zoom})">
        <div
          class="artboard"
          class:dark={paperTheme === 'dark'}
          data-testid="plots-artboard"
          style:width="{paperW}px"
          style:height="{paperH}px"
        >
          {#if objects.length === 0}
            <p class="art-hint">Add a plot from the toolbar, then drag to arrange it.</p>
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
                {#if renders[o.id]}
                  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                  <div class="obj-svg">{@html renders[o.id].svg}</div>
                {:else}
                  <div class="obj-loading">{plotKindLabel(o.kind)}…</div>
                {/if}
                {#if o.id === selectedId}
                  {#each ['nw', 'ne', 'sw', 'se'] as h (h)}
                    <!-- svelte-ignore a11y_no_static_element_interactions -->
                    <span
                      class="handle {h}"
                      data-testid="plots-handle-{h}"
                      onpointerdown={(e) => startResize(e, o, h)}
                    ></span>
                  {/each}
                {/if}
              </div>
            {/if}
          {/each}
        </div>
      </div>
    </div>

    <!-- Properties: the selected object, or the artboard when nothing is picked. -->
    <aside class="props" aria-label="Properties">
      {#if selected}
        <h2>{plotKindLabel(selected.kind)}</h2>
        <label class="field">
          <span>Name</span>
          <input
            type="text"
            data-testid="plots-obj-name"
            value={selected.name}
            oninput={(e) => patchSelected({ name: e.currentTarget.value })}
          />
        </label>
        <div class="field-row">
          <label class="field">
            <span>Start (s)</span>
            <input
              type="number"
              min="0"
              step="0.05"
              placeholder="0"
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
              value={selected.t1 ?? ''}
              oninput={(e) =>
                patchSelected({ t1: e.currentTarget.value === '' ? null : Number(e.currentTarget.value) })}
            />
          </label>
        </div>
        {#if selected.kind === 'spectrogram' || selected.kind === 'formant'}
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
          <label class="field">
            <span>Colour</span>
            <select
              value={selected.colormap}
              onchange={(e) => patchSelected({ colormap: e.currentTarget.value as FigureColormapName })}
            >
              {#each COLORMAPS as c (c)}
                <option value={c}>{c[0].toUpperCase() + c.slice(1)}</option>
              {/each}
            </select>
          </label>
        {/if}
      {:else}
        <h2>Artboard</h2>
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
        <label class="field">
          <span>Theme</span>
          <select bind:value={paperTheme} data-testid="plots-theme">
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </label>
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

  .ghost:hover {
    background: var(--panel);
    color: var(--text);
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
  .layer-del {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    flex: none;
    border: none;
    background: transparent;
    color: var(--muted);
    border-radius: var(--radius-sm);
  }

  .layer-eye:hover,
  .layer-del:hover {
    background: var(--panel-soft);
    color: var(--text);
  }

  .layer-del:hover {
    color: var(--danger, #d66);
  }

  .layer-eye :global(svg),
  .layer-del :global(svg) {
    font-size: 0.85rem;
  }

  .layer-name {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    border: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 0.8rem;
    padding: 0.25rem 0.2rem;
  }

  .layer.sel .layer-name {
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

  .obj {
    position: absolute;
    line-height: 0;
  }

  .obj.sel {
    outline: 1.5px solid var(--accent);
    outline-offset: 1px;
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

  .handle {
    position: absolute;
    width: 10px;
    height: 10px;
    background: var(--panel);
    border: 1.5px solid var(--accent);
    border-radius: 2px;
  }

  .handle.nw {
    top: -6px;
    left: -6px;
    cursor: nwse-resize;
  }

  .handle.ne {
    top: -6px;
    right: -6px;
    cursor: nesw-resize;
  }

  .handle.sw {
    bottom: -6px;
    left: -6px;
    cursor: nesw-resize;
  }

  .handle.se {
    bottom: -6px;
    right: -6px;
    cursor: nwse-resize;
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
</style>
