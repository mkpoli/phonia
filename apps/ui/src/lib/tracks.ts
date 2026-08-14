import type {
  CppTrackData,
  FormantTrackData,
  HarmonicityTrackData,
  IntensityTrackData,
  PitchTrackData
} from './types';

/** The overlay tracks a pane has fetched for drawing, shared for readouts. */
export interface OverlayTracks {
  pitch: PitchTrackData | null;
  formant: FormantTrackData | null;
  intensity: IntensityTrackData | null;
  harmonicity: HarmonicityTrackData | null;
  cpp: CppTrackData | null;
}

/** One formant reading: centre frequency and its bandwidth, in hertz. */
export interface FormantReading {
  frequencyHz: number;
  bandwidthHz: number;
}

/** Values the drawn overlay tracks attest at one instant. */
export interface TrackSample {
  /** F0 linearly interpolated between the bracketing voiced frames. */
  f0Hz: number | null;
  /** Formants interpolated at the cursor, lowest first (read as F1, F2, …),
   *  each with the bandwidth Burg's roots give its pole. */
  formants: FormantReading[];
  /** Centre frequencies only, for consumers that ignore bandwidth. */
  formantsHz: number[];
  /** Intensity linearly interpolated between the bracketing frames. */
  intensityDb: number | null;
  /** Harmonics-to-noise ratio at the nearest voiced frame, in dB. */
  hnrDb: number | null;
  /** Cepstral peak prominence at the nearest frame, in dB. */
  cppDb: number | null;
}

export function emptyOverlayTracks(): OverlayTracks {
  return { pitch: null, formant: null, intensity: null, harmonicity: null, cpp: null };
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

/**
 * The two frame indices bracketing `t` and the fraction of the way from the
 * first to the second. Before the first frame or after the last, both indices
 * collapse to the edge frame (fraction 0), leaving edge reads to nearest-frame.
 */
function bracket(times: Float64Array, t: number): [number, number, number] {
  const n = times.length;
  if (n === 0 || t <= times[0]) return [0, 0, 0];
  if (t >= times[n - 1]) return [n - 1, n - 1, 0];
  let lo = 0;
  let hi = n - 1;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (times[mid] < t) lo = mid + 1;
    else hi = mid;
  }
  const i0 = lo - 1;
  const span = times[lo] - times[i0];
  return [i0, lo, span > 0 ? (t - times[i0]) / span : 0];
}

// Praat's "Get value at time … linear": a straight-line blend of the two
// bracketing frames. Pitch stays undefined across a voicing boundary, so it
// interpolates only between two voiced frames and otherwise reads the nearest.
function samplePitch(track: PitchTrackData | null, t: number, tolerance: number): number | null {
  if (!track || track.times.length === 0) return null;
  const [i0, i1, frac] = bracket(track.times, t);
  const a = track.f0[i0];
  const b = track.f0[i1];
  const aOk = Number.isFinite(a) && a > 0;
  const bOk = Number.isFinite(b) && b > 0;
  if (i0 !== i1 && aOk && bOk) return a + (b - a) * frac;
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
  const [i0, i1, frac] = bracket(track.times, t);
  const a = track.db[i0];
  const b = track.db[i1];
  if (i0 !== i1 && Number.isFinite(a) && Number.isFinite(b)) return a + (b - a) * frac;
  const i = nearestIndex(track.times, t);
  if (Math.abs(track.times[i] - t) > tolerance) return null;
  const db = track.db[i];
  return Number.isFinite(db) ? db : null;
}

// The HNR at the nearest frame within tolerance. Undefined frames carry NaN and
// read as absent, so the value is not interpolated across a voicing gap.
function sampleHarmonicity(
  track: HarmonicityTrackData | null,
  t: number,
  tolerance: number
): number | null {
  if (!track || track.times.length === 0) return null;
  const i = nearestIndex(track.times, t);
  if (Math.abs(track.times[i] - t) > tolerance) return null;
  const hnr = track.hnr[i];
  return Number.isFinite(hnr) ? hnr : null;
}

// CPP at the nearest frame within tolerance. Undefined frames carry NaN and read
// as absent.
function sampleCpp(track: CppTrackData | null, t: number, tolerance: number): number | null {
  if (!track || track.times.length === 0) return null;
  const i = nearestIndex(track.times, t);
  if (Math.abs(track.times[i] - t) > tolerance) return null;
  const cpp = track.cpp[i];
  return Number.isFinite(cpp) ? cpp : null;
}

/** Frequency-sorted formant candidates recorded at exactly `frameTime`. */
function candidatesAt(points: Float64Array, frameTime: number, limit: number): FormantReading[] {
  const out: FormantReading[] = [];
  for (let i = 0; i < points.length; i += 3) {
    if (points[i] === frameTime) out.push({ frequencyHz: points[i + 1], bandwidthHz: points[i + 2] });
  }
  out.sort((a, b) => a.frequencyHz - b.frequencyHz);
  return out.slice(0, limit);
}

/**
 * Formants at `t`, lowest frequency first, matching Praat's Formant "Get value
 * at time … linear": the two bracketing frames' frequency-sorted candidates are
 * paired by rank (F1↔F1, F2↔F2, …) and each pair's frequency and bandwidth
 * blended. Edges, or a bracket frame with no candidates, read the nearest frame.
 * The flat `[time, frequency, bandwidth]` triples are frame-ordered, so a frame
 * is its run of triples that share one time.
 */
function sampleFormants(
  track: FormantTrackData | null,
  t: number,
  tolerance: number,
  limit: number
): FormantReading[] {
  if (!track || track.points.length < 3) return [];
  const points = track.points;
  const times: number[] = [];
  let last = Number.NaN;
  for (let i = 0; i < points.length; i += 3) {
    if (points[i] !== last) {
      times.push(points[i]);
      last = points[i];
    }
  }
  const stamps = Float64Array.from(times);
  const [i0, i1, frac] = bracket(stamps, t);
  const a = candidatesAt(points, times[i0], limit);
  const b = candidatesAt(points, times[i1], limit);
  if (i0 !== i1 && a.length > 0 && b.length > 0) {
    const n = Math.min(a.length, b.length);
    const out: FormantReading[] = [];
    for (let k = 0; k < n; k += 1) {
      out.push({
        frequencyHz: a[k].frequencyHz + (b[k].frequencyHz - a[k].frequencyHz) * frac,
        bandwidthHz: a[k].bandwidthHz + (b[k].bandwidthHz - a[k].bandwidthHz) * frac
      });
    }
    return out;
  }
  const near = nearestIndex(stamps, t);
  if (Math.abs(times[near] - t) > tolerance) return [];
  return candidatesAt(points, times[near], limit);
}

/** Reads the overlay tracks at time `t`, absent values as null/empty. */
export function sampleTracks(tracks: OverlayTracks, t: number): TrackSample {
  const formants = sampleFormants(tracks.formant, t, 0.02, 4);
  return {
    f0Hz: samplePitch(tracks.pitch, t, 0.02),
    formants,
    formantsHz: formants.map((f) => f.frequencyHz),
    intensityDb: sampleIntensity(tracks.intensity, t, 0.05),
    hnrDb: sampleHarmonicity(tracks.harmonicity, t, 0.05),
    cppDb: sampleCpp(tracks.cpp, t, 0.05)
  };
}
