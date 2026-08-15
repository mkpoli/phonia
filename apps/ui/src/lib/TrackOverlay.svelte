<script lang="ts">
  import { untrack } from 'svelte';
  import type {
    AudioInfo,
    CoreClientLike,
    CppTrackData,
    FormantTrackData,
    HarmonicityTrackData,
    IntensityTrackData,
    OverlayParams,
    OverlayStats,
    PitchTrackData,
    ViewportState
  } from './types';
  import { resizeCanvas } from './rendering';
  import { convertPitch, pitchUnitSuffix } from './pitch-units';
  import type { OverlayTracks } from './tracks';

  interface Props {
    client: CoreClientLike | null;
    audio: AudioInfo | null;
    viewport: ViewportState;
    theme: 'light' | 'dark';
    params: OverlayParams;
    onStats?: (stats: OverlayStats) => void;
    /** Reports the fetched tracks, so readouts can sample what is drawn. */
    onTracks?: (tracks: OverlayTracks) => void;
  }

  let { client, audio, viewport, theme, params, onStats, onTracks }: Props = $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  let renderToken = $state(0);

  let pitch = $state<PitchTrackData | null>(null);
  let formant = $state<FormantTrackData | null>(null);
  let intensity = $state<IntensityTrackData | null>(null);
  let harmonicity = $state<HarmonicityTrackData | null>(null);
  let cpp = $state<CppTrackData | null>(null);
  // Highest voiced value from the authoritative whole-signal track, not the
  // span preview, so the clipping badge never flickers on a partial window.
  let pitchMaxHz = $state(0);
  // Increments whenever fresh pitch data (preview or full) is applied; a test
  // hook for the visible-span re-render latency.
  let pitchDataToken = $state(0);

  $effect(() => {
    onTracks?.({ pitch, formant, intensity, harmonicity, cpp });
  });

  // Track colours carry their own dark halo, so they read over any colormap
  // in either theme without being tuned per background.
  const PITCH_COLOR = '#9cc4ff';
  const FORMANT_COLOR = '#ff5a52';
  const INTENSITY_COLOR = '#ffcc33';
  const HARMONICITY_COLOR = '#5ee0a0';
  const CPP_COLOR = '#c08cff';
  const HALO = 'rgba(4, 8, 16, 0.7)';

  // Whole-signal analysis (pitch especially) is proportional to duration; past
  // this length the auto-run is paused so a long file does not tie up the
  // worker. Viewport-following analysis for long files is a later step.
  const MAX_OVERLAY_SECONDS = 120;
  // Head start for the pane tiles: whole-signal passes enter the serial
  // worker queue after the first waveform/spectrogram requests, so the view
  // paints before the long analyses run.
  const WHOLE_SIGNAL_DELAY_MS = 300;
  let tooLong = $derived((audio?.duration ?? 0) > MAX_OVERLAY_SECONDS);

  function reportStats() {
    onStats?.({ pitchMaxHz, formantMaxHz: formant?.maxHz ?? 0 });
  }

  // Each analysis runs over the whole signal (its frame grid is a function of
  // the audio alone), so the fetched track is reused across zoom and scroll;
  // only a parameter edit or a new file refetches. The draw pass renders the
  // visible span from the cached track, which is what makes a ceiling change
  // repaint the viewport immediately.
  $effect(() => {
    const id = audio?.id;
    const show = params.pitch.show;
    const floorHz = params.pitch.floorHz;
    const ceilingHz = params.pitch.ceilingHz;
    const voicingThreshold = params.pitch.voicingThreshold;
    if (!client || id === undefined || !show || tooLong) {
      pitch = null;
      pitchMaxHz = 0;
      reportStats();
      return;
    }
    // The viewport is read untracked: a parameter edit recomputes, but a plain
    // pan or zoom reuses the whole-signal track this effect settles on.
    const previewT0 = untrack(() => viewport.t0);
    const previewT1 = untrack(() => viewport.t1);
    let cancelled = false;
    let fullArrived = false;
    // Phase 1: the visible span, rendered first (pitch is the one contour whose
    // whole-signal cost grows with duration).
    client
      .pitchTrackSpan(id, floorHz, ceilingHz, previewT0, previewT1, voicingThreshold)
      .then((track) => {
        if (cancelled || fullArrived) return;
        pitch = track;
        pitchDataToken += 1;
      })
      .catch(() => {});
    // Phase 2: the whole-signal track, which replaces the preview and drives
    // the clipping badge. It waits a beat so the first waveform and
    // spectrogram requests enter the serial worker queue ahead of it — on a
    // slow WASM tier this whole-signal pass takes seconds, and anything
    // queued behind it stays blank for that long.
    const fullTimer = setTimeout(() => {
      client
        .pitchTrack(id, floorHz, ceilingHz, voicingThreshold)
        .then((track) => {
          if (cancelled) return;
          fullArrived = true;
          pitch = track;
          pitchMaxHz = track.maxHz;
          pitchDataToken += 1;
          reportStats();
        })
        .catch(() => {});
    }, WHOLE_SIGNAL_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(fullTimer);
    };
  });

  $effect(() => {
    const id = audio?.id;
    const show = params.formant.show;
    const ceilingHz = params.formant.ceilingHz;
    const maxFormants = params.formant.maxFormants;
    const smoothed = params.formant.smoothed;
    if (!client || id === undefined || !show || tooLong) {
      formant = null;
      reportStats();
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      client
        .formantTrack(id, ceilingHz, maxFormants, smoothed)
        .then((track) => {
          if (cancelled) return;
          formant = track;
          reportStats();
        })
        .catch(() => {});
    }, WHOLE_SIGNAL_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    const id = audio?.id;
    const show = params.intensity.show;
    const floorHz = params.intensity.floorHz;
    const subtractMean = params.intensity.subtractMean;
    if (!client || id === undefined || !show || tooLong) {
      intensity = null;
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      client
        .intensityTrack(id, floorHz, subtractMean)
        .then((track) => {
          if (cancelled) return;
          intensity = track;
        })
        .catch(() => {});
    }, WHOLE_SIGNAL_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    const id = audio?.id;
    const show = params.harmonicity.show;
    const duration = audio?.duration ?? 0;
    if (!client || id === undefined || !show || tooLong || duration <= 0) {
      harmonicity = null;
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      client
        .harmonicityTrack(id, 0, duration)
        .then((track) => {
          if (cancelled) return;
          harmonicity = track;
        })
        .catch(() => {});
    }, WHOLE_SIGNAL_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    const id = audio?.id;
    const show = params.cpp.show;
    const duration = audio?.duration ?? 0;
    if (!client || id === undefined || !show || tooLong || duration <= 0) {
      cpp = null;
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      client
        .cppTrack(id, 0, duration)
        .then((track) => {
          if (cancelled) return;
          cpp = track;
        })
        .catch(() => {});
    }, WHOLE_SIGNAL_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  });

  $effect(() => {
    if (!canvas) return;
    const observer = new ResizeObserver(() => scheduleDraw());
    observer.observe(canvas);
    scheduleDraw();
    return () => observer.disconnect();
  });

  $effect(() => {
    // Redraw when the viewport, theme, tracks, or scale bounds change.
    viewport.t0;
    viewport.t1;
    viewport.f0;
    viewport.f1;
    theme;
    pitch;
    formant;
    intensity;
    harmonicity;
    cpp;
    params.pitch.ceilingHz;
    params.pitch.floorHz;
    params.pitch.voicingThreshold;
    params.pitch.unit;
    params.harmonicity.show;
    params.cpp.show;
    params.formant.mark;
    scheduleDraw();
  });

  function scheduleDraw() {
    requestAnimationFrame(() => draw());
  }

  function timeToX(time: number, width: number) {
    return ((time - viewport.t0) / (viewport.t1 - viewport.t0)) * width;
  }

  function freqToY(freq: number, height: number) {
    const span = Math.max(1, viewport.f1 - viewport.f0);
    return height * (1 - (freq - viewport.f0) / span);
  }

  function draw() {
    if (!canvas) return;
    const { width, height } = resizeCanvas(canvas);
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, width, height);
    if (!audio) {
      renderToken += 1;
      return;
    }
    if (tooLong) {
      drawNote(ctx, width, `Overlays paused above ${MAX_OVERLAY_SECONDS}s`);
      renderToken += 1;
      return;
    }

    if (params.intensity.show && intensity) drawIntensity(ctx, width, height);
    if (params.harmonicity.show && harmonicity) drawHarmonicity(ctx, width, height);
    if (params.cpp.show && cpp) drawCpp(ctx, width, height);
    if (params.formant.show && formant) drawFormants(ctx, width, height);
    if (params.pitch.show && pitch) {
      drawPitch(ctx, width, height);
      drawPitchAxis(ctx, width, height);
    }
    renderToken += 1;
  }

  function drawNote(ctx: CanvasRenderingContext2D, width: number, text: string) {
    ctx.font = '12px ui-sans-serif, system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.textBaseline = 'top';
    ctx.lineWidth = 3;
    ctx.strokeStyle = HALO;
    ctx.strokeText(text, width / 2, 8);
    ctx.fillStyle = '#f1f5f9';
    ctx.fillText(text, width / 2, 8);
  }

  function drawFormants(ctx: CanvasRenderingContext2D, width: number, height: number) {
    // A connected track is only drawn from the Viterbi-smoothed candidates
    // (see the inspector note): raw Burg candidates carry no cross-frame
    // identity, so speckles are the honest fallback if smoothing is off even
    // when 'track' is still selected mid-toggle.
    if (params.formant.mark === 'track' && params.formant.smoothed) {
      drawFormantTracks(ctx, width, height, groupFormantFrames(formant!.points));
      return;
    }
    const points = formant!.points;
    ctx.strokeStyle = HALO;
    ctx.lineWidth = 1;
    for (let i = 0; i < points.length; i += 3) {
      const time = points[i];
      if (time < viewport.t0 || time > viewport.t1) continue;
      const freq = points[i + 1];
      if (freq < viewport.f0 || freq > viewport.f1) continue;
      const bandwidth = points[i + 2];
      const x = timeToX(time, width);
      const y = freqToY(freq, height);
      // Wider bandwidth reads as a larger, fuzzier speckle.
      const r = Math.min(3.6, Math.max(1.1, 1.1 + bandwidth / 260));
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fillStyle = FORMANT_COLOR;
      ctx.globalAlpha = 0.72;
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.stroke();
    }
  }

  /** One analysis frame's formant candidates, ascending by frequency. */
  interface FormantFrameCandidates {
    time: number;
    freqs: Float64Array;
  }

  /**
   * Regroups the flat `[time, freq, bandwidth]` stream back into per-frame
   * candidate lists. Points from the same analysis frame share the same
   * `time` and arrive contiguously, so a run of equal `time` values is one
   * frame; candidates within it are already ascending by frequency (the
   * engine's own ordering), which is what a rank index below tracks.
   */
  function groupFormantFrames(points: Float64Array): FormantFrameCandidates[] {
    const frames: { time: number; freqs: number[] }[] = [];
    let current: { time: number; freqs: number[] } | null = null;
    for (let i = 0; i < points.length; i += 3) {
      const time = points[i];
      if (!current || current.time !== time) {
        current = { time, freqs: [] };
        frames.push(current);
      }
      current.freqs.push(points[i + 1]);
    }
    return frames.map((f) => ({ time: f.time, freqs: Float64Array.from(f.freqs) }));
  }

  // Rank identity can still slip even in the smoothed track: the engine's
  // Viterbi assignment is per-slot, but a frame where a low slot has no
  // candidate compacts the remaining ones down by one position before this
  // array ever sees them (see phx-formant's `track_smoothed`), which can
  // shift a rank onto its neighboring formant for that one frame. A
  // same-formant transition stays within a bounded frequency ratio frame to
  // frame; a slot collision does not, since adjacent formants typically sit
  // 1.5-3x apart. Doubling (or halving) is a deliberately generous
  // engineering threshold — wide enough to pass a fast genuine transition,
  // tight enough to catch a jump onto the wrong formant — not a value from
  // any tracking literature.
  const MAX_FRAME_TO_FRAME_RATIO = 2.0;

  /**
   * Connected per-formant tracks: one polyline per rank (lowest candidate in
   * a frame is rank 0, the next is rank 1, and so on) over the
   * Viterbi-smoothed candidates, whose slot assignment is an actual tracking
   * decision rather than frame-local sort order. The path breaks wherever a
   * frame has no candidate at a rank — an unvoiced stretch, or a formant the
   * tracker dropped that frame — and wherever consecutive frames jump more
   * than `MAX_FRAME_TO_FRAME_RATIO`, since that is the signature of a rank
   * landing on a different formant rather than a fast but genuine move. A
   * broken path never implies a measurement the analysis did not produce.
   */
  function drawFormantTracks(
    ctx: CanvasRenderingContext2D,
    width: number,
    height: number,
    frames: FormantFrameCandidates[]
  ) {
    let maxRank = 0;
    for (const frame of frames) maxRank = Math.max(maxRank, frame.freqs.length);

    const strokeRank = (rank: number, color: string, lineWidth: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;
      ctx.lineJoin = 'round';
      ctx.lineCap = 'round';
      let drawing = false;
      let prevFreq = 0;
      ctx.beginPath();
      for (const frame of frames) {
        const freq = frame.freqs[rank];
        const inView =
          freq !== undefined &&
          frame.time >= viewport.t0 &&
          frame.time <= viewport.t1 &&
          freq >= viewport.f0 &&
          freq <= viewport.f1;
        if (!inView) {
          drawing = false;
          continue;
        }
        const ratio = drawing ? freq / prevFreq : 1;
        const plausibleContinuation =
          ratio <= MAX_FRAME_TO_FRAME_RATIO && ratio >= 1 / MAX_FRAME_TO_FRAME_RATIO;
        const x = timeToX(frame.time, width);
        const y = freqToY(freq, height);
        if (!drawing || !plausibleContinuation) {
          ctx.moveTo(x, y);
          drawing = true;
        } else {
          ctx.lineTo(x, y);
        }
        prevFreq = freq;
      }
      ctx.stroke();
    };

    for (let rank = 0; rank < maxRank; rank += 1) {
      strokeRank(rank, HALO, 3.4);
      strokeRank(rank, FORMANT_COLOR, 1.5);
    }
  }

  // The pitch scale's vertical fraction (0 at the bottom, 1 at the top) for a
  // frequency, in the chosen display unit. Hertz keeps the linear 0→ceiling
  // scale; semitone, mel and ERB map the floor→ceiling band through their own
  // curve so the contour's shape matches how the pitch is heard.
  function pitchFraction(hz: number): number {
    const ceiling = Math.max(1, params.pitch.ceilingHz);
    const unit = params.pitch.unit;
    if (unit === 'hertz') return hz / ceiling;
    const floor = Math.max(1, Math.min(params.pitch.floorHz, ceiling - 1));
    const lo = convertPitch(floor, unit);
    const hi = convertPitch(ceiling, unit);
    const v = convertPitch(hz, unit);
    if (lo === null || hi === null || v === null || hi <= lo) return hz / ceiling;
    return (v - lo) / (hi - lo);
  }

  function drawPitch(ctx: CanvasRenderingContext2D, width: number, height: number) {
    const times = pitch!.times;
    const f0 = pitch!.f0;
    const yFor = (hz: number) => height * (1 - pitchFraction(hz));

    const stroke = (color: string, lineWidth: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;
      ctx.lineJoin = 'round';
      ctx.lineCap = 'round';
      let drawing = false;
      ctx.beginPath();
      for (let i = 0; i < times.length; i += 1) {
        const hz = f0[i];
        const time = times[i];
        if (!Number.isFinite(hz) || time < viewport.t0 || time > viewport.t1) {
          drawing = false;
          continue;
        }
        const x = timeToX(time, width);
        const y = yFor(hz);
        if (!drawing) {
          ctx.moveTo(x, y);
          drawing = true;
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.stroke();
    };

    stroke(HALO, 5.5);
    stroke(PITCH_COLOR, 2.6);
  }

  // Frequency-ruler corner reserves the top band for its units chip; the pitch
  // ceiling label steps below it so the two right-edge scales never stack.
  const CORNER_CHIP_PX = 24;

  function drawPitchAxis(ctx: CanvasRenderingContext2D, width: number, height: number) {
    const ceiling = Math.max(1, params.pitch.ceilingHz);
    const unit = params.pitch.unit;
    ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'middle';
    // Hertz spans 0→ceiling; a perceptual unit spans its floor→ceiling band, so
    // its bottom tick sits on the floor and a geometric midpoint reads evenly on
    // the log-like scales. The ceiling tick carries the unit suffix (in hertz the
    // ruler's corner chip already names it), so no label is ambiguous.
    let ticks: number[];
    if (unit === 'hertz') {
      ticks = [0, ceiling / 2, ceiling];
    } else {
      const floor = Math.max(1, Math.min(params.pitch.floorHz, ceiling - 1));
      ticks = [floor, Math.sqrt(floor * ceiling), ceiling];
    }
    for (const hz of ticks) {
      const y = Math.min(height - 7, Math.max(CORNER_CHIP_PX, height * (1 - pitchFraction(hz))));
      const shown = unit === 'hertz' ? hz : (convertPitch(hz, unit) ?? 0);
      const isTop = hz === ticks[ticks.length - 1];
      const label =
        unit === 'hertz' || !isTop ? `${Math.round(shown)}` : `${Math.round(shown)} ${pitchUnitSuffix(unit)}`;
      ctx.lineWidth = 3;
      ctx.strokeStyle = HALO;
      ctx.strokeText(label, width - 4, y);
      ctx.fillStyle = PITCH_COLOR;
      ctx.fillText(label, width - 4, y);
    }
  }

  function drawIntensity(ctx: CanvasRenderingContext2D, width: number, height: number) {
    const times = intensity!.times;
    const db = intensity!.db;
    let min = Infinity;
    let max = -Infinity;
    for (let i = 0; i < db.length; i += 1) {
      if (!Number.isFinite(db[i])) continue;
      if (db[i] < min) min = db[i];
      if (db[i] > max) max = db[i];
    }
    if (!Number.isFinite(min) || max - min < 1e-6) return;
    // Keep the contour inside a lower band so it does not fight the pitch line.
    const top = height * 0.12;
    const bottom = height * 0.94;
    const yFor = (value: number) => bottom - ((value - min) / (max - min)) * (bottom - top);

    const stroke = (color: string, lineWidth: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;
      ctx.lineJoin = 'round';
      ctx.lineCap = 'round';
      let drawing = false;
      ctx.beginPath();
      for (let i = 0; i < times.length; i += 1) {
        const time = times[i];
        if (time < viewport.t0 || time > viewport.t1 || !Number.isFinite(db[i])) {
          drawing = false;
          continue;
        }
        const x = timeToX(time, width);
        const y = yFor(db[i]);
        if (!drawing) {
          ctx.moveTo(x, y);
          drawing = true;
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.stroke();
    };

    stroke(HALO, 3.2);
    stroke(INTENSITY_COLOR, 1.4);
  }

  function drawHarmonicity(ctx: CanvasRenderingContext2D, width: number, height: number) {
    const times = harmonicity!.times;
    const hnr = harmonicity!.hnr;
    let min = Infinity;
    let max = -Infinity;
    for (let i = 0; i < hnr.length; i += 1) {
      if (!Number.isFinite(hnr[i])) continue;
      if (hnr[i] < min) min = hnr[i];
      if (hnr[i] > max) max = hnr[i];
    }
    if (!Number.isFinite(min) || max - min < 1e-6) return;
    const top = height * 0.1;
    const bottom = height * 0.9;
    const yFor = (value: number) => bottom - ((value - min) / (max - min)) * (bottom - top);

    const stroke = (color: string, lineWidth: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;
      ctx.lineJoin = 'round';
      ctx.lineCap = 'round';
      let drawing = false;
      ctx.beginPath();
      for (let i = 0; i < times.length; i += 1) {
        const time = times[i];
        if (time < viewport.t0 || time > viewport.t1 || !Number.isFinite(hnr[i])) {
          drawing = false;
          continue;
        }
        const x = timeToX(time, width);
        const y = yFor(hnr[i]);
        if (!drawing) {
          ctx.moveTo(x, y);
          drawing = true;
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.stroke();
    };

    stroke(HALO, 3.2);
    stroke(HARMONICITY_COLOR, 1.4);

    // A right-edge label names the auto-scaled band, since HNR shares the pane.
    ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'top';
    const label = `${Math.round(max)} dB HNR`;
    ctx.lineWidth = 3;
    ctx.strokeStyle = HALO;
    ctx.strokeText(label, width - 4, top);
    ctx.fillStyle = HARMONICITY_COLOR;
    ctx.fillText(label, width - 4, top);
  }

  function drawCpp(ctx: CanvasRenderingContext2D, width: number, height: number) {
    const times = cpp!.times;
    const values = cpp!.cpp;
    let min = Infinity;
    let max = -Infinity;
    for (let i = 0; i < values.length; i += 1) {
      if (!Number.isFinite(values[i])) continue;
      if (values[i] < min) min = values[i];
      if (values[i] > max) max = values[i];
    }
    if (!Number.isFinite(min) || max - min < 1e-6) return;
    const top = height * 0.1;
    const bottom = height * 0.9;
    const yFor = (value: number) => bottom - ((value - min) / (max - min)) * (bottom - top);

    const stroke = (color: string, lineWidth: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = lineWidth;
      ctx.lineJoin = 'round';
      ctx.lineCap = 'round';
      let drawing = false;
      ctx.beginPath();
      for (let i = 0; i < times.length; i += 1) {
        const time = times[i];
        if (time < viewport.t0 || time > viewport.t1 || !Number.isFinite(values[i])) {
          drawing = false;
          continue;
        }
        const x = timeToX(time, width);
        const y = yFor(values[i]);
        if (!drawing) {
          ctx.moveTo(x, y);
          drawing = true;
        } else {
          ctx.lineTo(x, y);
        }
      }
      ctx.stroke();
    };

    stroke(HALO, 3.2);
    stroke(CPP_COLOR, 1.4);

    // The label sits a line below the HNR band's so both read when shown together.
    ctx.font = '11px ui-sans-serif, system-ui, sans-serif';
    ctx.textAlign = 'right';
    ctx.textBaseline = 'top';
    const label = `${Math.round(max)} dB CPP`;
    const labelY = params.harmonicity.show ? top + 15 : top;
    ctx.lineWidth = 3;
    ctx.strokeStyle = HALO;
    ctx.strokeText(label, width - 4, labelY);
    ctx.fillStyle = CPP_COLOR;
    ctx.fillText(label, width - 4, labelY);
  }
</script>

<canvas
  bind:this={canvas}
  class="overlay"
  data-testid="track-overlay"
  data-overlay-token={renderToken}
  data-pitch-data-token={pitchDataToken}
  data-pitch-max={pitchMaxHz.toFixed(1)}
  data-formant-max={(formant?.maxHz ?? 0).toFixed(1)}
  aria-hidden="true"
></canvas>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
    z-index: 1;
  }
</style>
