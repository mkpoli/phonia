//! Typed errors for this crate's analysis entry points.

use std::error::Error;
use std::fmt;

use phonia_audio::AudioError;

/// Errors returned by [`crate::formant_track`], [`crate::track_smoothed`],
/// [`crate::effective_time_step`], and [`crate::frame_grid`].
///
/// Every variant here replaces a panicking `assert!` or `expect()`: a
/// library caller's out-of-range parameter or a resample that overruns
/// `phonia_audio`'s allocation limit must reach the caller as a value, not
/// crash the process.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FormantError {
    /// [`crate::FormantParams::ceiling_hz`] was not finite or not greater
    /// than 100 Hz.
    InvalidCeiling(f64),
    /// [`crate::FormantParams::max_formants`] was zero.
    ZeroMaxFormants,
    /// [`crate::FormantParams::window_length`] was not finite or not
    /// positive.
    InvalidWindowLength(f64),
    /// [`crate::FormantParams::time_step`] was set but not finite or not
    /// positive.
    InvalidTimeStep(f64),
    /// [`crate::FormantParams::preemphasis_from_hz`] was not finite.
    InvalidPreemphasis(f64),
    /// A [`crate::TrackingRefs::neutral_hz`] entry was not finite or not
    /// positive.
    InvalidNeutralFrequency(f64),
    /// [`crate::TrackingRefs::bandwidth_weight`] was not finite or negative.
    InvalidBandwidthWeight(f64),
    /// [`crate::TrackingRefs::frequency_weight`] was not finite or negative.
    InvalidFrequencyWeight(f64),
    /// [`crate::TrackingRefs::transition_weight`] was not finite or negative.
    InvalidTransitionWeight(f64),
    /// Resampling the input to twice the formant ceiling failed — the
    /// signal's whole-transform workspace exceeded `phonia_audio`'s
    /// allocation limit (about 67 M source samples; see
    /// `docs/plan/horizon.md`).
    Resample(AudioError),
}

impl fmt::Display for FormantError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCeiling(value) => {
                write!(
                    f,
                    "ceiling_hz must be finite and greater than 100 Hz, got {value}"
                )
            }
            Self::ZeroMaxFormants => write!(f, "max_formants must be positive"),
            Self::InvalidWindowLength(value) => {
                write!(f, "window_length must be finite and positive, got {value}")
            }
            Self::InvalidTimeStep(value) => {
                write!(
                    f,
                    "time_step must be finite and positive when set, got {value}"
                )
            }
            Self::InvalidPreemphasis(value) => {
                write!(f, "preemphasis_from_hz must be finite, got {value}")
            }
            Self::InvalidNeutralFrequency(value) => write!(
                f,
                "TrackingRefs::neutral_hz entries must be finite and positive, got {value}"
            ),
            Self::InvalidBandwidthWeight(value) => write!(
                f,
                "TrackingRefs::bandwidth_weight must be finite and non-negative, got {value}"
            ),
            Self::InvalidFrequencyWeight(value) => write!(
                f,
                "TrackingRefs::frequency_weight must be finite and non-negative, got {value}"
            ),
            Self::InvalidTransitionWeight(value) => write!(
                f,
                "TrackingRefs::transition_weight must be finite and non-negative, got {value}"
            ),
            Self::Resample(err) => write!(f, "resampling to the analysis rate failed: {err}"),
        }
    }
}

impl Error for FormantError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Resample(err) => Some(err),
            Self::InvalidCeiling(_)
            | Self::ZeroMaxFormants
            | Self::InvalidWindowLength(_)
            | Self::InvalidTimeStep(_)
            | Self::InvalidPreemphasis(_)
            | Self::InvalidNeutralFrequency(_)
            | Self::InvalidBandwidthWeight(_)
            | Self::InvalidFrequencyWeight(_)
            | Self::InvalidTransitionWeight(_) => None,
        }
    }
}
