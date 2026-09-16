//! Frame layout, analysis window, and the per-frame correlation: the
//! window-corrected autocorrelation of Boersma 1993 §1–§3, or the normalised
//! forward cross-correlation of Praat's "To Pitch (cc)".

use std::f64::consts::PI;

use phx_dsp::{RealFftPlan, next_pow2};

use crate::params::PitchParams;

/// Window autocorrelation values below this are treated as zero; with the
/// window's own autocorrelation the kept lags never come close.
const WINDOW_ACF_EPSILON: f64 = 1e-10;

/// How one frame is turned into a correlation over lag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Method {
    /// Window-corrected autocorrelation (Boersma 1993): Hanning window over
    /// three pitch-floor periods, or the Gaussian window over six when the
    /// parameters ask for the very-accurate setting.
    Autocorrelation,
    /// Normalised forward cross-correlation of an unwindowed frame against
    /// the samples that follow it, over `periods_per_window` periods.
    CrossCorrelation { periods_per_window: f64 },
}

/// Sizes derived from the parameters and the sampling rate; everything the
/// per-frame analysis needs to know about lags and buffers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Layout {
    pub(crate) sample_rate: f64,
    pub(crate) floor_hz: f64,
    pub(crate) method: Method,
    /// Gaussian window (autocorrelation with the very-accurate setting).
    pub(crate) gaussian: bool,
    /// Window length in periods of the pitch floor: 3 for the Hanning window,
    /// 6 for the Gaussian one, the caller's choice for cross-correlation.
    pub(crate) periods_per_window: f64,
    /// Duration in seconds of the span one frame reads, before rounding to
    /// samples: the window itself, plus one pitch-floor period of look-ahead
    /// for cross-correlation. The frame grid is built on this value.
    pub(crate) window_seconds: f64,
    /// Window length in samples, forced even.
    pub(crate) window_samples: usize,
    pub(crate) half_window: usize,
    /// Samples in one period of the pitch floor.
    pub(crate) period_samples: usize,
    pub(crate) half_period: usize,
    /// One past the largest lag examined for maxima.
    pub(crate) maximum_lag: usize,
    /// FFT length: a power of two with room for the lags kept after
    /// correction, so the circular autocorrelation is the linear one there.
    pub(crate) fft_len: usize,
    /// Largest lag with a corrected autocorrelation value; also the depth
    /// bound of the sinc interpolation around a maximum.
    pub(crate) max_lag: usize,
    /// Sinc interpolation depth for refining a maximum: 70 for the Hanning
    /// window, 700 for the Gaussian one and for cross-correlation.
    pub(crate) refine_depth: usize,
    /// Pitch ceiling clamped to Nyquist.
    pub(crate) ceiling_hz: f64,
    /// Candidates per frame including the unvoiced one, raised to the
    /// ceiling-to-floor frequency ratio when the parameter is lower.
    pub(crate) max_candidates: usize,
}

impl Layout {
    /// Returns `None` when the window rounds to fewer than four samples.
    pub(crate) fn new(params: &PitchParams, sample_rate: f64, method: Method) -> Option<Self> {
        let dx = 1.0 / sample_rate;
        let refine_depth = if params.very_accurate { 700 } else { 70 };
        let (periods_per_window, interpolation_depth, look_ahead) = match method {
            Method::Autocorrelation if params.very_accurate => (6.0, 0.25, 0.0),
            Method::Autocorrelation => (3.0, 0.5, 0.0),
            Method::CrossCorrelation { periods_per_window } => {
                (periods_per_window, 1.0, 1.0 / params.floor_hz)
            }
        };
        let window_duration = periods_per_window / params.floor_hz;
        let window_seconds = window_duration + look_ahead;
        let half_window = ((window_duration / dx).floor() as usize / 2).checked_sub(1)?;
        if half_window < 2 {
            return None;
        }
        let window_samples = 2 * half_window;
        let period_samples = (sample_rate / params.floor_hz).floor() as usize;
        let maximum_lag =
            ((window_samples as f64 / periods_per_window).floor() as usize + 2).min(window_samples);
        let fft_len =
            next_pow2((window_samples as f64 * (1.0 + interpolation_depth)).ceil() as usize);
        let max_lag = (window_samples as f64 * interpolation_depth).floor() as usize;
        let ceiling_hz = params.ceiling_hz.min(0.5 * sample_rate);
        Some(Self {
            sample_rate,
            floor_hz: params.floor_hz,
            method,
            gaussian: params.very_accurate && method == Method::Autocorrelation,
            periods_per_window,
            window_seconds,
            window_samples,
            half_window,
            period_samples,
            half_period: period_samples / 2 + 1,
            maximum_lag,
            fft_len,
            max_lag,
            refine_depth,
            ceiling_hz,
            max_candidates: params
                .max_candidates
                .max((ceiling_hz / params.floor_hz).floor() as usize),
        })
    }

