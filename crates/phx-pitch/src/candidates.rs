//! Per-frame pitch candidates (Boersma 1993 §1.3 and §4).

use phx_dsp::{sinc_interpolate, sinc_interpolate_max};

use crate::analysis::{FrameAnalyzer, Layout};
use crate::params::PitchParams;
use crate::types::PitchCandidate;

/// Interpolation depth (samples per side) for the sinc evaluation that scores a
/// freshly found maximum before it competes for a place in the list.
const STRENGTH_SINC_DEPTH: usize = 30;

/// Candidates of one frame before the path finder chooses among them.
#[derive(Debug, Clone)]
pub(crate) struct FrameCandidates {
    pub(crate) time: f64,
    /// The unvoiced candidate first, then the retained voiced candidates in
    /// no particular order, each carrying its raw correlation as strength. A
    /// maximum above the pitch ceiling is retained like any other and scored
    /// as unvoiced by the path finder.
    pub(crate) candidates: Vec<PitchCandidate>,
}

/// Finds the candidates of every frame of one signal.
pub(crate) struct CandidateFinder<'a> {
    analyzer: FrameAnalyzer,
    params: &'a PitchParams,
    global_peak: f64,
    /// `r_x` over lags `−max_lag..=max_lag`, mirrored around the middle so a
    /// sinc window around a short lag sees the even extension of the
    /// autocorrelation instead of an edge.
    mirrored: Vec<f64>,
    r: Vec<f64>,
    lags: Vec<usize>,
}

impl<'a> CandidateFinder<'a> {
    pub(crate) fn new(layout: Layout, params: &'a PitchParams, global_peak: f64) -> Self {
        Self {
            analyzer: FrameAnalyzer::new(layout),
            params,
            global_peak,
            mirrored: vec![0.0; 2 * layout.max_lag + 1],
            r: vec![0.0; layout.max_lag + 1],
            lags: Vec::with_capacity(layout.max_candidates),
        }
    }

    pub(crate) fn layout(&self) -> Layout {
        self.analyzer.layout
    }

    /// Candidates of the frame centred at `time` (§4 steps 3–5).
    ///
    /// Every local maximum of `r_x` stronger than half the voicing threshold
    /// becomes a candidate: its lag is first placed by parabolic interpolation
    /// and scored by one sinc evaluation there, and the strongest
    /// `max_candidates − 1` of them (eq. 24 decides who is weakest) survive.
    /// Each survivor is then refined to sub-sample precision by maximising the
    /// sinc-interpolated autocorrelation around its lag. Strengths above one,
    /// which short windows produce, are reflected to `1/r`.
    pub(crate) fn frame(&mut self, signal: &[f64], time: f64) -> FrameCandidates {
        let layout = self.analyzer.layout;
        let params = self.params;
        let intensity = self
            .analyzer
            .correlate(signal, self.global_peak, time, &mut self.r);
        let mut candidates = vec![PitchCandidate {
            frequency: 0.0,
            strength: unvoiced_strength(params, intensity),
        }];
        self.lags.clear();
        self.lags.push(0);
        if intensity == 0.0 || layout.max_candidates < 2 {
            return FrameCandidates { time, candidates };
        }

        let max_lag = layout.max_lag;
        for lag in 0..=max_lag {
            self.mirrored[max_lag + lag] = self.r[lag];
            self.mirrored[max_lag - lag] = self.r[lag];
        }
        let r = &self.r;
        let mirrored = &self.mirrored;
        let voicing_gate = 0.5 * params.voicing_threshold;

        for lag in 2..layout.maximum_lag.min(max_lag) {
            if !(r[lag] > voicing_gate && r[lag] > r[lag - 1] && r[lag] >= r[lag + 1]) {
                continue;
            }
            let dr = 0.5 * (r[lag + 1] - r[lag - 1]);
            let d2r = 2.0 * r[lag] - r[lag - 1] - r[lag + 1];
            // A strict maximum on the left keeps `d2r` positive and the
            // parabola's vertex within half a sample of `lag`.
            debug_assert!(d2r > 0.0);
            let peak_lag = lag as f64 + dr / d2r;
            let frequency = layout.sample_rate / peak_lag;
            let strength = reflect(sinc_interpolate(
                mirrored,
                max_lag as f64 + peak_lag,
                STRENGTH_SINC_DEPTH,
            ));

            let place = if candidates.len() < layout.max_candidates {
                candidates.push(PitchCandidate {
                    frequency: 0.0,
                    strength: 0.0,
                });
                self.lags.push(0);
                Some(candidates.len() - 1)
            } else {
                // Replace the weakest voiced candidate, ranking by eq. 24 so a
                // higher-frequency maximum of equal correlation wins the slot.
                let mut weakest = f64::INFINITY;
                let mut place = None;
                for (index, candidate) in candidates.iter().enumerate().skip(1) {
                    let local = voiced_strength(params, candidate.strength, candidate.frequency);
                    if local < weakest {
                        weakest = local;
                        place = Some(index);
                    }
                }
                if voiced_strength(params, strength, frequency) <= weakest {
                    None
                } else {
                    place
                }
            };
            if let Some(place) = place {
                candidates[place] = PitchCandidate {
                    frequency,
                    strength,
                };
                self.lags[place] = lag;
            }
        }

        for (candidate, &lag) in candidates.iter_mut().zip(&self.lags).skip(1) {
            let (position, value) =
                sinc_interpolate_max(mirrored, max_lag + lag, layout.refine_depth);
            let refined_lag = position - max_lag as f64;
            if refined_lag > 0.0 && value.is_finite() {
                candidate.frequency = layout.sample_rate / refined_lag;
                candidate.strength = reflect(value);
            }
        }

        FrameCandidates { time, candidates }
    }
}

/// Correlations above one arise from short windows; fold them back below one.
fn reflect(strength: f64) -> f64 {
    if strength > 1.0 {
        1.0 / strength
    } else {
        strength
    }
}

/// Strength of the unvoiced candidate (eq. 23) from the frame's local peak
/// relative to the global peak.
pub(crate) fn unvoiced_strength(params: &PitchParams, intensity: f64) -> f64 {
    let level = if params.silence_threshold > 0.0 {
        2.0 - intensity * (1.0 + params.voicing_threshold) / params.silence_threshold
    } else {
        0.0
    };
    params.voicing_threshold + level.max(0.0)
}

/// Local strength of a voiced candidate (eq. 24): its correlation less the
/// octave cost, measured against the pitch floor. Used only to rank
/// candidates within one frame, where the anchor cancels; the path finder
/// anchors the octave cost at the ceiling.
pub(crate) fn voiced_strength(params: &PitchParams, correlation: f64, frequency_hz: f64) -> f64 {
    correlation - params.octave_cost * (params.floor_hz / frequency_hz).log2()
}
