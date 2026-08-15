/// A pitch candidate for one analysis frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PitchCandidate {
    /// Candidate frequency in hertz; `0.0` marks the explicit unvoiced candidate.
    pub frequency: f64,
    /// Candidate strength `R`: Boersma eq. 24 for voiced candidates and eq. 23
    /// for the unvoiced candidate.
    pub strength: f64,
}

/// One frame of a pitch track.
#[derive(Debug, Clone, PartialEq)]
pub struct PitchFrame {
    /// Frame centre time in seconds.
    pub time: f64,
    /// Selected fundamental frequency in hertz, or `None` when the path is unvoiced.
    pub f0: Option<f64>,
    /// Strength stored on the selected candidate.
    pub strength: f64,
    /// All candidates generated for the frame, including the unvoiced candidate.
    pub candidates: Vec<PitchCandidate>,
}

/// A closed time interval `[start, end]` in seconds.
///
/// This crate-local type scopes pitch statistics until the workspace gains a
/// shared time-domain interval type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeSpan {
    /// Inclusive start time in seconds.
    pub start: f64,
    /// Inclusive end time in seconds.
    pub end: f64,
}

impl TimeSpan {
    /// Creates a closed interval `[start, end]` in seconds.
    ///
    /// # Panics
    ///
    /// Panics if either bound is non-finite or if `start > end`.
    #[must_use]
    pub fn new(start: f64, end: f64) -> Self {
        assert!(
            start.is_finite() && end.is_finite(),
            "TimeSpan bounds must be finite"
        );
        assert!(start <= end, "TimeSpan start must be <= end");
        Self { start, end }
    }

    /// Returns whether `t` lies inside the closed interval.
    #[must_use]
    pub fn contains(&self, t: f64) -> bool {
        t >= self.start && t <= self.end
    }
}

/// Pitch-analysis result across a frame grid.
#[derive(Debug, Clone, PartialEq)]
pub struct PitchTrack {
    frames: Vec<PitchFrame>,
}

impl PitchTrack {
    pub(crate) fn new(frames: Vec<PitchFrame>) -> Self {
        Self { frames }
    }

    /// Returns all frames on the analysis grid.
    #[must_use]
    pub fn frames(&self) -> &[PitchFrame] {
        &self.frames
    }

    /// Mean selected voiced frequency in hertz over `span`.
    #[must_use]
    pub fn mean_hz(&self, span: TimeSpan) -> Option<f64> {
        let values = self.voiced_hz(span);
        mean(&values)
    }

    /// Fraction of frames in `span` whose path is unvoiced — Praat's "Fraction
    /// of locally unvoiced frames". `None` when the span holds no frames.
    #[must_use]
    pub fn unvoiced_fraction(&self, span: TimeSpan) -> Option<f64> {
        let mut total = 0usize;
        let mut unvoiced = 0usize;
        for frame in &self.frames {
            if span.contains(frame.time) {
                total += 1;
                if frame.f0.is_none() {
                    unvoiced += 1;
                }
            }
        }
        (total > 0).then(|| unvoiced as f64 / total as f64)
    }

    /// Median selected voiced frequency in hertz over `span`.
    #[must_use]
    pub fn median_hz(&self, span: TimeSpan) -> Option<f64> {
        let mut values = self.voiced_hz(span);
        median(&mut values)
    }

    /// The `q`-quantile (`0..=1`) of the voiced fundamentals in hertz over
    /// `span`, by linear interpolation between order statistics — Praat's "Get
    /// quantile". `q = 0.5` is the median; `0.05` and `0.95` bound the contour
    /// against the octave errors the extrema catch.
    #[must_use]
    pub fn quantile_hz(&self, span: TimeSpan, q: f64) -> Option<f64> {
        let mut values = self.voiced_hz(span);
        quantile(&mut values, q)
    }

    /// Minimum selected voiced frequency in hertz over `span`.
    #[must_use]
    pub fn min_hz(&self, span: TimeSpan) -> Option<f64> {
        let values = self.voiced_hz(span);
        min(&values)
    }

    /// Maximum selected voiced frequency in hertz over `span`.
    #[must_use]
    pub fn max_hz(&self, span: TimeSpan) -> Option<f64> {
        let values = self.voiced_hz(span);
        max(&values)
    }

    /// Sample standard deviation of the voiced frequencies in hertz over `span`,
    /// absent when fewer than two voiced frames fall inside it.
    #[must_use]
    pub fn sd_hz(&self, span: TimeSpan) -> Option<f64> {
        std_dev(&self.voiced_hz(span))
    }

    /// Mean selected voiced frequency in semitones re 1 Hz over `span`.
    #[must_use]
    pub fn mean_semitones(&self, span: TimeSpan) -> Option<f64> {
        let values = self.voiced_semitones(span);
        mean(&values)
    }