    /// The analysis window over `window_samples` points: Hanning (eq. 6) or,
    /// for the very-accurate setting, the Gaussian of the paper's postscript.
    /// Both are sampled so that the window's zeros fall one sample outside
    /// either end, and the Gaussian is shifted and rescaled to reach exactly
    /// zero there. Cross-correlation frames are not windowed.
    pub(crate) fn window(&self) -> Vec<f64> {
        let n = self.window_samples;
        let span = (n + 1) as f64;
        if let Method::CrossCorrelation { .. } = self.method {
            return vec![1.0; n];
        }
        if self.gaussian {
            let mid = 0.5 * span;
            let edge = (-12.0_f64).exp();
            (1..=n)
                .map(|i| {
                    let u = i as f64 - mid;
                    ((-48.0 * u * u / (span * span)).exp() - edge) / (1.0 - edge)
                })
                .collect()
        } else {
            (1..=n)
                .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / span).cos())
                .collect()
        }
    }
}

/// Index of the last sample at or before `time`, with sample `k` centred at
/// `(k + ½)·dx`: `⌊(time − dx/2) / dx⌋`, evaluated in that form.
fn low_sample_index(time: f64, sample_rate: f64) -> i64 {
    let dx = 1.0 / sample_rate;
    ((time - 0.5 * dx) / dx).floor() as i64
}

/// Reusable state for analysing frames of one signal: the window, its
/// normalised autocorrelation `r_w` (eq. 8, computed numerically so it is
/// exact for either window shape), the FFT plan, and scratch buffers.
pub(crate) struct FrameAnalyzer {
    pub(crate) layout: Layout,
    window: Vec<f64>,
    window_acf: Vec<f64>,
    plan: RealFftPlan,
    frame: Vec<f64>,
    acf: Vec<f64>,
    /// Mean-removed samples of the cross-correlation span.
    span: Vec<f64>,
}

impl FrameAnalyzer {
    pub(crate) fn new(layout: Layout) -> Self {
        let window = layout.window();
        let mut plan = RealFftPlan::new();
        let mut frame = vec![0.0; layout.fft_len];
        let mut acf = vec![0.0; layout.fft_len];
        let window_acf: Vec<f64> = match layout.method {
            Method::Autocorrelation => {
                frame[..window.len()].copy_from_slice(&window);
                plan.autocorrelate_into(&mut frame, &mut acf);
                acf[..=layout.max_lag]
                    .iter()
                    .map(|&value| value / acf[0])
                    .collect()
            }
            Method::CrossCorrelation { .. } => Vec::new(),
        };
        Self {
            layout,
            window,
            window_acf,
            plan,
            frame,
            acf,
            span: Vec::new(),
        }
    }

