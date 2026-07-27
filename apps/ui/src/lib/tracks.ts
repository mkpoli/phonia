import type { FormantTrackData, IntensityTrackData, PitchTrackData } from './types';

/** The overlay tracks a pane has fetched for drawing, shared for readouts. */
export interface OverlayTracks {
  pitch: PitchTrackData | null;
  formant: FormantTrackData | null;
  intensity: IntensityTrackData | null;
}

/** Values the drawn overlay tracks attest at one instant. */
export interface TrackSample {
  f0Hz: number | null;
  /** Candidates at the nearest frame, lowest first (read as F1, F2, …). */
  formantsHz: number[];
  intensityDb: number | null;
}

export function emptyOverlayTracks(): OverlayTracks {
  return { pitch: null, formant: null, intensity: null };
}

/** Index of the value in ascending `times` closest to `t`. */
function nearestIndex(times: Float64Array, t: number): number {
  let lo = 0;
  let hi = times.length - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (times[mid] < t) lo = mid + 1;
    else hi = mid;
  }
  if (lo > 0 && Math.abs(times[lo - 1] - t) < Math.abs(times[lo] - t)) return lo - 1;
  return lo;
}

function samplePitch(track: PitchTrackData | null, t: number, tolerance: number): number | null {
  if (!track || track.times.length === 0) return null;
  const i = nearestIndex(track.times, t);
  if (Math.abs(track.times[i] - t) > tolerance) return null;
  const f0 = track.f0[i];
  return Number.isFinite(f0) && f0 > 0 ? f0 : null;
}

function sampleIntensity(
  track: IntensityTrackData | null,
  t: number,
  tolerance: number
): number | null {
  if (!track || track.times.length === 0) return null;
  const i = nearestIndex(track.times, t);
  if (Math.abs(track.times[i] - t) > tolerance) return null;
  const db = track.db[i];
  return Number.isFinite(db) ? db : null;
}

/**
 * Candidates of the formant frame nearest `t`, lowest frequency first. The
 * flat `[time, frequency, bandwidth]` triples are frame-ordered, so the
 * nearest frame is found by time and its members collected by equal time.
 */
function sampleFormants(
  track: FormantTrackData | null,
  t: number,
  tolerance: number,
  limit: number
): number[] {
  if (!track || track.points.length < 3) return [];
  const points = track.points;
  let bestTime = Number.NaN;
  let bestDist = Number.POSITIVE_INFINITY;
  for (let i = 0; i < points.length; i += 3) {
    const dist = Math.abs(points[i] - t);
    if (dist < bestDist) {
      bestDist = dist;
      bestTime = points[i];
    }
  }
  if (!(bestDist <= tolerance)) return [];
  const out: number[] = [];
  for (let i = 0; i < points.length; i += 3) {
    if (points[i] === bestTime) out.push(points[i + 1]);
  }
  out.sort((a, b) => a - b);
  return out.slice(0, limit);
}

/** Reads the overlay tracks at time `t`, absent values as null/empty. */
export function sampleTracks(tracks: OverlayTracks, t: number): TrackSample {
  return {
    f0Hz: samplePitch(tracks.pitch, t, 0.02),
    formantsHz: sampleFormants(tracks.formant, t, 0.02, 3),
    intensityDb: sampleIntensity(tracks.intensity, t, 0.05)
  };
}