    /// Median selected voiced frequency in semitones re 1 Hz over `span`.
    #[must_use]
    pub fn median_semitones(&self, span: TimeSpan) -> Option<f64> {
        let mut values = self.voiced_semitones(span);
        median(&mut values)
    }

    /// Minimum selected voiced frequency in semitones re 1 Hz over `span`.
    #[must_use]
    pub fn min_semitones(&self, span: TimeSpan) -> Option<f64> {
        let values = self.voiced_semitones(span);
        min(&values)
    }

    /// Maximum selected voiced frequency in semitones re 1 Hz over `span`.
    #[must_use]
    pub fn max_semitones(&self, span: TimeSpan) -> Option<f64> {
        let values = self.voiced_semitones(span);
        max(&values)
    }

    fn voiced_hz(&self, span: TimeSpan) -> Vec<f64> {
        self.frames
            .iter()
            .filter(|frame| span.contains(frame.time))
            .filter_map(|frame| frame.f0)
            .collect()
    }

    fn voiced_semitones(&self, span: TimeSpan) -> Vec<f64> {
        self.voiced_hz(span)
            .into_iter()
            .map(hz_to_semitones)
            .collect()
    }
}

/// Converts hertz to semitones re 1 Hz, the primitive Praat Hertz-to-semitone
/// reference used by this crate.
#[must_use]
pub fn hz_to_semitones(f0_hz: f64) -> f64 {
    12.0 * f0_hz.log2()
}

fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn std_dev(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    Some(variance.sqrt())
}

fn median(values: &mut [f64]) -> Option<f64> {
    quantile(values, 0.5)
}

fn quantile(values: &mut [f64], q: f64) -> Option<f64> {
    if values.is_empty() || !q.is_finite() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    if values.len() == 1 {
        return Some(values[0]);
    }
    let h = (values.len() - 1) as f64 * q.clamp(0.0, 1.0);
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(values.len() - 1);
    Some(values[lo] + (h - lo as f64) * (values[hi] - values[lo]))
}

fn min(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(f64::min)
}

fn max(values: &[f64]) -> Option<f64> {
    values.iter().copied().reduce(f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp_track(values: &[f64]) -> PitchTrack {
        let frames = values
            .iter()
            .enumerate()
            .map(|(i, &hz)| PitchFrame {
                time: i as f64 * 0.01,
                f0: Some(hz),
                strength: 0.9,
                candidates: Vec::new(),
            })
            .collect();
        PitchTrack::new(frames)
    }

    #[test]
    fn quantile_hz_interpolates_the_order_statistics() {
        // F0 ramps 100..=200 Hz over 101 frames, so the p-quantile is 100 + 100p.
        let values: Vec<f64> = (0..=100).map(|i| 100.0 + i as f64).collect();
        let track = ramp_track(&values);
        let span = TimeSpan::new(0.0, 100.0);
        let p5 = track.quantile_hz(span, 0.05).unwrap();
        let p50 = track.quantile_hz(span, 0.5).unwrap();
        let p95 = track.quantile_hz(span, 0.95).unwrap();
        assert!((p5 - 105.0).abs() < 1e-9, "p5 {p5}");
        assert!((p50 - 150.0).abs() < 1e-9, "p50 {p50}");
        assert!((p95 - 195.0).abs() < 1e-9, "p95 {p95}");
        // The 50% quantile agrees with the median exactly.
        assert_eq!(track.median_hz(span), Some(p50));
    }

    #[test]
    fn quantile_hz_is_none_without_voiced_frames() {
        let track = ramp_track(&[]);
        assert_eq!(track.quantile_hz(TimeSpan::new(0.0, 1.0), 0.5), None);
    }

    #[test]
    fn unvoiced_fraction_counts_unvoiced_frames_in_span() {
        // Ten frames at 0.00..0.09 s: three unvoiced (f0 = None).
        let frames = (0..10)
            .map(|i| PitchFrame {
                time: i as f64 * 0.01,
                f0: if i % 4 == 0 { None } else { Some(150.0) },
                strength: 0.9,
                candidates: Vec::new(),
            })
            .collect();
        let track = PitchTrack::new(frames);
        // Frames at i = 0, 4, 8 are unvoiced → 3 of 10.
        let all = track.unvoiced_fraction(TimeSpan::new(0.0, 0.09)).unwrap();
        assert!((all - 0.3).abs() < 1e-9, "fraction {all}");
        // Restricting the span narrows the denominator: frames 0..=4 → 2 of 5.
        let head = track.unvoiced_fraction(TimeSpan::new(0.0, 0.04)).unwrap();
        assert!((head - 0.4).abs() < 1e-9, "fraction {head}");
        // A span with no frames yields None.
        assert_eq!(track.unvoiced_fraction(TimeSpan::new(1.0, 2.0)), None);
    }
}