    /// Correlation of the frame centred at `time` over lags `0..=max_lag`,
    /// written into `r`, plus the frame's local peak relative to
    /// `global_peak` (the intensity that drives the unvoiced candidate,
    /// eq. 23).
    ///
    /// The frame's DC offset is removed with the local mean over one pitch
    /// period to either side of the centre (§4 step 3), and the local peak is
    /// read over half a period to either side of the centre of the windowed
    /// frame. For autocorrelation `r` is the window-corrected `r_x(τ)`
    /// (eq. 9); for cross-correlation it is the normalised product of the
    /// frame with the same-length span `τ` samples later, the frame itself
    /// starting half a pitch-floor period before the usual position so that
    /// the lagged spans stay centred on `time`.
    pub(crate) fn correlate(
        &mut self,
        signal: &[f64],
        global_peak: f64,
        time: f64,
        r: &mut [f64],
    ) -> f64 {
        let layout = self.layout;
        debug_assert_eq!(r.len(), layout.max_lag + 1);
        // Sample `k` sits at time `(k + ½)·dx`; `left` is the last sample at
        // or before `time`, found the way Praat forms it, `⌊(t − x₁)/dx⌋`,
        // so a frame centre that falls exactly on a sample rounds the same
        // way on both sides.
        let left = low_sample_index(time, layout.sample_rate).max(0) as isize;
        let right = left + 1;
        let n = signal.len() as isize;

        let mean_start = (right - layout.period_samples as isize).max(0);
        let mean_end = (left + layout.period_samples as isize + 1).min(n);
        let local_mean = if mean_end > mean_start {
            signal[mean_start as usize..mean_end as usize]
                .iter()
                .sum::<f64>()
                / (mean_end - mean_start) as f64
        } else {
            0.0
        };

        let start = right - layout.half_window as isize;
        self.frame.fill(0.0);
        for (j, w) in self.window.iter().enumerate() {
            let k = start + j as isize;
            if k >= 0 && k < n {
                self.frame[j] = (signal[k as usize] - local_mean) * w;
            }
        }

        let peak_start = layout.half_window.saturating_sub(layout.half_period);
        let peak_end = (layout.half_window + layout.half_period).min(layout.window_samples);
        let local_peak = self.frame[peak_start..peak_end]
            .iter()
            .fold(0.0_f64, |acc, &v| acc.max(v.abs()));
        let intensity = if global_peak > 0.0 {
            (local_peak / global_peak).min(1.0)
        } else {
            0.0
        };

        r[0] = 1.0;
        if local_peak == 0.0 {
            r[1..].fill(0.0);
            return intensity;
        }
        if let Method::CrossCorrelation { .. } = layout.method {
            self.cross_correlate(signal, local_mean, time, r);
            return intensity;
        }
        self.plan.autocorrelate_into(&mut self.frame, &mut self.acf);
        let zero = self.acf[0];
        for ((value, &raw), &rw) in r[1..=layout.max_lag]
            .iter_mut()
            .zip(&self.acf[1..])
            .zip(&self.window_acf[1..])
        {
            *value = if zero > 0.0 && rw.abs() > WINDOW_ACF_EPSILON {
                raw / (zero * rw)
            } else {
                0.0
            };
        }
        intensity
    }

