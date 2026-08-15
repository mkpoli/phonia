<script lang="ts">
  import IconArrowLeft from '~icons/lucide/arrow-left';
  import IconFolderOpen from '~icons/lucide/folder-open';
  import IconSun from '~icons/lucide/sun';
  import IconMoon from '~icons/lucide/moon';
  import AudioExportDialog from './AudioExportDialog.svelte';
  import ExportDialog from './ExportDialog.svelte';
  import RecordingSwitcher from './RecordingSwitcher.svelte';
  import InlineRename from './InlineRename.svelte';
  import RecordingsRail from './RecordingsRail.svelte';
  import InspectorPanel from './InspectorPanel.svelte';
  import LevelMeter from './LevelMeter.svelte';
  import OverviewStrip from './OverviewStrip.svelte';
  import PalettePicker from './PalettePicker.svelte';
  import ReadoutBar from './ReadoutBar.svelte';
  import SpectrogramPane from './SpectrogramPane.svelte';
  import TierPane from './TierPane.svelte';
  import TimeRuler from './TimeRuler.svelte';
  import Transport from './Transport.svelte';
  import GradientEditor from './GradientEditor.svelte';
  import VoiceReportCard from './VoiceReportCard.svelte';
  import MeasurementTable from './MeasurementTable.svelte';
  import SpectrumCard from './SpectrumCard.svelte';
  import FormantSweepCard from './FormantSweepCard.svelte';
  import VowelChartCard from './VowelChartCard.svelte';
  import WaveformPane from './WaveformPane.svelte';
  import { registerCommands } from './commands.svelte';
  import { chordFromEvent, getKeyBindings } from './keybindings.svelte';
  import { isGroup } from './library';
  import { emptyOverlayTracks, sampleTracks, type OverlayTracks } from './tracks';
  import { BUILTIN_PALETTES, newRampTemplate, type CustomRamp, type PaletteSelection } from './palette';
  import {
    clampViewport,
    defaultOverlayParams,
    defaultViewport,
    formatSampleRate,
    type AudioInfo,
    type CoreClientLike,
    type OverlayParams,
    type LibraryNode,
    type OverlayStats,
    type AudioExportOptions,
    type AudioExportRequest,
    type Selection,
    type SelectionReadout,
    type ViewportState,
    type VoiceReportData,
    type AudioId,
    type SpectrumSliceData
  } from './types';

  interface RecordingChoice {
    mediaId: number;
    name: string;
    duration: number;
    sampleRate: number;
    audioId: AudioId | null;
    hasAnnotation: boolean;
  }

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    annotationId: bigint | null;
    /** Bumped by the host when the active annotation is mutated without an id
     *  change, forcing the tier pane to re-fetch. */
    tierRefreshToken?: number;
    cursorTime: number;
    isPlaying: boolean;
    theme: 'light' | 'dark';
    palette: PaletteSelection;
    /** Whether the active ramp renders reversed (floor in the ceiling color). */
    paletteInvert: boolean;
    customRamps: CustomRamp[];
    onFile: (file: File) => void;
    onPlayToggle: () => void;
    onThemeChange: (theme: 'light' | 'dark') => void;
    onPaletteChange: (palette: PaletteSelection) => void;
    /** Flips the palette-inversion flag. */
    onPaletteInvertToggle: () => void;
    /** Persists a created or edited custom ramp. */
    onSaveRamp: (ramp: CustomRamp) => void;
    /** Removes a custom ramp by id. */
    onDeleteRamp: (id: string) => void;
    onCursorChange?: (time: number) => void;
    onAnnotationChange?: (id: bigint | null) => void;
    /** Reports the active time selection (or null) so a sibling view — Plots —
     *  can scope a figure to the same span the user picked here. */
    onSelectionChange?: (span: { t0: number; t1: number } | null) => void;
    onExit?: () => void;
    projectName?: string;
    /** Name of the home group the project belongs to; leads the breadcrumb. */
    projectGroupName?: string;
    recordings?: RecordingChoice[];
    /** The library tree, so the recording switcher mirrors the corpus's grouping. */
    groups?: LibraryNode[];
    currentRecordingId?: number | null;
    onSwitchRecording?: (mediaId: number) => void;
    onRenameRecording?: (mediaId: number, name: string) => void;
    /** Renames the open project from the breadcrumb; absent leaves it read-only. */
    onRenameProject?: (name: string) => void;
    onPlaySelection?: (t0: number, t1: number) => void;
    /** Plays a box selection through the engine's band filter; resolves when the
     *  rendered preview finishes sounding. */
    onPlayFilteredSelection?: (t0: number, t1: number, f0: number, f1: number) => Promise<void> | void;
    /** Encodes and downloads the whole recording or the selection as WAV;
     *  absent hides the audio-export affordances. */
    onExportAudio?: (request: AudioExportRequest) => void;
    /** Copies the time selection into a new recording in the library and opens
     *  it; absent hides the extract affordances. */
    onExtractSelection?: (t0: number, t1: number) => void;
    /** Extracts the selection to a new recording, carrying its tiers across
     *  cropped and time-shifted; absent hides the affordance. */
    onExtractSelectionWithTiers?: (t0: number, t1: number) => void;
    /** Passes a box selection through the engine's Hann band filter into a new
     *  library recording; absent hides the filter affordance. */
    onFilterSelection?: (t0: number, t1: number, f0: number, f1: number) => void;
    /** Attenuates a box selection's band (notch) into a new library recording;
     *  absent hides the notch affordance. */
    onNotchSelection?: (t0: number, t1: number, f0: number, f1: number) => void;
    /** Pre-emphasizes a span into a new library recording; absent hides the
     *  pre-emphasis affordance. */
    onPreemphasisSelection?: (t0: number, t1: number) => void;
    /** Removes a span's DC offset into a new library recording; absent hides the
     *  affordance. */
    onSubtractMeanSelection?: (t0: number, t1: number) => void;
    /** Reverses a span in time into a new library recording; absent hides the
     *  reverse affordance. */
    onReverseSelection?: (t0: number, t1: number) => void;
    /** Scales a span to a target average intensity into a new library recording;
     *  absent hides the scale affordance. */
    onScaleSelection?: (t0: number, t1: number) => void;
    /** Normalizes a span's peak into a new library recording; absent hides the
     *  scale-peak affordance. */
    onScalePeakSelection?: (t0: number, t1: number) => void;
    /** Resamples the whole recording to 16 kHz into a new library recording;
     *  absent hides the resample affordance. */
    onResample16k?: () => void;
    /** Resolves the zero crossing nearest a time, for snapping selection edges;
     *  absent hides the snap affordance. */
    onNearestZero?: (t: number) => Promise<number>;
    /** Resolves the per-frame harmonicity (HNR) track over a span, for the
     *  contour export; absent hides it. */
    onHarmonicityTrack?: (
      t0: number,
      t1: number
    ) => Promise<{ times: Float64Array; hnr: Float64Array }>;
    /** Resolves the per-frame CPP track over a span, for the contour export;
     *  absent hides it. */
    onCppTrack?: (t0: number, t1: number) => Promise<{ times: Float64Array; cpp: Float64Array }>;
    /** Saves each labelled interval of a tier as its own library recording;
     *  absent hides the affordance. */
    onExtractIntervals?: (spans: { t0: number; t1: number; label: string }[]) => Promise<void>;
    /** Writes each channel of a multichannel recording as its own mono
     *  recording; absent, or on a mono take, hides the affordance. */
    onExtractChannels?: () => Promise<void>;
    /** Mixes a multichannel recording down to one channel as a new recording;
     *  absent, or on a mono take, hides the affordance. */
    onConvertToMono?: () => Promise<void>;
    /** Resolves the LPC-smoothed spectral envelope over a span, for the
     *  spectrum card's formant-tracing overlay; absent hides it. */
    onLpcEnvelope?: (t0: number, t1: number) => Promise<SpectrumSliceData>;
    /** Joins the project's recordings into one new recording; absent hides it. */
    onConcatenate?: () => Promise<void>;
    /** Combines the current recording with another into a new stereo recording;
     *  absent, or with fewer than two recordings, hides it. */
    onCombineToStereo?: () => Promise<void>;
    /** Starts a microphone recording; absent when the browser cannot capture. */
    onStartRecording?: () => void;
    /** Whether a take is currently being captured. */
    recording?: boolean;
    /** Seconds captured so far; shown inside the REC pill while `recording`. */
    recordingElapsedSeconds?: number;
    /** Whether the transport repeats the active playback range; absent hides
     *  the loop control (the host has no loop-capable playback engine). */
    loopEnabled?: boolean;
    onLoopToggle?: () => void;
    /** True while an edit awaits autosave; drives the header's saved state. */
    dirty?: boolean;
  }

  let {
    client,
    audio,
    annotationId,
    tierRefreshToken = 0,
    cursorTime,
    isPlaying,
    theme,
    palette,
    paletteInvert,
    customRamps,
    onFile,
    onPlayToggle,
    onThemeChange,
    onPaletteChange,
    onPaletteInvertToggle,
    onSaveRamp,
    onDeleteRamp,
    onCursorChange,
    onAnnotationChange,
    onSelectionChange,
    onExit,
    projectName,
    projectGroupName,
    recordings,
    groups,
    currentRecordingId,
    onSwitchRecording,
    onRenameRecording,
    onRenameProject,
    onPlaySelection,
    onPlayFilteredSelection,
    onExportAudio,
    onExtractSelection,
    onExtractSelectionWithTiers,
    onFilterSelection,
    onNotchSelection,
    onPreemphasisSelection,
    onSubtractMeanSelection,
    onReverseSelection,
    onScaleSelection,
    onScalePeakSelection,
    onResample16k,
    onNearestZero,
    onHarmonicityTrack,
    onCppTrack,
    onExtractIntervals,
    onExtractChannels,
    onConvertToMono,
    onLpcEnvelope,
    onConcatenate,
    onCombineToStereo,
    onStartRecording,
    recording = false,
    recordingElapsedSeconds = 0,
    loopEnabled = false,
    onLoopToggle,
    dirty
  }: Props = $props();

  // Overlay tracks bubbled up from the spectrogram pane, sampled at the
  // playhead for the layer cards' live values.
  let overlayTracks = $state<OverlayTracks>(emptyOverlayTracks());
  const cursorSample = $derived(sampleTracks(overlayTracks, cursorTime));

  const channelsLabel = $derived(
    audio ? (audio.channels === 1 ? 'mono' : audio.channels === 2 ? 'stereo' : `${audio.channels} ch`) : ''
  );

  let fileInput = $state<HTMLInputElement | null>(null);

  function takeFileList(files: FileList | null) {
    const file = files?.item(0);
    if (file) onFile(file);
  }

  function handleBackKeydown(event: KeyboardEvent) {
    // The crumb carries a rename field, which claims its own keys.
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onExit?.();
    }
  }

  // Innermost library group holding the current recording, for the breadcrumb.
  const recordingGroupName = $derived.by<string | null>(() => {
    if (!groups || currentRecordingId == null) return null;
    let found: string | null = null;
    const walk = (nodes: LibraryNode[], trail: string[]): boolean => {
      for (const node of nodes) {
        if (isGroup(node)) {
          if (walk(node.Group.children, [...trail, node.Group.name])) return true;
        } else if (node.Media === currentRecordingId) {
          found = trail.at(-1) ?? null;
          return true;
        }
      }
      return false;
    };
    walk(groups, []);
    return found;
  });

  function skipToStart() {
    onCursorChange?.(0);
  }

  function skipToEnd() {
    if (audio) onCursorChange?.(audio.duration);
  }

  let audioExportOpen = $state(false);

  // Gradient editor. While a draft is open the spectrogram previews it live, so
  // the pane shows the draft ramp (recolor path) rather than the committed
  // palette. Cancelling drops the draft; saving commits and selects it.
  let editingRamp = $state<CustomRamp | null>(null);
  let editingExisting = $state(false);
  const activePalette = $derived<PaletteSelection>(
    editingRamp ? { kind: 'custom', ramp: editingRamp } : palette
  );

  function openNewRamp() {
    editingExisting = false;
    editingRamp = newRampTemplate();
  }

  function openEditRamp(ramp: CustomRamp) {
    editingExisting = true;
    editingRamp = { ...ramp, stops: ramp.stops.map((s) => ({ ...s })) };
  }

  function saveRamp(ramp: CustomRamp) {
    onSaveRamp(ramp);
    onPaletteChange({ kind: 'custom', ramp });
    editingRamp = null;
  }

  function deleteRamp(id: string) {
    onDeleteRamp(id);
    editingRamp = null;
  }

  // Resolve the dialog's choice into concrete signal coordinates: the whole file
  // from zero, or the live selection's span (band-limited to its box only when
  // the user asked and the selection carries frequency bounds).
  function resolveAudioExport(options: AudioExportOptions): AudioExportRequest | null {
    if (!audio) return null;
    if (options.scope === 'selection' && selection) {
      const box = selection.mode === 'box';
      return {
        scope: 'selection',
        t0: selection.t0,
        t1: selection.t1,
        f0: box ? selection.f0 : 0,
        f1: box ? selection.f1 : 0,
        bits: options.bits,
        filtered: options.filtered && box
      };
    }
    return {
      scope: 'whole',
      t0: 0,
      t1: audio.duration,
      f0: 0,
      f1: 0,
      bits: options.bits,
      filtered: false
    };
  }

  let switcher = $state<{ show: () => void } | null>(null);

  // The ceiling and amplitude a reset chip returns to; the chips appear the
  // instant the live value departs these.
  const DEFAULT_CEILING = defaultViewport().f1;
  const DEFAULT_AMP = defaultViewport().ampScale;

  let waveformVisible = $state(true);
  let filteredPlaying = $state(false);

  let viewport = $state<ViewportState>(defaultViewport());
  let overlayParams = $state<OverlayParams>(defaultOverlayParams());
  let overlayStats = $state<OverlayStats>({ pitchMaxHz: 0, formantMaxHz: 0 });
  let inspectorOpen = $state(true);
  let exportOpen = $state(false);
  let railOpen = $state(true);
  // A brief self-clearing status line for background actions (a contour copy).
  let toast = $state<string | null>(null);
  let toastToken = 0;
  function flashToast(message: string) {
    toast = message;
    const token = (toastToken += 1);
    setTimeout(() => {
      if (toastToken === token) toast = null;
    }, 2200);
  }

  let selection = $state<Selection | null>(null);
  let readout = $state<SelectionReadout | null>(null);
  let formantMeans = $state<number[] | null>(null);
  let formantBandwidths = $state<number[] | null>(null);
  let intensityStats = $state<{
    maxDb: number;
    maxTime: number;
    minDb: number;
    minTime: number;
  } | null>(null);
  let harmonicityStats = $state<{
    maxDb: number;
    maxTime: number;
    minDb: number;
    minTime: number;
  } | null>(null);
  let cppStats = $state<{
    maxDb: number;
    maxTime: number;
    minDb: number;
    minTime: number;
  } | null>(null);
  let formantStats = $state<{ slot: number; minHz: number; maxHz: number }[] | null>(null);

  // Glottal pulses for the waveform overlay, fetched once per (recording, pitch
  // range) while the overlay is on. Whole-signal, like the other contours.
  let pulseTimes = $state<Float64Array | null>(null);
  $effect(() => {
    const id = audio?.id;
    const show = overlayParams.pulses.show;
    const floor = overlayParams.pitch.floorHz;
    const ceiling = overlayParams.pitch.ceilingHz;
    if (!client || id === undefined || !show) {
      pulseTimes = null;
      return;
    }
    let cancelled = false;
    client
      .pulseTimes(id, floor, ceiling)
      .then((times) => {
        if (!cancelled) pulseTimes = times;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });
  let voiceReportOpen = $state(false);
  let voiceReport = $state<VoiceReportData | null>(null);
  let voiceReportLoading = $state(false);
  let measureOpen = $state(false);
  let vowelChartOpen = $state(false);
  let spectrumOpen = $state(false);
  let spectrumMode = $state<'spectrum' | 'ltas' | 'cepstrum'>('spectrum');
  let spectrumSpan = $state<{ t0: number; t1: number }>({ t0: 0, t1: 0 });

  function openSpectrum() {
    if (!selection) return;
    spectrumSpan = { t0: selection.t0, t1: selection.t1 };
    spectrumMode = 'spectrum';
    spectrumOpen = true;
  }

  function openLtas() {
    if (!selection) return;
    spectrumSpan = { t0: selection.t0, t1: selection.t1 };
    spectrumMode = 'ltas';
    spectrumOpen = true;
  }

  function openCepstrum() {
    if (!selection) return;
    spectrumSpan = { t0: selection.t0, t1: selection.t1 };
    spectrumMode = 'cepstrum';
    spectrumOpen = true;
  }

  let sweepOpen = $state(false);
  let sweepSpan = $state<{ t0: number; t1: number }>({ t0: 0, t1: 0 });

  function openFormantSweep() {
    if (!selection) return;
    sweepSpan = { t0: selection.t0, t1: selection.t1 };
    sweepOpen = true;
  }

  // Bumped after a direct annotation mutation so the tier pane refetches.
  let tierRevision = $state(0);

  // First-pass segmentation: threshold the intensity contour into sounding and
  // silent runs (Praat defaults: −25 dB, 0.1 s minima) and lay them onto a new
  // interval tier through the ordinary journaled commands.
  async function annotateBySilences() {
    if (!client || !audio || annotationId === null) return;
    try {
      const segments = await client.silenceIntervals(audio.id, -25, 0.1, 0.1);
      const tierId = await client.addIntervalTier(annotationId, 'silences');
      for (let i = 1; i < segments.length; i += 1) {
        await client.insertBoundary(annotationId, tierId, segments[i].t0);
      }
      const intervals = await client.intervalsInRange(annotationId, tierId, -1, 1e12);
      for (let i = 0; i < intervals.length && i < segments.length; i += 1) {
        await client.setIntervalLabel(
          annotationId,
          tierId,
          intervals[i].id,
          segments[i].sounding ? 'sounding' : 'silent'
        );
      }
      tierRevision += 1;
    } catch (caught) {
      console.error('annotate by silences failed', caught);
    }
  }

  async function annotateByVoicing() {
    if (!client || !audio || annotationId === null) return;
    try {
      const segments = await client.voicingIntervals(audio.id, 75, 600, 0.02, 0.02);
      const tierId = await client.addIntervalTier(annotationId, 'voicing');
      for (let i = 1; i < segments.length; i += 1) {
        await client.insertBoundary(annotationId, tierId, segments[i].t0);
      }
      const intervals = await client.intervalsInRange(annotationId, tierId, -1, 1e12);
      for (let i = 0; i < intervals.length && i < segments.length; i += 1) {
        await client.setIntervalLabel(
          annotationId,
          tierId,
          intervals[i].id,
          segments[i].voiced ? 'V' : 'U'
        );
      }
      tierRevision += 1;
    } catch (caught) {
      console.error('annotate by voicing failed', caught);
    }
  }

  $effect(() => {
    const duration = audio?.duration ?? 1;
    viewport = defaultViewport(duration);
    // A new recording invalidates any selection anchored in the old signal.
    selection = null;
  });

  // The recordings rail steps out of the way once measurement starts, per the
  // owner's standing question about screen space while deep in a session: each
  // fresh selection (not a held one) collapses it, but a manual reopen during
  // that same selection sticks — this only fires again on the next fresh one.
  let hadSelection = false;
  $effect(() => {
    const hasSelection = selection !== null;
    if (hasSelection && !hadSelection) railOpen = false;
    hadSelection = hasSelection;
  });

  // Selection readout: every value is an engine query over the box, so the bar
  // shows exactly what a script reading the same API returns.
  $effect(() => {
    const sel = selection;
    const id = audio?.id;
    const pitch = overlayParams.pitch;
    const intensityFloor = overlayParams.intensity.floorHz;
    if (!client || id === undefined || !sel) {
      readout = null;
      return;
    }
    let cancelled = false;
    client
      .selectionReadout(
        id,
        sel.t0,
        sel.t1,
        sel.f0,
        sel.f1,
        pitch.floorHz,
        pitch.ceilingHz,
        intensityFloor
      )
      .then((result) => {
        if (!cancelled) readout = result;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Provisional tracked-formant means, fetched only while the tracking toggle is
  // on (the raw Burg display is the default until T2.6 closes).
  $effect(() => {
    const sel = selection;
    const id = audio?.id;
    const formant = overlayParams.formant;
    if (!client || id === undefined || !sel || !formant.smoothed) {
      formantMeans = null;
      formantBandwidths = null;
      return;
    }
    let cancelled = false;
    client
      .formantSpanMeans(id, formant.ceilingHz, formant.maxFormants, true, sel.t0, sel.t1)
      .then((means) => {
        if (!cancelled) formantMeans = Array.from(means);
      })
      .catch(() => {});
    client
      .formantSpanBandwidthMeans(id, formant.ceilingHz, formant.maxFormants, true, sel.t0, sel.t1)
      .then((bandwidths) => {
        if (!cancelled) formantBandwidths = Array.from(bandwidths);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Intensity extrema over the selection — the loudest and quietest frames and
  // when they fall — read from the same track the overlay draws.
  $effect(() => {
    const sel = selection;
    const id = audio?.id;
    const floor = overlayParams.intensity.floorHz;
    if (!client || id === undefined || !sel) {
      intensityStats = null;
      return;
    }
    let cancelled = false;
    client
      .intensityTrack(id, floor)
      .then((track) => {
        if (cancelled) return;
        let maxDb = -Infinity;
        let minDb = Infinity;
        let maxTime = sel.t0;
        let minTime = sel.t0;
        let found = false;
        for (let i = 0; i < track.times.length; i += 1) {
          const time = track.times[i];
          const level = track.db[i];
          if (time < sel.t0 || time > sel.t1 || !Number.isFinite(level)) continue;
          found = true;
          if (level > maxDb) {
            maxDb = level;
            maxTime = time;
          }
          if (level < minDb) {
            minDb = level;
            minTime = time;
          }
        }
        intensityStats = found ? { maxDb, maxTime, minDb, minTime } : null;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Harmonicity extrema over the selection — the most and least periodic frames
  // and when they fall. Computed only while the harmonicity layer is on, since
  // HNR is the heavier track a phonetician opts into.
  $effect(() => {
    const sel = selection;
    const id = audio?.id;
    if (!client || id === undefined || !sel || !overlayParams.harmonicity.show) {
      harmonicityStats = null;
      return;
    }
    let cancelled = false;
    client
      .harmonicityTrack(id, sel.t0, sel.t1)
      .then((track) => {
        if (cancelled) return;
        let maxDb = -Infinity;
        let minDb = Infinity;
        let maxTime = sel.t0;
        let minTime = sel.t0;
        let found = false;
        for (let i = 0; i < track.times.length; i += 1) {
          const level = track.hnr[i];
          if (!Number.isFinite(level)) continue;
          found = true;
          if (level > maxDb) {
            maxDb = level;
            maxTime = track.times[i];
          }
          if (level < minDb) {
            minDb = level;
            minTime = track.times[i];
          }
        }
        harmonicityStats = found ? { maxDb, maxTime, minDb, minTime } : null;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // CPP extrema over the selection — the clearest and breathiest frames and when
  // they fall. Computed only while the CPP layer is on, like the HNR extrema.
  $effect(() => {
    const sel = selection;
    const id = audio?.id;
    if (!client || id === undefined || !sel || !overlayParams.cpp.show) {
      cppStats = null;
      return;
    }
    let cancelled = false;
    client
      .cppTrack(id, sel.t0, sel.t1)
      .then((track) => {
        if (cancelled) return;
        let maxDb = -Infinity;
        let minDb = Infinity;
        let maxTime = sel.t0;
        let minTime = sel.t0;
        let found = false;
        for (let i = 0; i < track.times.length; i += 1) {
          const level = track.cpp[i];
          if (!Number.isFinite(level)) continue;
          found = true;
          if (level > maxDb) {
            maxDb = level;
            maxTime = track.times[i];
          }
          if (level < minDb) {
            minDb = level;
            minTime = track.times[i];
          }
        }
        cppStats = found ? { maxDb, maxTime, minDb, minTime } : null;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Formant extrema per slot over the selection — the frequency range each
  // formant sweeps, the way a diphthong's F1/F2 excursion reads. Computed only
  // while the formant layer is on.
  $effect(() => {
    const sel = selection;
    const id = audio?.id;
    if (!client || id === undefined || !sel || !overlayParams.formant.show) {
      formantStats = null;
      return;
    }
    const maxF = overlayParams.formant.maxFormants;
    let cancelled = false;
    client
      .formantTrack(id, overlayParams.formant.ceilingHz, maxF, overlayParams.formant.smoothed)
      .then((track) => {
        if (cancelled) return;
        const pts = track.points;
        const slots = Array.from({ length: maxF }, () => ({
          minHz: Infinity,
          maxHz: -Infinity,
          found: false
        }));
        let i = 0;
        while (i < pts.length) {
          const time = pts[i];
          const freqs: number[] = [];
          while (i < pts.length && pts[i] === time) {
            freqs.push(pts[i + 1]);
            i += 3;
          }
          if (time < sel.t0 || time > sel.t1) continue;
          freqs.sort((a, b) => a - b);
          for (let k = 0; k < freqs.length && k < maxF; k += 1) {
            const hz = freqs[k];
            if (!Number.isFinite(hz)) continue;
            const slot = slots[k];
            slot.found = true;
            if (hz < slot.minHz) slot.minHz = hz;
            if (hz > slot.maxHz) slot.maxHz = hz;
          }
        }
        const result = slots.flatMap((slot, k) =>
          slot.found ? [{ slot: k + 1, minHz: slot.minHz, maxHz: slot.maxHz }] : []
        );
        formantStats = result.length ? result : null;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  // Mirror the selection's time span to the host, so Plots can offer to scope a
  // figure to it. Only t0/t1 travel — a box's frequency bounds don't apply.
  $effect(() => {
    onSelectionChange?.(selection ? { t0: selection.t0, t1: selection.t1 } : null);
  });

  function handleSelectionChange(next: Selection | null) {
    selection = next;
    if (!next) {
      readout = null;
      formantMeans = null;
      formantBandwidths = null;
    }
  }

  function clearSelection() {
    selection = null;
    readout = null;
    formantMeans = null;
    formantBandwidths = null;
    voiceReportOpen = false;
  }

  function zoomToSelection() {
    if (!selection) return;
    setViewport({
      ...viewport,
      t0: selection.t0,
      t1: selection.t1,
      f0: selection.mode === 'box' ? selection.f0 : viewport.f0,
      f1: selection.mode === 'box' ? selection.f1 : viewport.f1
    });
  }

  async function playSelection() {
    const sel = selection;
    if (!sel) return;
    // A box plays band-filtered through the engine; a time selection plays the
    // plain slice from the transport. The band case renders a preview and does
    // not move the file cursor.
    if (sel.mode === 'box' && onPlayFilteredSelection) {
      filteredPlaying = true;
      try {
        await onPlayFilteredSelection(sel.t0, sel.t1, sel.f0, sel.f1);
      } finally {
        filteredPlaying = false;
      }
      return;
    }
    onCursorChange?.(sel.t0);
    onPlaySelection?.(sel.t0, sel.t1);
  }

  // Copies the selection's time span into a new library recording (a box's
  // frequency bounds are ignored — extraction is a time operation).
  function extractCurrentSelection() {
    if (selection) onExtractSelection?.(selection.t0, selection.t1);
  }

  // Extracts the selection and carries its tier labels across, cropped.
  function extractWithTiersCurrentSelection() {
    if (selection) onExtractSelectionWithTiers?.(selection.t0, selection.t1);
  }

  // Pre-emphasizes the selection's time span into a new recording (a time
  // operation, so a box's frequency bounds are ignored).
  function preemphasizeCurrentSelection() {
    if (selection) onPreemphasisSelection?.(selection.t0, selection.t1);
  }

  // Removes the selection's DC offset into a new recording.
  function subtractMeanCurrentSelection() {
    if (selection) onSubtractMeanSelection?.(selection.t0, selection.t1);
  }

  // Filters a box selection's time span to its frequency band and stores the
  // result as a new recording. A time selection has no band, so it is ignored.
  function filterCurrentSelection() {
    if (selection?.mode === 'box') {
      onFilterSelection?.(selection.t0, selection.t1, selection.f0, selection.f1);
    }
  }

  // Attenuates a box selection's frequency band (notch) into a new recording.
  function notchCurrentSelection() {
    if (selection?.mode === 'box') {
      onNotchSelection?.(selection.t0, selection.t1, selection.f0, selection.f1);
    }
  }

  // Reverses the selection, or the whole file when nothing is selected, into a
  // new recording.
  function reverseCurrent() {
    if (!audio) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (t1 > t0) onReverseSelection?.(t0, t1);
  }

  // Scales the selection, or the whole file when nothing is selected, to the
  // target intensity into a new recording.
  function scaleCurrent() {
    if (!audio) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (t1 > t0) onScaleSelection?.(t0, t1);
  }

  // Normalizes the selection's, or the whole file's, peak into a new recording.
  function scalePeakCurrent() {
    if (!audio) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (t1 > t0) onScalePeakSelection?.(t0, t1);
  }

  // Snaps both selection edges to the nearest zero crossings. Setting the
  // selection re-runs the readout and re-draws the box.
  async function snapSelectionToZero() {
    const sel = selection;
    if (!sel || !onNearestZero) return;
    const [a, b] = await Promise.all([onNearestZero(sel.t0), onNearestZero(sel.t1)]);
    if (!Number.isFinite(a) || !Number.isFinite(b)) return;
    selection = { ...sel, t0: Math.min(a, b), t1: Math.max(a, b) };
  }

  // Copies the F0 contour over the selection (or the whole file) as CSV: one
  // `time_s,f0_hz` row per voiced frame, for a plot or stats package elsewhere.
  async function copyPitchContour() {
    if (!client || !audio) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (!(t1 > t0)) return;
    try {
      const track = await client.pitchTrackSpan(
        audio.id,
        overlayParams.pitch.floorHz,
        overlayParams.pitch.ceilingHz,
        t0,
        t1
      );
      const rows = ['time_s,f0_hz'];
      for (let i = 0; i < track.times.length; i += 1) {
        const f0 = track.f0[i];
        if (Number.isFinite(f0) && f0 > 0) rows.push(`${track.times[i].toFixed(4)},${f0.toFixed(2)}`);
      }
      await navigator.clipboard.writeText(rows.join('\n'));
      flashToast(`Pitch contour copied · ${rows.length - 1} points`);
    } catch {
      flashToast('Could not copy the pitch contour');
    }
  }

  // Copies the intensity contour over the selection (or the whole file) as CSV.
  // The engine reports intensity for the whole signal, so frames are kept by
  // time to the chosen span.
  async function copyIntensityContour() {
    if (!client || !audio) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (!(t1 > t0)) return;
    try {
      const track = await client.intensityTrack(audio.id, overlayParams.intensity.floorHz);
      const rows = ['time_s,intensity_db'];
      for (let i = 0; i < track.times.length; i += 1) {
        const time = track.times[i];
        const db = track.db[i];
        if (time >= t0 && time <= t1 && Number.isFinite(db)) {
          rows.push(`${time.toFixed(4)},${db.toFixed(2)}`);
        }
      }
      await navigator.clipboard.writeText(rows.join('\n'));
      flashToast(`Intensity contour copied · ${rows.length - 1} points`);
    } catch {
      flashToast('Could not copy the intensity contour');
    }
  }

  // Copies the formant tracks over the selection (or the whole file) as CSV, one
  // `time_s,f1_hz,f2_hz,…` row per frame. The flat `[time, freq, bandwidth]`
  // triples are frame-ordered, so a frame is the run that shares a time; its
  // candidates are sorted low to high and read as F1, F2, ….
  async function copyFormantContour() {
    if (!client || !audio) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (!(t1 > t0)) return;
    try {
      const maxF = overlayParams.formant.maxFormants;
      const track = await client.formantTrack(
        audio.id,
        overlayParams.formant.ceilingHz,
        maxF,
        overlayParams.formant.smoothed
      );
      const pts = track.points;
      const header = ['time_s'];
      for (let k = 1; k <= maxF; k += 1) header.push(`f${k}_hz`);
      const rows = [header.join(',')];
      let i = 0;
      while (i < pts.length) {
        const time = pts[i];
        const freqs: number[] = [];
        while (i < pts.length && pts[i] === time) {
          freqs.push(pts[i + 1]);
          i += 3;
        }
        if (time >= t0 && time <= t1) {
          freqs.sort((a, b) => a - b);
          const cols = [time.toFixed(4)];
          for (let k = 0; k < maxF; k += 1) cols.push(k < freqs.length ? freqs[k].toFixed(1) : '');
          rows.push(cols.join(','));
        }
      }
      await navigator.clipboard.writeText(rows.join('\n'));
      flashToast(`Formant contour copied · ${rows.length - 1} frames`);
    } catch {
      flashToast('Could not copy the formant contour');
    }
  }

  // Copies the averaged spectrum over the selection as CSV, one `freq_hz,db`
  // row per frequency bin.
  async function copySpectrum() {
    const sel = selection;
    if (!client || !audio || !sel || !(sel.t1 > sel.t0)) return;
    try {
      const data = await client.spectrumSlice(audio.id, sel.t0, sel.t1);
      const rows = ['freq_hz,db'];
      for (let i = 0; i < data.freqs.length; i += 1) {
        const freq = data.freqs[i];
        const level = data.db[i];
        if (Number.isFinite(freq) && Number.isFinite(level)) {
          rows.push(`${freq.toFixed(2)},${level.toFixed(2)}`);
        }
      }
      await navigator.clipboard.writeText(rows.join('\n'));
      flashToast(`Spectrum copied · ${rows.length - 1} bins`);
    } catch {
      flashToast('Could not copy the spectrum');
    }
  }

  // Copies the harmonics-to-noise ratio over the selection (or the whole file)
  // as CSV, one `time_s,hnr_db` row per voiced frame.
  async function copyHarmonicityContour() {
    if (!audio || !onHarmonicityTrack) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (!(t1 > t0)) return;
    try {
      const track = await onHarmonicityTrack(t0, t1);
      const rows = ['time_s,hnr_db'];
      for (let i = 0; i < track.times.length; i += 1) {
        const hnr = track.hnr[i];
        if (Number.isFinite(hnr)) rows.push(`${track.times[i].toFixed(4)},${hnr.toFixed(2)}`);
      }
      await navigator.clipboard.writeText(rows.join('\n'));
      flashToast(`Harmonicity copied · ${rows.length - 1} frames`);
    } catch {
      flashToast('Could not copy the harmonicity');
    }
  }

  async function copyCppContour() {
    if (!audio || !onCppTrack) return;
    const t0 = selection ? selection.t0 : 0;
    const t1 = selection ? selection.t1 : audio.duration;
    if (!(t1 > t0)) return;
    try {
      const track = await onCppTrack(t0, t1);
      const rows = ['time_s,cpp_db'];
      for (let i = 0; i < track.times.length; i += 1) {
        const cpp = track.cpp[i];
        if (Number.isFinite(cpp)) rows.push(`${track.times[i].toFixed(4)},${cpp.toFixed(2)}`);
      }
      await navigator.clipboard.writeText(rows.join('\n'));
      flashToast(`CPP copied · ${rows.length - 1} frames`);
    } catch {
      flashToast('Could not copy the CPP');
    }
  }

  // Saves a tier's labelled intervals as recordings, capped so an over-long
  // tier cannot spawn hundreds of takes in one click.
  const MAX_EXTRACTED_INTERVALS = 50;
  async function extractIntervalsFromTable(spans: { t0: number; t1: number; label: string }[]) {
    if (spans.length === 0 || !onExtractIntervals) return;
    if (spans.length > MAX_EXTRACTED_INTERVALS) {
      flashToast(`Too many intervals (${spans.length}); extract a shorter tier`);
      return;
    }
    try {
      await onExtractIntervals(spans);
      flashToast(`Extracted ${spans.length} interval${spans.length === 1 ? '' : 's'}`);
    } catch {
      flashToast('Could not extract the intervals');
    }
  }

  async function concatenateAll() {
    if (!onConcatenate) return;
    try {
      await onConcatenate();
      flashToast(`Concatenated ${recordings?.length ?? 0} recordings`);
    } catch {
      flashToast('Could not concatenate the recordings');
    }
  }

  async function combineStereo() {
    if (!onCombineToStereo) return;
    try {
      await onCombineToStereo();
      flashToast('Combined to stereo');
    } catch {
      flashToast('Could not combine to stereo');
    }
  }

  async function extractChannelsAll() {
    if (!onExtractChannels) return;
    // Snapshot the count now: extracting opens a mono channel, so reading it
    // afterwards would report 1.
    const count = audio?.channels ?? 0;
    try {
      await onExtractChannels();
      flashToast(`Extracted ${count} ${count === 1 ? 'channel' : 'channels'}`);
    } catch {
      flashToast('Could not extract the channels');
    }
  }

  async function convertToMonoAll() {
    if (!onConvertToMono) return;
    try {
      await onConvertToMono();
      flashToast('Converted to mono');
    } catch {
      flashToast('Could not convert to mono');
    }
  }

  // Transport Play / Space plays what the user is looking at, in priority order:
  // an active selection's time span, else the visible viewport when zoomed in,
  // else the whole file from the cursor. A box selection plays its time span
  // unfiltered here — the band-filtered preview stays on the readout's own
  // affordance.
  function handleTransportToggle() {
    if (isPlaying) {
      onPlayToggle();
      return;
    }
    if (selection) {
      onCursorChange?.(selection.t0);
      onPlaySelection?.(selection.t0, selection.t1);
      return;
    }
    const zoomedIn = !!audio && viewport.t1 - viewport.t0 < audio.duration - 1e-6;
    if (zoomedIn) {
      onCursorChange?.(viewport.t0);
      onPlaySelection?.(viewport.t0, viewport.t1);
      return;
    }
    onPlayToggle();
  }

  // Praat's "Play window" (Shift-Tab in Praat mode): always plays the whole
  // visible window, ignoring any selection, and always restarts rather than
  // toggling to a stop on a second press — unlike playPause/handleTransportToggle.
  function playWindow() {
    if (!audio) return;
    onCursorChange?.(viewport.t0);
    onPlaySelection?.(viewport.t0, viewport.t1);
  }

  // Double-click on a pane: inside the live selection zooms to it, empty pane
  // space fits the whole file.
  function handleDoubleZoom(intent: 'zoom' | 'fit') {
    if (intent === 'zoom' && selection) zoomToSelection();
    else fitFile();
  }

  function scaleFrequencyCeiling(factor: number) {
    const f1 = Math.max(200, Math.min(20000, viewport.f1 * factor));
    setViewport({ ...viewport, f1 });
  }

  function resetFrequencyCeiling() {
    setViewport({ ...viewport, f1: DEFAULT_CEILING });
  }

  function scaleAmplitude(factor: number) {
    setViewport({ ...viewport, ampScale: Math.max(0.25, Math.min(8, viewport.ampScale * factor)) });
  }

  function resetAmplitude() {
    setViewport({ ...viewport, ampScale: DEFAULT_AMP });
  }

  function resetVerticalScale() {
    setViewport({ ...viewport, f1: DEFAULT_CEILING, ampScale: DEFAULT_AMP });
  }

  function toggleWaveform() {
    waveformVisible = !waveformVisible;
  }

  function handleTierInterval(t0: number, t1: number) {
    selection = { t0, t1, f0: viewport.f0, f1: viewport.f1, mode: 'time' };
    onCursorChange?.(t0);
    onPlaySelection?.(t0, t1);
  }

  async function openVoiceReport() {
    if (!client || !audio || !selection) return;
    voiceReportOpen = true;
    voiceReportLoading = true;
    voiceReport = null;
    const sel = selection;
    try {
      voiceReport = await client.voiceReport(
        audio.id,
        sel.t0,
        sel.t1,
        overlayParams.pitch.floorHz,
        overlayParams.pitch.ceilingHz
      );
    } catch {
      voiceReport = null;
    } finally {
      voiceReportLoading = false;
    }
  }

  function setViewport(next: ViewportState) {
    viewport = clampViewport(next, audio?.duration ?? 1);
  }

  function fitFile() {
    if (!audio) return;
    setViewport(defaultViewport(audio.duration));
  }

  // `F` frames the current selection when one is set, and otherwise falls back
  // to the whole file, so a single key serves both the DAW "fit" gestures.
  function fitSelectionOrFile() {
    if (selection) zoomToSelection();
    else fitFile();
  }

  function zoomHorizontal(factor: number, anchorRatio: number) {
    if (!audio) return;
    const span = viewport.t1 - viewport.t0;
    const anchor = viewport.t0 + span * anchorRatio;
    const nextSpan = span * factor;
    setViewport({
      ...viewport,
      t0: anchor - nextSpan * anchorRatio,
      t1: anchor + nextSpan * (1 - anchorRatio)
    });
  }

  function scrollHorizontal(deltaSeconds: number) {
    setViewport({ ...viewport, t0: viewport.t0 + deltaSeconds, t1: viewport.t1 + deltaSeconds });
  }

  function zoomVertical(factor: number) {
    const f1 = Math.max(200, Math.min(12000, viewport.f1 * factor));
    setViewport({ ...viewport, ampScale: viewport.ampScale / factor, f1 });
  }

  function handleWheel(event: WheelEvent) {
    if (!audio) return;
    // Always swallow the wheel: a Ctrl/Cmd wheel is how a macOS trackpad pinch
    // arrives, and the browser would otherwise page-zoom the whole app.
    event.preventDefault();
    const target = event.currentTarget as HTMLElement;
    const rect = target.getBoundingClientRect();
    const anchorRatio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    if (event.altKey) {
      zoomVertical(event.deltaY < 0 ? 0.86 : 1.16);
      return;
    }
    if (event.shiftKey) {
      const span = viewport.t1 - viewport.t0;
      scrollHorizontal((event.deltaY / 600) * span);
      return;
    }
    // Plain wheel and Ctrl/Cmd wheel (the trackpad pinch) both drive time zoom
    // anchored on the pointer. Every pane reads the one shared viewport, so the
    // waveform and spectrogram stay locked to the same time axis.
    zoomHorizontal(event.deltaY < 0 ? 0.82 : 1.22, anchorRatio);
  }

  function handlePointer(event: PointerEvent) {
    if (!audio || event.buttons !== 1) return;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    const time = viewport.t0 + ratio * (viewport.t1 - viewport.t0);
    onCursorChange?.(time);
  }

  const keyBindings = getKeyBindings();

  // The `editor` scope's rebindable actions, keyed by command id. Which chord
  // fires which of these is entirely data (`keyBindings`, mode-dependent) —
  // this map only says what each command *does*, never what key it is.
  const editorActions: Record<string, () => void> = {
    playPause: handleTransportToggle,
    playWindow,
    fitFile,
    zoomToSelection: fitSelectionOrFile,
    toggleInspector: () => {
      inspectorOpen = !inspectorOpen;
    },
    toggleWaveform,
    clearSelection,
    exportFigure: () => {
      if (audio) exportOpen = !exportOpen;
    }
  };

  function handleKeydown(event: KeyboardEvent) {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
    // Escape closes whichever overlay sits on top before it does anything
    // else; this is baseline modal behavior, not a rebindable command.
    if (event.key === 'Escape' && measureOpen) {
      event.preventDefault();
      measureOpen = false;
      return;
    }
    if (event.key === 'Escape' && spectrumOpen) {
      event.preventDefault();
      spectrumOpen = false;
      return;
    }
    if (event.key === 'Escape' && sweepOpen) {
      event.preventDefault();
      sweepOpen = false;
      return;
    }
    if (event.key === 'Escape' && vowelChartOpen) {
      event.preventDefault();
      vowelChartOpen = false;
      return;
    }
    if (event.key === 'Escape' && (selection || voiceReportOpen)) {
      event.preventDefault();
      if (voiceReportOpen) voiceReportOpen = false;
      else clearSelection();
      return;
    }
    if (!keyBindings) return;
    const commandId = keyBindings.commandForChord('editor', chordFromEvent(event));
    const action = commandId ? editorActions[commandId] : undefined;
    if (!action) return;
    event.preventDefault();
    action();
  }

  const hasSelection = () => selection !== null;
  const hasAudio = () => audio !== null;

  registerCommands([
    {
      id: 'playPause',
      title: 'Play / pause',
      group: 'Playback',
      shortcut: () => keyBindings?.labelFor('playPause') ?? 'Space',
      keywords: ['transport', 'stop', 'play selection', 'play visible'],
      enabled: hasAudio,
      run: handleTransportToggle
    },
    {
      id: 'playWindow',
      title: 'Play visible window',
      group: 'Playback',
      shortcut: () => keyBindings?.labelFor('playWindow') ?? '',
      keywords: ['transport', 'play view', 'praat', 'play window'],
      enabled: hasAudio,
      run: playWindow
    },
    {
      id: 'fitFile',
      title: 'Fit whole file',
      group: 'View',
      shortcut: () => keyBindings?.labelFor('fitFile') ?? '0',
      keywords: ['zoom out', 'reset zoom', 'overview'],
      enabled: hasAudio,
      run: fitFile
    },
    {
      id: 'zoomToSelection',
      title: 'Zoom to selection',
      group: 'View',
      shortcut: () => keyBindings?.labelFor('zoomToSelection') ?? 'F',
      keywords: ['fit selection'],
      enabled: hasSelection,
      run: zoomToSelection
    },
    {
      id: 'zoomIn',
      title: 'Zoom in',
      group: 'View',
      shortcut: 'Wheel / pinch',
      keywords: ['time zoom', 'ctrl wheel'],
      enabled: hasAudio,
      run: () => zoomHorizontal(0.8, 0.5)
    },
    {
      id: 'zoomOut',
      title: 'Zoom out',
      group: 'View',
      shortcut: 'Wheel / pinch',
      keywords: ['time zoom', 'ctrl wheel'],
      enabled: hasAudio,
      run: () => zoomHorizontal(1.25, 0.5)
    },
    {
      id: 'zoomFrequency',
      title: 'Zoom frequency / amplitude',
      group: 'View',
      shortcut: 'Alt+wheel',
      keywords: ['vertical zoom', 'frequency range', 'amplitude'],
      enabled: hasAudio,
      run: () => zoomVertical(0.86)
    },
    {
      id: 'toggleInspector',
      title: 'Toggle inspector',
      group: 'View',
      shortcut: () => keyBindings?.labelFor('toggleInspector') ?? 'I',
      keywords: ['parameters', 'panel'],
      run: () => {
        inspectorOpen = !inspectorOpen;
      }
    },
    {
      id: 'toggleRecordingsRail',
      title: 'Toggle recordings rail',
      group: 'View',
      keywords: ['recordings', 'compare', 'corpus', 'panel'],
      enabled: () => (recordings?.length ?? 0) > 0,
      run: () => {
        railOpen = !railOpen;
      }
    },
    {
      id: 'toggleWaveform',
      title: 'Toggle waveform pane',
      group: 'View',
      shortcut: () => keyBindings?.labelFor('toggleWaveform') ?? 'W',
      keywords: ['waveform', 'ghost', 'overlay', 'envelope', 'hide'],
      enabled: hasAudio,
      run: toggleWaveform
    },
    {
      id: 'resetVerticalScale',
      title: 'Reset vertical scale',
      group: 'View',
      keywords: ['amplitude', 'frequency ceiling', 'gain', 'reset zoom'],
      enabled: hasAudio,
      run: resetVerticalScale
    },
    {
      id: 'togglePitchTrack',
      title: 'Toggle pitch track',
      group: 'Analysis',
      api: ['pitchTrack'],
      keywords: ['f0', 'overlay'],
      run: () => {
        overlayParams.pitch.show = !overlayParams.pitch.show;
      }
    },
    {
      id: 'copyPitchContour',
      title: 'Copy pitch contour (CSV)',
      group: 'Analysis',
      api: ['pitchTrackSpan'],
      keywords: ['f0', 'pitch', 'contour', 'csv', 'export', 'pitchtier', 'copy', 'list'],
      enabled: hasAudio,
      run: () => void copyPitchContour()
    },
    {
      id: 'copyIntensityContour',
      title: 'Copy intensity contour (CSV)',
      group: 'Analysis',
      api: ['intensityTrack'],
      keywords: ['intensity', 'db', 'loudness', 'contour', 'csv', 'export', 'copy', 'list'],
      enabled: hasAudio,
      run: () => void copyIntensityContour()
    },
    {
      id: 'copyFormantContour',
      title: 'Copy formant contour (CSV)',
      group: 'Analysis',
      api: ['formantTrack'],
      keywords: ['formant', 'f1', 'f2', 'contour', 'csv', 'export', 'copy', 'list', 'vowel'],
      enabled: hasAudio,
      run: () => void copyFormantContour()
    },
    {
      id: 'copySpectrum',
      title: 'Copy spectrum (CSV)',
      group: 'Analysis',
      api: ['spectrumSlice'],
      keywords: ['spectrum', 'fft', 'frequency', 'db', 'csv', 'export', 'copy', 'slice'],
      enabled: hasSelection,
      run: () => void copySpectrum()
    },
    {
      id: 'copyHarmonicityContour',
      title: 'Copy harmonicity contour (CSV)',
      group: 'Analysis',
      api: ['harmonicityTrack'],
      keywords: ['harmonicity', 'hnr', 'harmonics', 'noise', 'contour', 'csv', 'export', 'copy'],
      enabled: () => hasAudio() && onHarmonicityTrack !== undefined,
      run: () => void copyHarmonicityContour()
    },
    {
      id: 'copyCppContour',
      title: 'Copy CPP contour (CSV)',
      group: 'Analysis',
      api: ['cppTrack'],
      keywords: ['cpp', 'cepstral', 'peak', 'prominence', 'contour', 'csv', 'export', 'copy'],
      enabled: () => hasAudio() && onCppTrack !== undefined,
      run: () => void copyCppContour()
    },
    {
      id: 'toggleFormantTrack',
      title: 'Toggle formant track',
      group: 'Analysis',
      api: ['formantTrack'],
      keywords: ['overlay'],
      run: () => {
        overlayParams.formant.show = !overlayParams.formant.show;
      }
    },
    {
      id: 'toggleIntensityTrack',
      title: 'Toggle intensity track',
      group: 'Analysis',
      api: ['intensityTrack'],
      keywords: ['overlay', 'db'],
      run: () => {
        overlayParams.intensity.show = !overlayParams.intensity.show;
      }
    },
    {
      id: 'toggleHarmonicityTrack',
      title: 'Toggle harmonicity track',
      group: 'Analysis',
      api: ['harmonicityTrack'],
      keywords: ['overlay', 'hnr', 'harmonics', 'noise'],
      run: () => {
        overlayParams.harmonicity.show = !overlayParams.harmonicity.show;
      }
    },
    {
      id: 'toggleCppTrack',
      title: 'Toggle CPP track',
      group: 'Analysis',
      api: ['cppTrack'],
      keywords: ['overlay', 'cpp', 'cepstral', 'peak', 'prominence', 'breathiness'],
      run: () => {
        overlayParams.cpp.show = !overlayParams.cpp.show;
      }
    },
    {
      id: 'toggleFormantTracking',
      title: 'Toggle formant tracking',
      group: 'Analysis',
      api: ['formantSpanMeans'],
      keywords: ['smoothed', 'burg'],
      run: () => {
        overlayParams.formant.smoothed = !overlayParams.formant.smoothed;
      }
    },
    {
      id: 'voiceReport',
      title: 'Voice report over selection',
      group: 'Analysis',
      api: ['voiceReport'],
      keywords: ['jitter', 'shimmer', 'hnr'],
      enabled: hasSelection,
      run: () => void openVoiceReport()
    },
    {
      id: 'measureIntervals',
      title: 'Measure labelled intervals…',
      group: 'Analysis',
      api: ['selectionReadout', 'formantSpanMeans'],
      keywords: ['table', 'csv', 'tsv', 'export', 'harvest', 'formants', 'batch'],
      enabled: () => annotationId !== null,
      run: () => {
        measureOpen = true;
      }
    },
    {
      id: 'formantCeilingSweep',
      title: 'Formant ceiling sweep over selection',
      group: 'Analysis',
      api: ['formantSpanMeans'],
      keywords: ['formant', 'ceiling', 'lpc', 'fast track', 'vowel', 'sweep'],
      enabled: hasSelection,
      run: () => void openFormantSweep()
    },
    {
      id: 'ltas',
      title: 'LTAS over selection',
      group: 'Analysis',
      api: ['ltas'],
      keywords: ['ltas', 'long-term', 'average', 'spectrum', 'spectral tilt'],
      enabled: hasSelection,
      run: () => void openLtas()
    },
    {
      id: 'cepstrum',
      title: 'Cepstrum over selection',
      group: 'Analysis',
      api: ['cepstrumSlice'],
      keywords: ['cepstrum', 'quefrency', 'rahmonic', 'cpp', 'pitch period', 'f0'],
      enabled: hasSelection,
      run: () => void openCepstrum()
    },
    {
      id: 'vowelChart',
      title: 'Vowel F1–F2 chart…',
      group: 'Analysis',
      api: ['formantSpanMeans'],
      keywords: ['vowel', 'chart', 'f1', 'f2', 'scatter', 'space'],
      enabled: () => annotationId !== null,
      run: () => {
        vowelChartOpen = true;
      }
    },
    {
      id: 'annotateBySilences',
      title: 'Annotate by silences',
      group: 'Annotation',
      api: ['silenceIntervals'],
      keywords: ['silence', 'segment', 'vad', 'speech', 'textgrid', 'chunk', 'auto'],
      enabled: () => annotationId !== null,
      run: () => void annotateBySilences()
    },
    {
      id: 'annotateByVoicing',
      title: 'Annotate by voicing (V/U)',
      group: 'Annotation',
      api: ['voicingIntervals'],
      keywords: ['voicing', 'voiced', 'unvoiced', 'vuv', 'pitch', 'segment', 'textgrid', 'auto'],
      enabled: () => annotationId !== null,
      run: () => void annotateByVoicing()
    },
    {
      id: 'playSelection',
      title: 'Play selection',
      group: 'Selection',
      enabled: hasSelection,
      run: playSelection
    },
    {
      id: 'clearSelection',
      title: 'Clear selection',
      group: 'Selection',
      shortcut: () => keyBindings?.labelFor('clearSelection') ?? 'Esc',
      enabled: hasSelection,
      run: clearSelection
    },
    {
      id: 'extractSelection',
      title: 'Extract selection to new recording',
      group: 'Selection',
      api: ['exportSpanWav', 'importAudio'],
      keywords: ['extract', 'crop', 'clip', 'split', 'new sound', 'copy to', 'trim'],
      enabled: () => hasSelection() && onExtractSelection !== undefined,
      run: extractCurrentSelection
    },
    {
      id: 'extractSelectionWithTiers',
      title: 'Extract selection with tiers (new recording)',
      group: 'Selection',
      api: ['exportSpanWav', 'importAudio', 'addIntervalTier'],
      keywords: ['extract', 'crop', 'clip', 'tiers', 'textgrid', 'labels', 'part', 'trim'],
      enabled: () =>
        hasSelection() && annotationId !== null && onExtractSelectionWithTiers !== undefined,
      run: extractWithTiersCurrentSelection
    },
    {
      id: 'filterSelection',
      title: 'Filter selection (pass Hann band)',
      group: 'Selection',
      api: ['exportBandFilteredSpanWav', 'importAudio'],
      keywords: ['filter', 'band pass', 'hann', 'frequency', 'new sound', 'bandpass'],
      enabled: () => selection?.mode === 'box' && onFilterSelection !== undefined,
      run: filterCurrentSelection
    },
    {
      id: 'notchSelection',
      title: 'Filter selection (stop Hann band)',
      group: 'Selection',
      api: ['exportNotchFilteredSpanWav', 'importAudio'],
      keywords: ['filter', 'notch', 'band stop', 'reject', 'hum', 'hann', 'new sound'],
      enabled: () => selection?.mode === 'box' && onNotchSelection !== undefined,
      run: notchCurrentSelection
    },
    {
      id: 'preemphasisSelection',
      title: 'Pre-emphasize selection (new recording)',
      group: 'Selection',
      api: ['applyPreemphasisWav', 'importAudio'],
      keywords: ['pre-emphasis', 'preemphasis', 'high pass', 'tilt', 'formant', 'new sound'],
      enabled: () => hasSelection() && onPreemphasisSelection !== undefined,
      run: preemphasizeCurrentSelection
    },
    {
      id: 'subtractMeanSelection',
      title: 'Subtract mean (remove DC offset)',
      group: 'Selection',
      api: ['subtractMeanWav', 'importAudio'],
      keywords: ['dc', 'offset', 'subtract mean', 'centre', 'bias', 'new sound'],
      enabled: () => hasSelection() && onSubtractMeanSelection !== undefined,
      run: subtractMeanCurrentSelection
    },
    {
      id: 'reverseSelection',
      title: 'Reverse to new recording',
      group: 'Selection',
      api: ['reverseSpanWav', 'importAudio'],
      keywords: ['reverse', 'backwards', 'flip', 'retrograde', 'new sound', 'time'],
      enabled: () => hasAudio() && onReverseSelection !== undefined,
      run: reverseCurrent
    },
    {
      id: 'scaleSelection',
      title: 'Scale intensity to 70 dB (new recording)',
      group: 'Selection',
      api: ['scaleIntensitySpanWav', 'importAudio'],
      keywords: ['scale', 'intensity', 'loudness', 'normalize', 'gain', 'db', 'new sound'],
      enabled: () => hasAudio() && onScaleSelection !== undefined,
      run: scaleCurrent
    },
    {
      id: 'scalePeakSelection',
      title: 'Scale peak to 0.99 (new recording)',
      group: 'Selection',
      api: ['scalePeakSpanWav', 'importAudio'],
      keywords: ['scale', 'peak', 'normalize', 'amplitude', 'gain', 'new sound', 'clip'],
      enabled: () => hasAudio() && onScalePeakSelection !== undefined,
      run: scalePeakCurrent
    },
    {
      id: 'resample16k',
      title: 'Resample to 16 kHz (new recording)',
      group: 'Selection',
      api: ['resampleWav', 'importAudio'],
      keywords: ['resample', 'downsample', 'sample rate', '16000', 'convert', 'new sound'],
      enabled: () => hasAudio() && onResample16k !== undefined,
      run: () => onResample16k?.()
    },
    {
      id: 'concatenateRecordings',
      title: 'Concatenate all recordings (new recording)',
      group: 'Project',
      api: ['concatWav', 'importAudio'],
      keywords: ['concatenate', 'join', 'merge', 'combine', 'append', 'new sound'],
      enabled: () => (recordings?.length ?? 0) >= 2 && onConcatenate !== undefined,
      run: () => void concatenateAll()
    },
    {
      id: 'combineToStereo',
      title: 'Combine to stereo (new recording)',
      group: 'Project',
      api: ['combineStereoWav', 'importAudio'],
      keywords: ['stereo', 'combine', 'two channel', 'left right', 'pair', 'new sound'],
      enabled: () => (recordings?.length ?? 0) >= 2 && onCombineToStereo !== undefined,
      run: () => void combineStereo()
    },
    {
      id: 'extractChannels',
      title: 'Extract each channel (new recordings)',
      group: 'Project',
      api: ['exportChannelWav', 'importAudio'],
      keywords: ['extract', 'channel', 'stereo', 'left', 'right', 'split', 'mono', 'deinterleave'],
      enabled: () => (audio?.channels ?? 1) > 1 && onExtractChannels !== undefined,
      run: () => void extractChannelsAll()
    },
    {
      id: 'convertToMono',
      title: 'Convert to mono (new recording)',
      group: 'Project',
      api: ['convertToMono', 'importAudio'],
      keywords: ['mono', 'downmix', 'mixdown', 'stereo', 'channels', 'average', 'new sound'],
      enabled: () => (audio?.channels ?? 1) > 1 && onConvertToMono !== undefined,
      run: () => void convertToMonoAll()
    },
    {
      id: 'snapSelectionZero',
      title: 'Snap selection to zero crossings',
      group: 'Selection',
      api: ['nearestZeroCrossing'],
      keywords: ['zero', 'crossing', 'snap', 'click', 'boundary', 'cut'],
      enabled: () => hasSelection() && onNearestZero !== undefined,
      run: () => void snapSelectionToZero()
    },
    {
      id: 'exportFigure',
      title: 'Export figure',
      group: 'Figures',
      api: ['buildFigure', 'exportFigure'],
      shortcut: () => keyBindings?.labelFor('exportFigure') ?? 'E',
      keywords: ['svg', 'pdf', 'png', 'save image'],
      enabled: hasAudio,
      run: () => {
        exportOpen = !exportOpen;
      }
    },
    {
      id: 'exportAudio',
      title: 'Export audio (WAV)',
      group: 'Figures',
      api: ['exportSpanWav', 'exportBandFilteredSpanWav'],
      keywords: ['wav', 'save audio', 'selection', 'clip', 'download audio', 'sound'],
      enabled: () => hasAudio() && onExportAudio !== undefined,
      run: () => {
        audioExportOpen = true;
      }
    },
    ...BUILTIN_PALETTES.map((p) => ({
      id: `colormap${p.name}`,
      title: `Spectrogram palette: ${p.label}`,
      group: 'Appearance' as const,
      keywords: ['colormap', 'color', p.label.toLowerCase()],
      run: () => onPaletteChange({ kind: 'builtin', name: p.name })
    })),
    {
      id: 'colormapNewRamp',
      title: 'New custom spectrogram ramp…',
      group: 'Appearance',
      keywords: ['colormap', 'gradient', 'custom', 'palette', 'editor'],
      run: openNewRamp
    },
    {
      id: 'switchRecording',
      title: 'Switch recording',
      group: 'Project',
      keywords: ['open', 'take', 'corpus', 'change recording'],
      enabled: () => !!onSwitchRecording && (recordings?.length ?? 0) > 1,
      run: () => switcher?.show()
    },
    {
      id: 'closeRecording',
      title: 'Close recording',
      group: 'Project',
      keywords: ['back', 'corpus', 'exit'],
      enabled: () => onExit !== undefined,
      run: () => onExit?.()
    }
  ]);
</script>

<svelte:window onkeydown={handleKeydown} />

<div
  class="editor"
  data-testid="editor"
  data-visible-start={viewport.t0.toFixed(6)}
  data-visible-end={viewport.t1.toFixed(6)}
  data-visible-freq={viewport.f1.toFixed(6)}
  data-cursor-time={cursorTime.toFixed(6)}
>
    <nav class="breadcrumb" aria-label="Location" data-testid="editor-breadcrumb">
      {#if projectGroupName}
        <span class="crumb-group" data-testid="crumb-project-group">{projectGroupName}</span>
        <span class="crumb-sep" aria-hidden="true">›</span>
      {/if}
      {#if onExit}
        <span
          class="crumb-back"
          role="button"
          tabindex="0"
          data-testid="back-corpus"
          title="Back to the project"
          onclick={() => onExit?.()}
          onkeydown={handleBackKeydown}
        >
          <IconArrowLeft aria-hidden="true" />
          {#if onRenameProject}
            <InlineRename
              name={projectName ?? 'Project'}
              class="crumb-back-name"
              label="Rename project"
              testId="rename-editor-project"
              onRename={(next) => onRenameProject?.(next)}
            />
          {:else}
            <span>{projectName ?? 'Project'}</span>
          {/if}
        </span>
        <span class="crumb-sep" aria-hidden="true">›</span>
      {/if}
      {#if recordingGroupName}
        <span class="crumb-group" data-testid="crumb-recording-group">{recordingGroupName}</span>
        <span class="crumb-slash" aria-hidden="true">/</span>
      {/if}
      {#if recordings && recordings.length > 0 && onSwitchRecording}
        <RecordingSwitcher
          bind:this={switcher}
          {client}
          {theme}
          {recordings}
          {groups}
          currentRecordingId={currentRecordingId ?? null}
          onSwitch={(mediaId) => onSwitchRecording?.(mediaId)}
          onRename={onRenameRecording}
        />
      {:else}
        <span class="crumb-current">{recordings?.[0]?.name ?? audio?.name ?? ''}</span>
      {/if}

      <div class="crumb-spacer"></div>

      {#if audio}
        <span class="meta-chips" data-testid="audio-meta"
          >{formatSampleRate(audio.sampleRate)} · {channelsLabel}</span
        >
      {/if}
      {#if dirty !== undefined}
        <span class="save-state" class:dirty data-testid="save-state">
          {dirty ? 'Saving…' : 'Saved'}
        </span>
      {/if}

      <label class="crumb-import" title="Open a recording">
        <IconFolderOpen aria-hidden="true" />
        <span>Open</span>
        <input
          bind:this={fileInput}
          data-testid="file-input"
          type="file"
          accept=".wav,audio/wav,audio/x-wav,.aiff,.aif,audio/aiff,.flac,audio/flac"
          onchange={(event) => takeFileList(event.currentTarget.files)}
        />
      </label>

      <PalettePicker
        {palette}
        invert={paletteInvert}
        {customRamps}
        onSelect={onPaletteChange}
        onToggleInvert={onPaletteInvertToggle}
        onNewRamp={openNewRamp}
        onEditRamp={openEditRamp}
        onRenameRamp={(ramp, name) => onSaveRamp({ ...ramp, name })}
      />

      <button
        type="button"
        class="icon-button"
        aria-label="Toggle theme"
        title={theme === 'light' ? 'Switch to dark' : 'Switch to light'}
        onclick={() => onThemeChange(theme === 'light' ? 'dark' : 'light')}
      >
        {#if theme === 'light'}
          <IconMoon aria-hidden="true" />
        {:else}
          <IconSun aria-hidden="true" />
        {/if}
      </button>
    </nav>

  {#if editingRamp}
    <div class="ramp-editor-slot" data-testid="ramp-editor-slot">
      {#key editingRamp.id}
        <GradientEditor
          ramp={editingRamp}
          existing={editingExisting}
          onChange={(next) => (editingRamp = next)}
          onSave={saveRamp}
          onCancel={() => (editingRamp = null)}
          onDelete={deleteRamp}
        />
      {/key}
    </div>
  {/if}

  <OverviewStrip {client} {audio} {viewport} {theme} onViewportChange={setViewport} />

  {#if selection}
    <ReadoutBar
      {selection}
      {readout}
      {formantMeans}
      showFormants={overlayParams.formant.smoothed}
      pitchUnit={overlayParams.pitch.unit}
      {filteredPlaying}
      onPlay={playSelection}
      onZoom={zoomToSelection}
      onVoiceReport={openVoiceReport}
      onSpectrum={openSpectrum}
      onExtract={onExtractSelection ? extractCurrentSelection : undefined}
      onFilter={onFilterSelection ? filterCurrentSelection : undefined}
      onReverse={onReverseSelection ? reverseCurrent : undefined}
      onClear={clearSelection}
    />
  {/if}

  <div class="workspace">
    {#if recordings && recordings.length > 0 && onSwitchRecording}
      <RecordingsRail
        {client}
        {theme}
        recordings={recordings.map((rec) => ({
          mediaId: rec.mediaId,
          name: rec.name,
          duration: rec.duration,
          sampleRate: rec.sampleRate,
          audioId: rec.audioId
        }))}
        currentRecordingId={currentRecordingId ?? null}
        open={railOpen}
        onToggle={() => (railOpen = !railOpen)}
        onSwitch={(mediaId) => onSwitchRecording?.(mediaId)}
        onImport={() => fileInput?.click()}
        onRename={onRenameRecording}
      />
    {/if}

    <main
      class="timeline"
      data-testid="timeline"
      data-waveform-visible={waveformVisible}
      style:grid-template-rows={waveformVisible
        ? '1.5rem minmax(9rem, 22vh) minmax(12rem, 1fr) minmax(7rem, 32vh)'
        : '1.5rem minmax(12rem, 1fr) minmax(7rem, 32vh)'}
      onwheel={handleWheel}
      onpointerdown={handlePointer}
      onpointermove={handlePointer}
    >
      <TimeRuler {viewport} />
      {#if waveformVisible}
        <WaveformPane
          {client}
          {audio}
          {viewport}
          {theme}
          {selection}
          onSelectionChange={handleSelectionChange}
          onSeek={(time) => onCursorChange?.(time)}
          onScaleAmp={scaleAmplitude}
          onResetAmp={resetAmplitude}
          onDoubleZoom={handleDoubleZoom}
          pulses={overlayParams.pulses.show ? pulseTimes : null}
        />
      {/if}
      <SpectrogramPane
        {client}
        {audio}
        {viewport}
        {theme}
        palette={activePalette}
        {paletteInvert}
        {overlayParams}
        onOverlayStats={(stats) => (overlayStats = stats)}
        {selection}
        onSelectionChange={handleSelectionChange}
        onSeek={(time) => onCursorChange?.(time)}
        onScaleFrequency={scaleFrequencyCeiling}
        onResetFrequency={resetFrequencyCeiling}
        onDoubleZoom={handleDoubleZoom}
        onTracks={(next) => (overlayTracks = next)}
        {readout}
        {formantMeans}
        ghostWaveform={!waveformVisible}
      />
      <TierPane
        {client}
        audioId={audio?.id ?? null}
        {annotationId}
        audioDuration={audio?.duration ?? 0}
        sampleRate={audio?.sampleRate ?? 0}
        {viewport}
        {cursorTime}
        revision={tierRevision + tierRefreshToken}
        onSeek={(time) => onCursorChange?.(time)}
        {onAnnotationChange}
        onIntervalActivate={handleTierInterval}
      />

      {#if audio && cursorTime >= viewport.t0 && cursorTime <= viewport.t1}
        <div
          class="playhead"
          data-testid="playhead"
          style:left="{((cursorTime - viewport.t0) / (viewport.t1 - viewport.t0)) * 100}%"
        ></div>
      {/if}
    </main>

    {#if audio}
      <LevelMeter {client} audioId={audio.id} duration={audio.duration} {cursorTime} {isPlaying} />
    {/if}

    {#if inspectorOpen}
      <InspectorPanel
        {formantBandwidths}
        params={overlayParams}
        stats={overlayStats}
        {readout}
        {formantMeans}
        {formantStats}
        {intensityStats}
        {harmonicityStats}
        {cppStats}
        cursor={cursorSample}
        {cursorTime}
        onClose={() => (inspectorOpen = false)}
      />
    {/if}
  </div>

  <Transport
    {audio}
    {cursorTime}
    {isPlaying}
    {loopEnabled}
    selectionSeconds={selection ? selection.t1 - selection.t0 : null}
    viewSpanSeconds={viewport.t1 - viewport.t0}
    onSkipStart={skipToStart}
    onPlayToggle={handleTransportToggle}
    onSkipEnd={skipToEnd}
    {onLoopToggle}
    {onStartRecording}
    {recording}
    {recordingElapsedSeconds}
    {inspectorOpen}
    onToggleInspector={() => (inspectorOpen = !inspectorOpen)}
    {exportOpen}
    onToggleExportFigure={() => (exportOpen = !exportOpen)}
    onExportAudio={onExportAudio && (() => (audioExportOpen = !audioExportOpen))}
    {audioExportOpen}
  />

  {#if exportOpen && audio}
    <ExportDialog
      {client}
      {audio}
      {annotationId}
      {viewport}
      {overlayParams}
      appTheme={theme}
      appPalette={palette}
      onClose={() => (exportOpen = false)}
    />
  {/if}

  {#if audioExportOpen && audio && onExportAudio}
    <AudioExportDialog
      hasSelection={selection !== null}
      isBoxSelection={selection?.mode === 'box'}
      onExport={(options) => {
        const request = resolveAudioExport(options);
        if (request) onExportAudio?.(request);
        audioExportOpen = false;
      }}
      onClose={() => (audioExportOpen = false)}
    />
  {/if}

  {#if voiceReportOpen}
    <VoiceReportCard report={voiceReport} loading={voiceReportLoading} onClose={() => (voiceReportOpen = false)} />
  {/if}
  {#if measureOpen}
    <MeasurementTable
      {client}
      {audio}
      {annotationId}
      params={overlayParams}
      name={projectName ?? 'measurements'}
      onExtractIntervals={onExtractIntervals ? extractIntervalsFromTable : undefined}
      onClose={() => (measureOpen = false)}
    />
  {/if}
  {#if spectrumOpen}
    <SpectrumCard
      {client}
      {audio}
      t0={spectrumSpan.t0}
      t1={spectrumSpan.t1}
      mode={spectrumMode}
      onLpcEnvelope={onLpcEnvelope ?? null}
      onClose={() => (spectrumOpen = false)}
    />
  {/if}
  {#if vowelChartOpen}
    <VowelChartCard
      {client}
      {audio}
      {annotationId}
      maxFormants={overlayParams.formant.maxFormants}
      ceilingHz={overlayParams.formant.ceilingHz}
      onClose={() => (vowelChartOpen = false)}
    />
  {/if}
  {#if sweepOpen}
    <FormantSweepCard
      {client}
      {audio}
      t0={sweepSpan.t0}
      t1={sweepSpan.t1}
      maxFormants={overlayParams.formant.maxFormants}
      onClose={() => (sweepOpen = false)}
    />
  {/if}

  {#if toast}
    <div class="toast" data-testid="editor-toast" role="status" aria-live="polite">{toast}</div>
  {/if}
</div>

<style>
  /* Fits the viewport exactly, like a DAW: transport chrome keeps its natural
     height, the workspace takes whatever is left and shrinks panes internally
     (TierPane's row list, InspectorPanel) rather than growing the page. Flex
     stacking — not a fixed-row grid — so an optional row (ReadoutBar) never
     desyncs a row count from the number of children. */
  .editor {
    position: relative;
    height: 100vh;
    height: 100dvh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background: var(--chrome);
    color: var(--text);
  }

  .editor > :global(*) {
    flex: none;
  }

  /* The gradient editor floats below the transport, over the workspace, so
     opening it never reflows the panes and the live preview stays visible. */
  .ramp-editor-slot {
    position: absolute;
    top: 3rem;
    right: 0.85rem;
    max-height: calc(100% - 4rem);
    overflow-y: auto;
    z-index: 40;
  }

  @media (max-width: 720px) {
    .ramp-editor-slot {
      right: 0.5rem;
      left: 0.5rem;
    }
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    min-height: 2.1rem;
    padding: 0.25rem 0.75rem;
    border-bottom: 1px solid var(--chrome-strong);
    background: var(--chrome);
    font-size: 0.82rem;
  }

  .crumb-back {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    min-height: 1.6rem;
    padding: 0.2rem 0.55rem;
    cursor: pointer;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .crumb-back:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .crumb-back > :global(svg) {
    font-size: 0.95rem;
  }

  .crumb-back:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .crumb-current {
    color: var(--muted);
  }

  .crumb-group {
    color: var(--muted);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .crumb-sep {
    color: var(--muted);
    opacity: 0.6;
    user-select: none;
  }

  .crumb-slash {
    color: var(--muted);
    opacity: 0.6;
    margin: 0 -0.25rem;
    user-select: none;
  }

  .meta-chips {
    color: var(--muted);
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .save-state {
    color: var(--muted);
    font-size: 0.68rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    white-space: nowrap;
  }

  .save-state.dirty {
    color: var(--warn);
  }

  .crumb-spacer {
    flex: 1 1 auto;
  }

  .crumb-import {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    min-height: 1.6rem;
    padding: 0.2rem 0.55rem;
    font-size: 0.78rem;
    cursor: pointer;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .crumb-import:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .crumb-import :global(svg) {
    font-size: 0.95rem;
  }

  .crumb-import input {
    display: none;
  }

  .icon-button {
    display: grid;
    place-items: center;
    width: 1.9rem;
    height: 1.9rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font-size: 1rem;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .icon-button:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  /* Four possible columns — recordings rail, timeline, level meter, inspector —
     each conditionally rendered; an absent one leaves its `auto` track at zero
     width, so the timeline's `minmax(0, 1fr)` always absorbs whatever the
     chrome around it is not using. */
  .workspace {
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
  }

  .timeline {
    position: relative;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: 1.5rem minmax(9rem, 22vh) minmax(12rem, 1fr) minmax(7rem, 32vh);
    overflow: hidden;
    touch-action: none;
  }

  /* One playhead spans every time-aligned pane; the host advances cursorTime
     each animation frame during playback, so this is the moving line. */
  .playhead {
    position: absolute;
    top: 1.5rem;
    bottom: 0;
    width: 1px;
    margin-left: -0.5px;
    background: var(--warn);
    pointer-events: none;
    z-index: 6;
  }

  .toast {
    position: fixed;
    bottom: 1.25rem;
    left: 50%;
    transform: translateX(-50%);
    z-index: 60;
    padding: 0.5rem 0.9rem;
    border-radius: var(--radius-sm);
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--chrome-strong);
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.25);
    font-size: 0.82rem;
    pointer-events: none;
  }
</style>
