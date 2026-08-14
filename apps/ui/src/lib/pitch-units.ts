/** Display unit for a fundamental-frequency value. */
export type PitchUnit = 'hertz' | 'semitones' | 'mel' | 'erb';

export const PITCH_UNITS: { value: PitchUnit; label: string }[] = [
  { value: 'hertz', label: 'Hertz' },
  { value: 'semitones', label: 'Semitones (re 100 Hz)' },
  { value: 'mel', label: 'Mel' },
  { value: 'erb', label: 'ERB' }
];

const SUFFIX: Record<PitchUnit, string> = {
  hertz: 'Hz',
  semitones: 'st',
  mel: 'mel',
  erb: 'ERB'
};

/**
 * Converts a hertz value to `unit`, using Praat's own scale definitions:
 * semitones are `12·log2(f/100)` (re 100 Hz), mel is `550·ln(1 + f/550)`, and
 * ERB is the Moore–Glasberg (1983) rate `11.17·ln((f+312)/(f+14680)) + 43`.
 * Returns null for a non-positive or non-finite frequency.
 */
export function convertPitch(hz: number | null | undefined, unit: PitchUnit): number | null {
  if (hz === null || hz === undefined || !Number.isFinite(hz) || hz <= 0) return null;
  switch (unit) {
    case 'hertz':
      return hz;
    case 'semitones':
      return 12 * Math.log2(hz / 100);
    case 'mel':
      return 550 * Math.log(1 + hz / 550);
    case 'erb':
      return 11.17 * Math.log((hz + 312) / (hz + 14680)) + 43;
  }
}

/** A hertz value formatted in `unit`, e.g. `120 Hz`, `4.5 st`, `12.3 ERB`. */
export function formatPitch(hz: number | null | undefined, unit: PitchUnit, digits = 1): string {
  const value = convertPitch(hz, unit);
  if (value === null) return '—';
  return `${value.toFixed(digits)} ${SUFFIX[unit]}`;
}

export function pitchUnitSuffix(unit: PitchUnit): string {
  return SUFFIX[unit];
}
