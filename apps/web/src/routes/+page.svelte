<script lang="ts">
  import { onMount } from 'svelte';
  import { base } from '$app/paths';
  import {
    CommandPalette,
    EditorView,
    FirstRunKeyModePrompt,
    HomeView,
    ModeRail,
    PlotsView,
    ProjectView,
    RemovalUndoBanner,
    RecordingStrip,
    ShortcutEditor,
    provideCommandRegistry,
    provideKeyBindings,
    registerCommands,
    createGroup as treeCreateGroup,
    renameGroup as treeRenameGroup,
    dissolveGroup as treeDissolveGroup,
    moveNode as treeMoveNode,
    type AudioExportRequest,
    type AudioInfo,
    type HomeIndex,
    type LibraryNode,
    type ProjectExportMode,
    type ProjectSummary,
    type RecordingEntry,
    DEFAULT_PALETTE,
    loadCustomRamps,
    saveCustomRamps,
    type CustomRamp,
    type PaletteSelection
  } from '@phonia/ui';
  import { WasmCoreClient } from '$lib/core/WasmCoreClient';
  import { WebAudioPlayback } from '$lib/playback/WebAudioPlayback';
  import { MicRecorder, canRecord, type RecorderDevice, type RecorderLevel } from '$lib/audio/MicRecorder';
  import {
    AUTOSAVE_DEBOUNCE_MS,
    AUTOSAVE_MAX_WAIT_MS,
    ProjectStore,
    type ProjectState
  } from '$lib/project/ProjectStore';
  import LandingPage from '$lib/landing/LandingPage.svelte';

  type Route = 'home' | 'project' | 'editor' | 'plots';

  let client = $state<WasmCoreClient | null>(null);
  let store = $state<ProjectStore | null>(null);
  let playback = $state<WebAudioPlayback | null>(null);

  let route = $state<Route>('home');
  let projects = $state<ProjectSummary[]>([]);
  let homeIndex = $state<HomeIndex>({ pinned: [], groups: [] });
  let project = $state<ProjectState | null>(null);
  let recording = $state<RecordingEntry | null>(null);

  let audio = $state<AudioInfo | null>(null);
  const mode = $derived<'library' | 'analyze' | 'plots'>(
    route === 'editor' ? 'analyze' : route === 'plots' ? 'plots' : 'library'
  );
  // Home group holding the open project, shown as the breadcrumb's first crumb.
  const projectGroupName = $derived.by(() => {
    const p = project;
    if (!p) return undefined;
    return homeIndex.groups.find((g) => g.members.includes(p.id))?.name;
  });

  function handleModeNavigate(next: 'library' | 'analyze' | 'plots') {
    if (next === 'library') {
      if (route === 'editor' || route === 'plots') backToProject();
    } else if (next === 'analyze') {
      if (audio) route = 'editor';
    } else if (next === 'plots') {
      if (audio) route = 'plots';
    }
  }

  let annotationId = $state<bigint | null>(null);
  // Bumped when a host action mutates the active annotation without changing its
  // id (e.g. copying cropped tiers into a freshly opened extract), so the tier
  // pane re-fetches.
  let tierRefreshToken = $state(0);
  let cursorTime = $state(0);
  // The editor's live time selection, so Plots can scope a figure to it.
  let editorSelection = $state<{ t0: number; t1: number } | null>(null);
  let isPlaying = $state(false);
  let loopEnabled = $state(false);
  let theme = $state<'light' | 'dark'>('light');
  // The active spectrogram palette (default the brand ramp) and the machine's
  // saved custom ramps. Both persist in localStorage, app-wide. `paletteInvert`
  // renders whichever ramp is active in reverse; it persists alongside.
  let palette = $state<PaletteSelection>(DEFAULT_PALETTE);
  let paletteInvert = $state(false);
  let customRamps = $state<CustomRamp[]>([]);
  // App-wide UI scale as a fraction of the base root font size. rem-based layout
  // grows and shrinks with it; the bounds keep both extremes usable.
  let uiScale = $state(1);
  const UI_SCALE_MIN = 0.9;
  const UI_SCALE_MAX = 1.5;
  const UI_SCALE_STEP = 0.1;
  const UI_SCALE_BASE_PX = 16;
  let error = $state('');
  let busy = $state(false);
  let busyLabel = $state('');
  let dirty = $state(false);
  // Id of the temporary sample project while it is still unpromoted.
  let ephemeralId = $state<string | null>(null);
  let recovery = $state<{ id: string; name: string } | null>(null);

  // Deletion runs through the journaled detach; the row hides during the undo
  // window and the OPFS files are purged only when the project is saved.
  //
  // The toast's Undo action must target the delete's own journal entry, not
  // whatever the journal head happens to be when it's clicked: any other
  // journaled edit inside the 8-second window would otherwise be the thing
  // that actually gets undone. `journalEntryId` is the id captured right
  // after the delete; `stale` flips true once a later check finds the head
  // has moved on, at which point the button stops calling undo() at all.
  let pendingRemovals = $state<number[]>([]);
  let removalUndo = $state<{
    mediaId: number;
    name: string;
    journalEntryId: bigint | null;
    stale: boolean;
  } | null>(null);
  let removalTimer: ReturnType<typeof setTimeout> | null = null;

  function collapsedOf(target: ProjectState): number[] {
    const view = target.view as { collapsedGroups?: number[] } | null;
    return Array.isArray(view?.collapsedGroups) ? view.collapsedGroups : [];
  }

  function clearPendingRemovals() {
    pendingRemovals = [];
    removalUndo = null;
    if (removalTimer) clearTimeout(removalTimer);
    removalTimer = null;
  }

  // Microphone recording. The recorder lives on the main thread and forwards
  // planar chunks to the engine worker; the strip reads the meter and elapsed
  // time. `recordingSupported` gates the Record controls so the desktop shell
  // (no getUserMedia) simply never shows them.
  let recordingSupported = $state(false);
  let recorder: MicRecorder | null = null;
  let capturing = $state(false);
  let recordingId: bigint | null = null;
  let recordingName = '';
  let recordStartMs = 0;
  let recordDevices = $state<RecorderDevice[]>([]);
  let recordDeviceId = $state('');
  let recordLevel = $state<RecorderLevel>({ rms: 0, peak: 0, clipped: false });
  let recordClipLatched = $state(false);
  let recordElapsed = $state(0);
  let recordSampleRate = $state(0);
  // True while capturing into a project that this take just created (recording
  // started from Home with no project open), so the strip can name it plainly.
  let recordDestinationNew = $state(false);

  const commands = provideCommandRegistry();
  const keyBindings = provideKeyBindings();
  let shortcutEditorOpen = $state(false);

  registerCommands([
    {
      id: 'openShortcutEditor',
      title: 'Keyboard shortcuts…',
      group: 'Appearance',
      keywords: ['keymap', 'rebind', 'key mode', 'praat', 'shortcuts', 'keyboard'],
      run: () => {
        shortcutEditorOpen = true;
      }
    }
  ]);

  // First-time visitors land on the marketing page instead of the app; a
  // returning visitor (the flag below) skips straight to the app. Three
  // cases override the flag entirely:
  //  - Embedded in an iframe: always the app, never the landing page. The
  //    landing page's own live-app preview embeds this same route, so
  //    without this check a first-time visitor's landing page would embed
  //    a copy of itself embedding the app, recursing without end.
  //  - `?app=1`: the landing page's own "Open Phonia" link, when served
  //    from the marketing subdomain, uses this to land straight in the app
  //    on the very first cross-origin visit rather than showing the
  //    landing page a second time (localStorage isn't shared across
  //    origins, so the flag below can't do this on its own).
  //  - `about.phonia.app`: the marketing subdomain is the landing page at
  //    every path, regardless of visit history.
  const LANDING_VISITED_KEY = 'phonia:visited';

  function computeShowLanding(): boolean {
    if (typeof window === 'undefined') return false;
    if (window.top !== window.self) return false;
    if (new URLSearchParams(window.location.search).get('app') === '1') return false;
    if (window.location.hostname === 'about.phonia.app') return true;
    try {
      return localStorage.getItem(LANDING_VISITED_KEY) !== '1';
    } catch {
      return false;
    }
  }

  let showLanding = $state(computeShowLanding());

  function enterApp() {
    try {
      localStorage.setItem(LANDING_VISITED_KEY, '1');
    } catch {
      // Storage unavailable: the landing page simply shows again next visit.
    }
    showLanding = false;
  }

  if (typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('app') === '1') {
    try {
      localStorage.setItem(LANDING_VISITED_KEY, '1');
    } catch {
      // Storage unavailable: nothing to persist.
    }
  }

  // `?sample=1` drives the landing page's live embed: it opens the bundled
  // sample project and jumps straight to its first recording's analysis, the
  // same path the home screen's "Open sample project" button runs, so the
  // frame shows a working waveform, spectrogram, and tiers instead of the
  // empty home screen. Independent of `?app=1` — the embed always sends
  // both, but nothing here depends on that pairing.
  const autoOpenSample =
    typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('sample') === '1';

  // Autosave debounce, driven from a coarse tick against the engine state hash.
  let lastHash: bigint | null = null;
  let pendingSince: number | null = null;
  let lastChange = 0;
  let autosaveBusy = false;
  let frame = 0;
  let saveTimer: ReturnType<typeof setInterval> | null = null;

  onMount(() => {
    client = new WasmCoreClient();
    store = new ProjectStore(client);
    playback = new WebAudioPlayback();
    if (window.parent !== window) {
      window.parent.postMessage({ type: 'phonia:ready' }, '*');
    }
    const saved = localStorage.getItem('phonix-theme');
    theme =
      saved === 'dark' || saved === 'light'
        ? saved
        : window.matchMedia('(prefers-color-scheme: dark)').matches
          ? 'dark'
          : 'light';
    applyTheme(theme);

    customRamps = loadCustomRamps();
    ({ palette, invert: paletteInvert } = loadPalette(customRamps));

    const savedScale = Number(localStorage.getItem('phonix-ui-scale'));
    uiScale = Number.isFinite(savedScale) && savedScale > 0 ? clampScale(savedScale) : 1;
    applyUiScale(uiScale);

    void refreshProjects();
    if (autoOpenSample) void openSampleIntoEditor();

    recordingSupported = canRecord();
    if (recordingSupported) recorder = new MicRecorder(`${base}/recorder-worklet.js`);

    const tick = () => {
      if (playback) {
        cursorTime = playback.position;
        isPlaying = playback.playing;
      }
      if (capturing) recordElapsed = (performance.now() - recordStartMs) / 1000;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    saveTimer = setInterval(() => void autosaveTick(), 500);

    return () => {
      cancelAnimationFrame(frame);
      if (saveTimer) clearInterval(saveTimer);
      recorder?.cancel();
      client?.destroy();
      playback?.close();
    };
  });

  function applyTheme(next: 'light' | 'dark') {
    document.documentElement.classList.toggle('dark', next === 'dark');
    localStorage.setItem('phonix-theme', next);
  }

  function handleThemeChange(next: 'light' | 'dark') {
    theme = next;
    applyTheme(next);
  }

  const PALETTE_KEY = 'phonia:palette';

  // Restore the saved palette, resolving a custom selection against the live
  // ramp list so an edited ramp reloads with its current stops. Falls back to
  // the default when the saved ramp was deleted or nothing was stored.
  function loadPalette(ramps: CustomRamp[]): { palette: PaletteSelection; invert: boolean } {
    try {
      const raw = localStorage.getItem(PALETTE_KEY);
      if (!raw) return { palette: DEFAULT_PALETTE, invert: false };
      const saved = JSON.parse(raw) as { kind: string; name?: string; id?: string; invert?: boolean };
      const invert = saved.invert === true;
      if (saved.kind === 'custom' && saved.id) {
        const ramp = ramps.find((r) => r.id === saved.id);
        return { palette: ramp ? { kind: 'custom', ramp } : DEFAULT_PALETTE, invert };
      }
      if (saved.kind === 'builtin' && saved.name) {
        return { palette: { kind: 'builtin', name: saved.name } as PaletteSelection, invert };
      }
    } catch {
      // Unreadable selection: the default ramp stands.
    }
    return { palette: DEFAULT_PALETTE, invert: false };
  }

  function persistPalette(sel: PaletteSelection, invert: boolean) {
    try {
      const ref =
        sel.kind === 'custom'
          ? { kind: 'custom', id: sel.ramp.id, invert }
          : { kind: 'builtin', name: sel.name, invert };
      localStorage.setItem(PALETTE_KEY, JSON.stringify(ref));
    } catch {
      // Storage unavailable: the selection stays for the session.
    }
  }

  function handlePaletteChange(next: PaletteSelection) {
    palette = next;
    persistPalette(next, paletteInvert);
  }

  function handlePaletteInvertToggle() {
    paletteInvert = !paletteInvert;
    persistPalette(palette, paletteInvert);
  }

  // Persist a created or edited ramp, keeping the list keyed by id, and refresh
  // the active selection if it names the same ramp.
  function saveRamp(ramp: CustomRamp) {
    const idx = customRamps.findIndex((r) => r.id === ramp.id);
    customRamps =
      idx >= 0
        ? customRamps.map((r) => (r.id === ramp.id ? ramp : r))
        : [...customRamps, ramp];
    saveCustomRamps(customRamps);
    if (palette.kind === 'custom' && palette.ramp.id === ramp.id) {
      palette = { kind: 'custom', ramp };
    }
  }

  function deleteRamp(id: string) {
    customRamps = customRamps.filter((r) => r.id !== id);
    saveCustomRamps(customRamps);
    if (palette.kind === 'custom' && palette.ramp.id === id) {
      handlePaletteChange(DEFAULT_PALETTE);
    }
  }

  function clampScale(value: number): number {
    return Math.min(UI_SCALE_MAX, Math.max(UI_SCALE_MIN, Math.round(value * 100) / 100));
  }

  function applyUiScale(next: number) {
    document.documentElement.style.fontSize = `${(UI_SCALE_BASE_PX * next).toFixed(3)}px`;
    localStorage.setItem('phonix-ui-scale', String(next));
  }

  function setUiScale(next: number) {
    uiScale = clampScale(next);
    applyUiScale(uiScale);
  }

  function nudgeUiScale(direction: number) {
    setUiScale(uiScale + direction * UI_SCALE_STEP);
  }

  function resetUiScale() {
    setUiScale(1);
  }

  registerCommands([
    {
      id: 'switchTheme',
      title: 'Switch color theme',
      group: 'Appearance',
      keywords: ['dark', 'light', 'appearance', 'toggle theme'],
      run: () => handleThemeChange(theme === 'light' ? 'dark' : 'light')
    },
    {
      id: 'uiScaleUp',
      title: 'Increase UI scale',
      group: 'Appearance',
      shortcut: 'Ctrl/Cmd++',
      keywords: ['zoom interface', 'font size', 'bigger', 'text size'],
      run: () => nudgeUiScale(1)
    },
    {
      id: 'uiScaleDown',
      title: 'Decrease UI scale',
      group: 'Appearance',
      shortcut: 'Ctrl/Cmd+-',
      keywords: ['zoom interface', 'font size', 'smaller', 'text size'],
      run: () => nudgeUiScale(-1)
    },
    {
      id: 'uiScaleReset',
      title: 'Reset UI scale',
      group: 'Appearance',
      shortcut: 'Ctrl/Cmd+0',
      keywords: ['zoom interface', 'font size', 'default'],
      run: resetUiScale
    }
  ]);

  async function refreshProjects() {
    if (!store) return;
    try {
      projects = await store.list();
      await loadHomeIndex();
    } catch (caught) {
      report(caught);
    }
  }

  /**
   * Reads the home index and drops any pin or group membership naming a project
   * that no longer exists, so a deleted project leaves no dangling reference.
   * Writes back only when pruning changed something.
   */
  async function loadHomeIndex() {
    if (!store) return;
    const known = new Set(projects.map((p) => p.id));
    const raw = await store.readHomeIndex();
    const pinned = raw.pinned.filter((id) => known.has(id));
    const groups = raw.groups.map((g) => ({
      ...g,
      members: g.members.filter((id) => known.has(id))
    }));
    const pruned =
      pinned.length !== raw.pinned.length ||
      groups.some((g, i) => g.members.length !== raw.groups[i].members.length);
    homeIndex = { pinned, groups };
    if (pruned) await store.writeHomeIndex(homeIndex);
  }

  async function updateHomeIndex(next: HomeIndex) {
    if (!store) return;
    homeIndex = next;
    try {
      await store.writeHomeIndex(next);
    } catch (caught) {
      report(caught);
    }
  }

  function togglePin(id: string) {
    const pinned = homeIndex.pinned.includes(id)
      ? homeIndex.pinned.filter((x) => x !== id)
      : [...homeIndex.pinned, id];
    void updateHomeIndex({ ...homeIndex, pinned });
  }

  // A project lives in one group: seeding or moving into a group first drops it
  // from every other group's membership.
  function withoutMembers(groups: HomeIndex['groups'], ids: string[]): HomeIndex['groups'] {
    const drop = new Set(ids);
    return groups.map((g) => ({ ...g, members: g.members.filter((m) => !drop.has(m)) }));
  }

  function createGroupFrom(memberIds: string[]) {
    const group = {
      id: crypto.randomUUID(),
      name: 'New group',
      members: [...memberIds],
      collapsed: false
    };
    const groups = [...withoutMembers(homeIndex.groups, memberIds), group];
    void updateHomeIndex({ ...homeIndex, groups });
  }

  function renameHomeGroup(groupId: string, name: string) {
    const trimmed = name.trim();
    if (!trimmed) return;
    const groups = homeIndex.groups.map((g) => (g.id === groupId ? { ...g, name: trimmed } : g));
    void updateHomeIndex({ ...homeIndex, groups });
  }

  function dissolveHomeGroup(groupId: string) {
    const groups = homeIndex.groups.filter((g) => g.id !== groupId);
    void updateHomeIndex({ ...homeIndex, groups });
  }

  function toggleGroupCollapse(groupId: string) {
    const groups = homeIndex.groups.map((g) =>
      g.id === groupId ? { ...g, collapsed: !g.collapsed } : g
    );
    void updateHomeIndex({ ...homeIndex, groups });
  }

  function moveToGroup(id: string, groupId: string | null) {
    let groups = withoutMembers(homeIndex.groups, [id]);
    if (groupId !== null) {
      groups = groups.map((g) => (g.id === groupId ? { ...g, members: [...g.members, id] } : g));
    }
    void updateHomeIndex({ ...homeIndex, groups });
  }

  async function exportStoredProject(id: string) {
    if (!store) return;
    try {
      const { name, bytes } = await store.exportStored(id);
      downloadBytes(bytes, `${sanitizeFileName(name)}.phxproj`, 'application/zip');
    } catch (caught) {
      report(caught);
    }
  }

  async function batchDeleteProjects(ids: string[]) {
    if (!store || ids.length === 0) return;
    error = '';
    busy = true;
    busyLabel = `Deleting ${ids.length} ${ids.length === 1 ? 'project' : 'projects'}…`;
    try {
      for (const id of ids) await store.delete(id);
      await refreshProjects();
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  function report(caught: unknown) {
    error = caught instanceof Error ? caught.message : String(caught);
  }

  const AUDIO_EXTENSIONS = ['.wav', '.aiff', '.aif', '.flac'];

  function deriveName(files: File[]): string {
    for (const file of files) {
      const rel = (file as File & { webkitRelativePath?: string }).webkitRelativePath;
      if (rel && rel.includes('/')) return rel.split('/')[0];
    }
    const audio = files.find((file) => {
      const lower = file.name.toLowerCase();
      return AUDIO_EXTENSIONS.some((ext) => lower.endsWith(ext));
    });
    if (audio) return audio.name.replace(/\.[^.]+$/, '');
    return 'Untitled project';
  }

  async function importToNewProject(files: File[]) {
    if (!store) return;
    error = '';
    busy = true;
    busyLabel = 'Importing recordings…';
    try {
      const created = await store.create(deriveName(files));
      project = created;
      route = 'project';
      await store.importFiles(created, files, () => {
        project = { ...created };
      });
      project = { ...created };
      resetAutosaveBaseline();
      await refreshProjects();
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  interface SampleManifest {
    name: string;
    files: Array<{ path: string; name: string; mime: string }>;
  }

  // The sample opens as a temporary project: hidden from the home screen and
  // swept from disk on the next sample open. The first real change the user
  // makes promotes it into a permanent project (see promoteEphemeral).
  async function openSampleProject() {
    if (!store) return;
    error = '';
    busy = true;
    busyLabel = 'Loading sample project…';
    try {
      const manifest: SampleManifest = await fetch(`${base}/sample/manifest.json`).then((res) => {
        if (!res.ok) throw new Error('Sample project manifest is unavailable.');
        return res.json();
      });
      const files = await Promise.all(
        manifest.files.map(async (entry) => {
          const res = await fetch(`${base}/sample/${entry.path}`);
          if (!res.ok) throw new Error(`Sample file ${entry.path} is unavailable.`);
          return new File([await res.arrayBuffer()], entry.name, { type: entry.mime });
        })
      );
      project = null;
      resetAutosaveBaseline();
      while (autosaveBusy) await new Promise((resolve) => setTimeout(resolve, 50));
      await store.sweepEphemeral();
      const created = await store.create(manifest.name, { ephemeral: true });
      ephemeralId = created.id;
      project = created;
      route = 'project';
      await store.importFiles(created, files, () => {
        project = { ...created };
      });
      project = { ...created };
      resetAutosaveBaseline();
      await refreshProjects();
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  // A temporary sample project becomes permanent on the first real change.
  async function promoteEphemeral() {
    if (!store || !project || project.id !== ephemeralId) return;
    ephemeralId = null;
    try {
      await store.promote(project.id);
      await refreshProjects();
    } catch (caught) {
      report(caught);
    }
  }

  // Drives the sample straight into the analysis editor for `?sample=1`
  // (the landing page's live embed): open it exactly as the "Open sample
  // project" button does, then open its first recording — the one with a
  // matching bundled TextGrid — so the frame lands on a rendered waveform,
  // spectrogram, and tier view rather than the project's file list.
  async function openSampleIntoEditor() {
    await openSampleProject();
    const first = project?.recordings[0];
    if (first) await openRecording(first);
  }

  async function createEmptyProject(name: string) {
    if (!store) return;
    error = '';
    try {
      const created = await store.create(name);
      project = created;
      route = 'project';
      resetAutosaveBaseline();
      await refreshProjects();
    } catch (caught) {
      report(caught);
    }
  }

  function requestOpen(id: string) {
    const summary = projects.find((entry) => entry.id === id);
    if (summary?.hasRecovery) {
      recovery = { id, name: summary.name };
      return;
    }
    void doOpen(id);
  }

  async function doOpen(id: string) {
    if (!store) return;
    error = '';
    busy = true;
    busyLabel = 'Opening project…';
    try {
      const result = await store.open(id);
      project = result.project;
      route = 'project';
      dirty = false;
      clearPendingRemovals();
      resetAutosaveBaseline();
      await refreshProjects();
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  async function recoverAccept() {
    const target = recovery;
    recovery = null;
    if (target) await doOpen(target.id);
  }

  async function recoverDiscard() {
    const target = recovery;
    recovery = null;
    if (target && store) {
      await store.discardRecovery(target.id);
      await doOpen(target.id);
    }
  }

  async function addFilesToProject(files: File[]) {
    if (!store || !project) return;
    error = '';
    busy = true;
    busyLabel = 'Importing recordings…';
    const current = project;
    try {
      await store.importFiles(current, files, () => {
        project = { ...current };
      });
      project = { ...current };
      await promoteEphemeral();
      resetAutosaveBaseline();
      await refreshProjects();
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  async function openRecording(entry: RecordingEntry) {
    if (!client || !store || !project) return;
    error = '';
    try {
      if (entry.audioId === null) return;
      if (entry.annotationId === null) {
        entry.annotationId = await client.createAnnotation(entry.audioId, 0, entry.duration);
        entry.hasAnnotation = true;
        project = { ...project };
      }
      recording = entry;
      audio = {
        id: entry.audioId,
        duration: entry.duration,
        sampleRate: entry.sampleRate,
        channels: entry.channels,
        name: entry.name,
        hash: entry.hash
      };
      annotationId = entry.annotationId;
      cursorTime = 0;
      // Playback decodes through the browser's native decoder, which does not
      // reliably support every container the engine imports (AIFF in
      // particular). Re-encoding through the engine's own decoded buffer keeps
      // playback independent of the stored file's format — every recording
      // plays back the same way regardless of whether it was imported as WAV,
      // AIFF, or FLAC.
      const wav = await client.exportSpanWav(entry.audioId, 0, entry.duration, 'Pcm16');
      const owned = new Uint8Array(wav.byteLength);
      owned.set(wav);
      await playback?.load(new File([owned], `${entry.name}.wav`, { type: 'audio/wav' }));
      playback?.seek(0);
      resetAutosaveBaseline();
      route = 'editor';
    } catch (caught) {
      report(caught);
    }
  }

  function switchRecording(mediaId: number) {
    const entry = project?.recordings.find((item) => item.mediaId === mediaId);
    if (entry) void openRecording(entry);
  }

  async function editorImportFile(file: File) {
    if (!store || !project) return;
    error = '';
    busy = true;
    busyLabel = 'Importing recording…';
    const current = project;
    try {
      const before = current.recordings.length;
      await store.importFiles(current, [file], () => {
        project = { ...current };
      });
      project = { ...current };
      await promoteEphemeral();
      resetAutosaveBaseline();
      await refreshProjects();
      const added = current.recordings[before] ?? current.recordings.at(-1);
      if (added) await openRecording(added);
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  async function handlePlayToggle() {
    if (!playback || !audio) return;
    error = '';
    try {
      isPlaying = await playback.toggle(cursorTime);
    } catch (caught) {
      report(caught);
    }
  }

  function handleLoopToggle() {
    loopEnabled = !loopEnabled;
    playback?.setLoop(loopEnabled);
  }

  function handleCursorChange(time: number) {
    cursorTime = time;
    playback?.seek(time);
  }

  function backToProject() {
    route = 'project';
  }

  function backToHome() {
    clearPendingRemovals();
    void refreshProjects();
    route = 'home';
  }

  async function saveProject() {
    if (!store || !project) return;
    try {
      if (pendingRemovals.length > 0) {
        await store.finalizeRemovals(project, pendingRemovals);
        clearPendingRemovals();
      }
      await store.writeProjectFile(project);
      dirty = false;
      pendingSince = null;
      await promoteEphemeral();
      await refreshProjects();
    } catch (caught) {
      report(caught);
    }
  }

  async function deleteProject(id: string) {
    if (!store) return;
    try {
      await store.delete(id);
      await refreshProjects();
    } catch (caught) {
      report(caught);
    }
  }

  async function renameProject(id: string, name: string) {
    if (!store) return;
    try {
      await store.rename(id, name);
      if (project?.id === id) {
        project = { ...project, name: name.trim() || project.name };
        await promoteEphemeral();
      }
      await refreshProjects();
    } catch (caught) {
      report(caught);
    }
  }

  async function renameRecording(mediaId: number, name: string) {
    if (!store || !project) return;
    try {
      await store.renameRecording(project, mediaId, name);
      project = { ...project };
      await promoteEphemeral();
      if (recording?.mediaId === mediaId) {
        recording = project.recordings.find((entry) => entry.mediaId === mediaId) ?? recording;
        if (audio && recording) audio = { ...audio, name: recording.name };
      }
      await refreshProjects();
    } catch (caught) {
      report(caught);
    }
  }

  async function duplicateProject(id: string) {
    if (!store) return;
    try {
      await store.duplicate(id);
      await refreshProjects();
    } catch (caught) {
      report(caught);
    }
  }

  // --- Project and audio I/O ---

  let notice = $state('');

  function sanitizeFileName(name: string): string {
    const cleaned = name.replace(/[\\/:*?"<>|]/g, '_').trim();
    return cleaned || 'untitled';
  }

  function downloadBytes(bytes: Uint8Array, fileName: string, mime: string) {
    const owned = new Uint8Array(bytes.byteLength);
    owned.set(bytes);
    const blob = new Blob([owned], { type: mime });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = fileName;
    anchor.click();
    URL.revokeObjectURL(url);
  }

  async function exportProject(mode: ProjectExportMode) {
    if (!store || !project) return;
    error = '';
    busy = true;
    busyLabel = mode === 'bundle' ? 'Building bundle…' : 'Exporting project…';
    try {
      const bytes = await store.exportProject(project, mode);
      downloadBytes(bytes, `${sanitizeFileName(project.name)}.phxproj`, 'application/zip');
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  async function openProjectFile(file: File) {
    if (!store) return;
    error = '';
    notice = '';
    busy = true;
    busyLabel = 'Opening project file…';
    try {
      const result = await store.importProjectFile(file);
      project = result.project;
      route = 'project';
      dirty = false;
      clearPendingRemovals();
      resetAutosaveBaseline();
      await refreshProjects();
      if (result.gaps.length > 0) {
        const names = result.gaps.map((gap) => gap.name).join(', ');
        notice = `Imported. ${result.gaps.length} recording(s) could not be located: ${names}. Re-link them by adding the source audio.`;
      }
    } catch (caught) {
      report(caught);
    } finally {
      busy = false;
    }
  }

  async function exportRecordingAudio(mediaId: number) {
    if (!client || !project) return;
    const entry = project.recordings.find((r) => r.mediaId === mediaId);
    if (!entry || entry.audioId === null) return;
    error = '';
    try {
      const bytes = await client.exportSpanWav(entry.audioId, 0, entry.duration, 'Pcm16');
      downloadBytes(bytes, `${sanitizeFileName(entry.name)}.wav`, 'audio/wav');
    } catch (caught) {
      report(caught);
    }
  }

  async function exportEditorAudio(request: AudioExportRequest) {
    if (!client || !audio) return;
    error = '';
    try {
      const bytes = request.filtered
        ? await client.exportBandFilteredSpanWav(
            audio.id,
            request.t0,
            request.t1,
            request.f0,
            request.f1,
            request.bits
          )
        : await client.exportSpanWav(audio.id, request.t0, request.t1, request.bits);
      const base = sanitizeFileName(recording?.name ?? audio.name ?? 'audio');
      const suffix = request.scope === 'selection' ? '-selection' : '';
      downloadBytes(bytes, `${base}${suffix}.wav`, 'audio/wav');
    } catch (caught) {
      report(caught);
    }
  }

  // --- Library tree ---

  async function applyLibrary(next: LibraryNode[]) {
    if (!store || !project) return;
    try {
      await store.updateLibrary(project, next);
      project = { ...project };
      await promoteEphemeral();
    } catch (caught) {
      report(caught);
    }
  }

  function createGroup() {
    if (!project) return;
    void applyLibrary(treeCreateGroup(project.groups, 'New group', null));
  }

  function renameGroup(groupId: number, name: string) {
    if (!project) return;
    void applyLibrary(treeRenameGroup(project.groups, groupId, name));
  }

  function dissolveGroup(groupId: number) {
    if (!project) return;
    void applyLibrary(treeDissolveGroup(project.groups, groupId));
  }

  function moveNode(key: string, targetGroupId: number | null, index: number) {
    if (!project) return;
    void applyLibrary(treeMoveNode(project.groups, key, targetGroupId, index));
  }

  async function toggleCollapse(groupId: number) {
    if (!store || !project) return;
    const current = collapsedOf(project);
    const next = current.includes(groupId)
      ? current.filter((id) => id !== groupId)
      : [...current, groupId];
    const view = { ...((project.view as object | null) ?? {}), collapsedGroups: next };
    try {
      await store.updateView(project, view);
      project = { ...project };
    } catch (caught) {
      report(caught);
    }
  }

  // --- Metadata ---

  async function updateRecordingMetadata(
    mediaId: number,
    metadata: { description: string; authors: string[]; tags: string[] }
  ) {
    if (!store || !project) return;
    try {
      await store.updateRecordingMetadata(project, mediaId, metadata);
      project = { ...project };
      await promoteEphemeral();
    } catch (caught) {
      report(caught);
    }
  }

  async function updateProjectMetadata(metadata: {
    description: string;
    authors: string[];
    tags: string[];
  }) {
    if (!store || !project) return;
    try {
      await store.updateProjectMetadata(project, metadata);
      project = { ...project };
      await promoteEphemeral();
    } catch (caught) {
      report(caught);
    }
  }

  // --- Delete with undo ---

  async function deleteRecording(mediaId: number) {
    if (!client || !project) return;
    const entry = project.recordings.find((r) => r.mediaId === mediaId);
    if (!entry) return;
    try {
      let journalEntryId: bigint | null = null;
      if (entry.audioId !== null) {
        await client.detachAudio(entry.audioId);
        // The detach just applied is the journal head; capture its id so the
        // toast can later confirm it is still the entry undo() would target.
        journalEntryId = await client.journalHeadId();
      }
      // The detach cascaded the annotation off the session; drop the reference so
      // an autosave inside the undo window does not serialize a removed document.
      entry.annotationId = null;
      entry.hasAnnotation = false;
      pendingRemovals = [...pendingRemovals, mediaId];
      removalUndo = { mediaId, name: entry.name, journalEntryId, stale: false };
      project = { ...project };
      if (removalTimer) clearTimeout(removalTimer);
      removalTimer = setTimeout(() => (removalUndo = null), 8000);
      // Deleting from the recordings rail may take the take that is open; the
      // editor must not keep displaying detached audio. Move to a neighbouring
      // take, or leave the editor when the corpus has nothing left.
      if (route === 'editor' && recording?.mediaId === mediaId) {
        const list = project.recordings;
        const index = list.indexOf(entry);
        const remaining = list.filter((r) => !pendingRemovals.includes(r.mediaId));
        const neighbour = remaining.find((r) => list.indexOf(r) > index) ?? remaining.at(-1) ?? null;
        if (neighbour) {
          await openRecording(neighbour);
        } else {
          recording = null;
          audio = null;
          annotationId = null;
          editorSelection = null;
          route = 'project';
        }
      }
    } catch (caught) {
      report(caught);
    }
  }

  async function undoRemoval() {
    if (!client || !project || !removalUndo || removalUndo.stale) return;
    const target = removalUndo;
    try {
      const head = target.journalEntryId === null ? null : await client.journalHeadId();
      if (target.journalEntryId === null || head !== target.journalEntryId) {
        // Something else was journaled since the delete (or there was never
        // anything to undo); a blind undo() would hit that entry instead of
        // restoring this recording. Stop offering the action rather than
        // undo the wrong thing.
        removalUndo = { ...target, stale: true };
        return;
      }
      if (removalTimer) clearTimeout(removalTimer);
      removalTimer = null;
      removalUndo = null;
      await client.undo();
      const entry = project.recordings.find((r) => r.mediaId === target.mediaId);
      if (entry && entry.audioId !== null) {
        const anns = await client.listAnnotations(entry.audioId);
        entry.annotationId = anns.length ? anns[anns.length - 1] : null;
        entry.hasAnnotation = anns.length > 0;
      }
      pendingRemovals = pendingRemovals.filter((id) => id !== target.mediaId);
      project = { ...project };
    } catch (caught) {
      report(caught);
    }
  }

  function resetAutosaveBaseline() {
    lastHash = null;
    pendingSince = null;
  }

  async function autosaveTick() {
    if (!client || !store || !project || autosaveBusy) return;
    if (route === 'home') return;
    autosaveBusy = true;
    try {
      const hash = await client.stateHash();
      const now = Date.now();
      if (lastHash === null) {
        lastHash = hash;
      } else if (hash !== lastHash) {
        lastHash = hash;
        lastChange = now;
        pendingSince ??= now;
        dirty = true;
        await promoteEphemeral();
      }
      if (pendingSince !== null) {
        const quiet = now - lastChange >= AUTOSAVE_DEBOUNCE_MS;
        const waited = now - pendingSince >= AUTOSAVE_MAX_WAIT_MS;
        if (quiet || waited) {
          pendingSince = null;
          await store.writeAutosave(project);
          await refreshProjects();
        }
      }
    } catch (caught) {
      report(caught);
    } finally {
      autosaveBusy = false;
    }
  }

  function timestampName(): string {
    const now = new Date();
    const pad = (value: number) => String(value).padStart(2, '0');
    return (
      `Recording ${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())} ` +
      `${pad(now.getHours())}.${pad(now.getMinutes())}.${pad(now.getSeconds())}`
    );
  }

  function sessionName(): string {
    const now = new Date();
    const pad = (value: number) => String(value).padStart(2, '0');
    return (
      `Recordings ${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())} ` +
      `${pad(now.getHours())}.${pad(now.getMinutes())}`
    );
  }

  function handleRecordLevel(level: RecorderLevel) {
    recordLevel = level;
    if (level.clipped) recordClipLatched = true;
  }

  async function startRecording() {
    if (!client || !store || !recorder || capturing) return;
    error = '';
    try {
      // Recording always lands in a project; make one, named from the moment,
      // on the home screen so the take has a home the strip can announce.
      if (!project) {
        const created = await store.create(sessionName());
        project = created;
        recordDestinationNew = true;
        route = 'project';
        resetAutosaveBaseline();
        await refreshProjects();
      } else {
        recordDestinationNew = false;
      }
      recordingName = timestampName();
      recordClipLatched = false;
      recordLevel = { rms: 0, peak: 0, clipped: false };
      recordElapsed = 0;
      recordSampleRate = 0;

      // Buffer chunks that arrive before the engine take is open, then forward
      // in arrival order so no leading audio is lost to the startup race.
      let recId: bigint | null = null;
      const buffered: Float32Array[] = [];
      const forward = (samples: Float32Array) => {
        if (recId === null) buffered.push(samples);
        else void client?.appendSamples(recId, samples);
      };

      const started = await recorder.start({
        deviceId: recordDeviceId || undefined,
        onChunk: (chunk) => forward(chunk.samples),
        onLevel: handleRecordLevel
      });
      recordSampleRate = started.sampleRate;
      recId = await client.beginRecording(started.sampleRate, started.channels);
      for (const samples of buffered) void client.appendSamples(recId, samples);
      recordingId = recId;
      recordStartMs = performance.now();
      capturing = true;

      // Device labels are readable now that permission is granted.
      recordDevices = await recorder.listDevices();
      if (!recordDeviceId && recordDevices.length > 0) recordDeviceId = recordDevices[0].deviceId;
    } catch (caught) {
      recorder?.cancel();
      capturing = false;
      recordingId = null;
      report(caught);
    }
  }

  async function stopRecording() {
    if (!client || !store || !recorder || !capturing || recordingId === null || !project) return;
    const current = project;
    const recId = recordingId;
    const name = recordingName;
    try {
      await recorder.stop();
      const finished = await client.finishRecording(recId, name);
      const entry = await store.addRecording(current, name, finished);
      project = { ...current };
      capturing = false;
      recordingId = null;
      resetAutosaveBaseline();
      await refreshProjects();
      await openRecording(entry);
    } catch (caught) {
      capturing = false;
      recordingId = null;
      report(caught);
    }
  }

  // Re-imports WAV bytes and appends them to the library as a persisted
  // recording, without switching to it. Returns the new entry, or null.
  async function addWavRecording(wav: Uint8Array, label: string) {
    if (!client || !store || !project) return null;
    const current = project;
    const file = new File([new Uint8Array(wav)], `${label}.wav`, { type: 'audio/wav' });
    const info = await client.importAudio(file);
    const finished = {
      audioId: info.id,
      duration: info.duration,
      sampleRate: info.sampleRate,
      channels: info.channels,
      hash: info.hash ?? (await client.contentHash(wav)),
      wav
    };
    const entry = await store.addRecording(current, label, finished);
    project = { ...current };
    return entry;
  }

  // Registers WAV bytes as a new library recording and opens it. The bytes are
  // re-imported so the take persists to OPFS and switches in exactly as an
  // imported or mic-recorded one does.
  async function persistWavAsRecording(wav: Uint8Array, label: string) {
    const entry = await addWavRecording(wav, label);
    if (!entry) return;
    resetAutosaveBaseline();
    await refreshProjects();
    await openRecording(entry);
  }

  // Saves each labelled interval of a tier as its own recording, in sequence so
  // the OPFS writes don't overlap, refreshing the project once at the end.
  async function extractIntervals(spans: { t0: number; t1: number; label: string }[]) {
    if (!client || !audio || !recording) return;
    const base = recording.name;
    for (const span of spans) {
      if (!(span.t1 > span.t0)) continue;
      const wav = await client.exportSpanWav(audio.id, span.t0, span.t1, 'Float32');
      const label = `${base}_${span.label.replace(/[/\\]/g, '-')}`;
      await addWavRecording(wav, label);
    }
    resetAutosaveBaseline();
    await refreshProjects();
  }

  // Joins every loaded recording in the project, in library order, into one new
  // recording and opens it.
  async function concatenateProject() {
    if (!client || !project) return;
    const ids = project.recordings.flatMap((entry) =>
      entry.audioId !== null ? [entry.audioId] : []
    );
    if (ids.length < 2) return;
    try {
      const wav = await client.concatWav(ids, 'Float32');
      await persistWavAsRecording(wav, `${project.name ?? 'project'} concatenated`);
    } catch (caught) {
      report(caught);
    }
  }

  // Combines the current recording with the next other project recording into a
  // new stereo recording (left = current, right = the other).
  async function combineToStereo() {
    if (!client || !project || !audio) return;
    const currentId = audio.id;
    const other = project.recordings.find(
      (entry) => entry.audioId !== null && entry.audioId !== currentId
    )?.audioId;
    if (other === undefined || other === null) return;
    try {
      const wav = await client.combineStereoWav(currentId, other, 'Float32');
      await persistWavAsRecording(wav, `${recording?.name ?? project.name ?? 'project'} [stereo]`);
    } catch (caught) {
      report(caught);
    }
  }

  // Copies the editor's time selection into a new library recording, encoded to
  // a lossless float WAV so the extract is sample-exact.
  async function extractSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const label = `${recording.name} [${t0.toFixed(2)}–${t1.toFixed(2)} s]`;
    try {
      const wav = await client.exportSpanWav(audio.id, t0, t1, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Extracts the selection like `extractSelection`, but also carries each source
  // tier's labels across, cropped and shifted so the new recording starts at 0.
  async function extractSelectionWithTiers(t0: number, t1: number) {
    if (!client || !audio || !recording || annotationId === null || !(t1 > t0)) return;
    const c = client;
    const audioId = audio.id;
    const sourceAnnId = annotationId;
    const dur = t1 - t0;
    try {
      // Snapshot the source tiers' content in [t0, t1] before the extract opens
      // a different annotation as the active one.
      const tiers = await c.annotationTiers(sourceAnnId);
      const snapshots = await Promise.all(
        tiers.map(async (tier) =>
          tier.kind === 'interval'
            ? {
                name: tier.name,
                kind: 'interval' as const,
                intervals: await c.intervalsInRange(sourceAnnId, tier.id, t0, t1)
              }
            : {
                name: tier.name,
                kind: 'point' as const,
                points: await c.pointsInRange(sourceAnnId, tier.id, t0, t1)
              }
        )
      );

      const wav = await c.exportSpanWav(audioId, t0, t1, 'Float32');
      const label = `${recording.name} [${t0.toFixed(2)}–${t1.toFixed(2)} s]`;
      await persistWavAsRecording(wav, label);
      const newAnnId = annotationId;
      if (newAnnId === null) return;

      for (const snap of snapshots) {
        if (snap.kind === 'interval') {
          const ivs = snap.intervals.filter((iv) => iv.xmax > t0 && iv.xmin < t1);
          if (ivs.length === 0) continue;
          const tierId = await c.addIntervalTier(newAnnId, snap.name);
          for (const iv of ivs) {
            const start = iv.xmin - t0;
            if (start > 0 && start < dur) await c.insertBoundary(newAnnId, tierId, start);
          }
          const newIvs = await c.intervalsInRange(newAnnId, tierId, -1, dur + 1);
          for (let i = 0; i < newIvs.length && i < ivs.length; i += 1) {
            if (ivs[i].label) {
              await c.setIntervalLabel(newAnnId, tierId, newIvs[i].id, ivs[i].label);
            }
          }
        } else {
          const pts = snap.points.filter((p) => p.time >= t0 && p.time <= t1);
          if (pts.length === 0) continue;
          const tierId = await c.addPointTier(newAnnId, snap.name);
          for (const p of pts) {
            await c.insertPoint(newAnnId, tierId, p.time - t0, p.label);
          }
        }
      }
      resetAutosaveBaseline();
      await refreshProjects();
      tierRefreshToken += 1;
    } catch (caught) {
      report(caught);
    }
  }

  // Scales a span to Praat's default 70 dB average intensity and stores it as a
  // new library recording.
  async function scaleSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const targetDb = 70;
    const label = `${recording.name} [${targetDb} dB]`;
    try {
      const wav = await client.scaleIntensitySpanWav(audio.id, t0, t1, targetDb, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Resolves the zero crossing nearest a time for the editor's snap command.
  async function nearestZero(t: number): Promise<number> {
    if (!client || !audio) return t;
    return client.nearestZeroCrossing(audio.id, t);
  }

  // Resolves the per-frame harmonicity track for the editor's contour export.
  async function harmonicityTrack(
    t0: number,
    t1: number
  ): Promise<{ times: Float64Array; hnr: Float64Array }> {
    if (!client || !audio) return { times: new Float64Array(), hnr: new Float64Array() };
    return client.harmonicityTrack(audio.id, t0, t1);
  }

  // Resolves the per-frame CPP track for the editor's overlay and contour export.
  async function cppTrack(
    t0: number,
    t1: number
  ): Promise<{ times: Float64Array; cpp: Float64Array }> {
    if (!client || !audio) return { times: new Float64Array(), cpp: new Float64Array() };
    return client.cppTrack(audio.id, t0, t1);
  }

  // Resolves the LPC-smoothed spectral envelope for the spectrum card overlay.
  async function lpcEnvelope(t0: number, t1: number): Promise<{ freqs: number[]; db: number[] }> {
    if (!client || !audio) return { freqs: [], db: [] };
    return client.lpcSpectrum(audio.id, t0, t1);
  }

  // Normalizes a span so its peak reaches 0.99 and stores it as a new recording.
  async function scalePeakSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const target = 0.99;
    const label = `${recording.name} [peak ${target}]`;
    try {
      const wav = await client.scalePeakSpanWav(audio.id, t0, t1, target, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Resamples the whole recording to a chosen rate and stores it as a new recording.
  async function resampleRecording(hz: number) {
    if (!client || !audio || !recording) return;
    const rate = Math.round(hz);
    if (!Number.isFinite(rate) || rate < 1) return;
    const khz = parseFloat((rate / 1000).toFixed(3));
    const label = `${recording.name} [resampled ${khz} kHz]`;
    try {
      const wav = await client.resampleWav(audio.id, rate, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Reverses a span in time and stores it as a new library recording.
  async function reverseSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const label = `${recording.name} [reversed]`;
    try {
      const wav = await client.reverseSpanWav(audio.id, t0, t1, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Passes the box selection's time span through the engine's Hann band filter
  // and stores the mono result as a new library recording.
  async function filterSelection(t0: number, t1: number, f0: number, f1: number) {
    if (!client || !audio || !recording || !(t1 > t0) || !(f1 > f0)) return;
    const label = `${recording.name} [${Math.round(f0)}–${Math.round(f1)} Hz]`;
    try {
      const wav = await client.exportBandFilteredSpanWav(audio.id, t0, t1, f0, f1, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Attenuates the box selection's frequency band and stores the notch-filtered
  // result as a new recording.
  async function notchSelection(t0: number, t1: number, f0: number, f1: number) {
    if (!client || !audio || !recording || !(t1 > t0) || !(f1 > f0)) return;
    const label = `${recording.name} [notch ${Math.round(f0)}–${Math.round(f1)} Hz]`;
    try {
      const wav = await client.exportNotchFilteredSpanWav(audio.id, t0, t1, f0, f1, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Pre-emphasizes the selection into a new library recording — Praat's +6 dB/
  // octave high-pass with its corner at 50 Hz.
  async function preemphasisSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const label = `${recording.name} [pre-emphasis]`;
    try {
      const wav = await client.applyPreemphasisWav(audio.id, t0, t1, 50, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // De-emphasizes the selection into a new library recording — the recursive
  // integrator with its corner at 50 Hz that undoes a pre-emphasis tilt.
  async function deemphasisSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const label = `${recording.name} [de-emphasis]`;
    try {
      const wav = await client.applyDeemphasisWav(audio.id, t0, t1, 50, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Removes the selection's DC offset (mean) into a new library recording.
  async function subtractMeanSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const label = `${recording.name} [DC removed]`;
    try {
      const wav = await client.subtractMeanWav(audio.id, t0, t1, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Silences the selection while keeping the rest of the recording — Praat's
  // "Set part to zero", for punching out a click or cough.
  async function zeroSelection(t0: number, t1: number) {
    if (!client || !audio || !recording || !(t1 > t0)) return;
    const label = `${recording.name} [silenced]`;
    try {
      const wav = await client.applyZeroWav(audio.id, t0, t1, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  // Writes each channel of a multichannel recording as its own mono recording,
  // in channel order, then opens the first so the split is visible.
  async function extractChannels() {
    if (!client || !audio || !recording || audio.channels < 2) return;
    const base = recording.name;
    try {
      let first: RecordingEntry | null = null;
      for (let channel = 0; channel < audio.channels; channel += 1) {
        const wav = await client.exportChannelWav(audio.id, channel, 'Float32');
        const entry = await addWavRecording(wav, `${base} [ch ${channel + 1}]`);
        if (entry && !first) first = entry;
      }
      resetAutosaveBaseline();
      await refreshProjects();
      if (first) await openRecording(first);
    } catch (caught) {
      report(caught);
    }
  }

  // Mixes a multichannel recording down to one channel as a new library recording.
  async function convertToMono() {
    if (!client || !audio || !recording || audio.channels < 2) return;
    const label = `${recording.name} [mono]`;
    try {
      const wav = await client.convertToMono(audio.id, 'Float32');
      await persistWavAsRecording(wav, label);
    } catch (caught) {
      report(caught);
    }
  }

  async function cancelRecording() {
    if (!recorder || !capturing) return;
    const recId = recordingId;
    recorder.cancel();
    capturing = false;
    recordingId = null;
    try {
      if (recId !== null) await client?.abortRecording(recId);
    } catch (caught) {
      report(caught);
    }
  }

  function toggleRecording() {
    if (capturing) void stopRecording();
    else void startRecording();
  }

  async function selectRecordDevice(deviceId: string) {
    recordDeviceId = deviceId;
    // Switching devices mid-take restarts the capture graph on the new input
    // while the same engine take keeps accumulating.
    if (!capturing || !recorder || recordingId === null) return;
    const recId = recordingId;
    try {
      recorder.cancel();
      const started = await recorder.start({
        deviceId: deviceId || undefined,
        onChunk: (chunk) => void client?.appendSamples(recId, chunk.samples),
        onLevel: handleRecordLevel
      });
      recordSampleRate = started.sampleRate;
    } catch (caught) {
      report(caught);
    }
  }

  registerCommands([
    {
      id: 'startRecording',
      title: 'Start recording',
      group: 'Project',
      api: ['beginRecording'],
      shortcut: 'R',
      keywords: ['microphone', 'capture', 'mic', 'record'],
      enabled: () => recordingSupported && !capturing,
      run: () => void startRecording()
    },
    {
      id: 'stopRecording',
      title: 'Stop recording',
      group: 'Project',
      api: ['finishRecording'],
      shortcut: 'R',
      keywords: ['microphone', 'capture', 'mic', 'finish'],
      enabled: () => capturing,
      run: () => void stopRecording()
    }
  ]);

  function handleWindowKeydown(event: KeyboardEvent) {
    if (showLanding) return;
    // App-wide UI scale on Ctrl/Cmd +/-/0, ahead of the record shortcut and
    // regardless of recording support. Preventing default also stops the
    // browser's own page zoom from firing.
    if (event.ctrlKey || event.metaKey) {
      if (event.key === '=' || event.key === '+') {
        event.preventDefault();
        nudgeUiScale(1);
        return;
      }
      if (event.key === '-' || event.key === '_') {
        event.preventDefault();
        nudgeUiScale(-1);
        return;
      }
      if (event.key === '0') {
        event.preventDefault();
        resetUiScale();
        return;
      }
    }
    if (!recordingSupported) return;
    if (event.key.toLowerCase() !== 'r' || event.metaKey || event.ctrlKey || event.altKey) return;
    const target = event.target;
    if (
      target instanceof HTMLInputElement ||
      target instanceof HTMLSelectElement ||
      target instanceof HTMLTextAreaElement
    ) {
      return;
    }
    event.preventDefault();
    toggleRecording();
  }

  // Recordings offered to the editor's switcher and rail; takes inside the
  // removal undo window drop out until the window closes or the delete is
  // undone.
  const recordingChoices = $derived(
    (project?.recordings ?? [])
      .filter((entry) => !pendingRemovals.includes(entry.mediaId))
      .map((entry) => ({
        mediaId: entry.mediaId,
        name: entry.name,
        duration: entry.duration,
        sampleRate: entry.sampleRate,
        audioId: entry.audioId,
        hasAnnotation: entry.hasAnnotation
      }))
  );

  // Test hook: the batch-equals-GUI invariant check reads the live client and
  // the open recording's audio id to run a direct engine query at the same
  // coordinates the readout used.
  $effect(() => {
    (globalThis as unknown as { __phonia?: unknown }).__phonia = {
      client,
      audioId: audio?.id ?? null
    };
  });
</script>

<svelte:window onkeydown={handleWindowKeydown} />

{#if showLanding}
  <LandingPage onEnterApp={enterApp} />
{:else}
<ModeRail
  active={mode}
  analyzeEnabled={audio !== null}
  plotsEnabled={audio !== null}
  onNavigate={handleModeNavigate}
/>

<div class="app-content">
  {#if route === 'home'}
    <HomeView
      {projects}
      {busy}
      {busyLabel}
      {theme}
      onImportFiles={importToNewProject}
      onNewProject={createEmptyProject}
      onOpenSample={openSampleProject}
      onOpenProjectFile={openProjectFile}
      onOpenProject={requestOpen}
      onRenameProject={renameProject}
      onDeleteProject={deleteProject}
      onDuplicateProject={duplicateProject}
      onThemeChange={handleThemeChange}
      onOpenShortcuts={() => (shortcutEditorOpen = true)}
      onStartRecording={recordingSupported ? startRecording : undefined}
      recording={capturing}
      {homeIndex}
      onTogglePin={togglePin}
      onCreateGroupFrom={createGroupFrom}
      onRenameGroup={renameHomeGroup}
      onDissolveGroup={dissolveHomeGroup}
      onToggleGroupCollapse={toggleGroupCollapse}
      onMoveToGroup={moveToGroup}
      onExportStored={exportStoredProject}
      onBatchDelete={batchDeleteProjects}
    />
  {:else if route === 'project' && project}
    <ProjectView
      {client}
      projectName={project.name}
      recordings={project.recordings}
      {theme}
      {busy}
      {busyLabel}
      {dirty}
      savedAt={project.savedAt}
      onOpenRecording={openRecording}
      onImportFiles={addFilesToProject}
      onBack={backToHome}
      onSave={saveProject}
      onThemeChange={handleThemeChange}
      onStartRecording={recordingSupported ? startRecording : undefined}
      recording={capturing}
      groups={project.groups}
      collapsed={collapsedOf(project)}
      onToggleCollapse={toggleCollapse}
      onCreateGroup={createGroup}
      onRenameProject={(name) => {
        if (project) void renameProject(project.id, name);
      }}
      onRenameGroup={renameGroup}
      onDissolveGroup={dissolveGroup}
      onMoveNode={moveNode}
      onRenameRecording={renameRecording}
      onDeleteRecording={deleteRecording}
      onExportProject={exportProject}
      onExportRecording={exportRecordingAudio}
      onUpdateRecordingMetadata={updateRecordingMetadata}
      onUpdateProjectMetadata={updateProjectMetadata}
      projectDescription={project.description}
      projectAuthors={project.authors}
      projectTags={project.tags}
      {pendingRemovals}
    />
  {:else if route === 'editor'}
    <EditorView
      {client}
      {audio}
      {annotationId}
      {tierRefreshToken}
      {cursorTime}
      {isPlaying}
      {theme}
      {palette}
      {paletteInvert}
      {customRamps}
      onFile={editorImportFile}
      onPlayToggle={handlePlayToggle}
      onThemeChange={handleThemeChange}
      onPaletteChange={handlePaletteChange}
      onPaletteInvertToggle={handlePaletteInvertToggle}
      onSaveRamp={saveRamp}
      onDeleteRamp={deleteRamp}
      onCursorChange={handleCursorChange}
      onSelectionChange={(span) => (editorSelection = span)}
      onAnnotationChange={(id) => {
        annotationId = id;
        if (recording) {
          recording.annotationId = id;
          recording.hasAnnotation = id !== null;
        }
      }}
      onExit={backToProject}
      projectName={project?.name}
      projectGroupName={projectGroupName}
      recordings={recordingChoices}
      groups={project?.groups}
      currentRecordingId={recording?.mediaId ?? null}
      onSwitchRecording={switchRecording}
      onRenameRecording={renameRecording}
      onDeleteRecording={deleteRecording}
      onRenameProject={(name) => {
        if (project) void renameProject(project.id, name);
      }}
      onPlaySelection={(t0, t1) => {
        cursorTime = t0;
        void playback?.playRange(t0, t1);
      }}
      onPlayFilteredSelection={async (t0, t1, f0, f1) => {
        if (!client || !audio || !playback) return;
        try {
          const samples = await client.bandFilteredSpan(audio.id, t0, t1, f0, f1);
          await playback.playBuffer(samples, audio.sampleRate);
        } catch (caught) {
          report(caught);
        }
      }}
      onExportAudio={exportEditorAudio}
      onExtractSelection={extractSelection}
      onExtractSelectionWithTiers={extractSelectionWithTiers}
      onFilterSelection={filterSelection}
      onNotchSelection={notchSelection}
      onPreemphasisSelection={preemphasisSelection}
      onDeemphasisSelection={deemphasisSelection}
      onSubtractMeanSelection={subtractMeanSelection}
      onZeroSelection={zeroSelection}
      onReverseSelection={reverseSelection}
      onScaleSelection={scaleSelection}
      onScalePeakSelection={scalePeakSelection}
      onResample={resampleRecording}
      onNearestZero={nearestZero}
      onHarmonicityTrack={harmonicityTrack}
      onCppTrack={cppTrack}
      onLpcEnvelope={lpcEnvelope}
      onExtractIntervals={extractIntervals}
      onExtractChannels={extractChannels}
      onConvertToMono={convertToMono}
      onConcatenate={concatenateProject}
      onCombineToStereo={combineToStereo}
      onStartRecording={recordingSupported ? startRecording : undefined}
      recording={capturing}
      recordingElapsedSeconds={recordElapsed}
      {loopEnabled}
      onLoopToggle={handleLoopToggle}
      {dirty}
    />
  {/if}

  <!-- Plots stays mounted while a recording is open so switching to Analyse and
       back does not discard the figure being composed. It only shows on the
       Plots route; hidden elsewhere, its state simply persists. -->
  {#if audio}
    <div class="plots-host" class:hidden={route !== 'plots'} aria-hidden={route !== 'plots'}>
      <PlotsView
        {client}
        {audio}
        {annotationId}
        {theme}
        selection={editorSelection}
        projectName={project?.name}
        onRenameProject={(name) => {
          if (project) void renameProject(project.id, name);
        }}
        onExit={backToProject}
      />
    </div>
  {/if}
</div>

{#if removalUndo}
  <RemovalUndoBanner
    name={removalUndo.name}
    stale={removalUndo.stale}
    onUndo={undoRemoval}
  />
{/if}

{#if capturing}
  <RecordingStrip
    devices={recordDevices}
    selectedDeviceId={recordDeviceId}
    level={recordLevel}
    clipLatched={recordClipLatched}
    elapsedSeconds={recordElapsed}
    sampleRate={recordSampleRate}
    destinationName={project?.name}
    destinationIsNew={recordDestinationNew}
    onRenameDestination={(name) => {
      if (project) void renameProject(project.id, name);
    }}
    onSelectDevice={selectRecordDevice}
    onStop={stopRecording}
    onCancel={cancelRecording}
  />
{/if}

{#if recovery}
  <div class="modal-backdrop" data-testid="recovery-prompt">
    <div class="modal" role="dialog" aria-modal="true" aria-label="Recover unsaved work">
      <h2>Recover unsaved work?</h2>
      <p>
        “{recovery.name}” has autosaved changes from a session that did not finish. Recover them, or
        discard and open the last saved version.
      </p>
      <div class="modal-actions">
        <button type="button" class="secondary" data-testid="recovery-discard" onclick={recoverDiscard}>
          Discard
        </button>
        <button type="button" class="primary" data-testid="recovery-accept" onclick={recoverAccept}>
          Recover
        </button>
      </div>
    </div>
  </div>
{/if}

{#if error}
  <div class="error" role="alert" data-testid="error">{error}</div>
{/if}

{#if notice}
  <div class="notice" role="status" data-testid="notice">
    <span>{notice}</span>
    <button type="button" class="notice-close" aria-label="Dismiss" onclick={() => (notice = '')}>×</button>
  </div>
{/if}

<CommandPalette registry={commands} />

{#if keyBindings.promptDue}
  <FirstRunKeyModePrompt
    onChoose={(mode) => keyBindings.answerPrompt(mode)}
    onDismiss={() => keyBindings.dismissPrompt()}
  />
{/if}

{#if shortcutEditorOpen}
  <ShortcutEditor onClose={() => (shortcutEditorOpen = false)} />
{/if}
{/if}

<style>
  .app-content {
    /* Keep this offset in sync with the ModeRail width. */
    margin-left: 4.75rem;
    min-height: 100dvh;
  }

  @media (max-width: 720px) {
    .app-content {
      margin-left: 0;
      min-height: calc(100dvh - var(--mobile-rail-height) - var(--safe-bottom));
      padding-bottom: calc(var(--mobile-rail-height) + var(--safe-bottom));
    }

    .error,
    .notice {
      bottom: calc(var(--mobile-rail-height) + var(--safe-bottom) + 0.75rem);
    }

    .error {
      right: calc(var(--safe-right) + 0.5rem);
      left: calc(var(--safe-left) + 0.5rem);
      max-width: none;
    }

    .notice {
      width: calc(100vw - var(--safe-left) - var(--safe-right) - 1rem);
      max-width: none;
    }

    .modal-backdrop {
      padding: calc(var(--safe-top) + 0.5rem) calc(var(--safe-right) + 0.5rem)
        calc(var(--safe-bottom) + var(--mobile-rail-height) + 0.5rem)
        calc(var(--safe-left) + 0.5rem);
    }

    .modal {
      width: min(26rem, 100%);
    }
  }

  /* Plots is a fixed-position overlay kept mounted to preserve its figure;
     display:none takes it (and its fixed children) fully out of view. */
  .plots-host.hidden {
    display: none;
  }

  .error {
    position: fixed;
    right: 1rem;
    bottom: 1rem;
    max-width: min(30rem, calc(100vw - 2rem));
    padding: 0.75rem 0.9rem;
    border: 1px solid color-mix(in oklab, var(--warn), transparent 30%);
    border-radius: var(--radius-md);
    background: var(--panel);
    color: var(--warn);
    box-shadow: var(--shadow-lg);
  }

  .notice {
    position: fixed;
    left: 50%;
    bottom: 1rem;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 0.75rem;
    max-width: min(38rem, calc(100vw - 2rem));
    padding: 0.6rem 0.7rem 0.6rem 0.95rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-lg);
    font-size: 0.85rem;
    z-index: 20;
  }

  .notice-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    background: transparent;
    color: var(--muted);
    font-size: 1.05rem;
    line-height: 1;
    padding: 0 0.2rem;
    cursor: pointer;
  }

  .notice-close:hover {
    color: var(--text);
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in oklab, #000 52%, transparent);
    backdrop-filter: blur(2px);
    z-index: 20;
  }

  .modal {
    max-width: 26rem;
    padding: 1.25rem 1.4rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-xl);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-lg);
  }

  .modal h2 {
    margin: 0 0 0.5rem;
    font-size: 1.05rem;
  }

  .modal p {
    margin: 0 0 1rem;
    color: var(--muted);
    font-size: 0.9rem;
    line-height: 1.45;
  }

  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
  }

  .modal-actions button {
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md);
    padding: 0.45rem 0.95rem;
    background: var(--panel-soft);
    color: var(--text);
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .modal-actions button:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .modal-actions .primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent);
  }

  .modal-actions .primary:hover {
    background: var(--accent-strong);
    border-color: var(--accent-strong);
  }
</style>
