<script lang="ts">
  import IconListTree from '~icons/lucide/list-tree';
  import IconMapPin from '~icons/lucide/map-pin';
  import IconSeparatorHorizontal from '~icons/lucide/separator-horizontal';
  import IconFileUp from '~icons/lucide/file-up';
  import IconKeyboard from '~icons/lucide/keyboard';
  import IconFileDown from '~icons/lucide/file-down';
  import IconChevronUp from '~icons/lucide/chevron-up';
  import IconChevronDown from '~icons/lucide/chevron-down';
  import IconCopy from '~icons/lucide/copy';
  import IconX from '~icons/lucide/x';
  import IconSplit from '~icons/lucide/split';
  import IconMerge from '~icons/lucide/merge';
  import IconPencil from '~icons/lucide/pencil';
  import IconMagnet from '~icons/lucide/magnet';
  import IconUndo from '~icons/lucide/undo-2';
  import IconRedo from '~icons/lucide/redo-2';
  import InlineRename from './InlineRename.svelte';
  import SearchBar from './SearchBar.svelte';
  import TierLane from './TierLane.svelte';
  import IpaPad from './IpaPad.svelte';
  import { getCommandRegistry, registerCommands } from './commands.svelte';
  import { chordFromEvent, getKeyBindings } from './keybindings.svelte';
  import type {
    AnnotationClientLike,
    AppliedChange,
    IntervalData,
    LabelHit,
    PointData,
    TierInfo,
    ViewportState
  } from './types';

  interface EditingState {
    tierId: bigint;
    kind: 'interval' | 'point';
    targetId: bigint;
    value: string;
  }

  interface Props {
    client: AnnotationClientLike | null;
    audioId: bigint | null;
    annotationId: bigint | null;
    audioDuration: number;
    sampleRate: number;
    viewport: ViewportState;
    cursorTime: number;
    onSeek?: (time: number) => void;
    /** Bump to force a refetch after the host mutates the annotation directly
     * (e.g. annotate-by-silences), which leaves `annotationId` unchanged. */
    revision?: number;
    /** Fires when the pane repoints to a different document, including the
     * no-annotation state (`null`) reached by undoing an import or attach. */
    onAnnotationChange?: (id: bigint | null) => void;
    /**
     * Fires when an interval is clicked: the editor sets the time selection to
     * `[t0, t1]` and plays it. Click never opens the label editor — that stays
     * on Enter, double-click, and type-to-edit.
     */
    onIntervalActivate?: (t0: number, t1: number) => void;
    /** Resolves the zero crossing nearest a time, for snapping the active
     * boundary/point; absent (e.g. desktop) hides the snap affordance. */
    onNearestZero?: (t: number) => Promise<number>;
  }

  let {
    client,
    audioId,
    annotationId,
    audioDuration,
    sampleRate,
    viewport,
    cursorTime,
    onSeek,
    revision = 0,
    onAnnotationChange,
    onIntervalActivate,
    onNearestZero
  }: Props = $props();

  let paneEl = $state<HTMLElement | null>(null);
  let rowsEl = $state<HTMLDivElement | null>(null);
  let rowsWidth = $state(1);
  let fileInput = $state<HTMLInputElement | null>(null);

  let tiers = $state<TierInfo[]>([]);
  let intervalsByTier = $state<Map<bigint, IntervalData[]>>(new Map());
  let pointsByTier = $state<Map<bigint, PointData[]>>(new Map());
  let activeTierId = $state<bigint | null>(null);
  let activeIndex = $state(0);
  // The tier created by the most recent add — its name field opens once so
  // the tier is named where it is born, then focus returns to the pane.
  let justAddedTierId = $state<bigint | null>(null);
  let editing = $state<EditingState | null>(null);
  let ipaPadOpen = $state(false);
  let undoDepth = $state(0);
  let redoDepth = $state(0);
  let stateHash = $state<bigint>(0n);
  let status = $state('');

  let query = $state('');
  let replacement = $state('');
  let hits = $state<LabelHit[]>([]);
  let hitIndex = $state(0);

  let loadToken = 0;

  const activeTier = $derived(tiers.find((t) => t.id === activeTierId) ?? null);
  const activeIntervals = $derived(
    activeTier && activeTier.kind === 'interval' ? (intervalsByTier.get(activeTier.id) ?? []) : []
  );
  const activePoints = $derived(
    activeTier && activeTier.kind === 'point' ? (pointsByTier.get(activeTier.id) ?? []) : []
  );
  const activeCount = $derived(activeTier?.kind === 'point' ? activePoints.length : activeIntervals.length);

  // Every interior boundary time and every point time in the document — the
  // magnetic targets a dragged boundary snaps to across tiers.
  const snapTimes = $derived.by(() => {
    const times: number[] = [];
    for (const tier of tiers) {
      if (tier.kind === 'interval') {
        const ivs = intervalsByTier.get(tier.id) ?? [];
        for (let i = 0; i < ivs.length - 1; i += 1) times.push(ivs[i].xmax);
      } else {
        for (const point of pointsByTier.get(tier.id) ?? []) times.push(point.time);
      }
    }
    return times;
  });

  $effect(() => {
    // Refetch whenever the document identity changes, or the host signals a
    // direct mutation via `revision`.
    annotationId;
    revision;
    void refresh();
  });

  $effect(() => {
    if (!rowsEl) return;
    const observer = new ResizeObserver(() => {
      rowsWidth = rowsEl?.clientWidth ?? 1;
    });
    observer.observe(rowsEl);
    rowsWidth = rowsEl.clientWidth || 1;
    return () => observer.disconnect();
  });

  async function refresh(ann: bigint | null = annotationId) {
    const active = client;
    if (!active || ann === null) {
      tiers = [];
      intervalsByTier = new Map();
      pointsByTier = new Map();
      undoDepth = 0;
      redoDepth = 0;
      return;
    }
    const token = ++loadToken;
    let list: TierInfo[] = [];
    const intervals = new Map<bigint, IntervalData[]>();
    const points = new Map<bigint, PointData[]>();
    try {
      list = await active.annotationTiers(ann);
      if (token !== loadToken) return;
      for (const tier of list) {
        if (tier.kind === 'interval') {
          intervals.set(tier.id, await active.intervalsInRange(ann, tier.id, -1, 1e12));
        } else {
          points.set(tier.id, await active.pointsInRange(ann, tier.id, -1, 1e12));
        }
        if (token !== loadToken) return;
      }
    } catch {
      // The document can vanish mid-read when undo detaches it; render empty.
      if (token !== loadToken) return;
      list = [];
      intervals.clear();
      points.clear();
    }
    tiers = list;
    intervalsByTier = intervals;
    pointsByTier = points;
    if (activeTierId === null || !list.some((t) => t.id === activeTierId)) {
      activeTierId = list[0]?.id ?? null;
      activeIndex = 0;
    }
    clampActiveIndex();
    const [u, r, h] = await Promise.all([active.undoDepth(), active.redoDepth(), active.stateHash()]);
    if (token !== loadToken) return;
    undoDepth = u;
    redoDepth = r;
    stateHash = h;
    if (query) hits = (await active.searchLabels(query, false)).filter((hit) => hit.annotation === ann);
  }

  function clampActiveIndex() {
    const count = activeTier?.kind === 'point'
      ? (pointsByTier.get(activeTier.id)?.length ?? 0)
      : activeTier
        ? (intervalsByTier.get(activeTier.id)?.length ?? 0)
        : 0;
    if (activeIndex > count - 1) activeIndex = Math.max(0, count - 1);
    if (activeIndex < 0) activeIndex = 0;
  }

  // Focused from the pane itself and from EditorView after a cursor placement,
  // so the annotation keys act on the active tier wherever the cursor was set.
  // `preventScroll`: refocusing must never jump the page while scrubbing.
  export function focusPane() {
    paneEl?.focus({ preventScroll: true });
  }

  // The timeline's scrub surface calls this for every pointerdown that
  // reaches it. Presses that begin inside the tier pane — a label being
  // edited, the IPA pad, a lane — keep their own focus; only presses from the
  // waveform, spectrogram, and ruler hand the pane the keyboard.
  export function focusPaneFromPointer(target: EventTarget | null) {
    if (target instanceof Node && paneEl?.contains(target)) return;
    paneEl?.focus({ preventScroll: true });
  }

  function itemTime(index: number): number | null {
    if (!activeTier) return null;
    if (activeTier.kind === 'interval') return activeIntervals[index]?.xmin ?? null;
    return activePoints[index]?.time ?? null;
  }

  function selectIndex(index: number) {
    const count = activeCount;
    if (count === 0) return;
    activeIndex = Math.min(count - 1, Math.max(0, index));
    const time = itemTime(activeIndex);
    if (time !== null) onSeek?.(time);
  }

  function activateTier(tierId: bigint, index = 0) {
    activeTierId = tierId;
    activeIndex = index;
    focusPane();
  }

  // A click on a lane interval: activate it and hand its span up so the editor
  // sets the time selection and plays it. Editing stays on Enter/double-click/
  // type-to-edit, so a plain click never opens the label field.
  function activateFromLane(tier: TierInfo, index: number) {
    activateTier(tier.id, index);
    if (tier.kind === 'interval') {
      const interval = (intervalsByTier.get(tier.id) ?? [])[index];
      if (interval) onIntervalActivate?.(interval.xmin, interval.xmax);
    } else {
      const point = (pointsByTier.get(tier.id) ?? [])[index];
      if (point) onSeek?.(point.time);
    }
  }

  function focusTierByDigit(digit: number) {
    const tier = tiers[digit - 1];
    if (!tier) return;
    activeTierId = tier.id;
    activeIndex = 0;
    const time = itemTime(0);
    if (time !== null) onSeek?.(time);
  }

  function openEditor(index: number, initial?: string) {
    if (!activeTier) return;
    const items = activeTier.kind === 'interval' ? activeIntervals : activePoints;
    const item = items[index];
    if (!item) {
      // A fresh point tier has nothing to label yet; say what unlocks the key.
      if (activeTier.kind === 'point') {
        status = `${activeTier.name} has no points yet — place the cursor in the waveform and press ${keyBindings?.labelFor('insertBoundary') ?? 'S'} to add one.`;
      }
      return;
    }
    activeIndex = index;
    const current = activeTier.kind === 'interval'
      ? (item as IntervalData).label
      : (item as PointData).label;
    editing = {
      tierId: activeTier.id,
      kind: activeTier.kind,
      targetId: item.id,
      value: initial ?? current
    };
  }

  async function commitEdit(advance = false) {
    const edit = editing;
    if (!edit || !client || annotationId === null) {
      editing = null;
      return;
    }
    editing = null;
    try {
      if (edit.kind === 'interval') {
        await client.setIntervalLabel(annotationId, edit.tierId, edit.targetId, edit.value);
      } else {
        await client.setPointLabel(annotationId, edit.tierId, edit.targetId, edit.value);
      }
      status = '';
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
    await refresh();
    // Enter commits and steps to the next item, so labeling a run of segments
    // is type-Enter-type without ever reaching for Tab.
    if (advance) selectIndex(activeIndex + 1);
    focusPane();
  }

  function cancelEdit() {
    editing = null;
    focusPane();
  }

  // The split key acts at the cursor on the active tier. Two cursor positions
  // can never take a boundary — on the tier's edge, or exactly on a boundary
  // the tier already has (stepping with Tab parks the cursor on interval
  // starts) — and both are answered locally with a sentence, never a raw
  // engine error.
  async function splitAtCursor() {
    if (!client || annotationId === null) return;
    const at = cursorTime.toFixed(3);
    try {
      if (activeTier?.kind === 'interval') {
        const ivs = activeIntervals;
        if (ivs.length === 0) return;
        if (cursorTime <= ivs[0].xmin || cursorTime >= ivs[ivs.length - 1].xmax) {
          status = `The cursor sits on the edge of ${activeTier.name} — click in the waveform where the boundary should go.`;
          return;
        }
        if (ivs.some((iv) => iv.xmin === cursorTime || iv.xmax === cursorTime)) {
          status = `${activeTier.name} already has a boundary at ${at} s.`;
          return;
        }
        await client.insertBoundary(annotationId, activeTier.id, cursorTime);
      } else if (activeTier?.kind === 'point') {
        if (activePoints.some((pt) => pt.time === cursorTime)) {
          status = `${activeTier.name} already has a point at ${at} s.`;
          return;
        }
        await client.insertPoint(annotationId, activeTier.id, cursorTime, '');
      } else {
        return;
      }
      status = '';
      await refresh();
      selectByTime(cursorTime);
      focusPane();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  // Praat's "Add on all tiers": drop a boundary on every interval tier and a
  // point on every point tier at the cursor. A tier that already carries a mark
  // at this instant keeps it — that one insert is skipped, the rest still land.
  async function splitAllTiersAtCursor() {
    if (!client || annotationId === null || tiers.length === 0) return;
    let added = 0;
    for (const tier of tiers) {
      try {
        if (tier.kind === 'interval') {
          await client.insertBoundary(annotationId, tier.id, cursorTime);
        } else {
          await client.insertPoint(annotationId, tier.id, cursorTime, '');
        }
        added += 1;
      } catch {
        // Tier already has a mark at the cursor; leave it and continue.
      }
    }
    status = added > 0
      ? `Added on ${added} ${added === 1 ? 'tier' : 'tiers'}`
      : `Every tier already has a mark at ${cursorTime.toFixed(3)} s`;
    await refresh();
    selectByTime(cursorTime);
    focusPane();
  }

  async function mergeActive() {
    if (!client || annotationId === null) return;
    if (activeTier?.kind === 'point') {
      const point = activePoints[activeIndex];
      if (!point) return;
      try {
        await client.removePoint(annotationId, point.id);
        status = '';
        await refresh();
        clampActiveIndex();
        focusPane();
      } catch (error) {
        status = error instanceof Error ? error.message : String(error);
      }
      return;
    }
    if (activeTier?.kind !== 'interval') return;
    const ivs = activeIntervals;
    if (ivs.length < 2) return;
    const interval = ivs[activeIndex];
    let boundary: bigint;
    if (activeIndex < ivs.length - 1) boundary = interval.endBoundary;
    else boundary = interval.startBoundary;
    try {
      await client.removeBoundary(annotationId, boundary);
      status = '';
      await refresh();
      clampActiveIndex();
      focusPane();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  function activeBoundaryId(): bigint | null {
    const ivs = activeIntervals;
    if (!ivs.length) return null;
    const interval = ivs[activeIndex];
    if (activeIndex > 0) return interval.startBoundary;
    if (ivs.length > 1) return interval.endBoundary;
    return null;
  }

  function boundaryTime(boundaryId: bigint): number | null {
    for (const interval of activeIntervals) {
      if (interval.startBoundary === boundaryId) return interval.xmin;
      if (interval.endBoundary === boundaryId) return interval.xmax;
    }
    return null;
  }

  async function nudgeBoundary(direction: number, oneFrame: boolean) {
    if (!client || annotationId === null || activeTier?.kind !== 'interval') return;
    const boundary = activeBoundaryId();
    if (boundary === null) return;
    const from = boundaryTime(boundary);
    if (from === null) return;
    const pixelSeconds = (viewport.t1 - viewport.t0) / Math.max(1, rowsWidth);
    const step = oneFrame && sampleRate > 0 ? 1 / sampleRate : pixelSeconds;
    try {
      await client.moveBoundary(annotationId, boundary, from + direction * step, true);
      status = '';
      await refresh();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  async function moveBoundaryTo(boundaryId: bigint, toTime: number) {
    if (!client || annotationId === null) return;
    try {
      await client.moveBoundary(annotationId, boundaryId, toTime, true);
      status = '';
      await refresh();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  async function movePointTo(pointId: bigint, toTime: number) {
    if (!client || annotationId === null) return;
    try {
      await client.movePoint(annotationId, pointId, toTime);
      status = '';
      await refresh();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  // Praat's "Move to nearest zero crossing": shift the active boundary (interval
  // tier) or point (point tier) onto the waveform's nearest zero crossing.
  async function snapActiveToZero() {
    if (!client || annotationId === null || !onNearestZero) return;
    if (activeTier?.kind === 'interval') {
      const boundary = activeBoundaryId();
      if (boundary === null) return;
      const from = boundaryTime(boundary);
      if (from === null) return;
      const to = await onNearestZero(from);
      if (!Number.isFinite(to)) return;
      await moveBoundaryTo(boundary, to);
      selectByTime(to);
    } else if (activeTier?.kind === 'point') {
      const point = activePoints[activeIndex];
      if (!point) return;
      const to = await onNearestZero(point.time);
      if (!Number.isFinite(to)) return;
      await movePointTo(point.id, to);
      selectByTime(to);
    } else {
      return;
    }
    status = 'Snapped to zero crossing';
    focusPane();
  }

  function selectByTime(time: number) {
    if (activeTier?.kind === 'interval') {
      const index = activeIntervals.findIndex((iv) => time >= iv.xmin && time < iv.xmax);
      if (index >= 0) activeIndex = index;
    } else if (activeTier?.kind === 'point' && activePoints.length) {
      let best = 0;
      let bestDist = Infinity;
      activePoints.forEach((point, i) => {
        const dist = Math.abs(point.time - time);
        if (dist < bestDist) {
          bestDist = dist;
          best = i;
        }
      });
      activeIndex = best;
    }
  }

  // Auto-names a new tier "interval N"/"point N", skipping numbers the
  // document already uses so remove-and-readd cycles never mint a duplicate.
  function nextTierName(kind: 'interval' | 'point') {
    const taken = new Set(tiers.map((t) => t.name));
    let n = tiers.filter((t) => t.kind === kind).length + 1;
    while (taken.has(`${kind} ${n}`)) n += 1;
    return `${kind} ${n}`;
  }

  async function addTier(kind: 'interval' | 'point') {
    if (!client || annotationId === null) return;
    const name = nextTierName(kind);
    try {
      const id = kind === 'interval'
        ? await client.addIntervalTier(annotationId, name)
        : await client.addPointTier(annotationId, name);
      status = `Added ${name}`;
      await refresh();
      activateTier(id, 0);
      justAddedTierId = id;
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  async function renameTier(tierId: bigint, name: string) {
    if (!client || annotationId === null) return;
    try {
      await client.renameTier(annotationId, tierId, name);
      status = '';
      await refresh();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  async function removeTier(tierId: bigint) {
    if (!client || annotationId === null) return;
    const name = tiers.find((t) => t.id === tierId)?.name ?? '';
    try {
      await client.removeTier(annotationId, tierId);
      // The removal is one journal entry — name it so Ctrl-Z is discoverable.
      status = `Removed tier "${name}"`;
      if (activeTierId === tierId) activeTierId = null;
      await refresh();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  // Copies a tier and its contents into a new tier directly below it.
  async function duplicateTier(tierId: bigint) {
    if (!client || annotationId === null) return;
    try {
      await client.duplicateTier(annotationId, tierId);
      status = '';
      await refresh();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  // Moves a tier one place up (`delta = -1`) or down (`delta = +1`) in the stack.
  async function moveTier(tierId: bigint, delta: number) {
    if (!client || annotationId === null) return;
    const from = tiers.findIndex((tier) => tier.id === tierId);
    if (from < 0) return;
    const to = from + delta;
    if (to < 0 || to >= tiers.length) return;
    try {
      await client.reorderTier(annotationId, tierId, to);
      status = '';
      await refresh();
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  /**
   * Reconciles the current document against an undo or redo that attached or
   * detached one, using the change the engine reports rather than guessing
   * from the live list. An attach can add a fresh document on top of one the
   * pane still legitimately points at (importing a TextGrid never replaces
   * the recording's earlier document, so both stay attached at once) — so
   * "is my current document still live" can't tell the pane whether it
   * should follow a document that was just reattached above it. The applied
   * change names exactly which annotation was attached or detached, so the
   * pane follows an attach unconditionally and only re-derives a fallback on
   * a detach of the document it was showing. The caller's own
   * `onAnnotationChange` propagates the new id up so the rest of the editor
   * (export, the audio store) stays in sync, and the resolved id is returned
   * so the pane can refresh itself immediately rather than waiting on that
   * round trip.
   */
  async function reconcileAnnotation(applied: AppliedChange | null): Promise<bigint | null> {
    if (!client || audioId === null) return annotationId;
    if (applied?.kind === 'annotationAttached' && applied.audio === audioId && applied.annotation !== undefined) {
      const next = applied.annotation;
      if (next !== annotationId) onAnnotationChange?.(next);
      return next;
    }
    if (applied?.kind === 'annotationDetached' && applied.annotation === annotationId) {
      const live = await client.listAnnotations(audioId);
      const next = live.length > 0 ? live[live.length - 1] : null;
      if (next !== annotationId) onAnnotationChange?.(next);
      return next;
    }
    return annotationId;
  }

  async function undo() {
    if (!client) return;
    editing = null;
    const applied = await client.undo();
    const next = await reconcileAnnotation(applied);
    await refresh(next);
    focusPane();
  }

  async function redo() {
    if (!client) return;
    editing = null;
    const applied = await client.redo();
    const next = await reconcileAnnotation(applied);
    await refresh(next);
    focusPane();
  }

  async function runSearch(text: string) {
    query = text;
    if (!client || annotationId === null || !text) {
      hits = [];
      hitIndex = 0;
      return;
    }
    const found = await client.searchLabels(text, false);
    hits = found.filter((hit) => hit.annotation === annotationId);
    hitIndex = 0;
    goToHit();
  }

  function goToHit() {
    const hit = hits[hitIndex];
    if (!hit) return;
    const tier = tiers.find((t) => t.id === hit.tier);
    if (!tier) return;
    activeTierId = tier.id;
    if (tier.kind === 'interval') {
      const ivs = intervalsByTier.get(tier.id) ?? [];
      const index = ivs.findIndex((iv) => iv.id === hit.target);
      if (index >= 0) {
        activeIndex = index;
        onSeek?.(ivs[index].xmin);
      }
    } else {
      const pts = pointsByTier.get(tier.id) ?? [];
      const index = pts.findIndex((pt) => pt.id === hit.target);
      if (index >= 0) {
        activeIndex = index;
        onSeek?.(pts[index].time);
      }
    }
  }

  function nextHit() {
    if (hits.length === 0) return;
    hitIndex = (hitIndex + 1) % hits.length;
    goToHit();
  }

  function prevHit() {
    if (hits.length === 0) return;
    hitIndex = (hitIndex - 1 + hits.length) % hits.length;
    goToHit();
  }

  // The current label text of a search hit, looked up from the tier maps the
  // pane already holds — no extra fetch.
  function hitLabel(hit: LabelHit): string | null {
    if (hit.kind === 'interval') {
      return (intervalsByTier.get(hit.tier) ?? []).find((iv) => iv.id === hit.target)?.label ?? null;
    }
    return (pointsByTier.get(hit.tier) ?? []).find((pt) => pt.id === hit.target)?.label ?? null;
  }

  async function setHitLabel(hit: LabelHit, text: string) {
    if (annotationId === null || !client) return;
    if (hit.kind === 'interval') {
      await client.setIntervalLabel(annotationId, hit.tier, hit.target, text);
    } else {
      await client.setPointLabel(annotationId, hit.tier, hit.target, text);
    }
  }

  // Replaces every occurrence of the query inside a matched label with the
  // replacement text.
  function substitute(label: string): string {
    return label.split(query).join(replacement);
  }

  async function replaceCurrentHit() {
    if (!client || annotationId === null || !query) return;
    const hit = hits[hitIndex];
    if (!hit) return;
    const current = hitLabel(hit);
    if (current === null || !current.includes(query)) return;
    try {
      await setHitLabel(hit, substitute(current));
      status = '';
      await refresh();
      await runSearch(query);
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  async function replaceAllHits() {
    if (!client || annotationId === null || !query || hits.length === 0) return;
    try {
      let changed = 0;
      for (const hit of [...hits]) {
        const current = hitLabel(hit);
        if (current === null || !current.includes(query)) continue;
        const next = substitute(current);
        if (next === current) continue;
        await setHitLabel(hit, next);
        changed += 1;
      }
      status = changed ? `Replaced in ${changed} ${changed === 1 ? 'label' : 'labels'}` : '';
      await refresh();
      await runSearch(query);
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  async function exportTextGrid() {
    if (!client || annotationId === null) return;
    const bytes = await client.exportTextGrid(annotationId);
    const blob = new Blob([bytes as BlobPart], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = 'annotation.TextGrid';
    anchor.click();
    URL.revokeObjectURL(url);
  }

  // Quotes a field per RFC 4180 when it holds a comma, quote, or newline, so a
  // label like `a, b` or one with quotation marks survives the CSV round-trip.
  function csvCell(value: string): string {
    return /[",\n\r]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
  }

  // Praat's "Down to Table": flatten every tier — intervals and points — into a
  // single CSV, one row per label, using the already-loaded tier contents.
  async function copyAnnotationTable() {
    if (annotationId === null || tiers.length === 0) return;
    const rows = ['tier,kind,label,tmin,tmax'];
    for (const tier of tiers) {
      if (tier.kind === 'interval') {
        for (const iv of intervalsByTier.get(tier.id) ?? []) {
          rows.push(
            `${csvCell(tier.name)},interval,${csvCell(iv.label)},${iv.xmin.toFixed(6)},${iv.xmax.toFixed(6)}`
          );
        }
      } else {
        for (const pt of pointsByTier.get(tier.id) ?? []) {
          rows.push(
            `${csvCell(tier.name)},point,${csvCell(pt.label)},${pt.time.toFixed(6)},${pt.time.toFixed(6)}`
          );
        }
      }
    }
    try {
      await navigator.clipboard.writeText(rows.join('\n'));
      status = `Copied ${rows.length - 1} rows`;
    } catch {
      status = 'Could not copy the table';
    }
  }

  async function importTextGridFile(file: File) {
    if (!client || audioId === null) return;
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const newId = await client.importTextGrid(audioId, bytes);
      activeTierId = null;
      onAnnotationChange?.(newId);
      status = '';
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }

  const keyBindings = getKeyBindings();
  const commandRegistry = getCommandRegistry();

  // The `tierpane` scope's rebindable actions. Which chord fires which of
  // these is data (`keyBindings`, mode-dependent) — see keybindings.svelte.ts.
  const tierPaneActions: Record<string, () => void> = {
    insertBoundary: () => void splitAtCursor(),
    removeBoundary: () => void mergeActive(),
    editLabel: () => openEditor(activeIndex),
    nextInterval: () => selectIndex(activeIndex + 1),
    previousInterval: () => selectIndex(activeIndex - 1)
  };

  function handleKeydown(event: KeyboardEvent) {
    if (editing) return;
    if (annotationId === null) return;
    // Keys typed into toolbar controls (buttons, the search field) keep their
    // native behavior; the annotation loop reads keys only from the pane body.
    const target = event.target;
    if (
      target instanceof HTMLButtonElement ||
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement
    ) {
      return;
    }
    const { key } = event;

    if (/^[1-9]$/.test(key)) {
      event.preventDefault();
      event.stopPropagation();
      focusTierByDigit(Number(key));
      return;
    }
    if (keyBindings) {
      const chord = chordFromEvent(event);
      const tierCommandId = keyBindings.commandForChord('tierpane', chord);
      const tierAction = tierCommandId ? tierPaneActions[tierCommandId] : undefined;
      if (tierAction) {
        event.preventDefault();
        event.stopPropagation();
        tierAction();
        return;
      }
      // No tier-pane command claims this chord in the active mode — Praat
      // mode leaves Tab unbound here on purpose, since Praat's Tab plays
      // regardless of what has focus. Fall back to whatever the editor
      // scope binds the same chord to, through the shared command registry
      // so it runs the identical code path a Space press or palette click
      // would.
      const editorCommandId = keyBindings.commandForChord('editor', chord);
      if (editorCommandId && commandRegistry?.find(editorCommandId)) {
        event.preventDefault();
        event.stopPropagation();
        void commandRegistry.run(editorCommandId);
        return;
      }
    }
    if (key === 'ArrowLeft' || key === 'ArrowRight') {
      event.preventDefault();
      event.stopPropagation();
      void nudgeBoundary(key === 'ArrowLeft' ? -1 : 1, event.altKey);
      return;
    }
    // Type-to-edit: a printable character opens the label editor seeded with it.
    if (key.length === 1 && key !== ' ' && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault();
      event.stopPropagation();
      openEditor(activeIndex, key);
    }
  }

  function handleGlobalKeydown(event: KeyboardEvent) {
    const target = event.target;
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return;
    if (!keyBindings) return;
    const chord = chordFromEvent(event);
    const commandId = keyBindings.commandForChord('global', chord);
    if (commandId === 'undo') {
      event.preventDefault();
      void undo();
    } else if (commandId === 'redo') {
      event.preventDefault();
      void redo();
    }
  }

  const hasDocument = () => annotationId !== null;
  const onIntervalTier = () => annotationId !== null && activeTier?.kind === 'interval';
  const onPointTier = () => annotationId !== null && activeTier?.kind === 'point';

  // Toolbar gating — one predicate per button, read from the same live state
  // the keyboard loop uses.
  const canSplit = () => annotationId !== null && activeTier !== null;
  const canMerge = () =>
    annotationId !== null &&
    (onPointTier() ? activePoints.length > 0 : onIntervalTier() && activeIntervals.length > 1);
  const canEditLabel = () => annotationId !== null && activeCount > 0;
  const canSnap = () => annotationId !== null && onNearestZero !== undefined && activeTier !== null;
  const shortcut = (id: string, fallback: string) => keyBindings?.labelFor(id) ?? fallback;

  registerCommands([
    {
      id: 'addIntervalTier',
      title: 'Add interval tier',
      group: 'Annotation',
      api: ['addIntervalTier'],
      keywords: ['tier', 'new'],
      enabled: hasDocument,
      run: () => void addTier('interval')
    },
    {
      id: 'addPointTier',
      title: 'Add point tier',
      group: 'Annotation',
      api: ['addPointTier'],
      keywords: ['tier', 'new'],
      enabled: hasDocument,
      run: () => void addTier('point')
    },
    {
      id: 'removeTier',
      title: 'Remove active tier',
      group: 'Annotation',
      api: ['removeTier'],
      keywords: ['delete tier'],
      enabled: () => activeTierId !== null,
      run: () => {
        if (activeTierId !== null) void removeTier(activeTierId);
      }
    },
    {
      id: 'insertBoundary',
      title: 'Split interval / add point at cursor',
      group: 'Annotation',
      api: ['insertBoundary', 'insertPoint'],
      shortcut: () => keyBindings?.labelFor('insertBoundary') ?? 'S',
      keywords: ['boundary', 'divide', 'point', 'add', 'mark'],
      enabled: () => onIntervalTier() || onPointTier(),
      run: () => void splitAtCursor()
    },
    {
      id: 'insertBoundaryAllTiers',
      title: 'Add boundary/point on all tiers at cursor',
      group: 'Annotation',
      api: ['insertBoundary', 'insertPoint'],
      keywords: ['boundary', 'point', 'all tiers', 'add', 'mark'],
      enabled: () => annotationId !== null && tiers.length > 0,
      run: () => void splitAllTiersAtCursor()
    },
    {
      id: 'removeBoundary',
      title: 'Merge intervals / remove point',
      group: 'Annotation',
      api: ['removeBoundary', 'removePoint'],
      shortcut: () => keyBindings?.labelFor('removeBoundary') ?? 'M',
      keywords: ['boundary', 'join', 'point', 'delete', 'remove'],
      enabled: () =>
        (onIntervalTier() && activeIntervals.length >= 2) ||
        (onPointTier() && activePoints.length >= 1),
      run: () => void mergeActive()
    },
    {
      id: 'snapBoundaryZero',
      title: 'Move boundary/point to nearest zero crossing',
      group: 'Annotation',
      api: ['nearestZeroCrossing', 'moveBoundary', 'movePoint'],
      keywords: ['zero', 'crossing', 'snap', 'boundary', 'point', 'align'],
      enabled: () =>
        onNearestZero !== undefined &&
        ((onIntervalTier() && activeBoundaryId() !== null) ||
          (onPointTier() && activePoints.length >= 1)),
      run: () => void snapActiveToZero()
    },
    {
      id: 'editLabel',
      title: 'Edit label',
      group: 'Annotation',
      api: ['setIntervalLabel', 'setPointLabel'],
      shortcut: () => keyBindings?.labelFor('editLabel') ?? 'Enter',
      enabled: () => activeCount > 0,
      run: () => openEditor(activeIndex)
    },
    {
      id: 'nextInterval',
      title: 'Next interval',
      group: 'Annotation',
      shortcut: () => keyBindings?.labelFor('nextInterval') ?? 'Tab',
      enabled: () => activeCount > 0,
      run: () => selectIndex(activeIndex + 1)
    },
    {
      id: 'previousInterval',
      title: 'Previous interval',
      group: 'Annotation',
      shortcut: () => keyBindings?.labelFor('previousInterval') ?? 'Shift+Tab',
      enabled: () => activeCount > 0,
      run: () => selectIndex(activeIndex - 1)
    },
    {
      id: 'undo',
      title: 'Undo',
      group: 'Annotation',
      api: ['undo'],
      shortcut: () => keyBindings?.labelFor('undo') ?? 'Ctrl/Cmd+Z',
      enabled: () => undoDepth > 0,
      run: () => void undo()
    },
    {
      id: 'redo',
      title: 'Redo',
      group: 'Annotation',
      api: ['redo'],
      shortcut: () => keyBindings?.labelFor('redo') ?? 'Ctrl/Cmd+Shift+Z',
      enabled: () => redoDepth > 0,
      run: () => void redo()
    },
    {
      id: 'importTextGrid',
      title: 'Import TextGrid',
      group: 'Annotation',
      api: ['importTextGrid'],
      keywords: ['open', 'praat'],
      enabled: () => audioId !== null,
      run: () => fileInput?.click()
    },
    {
      id: 'exportTextGrid',
      title: 'Export TextGrid',
      group: 'Annotation',
      api: ['exportTextGrid'],
      keywords: ['save', 'praat'],
      enabled: hasDocument,
      run: () => void exportTextGrid()
    },
    {
      id: 'copyAnnotationTable',
      title: 'Copy annotation table (CSV)',
      group: 'Annotation',
      api: ['annotationTiers', 'intervalsInRange', 'pointsInRange'],
      keywords: ['table', 'csv', 'export', 'copy', 'labels', 'down to table', 'tiers'],
      enabled: () => annotationId !== null && tiers.length > 0,
      run: () => void copyAnnotationTable()
    }
  ]);
</script>

<svelte:window onkeydown={handleGlobalKeydown} />

<!-- A focusable editing surface: the annotation loop is keyboard-driven, so the
     pane takes focus and key events directly (the a11y rules below assume
     content, not an editor). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="tier-pane"
  data-testid="tier-pane"
  data-tier-count={tiers.length}
  data-undo-depth={undoDepth}
  data-redo-depth={redoDepth}
  data-state-hash={stateHash.toString()}
  data-active-tier={activeTier?.name ?? ''}
  data-active-index={activeIndex}
  role="application"
  aria-label="Annotation tiers"
  tabindex="0"
  bind:this={paneEl}
  onkeydown={handleKeydown}
>
  <div class="anno-toolbar" role="toolbar" aria-label="Annotation actions" tabindex="-1" onpointerdown={(event) => event.stopPropagation()}>
    <button type="button" data-testid="add-interval-tier" disabled={annotationId === null} onclick={() => addTier('interval')}>
      <IconListTree aria-hidden="true" /><span>Interval tier</span>
    </button>
    <button type="button" data-testid="add-point-tier" disabled={annotationId === null} onclick={() => addTier('point')}>
      <IconMapPin aria-hidden="true" /><span>Point tier</span>
    </button>
    <button
      type="button"
      data-testid="add-on-all-tiers"
      title="Add a boundary/point on every tier at the cursor"
      disabled={annotationId === null || tiers.length === 0}
      onclick={() => void splitAllTiersAtCursor()}
    >
      <IconSeparatorHorizontal aria-hidden="true" /><span>All tiers</span>
    </button>
    <div class="group" aria-hidden="true"></div>
    <button
      type="button"
      data-testid="split-at-cursor"
      title={`Split the active tier at the cursor (${shortcut('insertBoundary', 'S')})`}
      disabled={!canSplit()}
      onclick={() => void splitAtCursor()}
    >
      <IconSplit aria-hidden="true" /><span>Split</span><kbd>{shortcut('insertBoundary', 'S')}</kbd>
    </button>
    <button
      type="button"
      data-testid="merge-active"
      title={`Merge the active interval with its neighbour, or remove the active point (${shortcut('removeBoundary', 'M')})`}
      disabled={!canMerge()}
      onclick={() => void mergeActive()}
    >
      <IconMerge aria-hidden="true" /><span>Merge</span><kbd>{shortcut('removeBoundary', 'M')}</kbd>
    </button>
    <button
      type="button"
      data-testid="edit-label"
      title={`Edit the label of the active item (${shortcut('editLabel', 'Enter')})`}
      disabled={!canEditLabel()}
      onclick={() => openEditor(activeIndex)}
    >
      <IconPencil aria-hidden="true" /><span>Label</span><kbd>{shortcut('editLabel', 'Enter')}</kbd>
    </button>
    <button
      type="button"
      data-testid="snap-active"
      title="Move the active boundary or point onto the waveform's nearest zero crossing"
      disabled={!canSnap()}
      onclick={() => void snapActiveToZero()}
    >
      <IconMagnet aria-hidden="true" /><span>Snap</span>
    </button>
    <div class="group" aria-hidden="true"></div>
    <button
      type="button"
      data-testid="tier-undo"
      class="icon-only"
      aria-label={`Undo (${shortcut('undo', 'Ctrl+Z')})`}
      title={`Undo (${shortcut('undo', 'Ctrl+Z')})`}
      disabled={undoDepth === 0}
      onclick={() => void undo()}
    >
      <IconUndo aria-hidden="true" />
    </button>
    <button
      type="button"
      data-testid="tier-redo"
      class="icon-only"
      aria-label={`Redo (${shortcut('redo', 'Ctrl+Shift+Z')})`}
      title={`Redo (${shortcut('redo', 'Ctrl+Shift+Z')})`}
      disabled={redoDepth === 0}
      onclick={() => void redo()}
    >
      <IconRedo aria-hidden="true" />
    </button>
    <div class="spacer"></div>
    <SearchBar
      query={query}
      count={hits.length}
      index={hitIndex}
      replacement={replacement}
      onQuery={runSearch}
      onNext={nextHit}
      onPrev={prevHit}
      onReplacement={(text) => (replacement = text)}
      onReplace={replaceCurrentHit}
      onReplaceAll={replaceAllHits}
    />
    <button type="button" data-testid="import-textgrid" disabled={audioId === null} onclick={() => fileInput?.click()}>
      <IconFileUp aria-hidden="true" /><span>Import TextGrid</span>
    </button>
    <button type="button" data-testid="export-textgrid" disabled={annotationId === null} onclick={exportTextGrid}>
      <IconFileDown aria-hidden="true" /><span>Export TextGrid</span>
    </button>
    <button
      type="button"
      data-testid="ipa-toggle"
      class:on={ipaPadOpen}
      aria-pressed={ipaPadOpen}
      title="IPA input pad"
      onclick={() => (ipaPadOpen = !ipaPadOpen)}
    >
      <IconKeyboard aria-hidden="true" /><span>IPA</span>
    </button>
    <input
      bind:this={fileInput}
      class="hidden-input"
      data-testid="textgrid-input"
      type="file"
      accept=".TextGrid,.textgrid,text/plain"
      onchange={(event) => {
        const file = event.currentTarget.files?.item(0);
        if (file) void importTextGridFile(file);
        event.currentTarget.value = '';
      }}
    />
  </div>

  <div class="tier-rows" bind:this={rowsEl}>
    {#if tiers.length === 0}
      <div class="empty" data-testid="tier-empty">
        <p class="empty-lead">No tiers yet.</p>
        <p class="empty-sub">
          Add an interval tier for labeled spans, or a point tier for instants, from the toolbar
          above.
        </p>
        <p class="empty-keys">
          Once a tier holds intervals: <kbd>Tab</kbd> moves, <kbd>Enter</kbd> edits,
          <kbd>S</kbd> splits at the cursor, <kbd>M</kbd> merges.
        </p>
      </div>
    {/if}
    {#each tiers as tier, tierIndex (tier.id)}
      <div class="tier-row">
        <TierLane
          tier={tier}
          intervals={intervalsByTier.get(tier.id) ?? []}
          points={pointsByTier.get(tier.id) ?? []}
          viewport={viewport}
          active={tier.id === activeTierId}
          activeIndex={activeIndex}
          editing={editing && editing.tierId === tier.id ? { targetId: editing.targetId, value: editing.value } : null}
          snapTimes={snapTimes}
          cursorTime={cursorTime}
          onActivate={(index) => activateFromLane(tier, index)}
          onEditRequest={(index) => { activateTier(tier.id, index); openEditor(index); }}
          onMoveBoundary={moveBoundaryTo}
          onMovePoint={movePointTo}
          onEditInput={(value) => { if (editing) editing = { ...editing, value }; }}
          onEditCommit={commitEdit}
          onEditCancel={cancelEdit}
        />
        <div class="tier-chip">
          <span class="tier-name">
            <span class="tier-digit">{tierIndex + 1}</span>
            <InlineRename
              name={tier.name}
              class="tier-label"
              label="Rename tier"
              testId="tier-name"
              autoEdit={tier.id === justAddedTierId}
              onActivate={() => activateTier(tier.id)}
              onRename={(next) => void renameTier(tier.id, next)}
              onClose={() => {
                justAddedTierId = null;
                focusPane();
              }}
            />
          </span>
          <button
            type="button"
            class="tier-move"
            aria-label={`Move ${tier.name} up`}
            data-testid="move-tier-up"
            disabled={tierIndex === 0}
            onclick={() => moveTier(tier.id, -1)}
          >
            <IconChevronUp aria-hidden="true" />
          </button>
          <button
            type="button"
            class="tier-move"
            aria-label={`Move ${tier.name} down`}
            data-testid="move-tier-down"
            disabled={tierIndex === tiers.length - 1}
            onclick={() => moveTier(tier.id, 1)}
          >
            <IconChevronDown aria-hidden="true" />
          </button>
          <button
            type="button"
            class="tier-move"
            aria-label={`Duplicate ${tier.name}`}
            data-testid="duplicate-tier"
            onclick={() => duplicateTier(tier.id)}
          >
            <IconCopy aria-hidden="true" />
          </button>
          <button type="button" class="tier-remove" aria-label={`Remove ${tier.name}`} data-testid="remove-tier" onclick={() => removeTier(tier.id)}>
            <IconX aria-hidden="true" />
          </button>
        </div>
      </div>
    {/each}
  </div>

  {#if status}
    <div class="status" role="status" data-testid="tier-status">{status}</div>
  {/if}
  {#if ipaPadOpen}
    <IpaPad onClose={() => (ipaPadOpen = false)} />
  {/if}
</div>

<style>
  .tier-pane {
    position: relative;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--panel);
    outline: none;
  }

  .tier-pane:focus-visible {
    box-shadow: inset 0 0 0 2px color-mix(in oklab, var(--accent), transparent 55%);
  }

  .anno-toolbar {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-height: 2.1rem;
    padding: 0.35rem 0.6rem;
    border-bottom: 1px solid var(--chrome-strong);
    background: var(--panel-soft);
    flex-wrap: wrap;
  }

  .anno-toolbar .spacer {
    flex: 1 1 auto;
  }

  .anno-toolbar .group {
    align-self: stretch;
    width: 1px;
    margin: 0.1rem 0.15rem;
    background: var(--chrome-strong);
  }

  .anno-toolbar button kbd {
    padding: 0 0.25rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    color: var(--muted);
    font: inherit;
    font-size: 0.68rem;
    line-height: 1.1;
  }

  .anno-toolbar button.icon-only {
    padding-inline: 0.4rem;
  }

  .anno-toolbar button {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel);
    color: var(--text);
    min-height: 1.6rem;
    padding: 0.2rem 0.5rem;
    font-size: 0.8rem;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .anno-toolbar button :global(svg) {
    font-size: 0.9rem;
  }

  .anno-toolbar button:hover:not(:disabled) {
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .anno-toolbar button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .tier-rows {
    position: relative;
    overflow-y: auto;
    min-height: 3rem;
  }

  .tier-row {
    position: relative;
  }

  .tier-chip {
    position: absolute;
    top: 0.2rem;
    left: 0.3rem;
    display: flex;
    align-items: stretch;
    gap: 1px;
    z-index: 4;
    font-size: 0.72rem;
    border-radius: 4px;
    overflow: hidden;
    box-shadow: 0 0 0 1px var(--chip-ring);
  }

  .tier-name {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    border: none;
    background: var(--chip-bg);
    color: var(--chip-fg);
    padding: 0.1rem 0.4rem;
    font-size: 0.72rem;
  }

  .tier-name :global(.tier-label) {
    color: var(--chip-fg);
  }

  .tier-digit {
    display: inline-grid;
    place-items: center;
    width: 1rem;
    height: 1rem;
    border-radius: 3px;
    background: color-mix(in oklab, var(--accent), transparent 55%);
    color: var(--chip-fg);
    font-variant-numeric: tabular-nums;
  }

  .tier-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: var(--chip-bg);
    color: var(--muted);
    padding: 0 0.35rem;
    line-height: 1;
  }

  .tier-remove :global(svg) {
    font-size: 0.8rem;
  }

  .tier-remove:hover {
    color: var(--danger);
  }

  .tier-move {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: var(--chip-bg);
    color: var(--muted);
    padding: 0 0.2rem;
    line-height: 1;
    cursor: pointer;
  }

  .tier-move :global(svg) {
    font-size: 0.8rem;
  }

  .tier-move:hover:not(:disabled) {
    color: var(--text);
  }

  .tier-move:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .empty {
    padding: 0.85rem 0.7rem;
    color: var(--muted);
    font-size: 0.85rem;
  }

  .empty p {
    margin: 0 0 0.35rem;
  }

  .empty-lead {
    color: var(--text);
    font-weight: 600;
  }

  .empty-sub {
    max-width: 34rem;
  }

  .empty-keys {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.3rem;
    font-size: 0.8rem;
  }

  .empty-keys kbd {
    border: 1px solid var(--chrome-strong);
    border-radius: 4px;
    background: var(--panel-soft);
    color: var(--text);
    padding: 0.02rem 0.32rem;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.74rem;
  }

  .hidden-input {
    display: none;
  }

  .status {
    padding: 0.25rem 0.6rem;
    color: var(--warn);
    font-size: 0.78rem;
    border-top: 1px solid var(--chrome-strong);
  }
</style>