    /// Normalised forward cross-correlation for lags `1..maximum_lag`, with
    /// lags beyond that left at zero. The reference span starts at
    /// `time − (period + window)/2` and the running sum of squares of the
    /// lagged span is updated one sample at a time.
    fn cross_correlate(&mut self, signal: &[f64], local_mean: f64, time: f64, r: &mut [f64]) {
        let layout = self.layout;
        let n = layout.window_samples;
        let start_time = time - 0.5 * (1.0 + layout.periods_per_window) / layout.floor_hz;
        let start = low_sample_index(start_time, layout.sample_rate).max(0) as usize;
        let available = signal.len().saturating_sub(start);
        let span = (layout.maximum_lag + n).min(available);
        let local_maximum_lag = span.saturating_sub(n);
        r[1..].fill(0.0);
        if local_maximum_lag == 0 {
            return;
        }
        self.span.clear();
        self.span
            .extend(signal[start..start + span].iter().map(|&v| v - local_mean));
        let x = &self.span;
        let sum_x2: f64 = x[..n].iter().map(|&v| v * v).sum();
        let mut sum_y2 = sum_x2;
        for lag in 1..=local_maximum_lag {
            let dropped = x[lag - 1];
            let added = x[lag + n - 1];
            sum_y2 += added * added - dropped * dropped;
            let product: f64 = x[..n]
                .iter()
                .zip(&x[lag..lag + n])
                .map(|(&a, &b)| a * b)
                .sum();
            let norm = (sum_x2 * sum_y2).sqrt();
            r[lag] = if norm > 0.0 { product / norm } else { 0.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_follows_the_documented_sizes() {
        let params = PitchParams {
            floor_hz: 65.0,
            ceiling_hz: 500.0,
            very_accurate: true,
            ..PitchParams::default()
        };
        let layout = Layout::new(&params, 16_000.0, Method::Autocorrelation).unwrap();
        // 6 / 65 s at 16 kHz = 1476.9 samples → 1476 → half 737 → 1474.
        assert_eq!(layout.window_samples, 1474);
        assert_eq!(layout.period_samples, 246);
        assert_eq!(layout.half_period, 124);
        assert_eq!(layout.maximum_lag, 1474 / 6 + 2);
        assert_eq!(layout.max_lag, 368);
        assert_eq!(layout.fft_len, 2048);
        assert_eq!(layout.refine_depth, 700);
        assert_eq!(layout.max_candidates, 15);

        let hanning =
            Layout::new(&PitchParams::default(), 44_100.0, Method::Autocorrelation).unwrap();
        assert_eq!(
            hanning.window_samples,
            (44_100.0_f64 * 3.0 / 75.0) as usize / 2 * 2 - 2
        );
        assert_eq!(hanning.refine_depth, 70);
        assert_eq!(hanning.fft_len, next_pow2(hanning.window_samples * 3 / 2));
    }

    #[test]
    fn cross_correlation_layout_adds_a_period_of_look_ahead() {
        let params = PitchParams::default();
        let layout = Layout::new(
            &params,
            16_000.0,
            Method::CrossCorrelation {
                periods_per_window: 1.0,
            },
        )
        .unwrap();
        // 1 / 75 s at 16 kHz = 213.3 samples → 213 → half 105 → 210.
        assert_eq!(layout.window_samples, 210);
        assert!((layout.window_seconds - 2.0 / 75.0).abs() < 1e-12);
        // ⌊210 / 1⌋ + 2 clamps to the window length.
        assert_eq!(layout.maximum_lag, 210);
        assert_eq!(layout.max_lag, 210);
        assert_eq!(layout.refine_depth, 70);
        assert!(!layout.gaussian);
        assert!(layout.window().iter().all(|&w| w == 1.0));
    }

    #[test]
    fn cross_correlation_of_a_tone_peaks_at_its_period() {
        let rate = 16_000.0;
        // Two periods per window: lags run to ⌊424 / 2⌋ + 2 = 214 while the
        // correlation buffer reaches 424, so the tail past 214 stays zero.
        let layout = Layout::new(
            &PitchParams::default(),
            rate,
            Method::CrossCorrelation {
                periods_per_window: 2.0,
            },
        )
        .unwrap();
        assert_eq!(layout.maximum_lag, 214);
        let mut analyzer = FrameAnalyzer::new(layout);
        let period = 100.0;
        let signal: Vec<f64> = (0..16_000)
            .map(|i| 0.3 + (2.0 * PI * i as f64 / period).sin())
            .collect();
        let mut r = vec![0.0; layout.max_lag + 1];
        let intensity = analyzer.correlate(&signal, 1.0, 0.5, &mut r);
        assert!(intensity > 0.9, "{intensity}");
        assert!((r[100] - 1.0).abs() < 1e-6, "r[100] = {}", r[100]);
        assert!(r[50] < -0.99, "r[50] = {}", r[50]);
        assert!((r[200] - 1.0).abs() < 1e-6, "r[200] = {}", r[200]);
        assert_eq!(r[300], 0.0, "past the last computed lag");
    }

    #[test]
    fn layout_rejects_windows_too_short_to_analyse() {
        // 3 / 75 s at 80 Hz sampling is three samples: no frame fits.
        assert!(Layout::new(&PitchParams::default(), 80.0, Method::Autocorrelation).is_none());
        // A floor above the sampling rate leaves no period to examine.
        let params = PitchParams {
            floor_hz: 20_000.0,
            ceiling_hz: 30_000.0,
            ..PitchParams::default()
        };
        assert!(Layout::new(&params, 16_000.0, Method::Autocorrelation).is_none());
        // The ceiling never exceeds Nyquist.
        let params = PitchParams {
            ceiling_hz: 20_000.0,
            ..PitchParams::default()
        };
        assert_eq!(
            Layout::new(&params, 16_000.0, Method::Autocorrelation)
                .unwrap()
                .ceiling_hz,
            8_000.0
        );
    }

    #[test]
    fn gaussian_window_falls_to_zero_one_sample_past_its_ends() {
        let params = PitchParams {
            very_accurate: true,
            ..PitchParams::default()
        };
        let layout = Layout::new(&params, 16_000.0, Method::Autocorrelation).unwrap();
        let window = layout.window();
        let n = window.len();
        assert!(window[0] > 0.0 && window[0] < 1e-3, "{}", window[0]);
        assert!((window[0] - window[n - 1]).abs() < 1e-12);
        assert!((window[n / 2] - 1.0).abs() < 1e-3);
        // One sample outside either end the continuous window is exactly zero.
        let span = (n + 1) as f64;
        let edge = (-12.0_f64).exp();
        let outside = ((-48.0 * (0.5 * span).powi(2) / (span * span)).exp() - edge) / (1.0 - edge);
        assert!(outside.abs() < 1e-15);
    }

    #[test]
    fn window_acf_is_one_at_zero_lag_and_decays() {
        let layout =
            Layout::new(&PitchParams::default(), 16_000.0, Method::Autocorrelation).unwrap();
        let analyzer = FrameAnalyzer::new(layout);
        assert!((analyzer.window_acf[0] - 1.0).abs() < 1e-12);
        let mid = analyzer.window_acf[layout.max_lag / 2];
        assert!(mid > 0.0 && mid < 1.0);
        assert!(analyzer.window_acf[layout.max_lag] < mid);
        // Hanning: the closed form of eq. 8 with T = window length + 1 samples.
        let t = (layout.window_samples + 1) as f64;
        let tau = (layout.max_lag / 3) as f64;
        let ratio = tau / t;
        let expected = (1.0 - ratio) * (2.0 / 3.0 + (1.0 / 3.0) * (2.0 * PI * ratio).cos())
            + (1.0 / (2.0 * PI)) * (2.0 * PI * ratio).sin();
        assert!(
            (analyzer.window_acf[tau as usize] - expected).abs() < 1e-3,
            "{} vs {expected}",
            analyzer.window_acf[tau as usize]
        );
    }

    #[test]
    fn pure_tone_correlates_to_one_at_its_period() {
        let rate = 16_000.0;
        let layout = Layout::new(&PitchParams::default(), rate, Method::Autocorrelation).unwrap();
        let mut analyzer = FrameAnalyzer::new(layout);
        let period = 100.0;
        let signal: Vec<f64> = (0..16_000)
            .map(|i| (2.0 * PI * i as f64 / period).sin())
            .collect();
        let mut r = vec![0.0; layout.max_lag + 1];
        let intensity = analyzer.correlate(&signal, 1.0, 0.5, &mut r);
        // The peak is read on the windowed frame, so it sits just under the
        // window's centre value.
        assert!(intensity > 0.9 && intensity <= 1.0, "intensity {intensity}");
        assert!((r[100] - 1.0).abs() < 1e-3, "r[100] = {}", r[100]);
        assert!(r[50] < -0.99, "r[50] = {}", r[50]);
    }
}
