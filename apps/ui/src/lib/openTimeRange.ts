export interface OpenTimeRange {
  lo: number;
  hi: number;
}

/** Keeps a time strictly inside a range while retaining a small screen-space gap. */
export function clampToOpenTimeRange(
  time: number,
  range: OpenTimeRange,
  preferredMargin: number
): number {
  const width = range.hi - range.lo;
  if (!(width > 0) || !Number.isFinite(time)) return time;

  // Short neighboring intervals can be narrower than the normal visual
  // margin. Capping it at a quarter of their combined width keeps both sides
  // representable and prevents the lower and upper clamps from crossing.
  const margin = Math.min(Math.max(0, preferredMargin), width / 4);
  const lower = range.lo + margin;
  const upper = range.hi - margin;
  return Math.min(upper, Math.max(lower, time));
}
