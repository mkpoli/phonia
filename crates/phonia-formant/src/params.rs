use phonia_dsp::FrameGrid;

use crate::FormantError;

const DEFAULT_CEILING_HZ: f64 = 5500.0;
const DEFAULT_MAX_FORMANTS: usize = 5;
const DEFAULT_WINDOW_LENGTH: f64 = 0.025;
const DEFAULT_PREEMPHASIS_FROM_HZ: f64 = 50.0;

/// Burg formant analysis parameters.
///
/// Defaults follow Praat's documented "Sound: To Formant (burg)..." values:
/// formant ceiling 5500 Hz, maximum formants 5, Gaussian effective window
/// length 25 ms, time step `0.0` meaning 25% of the window length, and
/// pre-emphasis from 50 Hz. The 5500 Hz ceiling default is Praat's
/// overall/adult-female default; Praat's documented adult-male default is
/// 5000 Hz and using the female default on male speakers inflates F1 by
/// roughly 120 Hz (Schiel & Zitzelsberger, LREC 2018). Callers analysing male
/// or child speech should override `ceiling_hz` explicitly (5000 Hz adult male,
/// 8000 Hz child, per the Praat manual).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormantParams {
    /// Formant ceiling in hertz; Praat's overall/adult-female default is 5500 Hz.
    pub ceiling_hz: f64,
    /// Maximum number of formants; Praat's default is 5.
    pub max_formants: usize,
    /// Gaussian effective window length in seconds; Praat's default is 0.025 s.
    pub window_length: f64,
    /// Frame hop in seconds; `None` follows Praat's `0.0` meaning 25% of the window length.
    pub time_step: Option<f64>,
    /// Pre-emphasis corner in hertz; Praat's default is 50 Hz.
    pub preemphasis_from_hz: f64,
}

impl Default for FormantParams {
    fn default() -> Self {
        Self {
            ceiling_hz: DEFAULT_CEILING_HZ,
            max_formants: DEFAULT_MAX_FORMANTS,
            window_length: DEFAULT_WINDOW_LENGTH,
            time_step: None,
            preemphasis_from_hz: DEFAULT_PREEMPHASIS_FROM_HZ,
        }
    }
}

/// Returns the analysis hop in seconds after Praat's `0.0` time-step rule.
///
/// # Errors
/// Returns [`FormantError`] when `params` carries a non-finite or
/// out-of-range value; see [`FormantError`]'s variants for which field.
pub fn effective_time_step(params: &FormantParams) -> Result<f64, FormantError> {
    validate_params(params)?;
    Ok(effective_time_step_unchecked(params))
}

pub(crate) fn effective_time_step_unchecked(params: &FormantParams) -> f64 {
    params.time_step.unwrap_or(0.25 * params.window_length)
}

/// Builds the frame grid used by formant analysis.
///
/// The grid margin uses the physical Gaussian window length, twice the
/// `window_length` parameter. Praat's manual ("Sound: To Formant (burg)...")
/// states the actual analysis window "is twice this value, because Praat uses
/// a Gaussian-like analysis window", and the frame count subtracts that actual
/// window from the signal duration before dividing by the step. `duration`
/// must be the analyzed signal's sample count times its sampling period, the
/// same discrete duration Praat measures frames over.
///
/// # Errors
/// Returns [`FormantError`] under the same conditions as
/// [`effective_time_step`].
pub fn frame_grid(duration: f64, params: &FormantParams) -> Result<FrameGrid, FormantError> {
    validate_params(params)?;
    Ok(frame_grid_unchecked(duration, params))
}

pub(crate) fn frame_grid_unchecked(duration: f64, params: &FormantParams) -> FrameGrid {
    FrameGrid::new(
        duration,
        2.0 * params.window_length,
        effective_time_step_unchecked(params),
    )
}

pub(crate) fn validate_params(params: &FormantParams) -> Result<(), FormantError> {
    if !(params.ceiling_hz.is_finite() && params.ceiling_hz > 100.0) {
        return Err(FormantError::InvalidCeiling(params.ceiling_hz));
    }
    if params.max_formants == 0 {
        return Err(FormantError::ZeroMaxFormants);
    }
    if !(params.window_length.is_finite() && params.window_length > 0.0) {
        return Err(FormantError::InvalidWindowLength(params.window_length));
    }
    if let Some(time_step) = params.time_step
        && !(time_step.is_finite() && time_step > 0.0)
    {
        return Err(FormantError::InvalidTimeStep(time_step));
    }
    if !params.preemphasis_from_hz.is_finite() {
        return Err(FormantError::InvalidPreemphasis(params.preemphasis_from_hz));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_defaults_pass_validation() {
        assert!(validate_params(&FormantParams::default()).is_ok());
    }

    #[test]
    fn rejects_non_finite_ceiling() {
        let params = FormantParams {
            ceiling_hz: f64::NAN,
            ..FormantParams::default()
        };
        assert!(matches!(
            validate_params(&params),
            Err(FormantError::InvalidCeiling(value)) if value.is_nan()
        ));
    }

    #[test]
    fn rejects_ceiling_at_or_below_100_hz() {
        let params = FormantParams {
            ceiling_hz: 100.0,
            ..FormantParams::default()
        };
        assert_eq!(
            validate_params(&params),
            Err(FormantError::InvalidCeiling(100.0))
        );
    }

    #[test]
    fn rejects_zero_max_formants() {
        let params = FormantParams {
            max_formants: 0,
            ..FormantParams::default()
        };
        assert_eq!(validate_params(&params), Err(FormantError::ZeroMaxFormants));
    }

    #[test]
    fn rejects_non_positive_window_length() {
        let params = FormantParams {
            window_length: 0.0,
            ..FormantParams::default()
        };
        assert_eq!(
            validate_params(&params),
            Err(FormantError::InvalidWindowLength(0.0))
        );
    }

    #[test]
    fn rejects_non_positive_time_step_when_set() {
        let params = FormantParams {
            time_step: Some(-1.0),
            ..FormantParams::default()
        };
        assert_eq!(
            validate_params(&params),
            Err(FormantError::InvalidTimeStep(-1.0))
        );
    }

    #[test]
    fn accepts_no_time_step() {
        let params = FormantParams {
            time_step: None,
            ..FormantParams::default()
        };
        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn rejects_non_finite_preemphasis() {
        let params = FormantParams {
            preemphasis_from_hz: f64::INFINITY,
            ..FormantParams::default()
        };
        assert_eq!(
            validate_params(&params),
            Err(FormantError::InvalidPreemphasis(f64::INFINITY))
        );
    }

    #[test]
    fn effective_time_step_rejects_invalid_params() {
        let params = FormantParams {
            max_formants: 0,
            ..FormantParams::default()
        };
        assert_eq!(
            effective_time_step(&params),
            Err(FormantError::ZeroMaxFormants)
        );
    }

    #[test]
    fn frame_grid_rejects_invalid_params() {
        let params = FormantParams {
            ceiling_hz: 0.0,
            ..FormantParams::default()
        };
        assert_eq!(
            frame_grid(1.0, &params),
            Err(FormantError::InvalidCeiling(0.0))
        );
    }
}
