use std::error::Error;
use std::fmt;

/// Parameters for Boersma-style raw-autocorrelation pitch analysis.
///
/// Defaults follow Praat's documented "Sound: To Pitch (raw autocorrelation)"
/// command reference: automatic time step, 75 Hz floor, 600 Hz ceiling, 15
/// candidates, normal accuracy, silence threshold 0.03, voicing threshold 0.45,
/// octave cost 0.01, octave-jump cost 0.35, and voiced/unvoiced cost 0.14. The
/// Praat FAQ text mentions older 0.09/0.50 threshold values; the command
/// reference is authoritative here. Automatic time step is resolved as
/// `0.75 / floor_hz`: the manual specifies four pitch values per window length
/// (oversampling degree 4), and the window is `3 / floor_hz` long, so the hop
/// is a quarter of that. The manual gives the worked value 0.01 s at a 75 Hz
/// floor.
#[derive(Debug, Clone, PartialEq)]
pub struct PitchParams {
    /// Frame hop in seconds, or `None` for `0.75 / floor_hz`.
    pub time_step: Option<f64>,
    /// Lowest voiced frequency considered, in hertz.
    pub floor_hz: f64,
    /// Highest voiced frequency considered, in hertz.
    pub ceiling_hz: f64,
    /// Maximum candidates retained per frame, including the unvoiced
    /// candidate. A value below the ceiling-to-floor ratio is raised to it.
    pub max_candidates: usize,
    /// Uses the longer Gaussian window when `true`; otherwise uses Hanning.
    pub very_accurate: bool,
    /// Silence threshold in the unvoiced-candidate strength formula.
    pub silence_threshold: f64,
    /// Voicing threshold in the unvoiced-candidate strength formula.
    pub voicing_threshold: f64,
    /// Per-octave bias applied to voiced candidate strength.
    pub octave_cost: f64,
    /// Per-octave cost for voiced-to-voiced frequency jumps.
    pub octave_jump_cost: f64,
    /// Cost for transitions between voiced and unvoiced candidates.
    pub voiced_unvoiced_cost: f64,
}

impl Default for PitchParams {
    fn default() -> Self {
        Self {
            time_step: None,
            floor_hz: 75.0,
            ceiling_hz: 600.0,
            max_candidates: 15,
            very_accurate: false,
            silence_threshold: 0.03,
            voicing_threshold: 0.45,
            octave_cost: 0.01,
            octave_jump_cost: 0.35,
            voiced_unvoiced_cost: 0.14,
        }
    }
}

impl PitchParams {
    /// The frame step: the caller's, or the documented automatic one —
    /// `0.75 / floor` for autocorrelation, `0.25 / floor` for
    /// cross-correlation (the manual's "Time step" defaults).
    pub(crate) fn resolved_step(&self, automatic_periods: f64) -> Option<f64> {
        let step = self.time_step.unwrap_or(automatic_periods / self.floor_hz);
        (step.is_finite() && step > 0.0).then_some(step)
    }

    /// Checks every field for a value the analysis cannot use.
    ///
    /// Returns the first violation found, in field-declaration order: an
    /// explicit `time_step` that is not finite or not positive (`None`, the
    /// automatic step, is always valid), a non-finite or non-positive
    /// `floor_hz`, a `ceiling_hz` that is not finite or not strictly above
    /// `floor_hz`, a `max_candidates` of zero, and finally a NaN or infinite
    /// cost/threshold field. Audio that is shorter than one analysis window
    /// is not a parameter error: [`crate::pitch_track`] and
    /// [`crate::pitch_track_cc`] return an empty, valid [`crate::PitchTrack`]
    /// for it instead.
    ///
    /// # Errors
    /// Returns the [`PitchError`] variant matching the first invalid
    /// field found.
    pub fn validate(&self) -> Result<(), PitchError> {
        if let Some(step) = self.time_step
            && (!step.is_finite() || step <= 0.0)
        {
            return Err(PitchError::InvalidTimeStep(step));
        }
        if !self.floor_hz.is_finite() || self.floor_hz <= 0.0 {
            return Err(PitchError::InvalidFloor(self.floor_hz));
        }
        if !self.ceiling_hz.is_finite() || self.ceiling_hz <= self.floor_hz {
            return Err(PitchError::CeilingNotAboveFloor {
                floor_hz: self.floor_hz,
                ceiling_hz: self.ceiling_hz,
            });
        }
        if self.max_candidates == 0 {
            return Err(PitchError::ZeroCandidates);
        }
        for (field, value) in [
            ("silence_threshold", self.silence_threshold),
            ("voicing_threshold", self.voicing_threshold),
            ("octave_cost", self.octave_cost),
            ("octave_jump_cost", self.octave_jump_cost),
            ("voiced_unvoiced_cost", self.voiced_unvoiced_cost),
        ] {
            if !value.is_finite() {
                return Err(PitchError::NonFiniteField { field, value });
            }
        }
        Ok(())
    }
}

/// Why a [`PitchParams`] value, or the `periods_per_window` argument of
/// [`crate::pitch_track_cc`], cannot be analysed.
///
/// [`PitchParams::validate`] returns the first violation it finds, in
/// field-declaration order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PitchError {
    /// An explicit `time_step` is not finite or not positive. `None` (the
    /// automatic step) is always valid.
    InvalidTimeStep(f64),
    /// `floor_hz` is not finite or not positive.
    InvalidFloor(f64),
    /// `ceiling_hz` is not finite, or not strictly greater than `floor_hz`.
    CeilingNotAboveFloor {
        /// The floor the ceiling failed to exceed.
        floor_hz: f64,
        /// The offending ceiling.
        ceiling_hz: f64,
    },
    /// `max_candidates` is zero; a frame needs at least the unvoiced
    /// candidate's slot.
    ZeroCandidates,
    /// A cost or threshold field is NaN or infinite.
    NonFiniteField {
        /// The field's name, as written on [`PitchParams`].
        field: &'static str,
        /// The offending value.
        value: f64,
    },
    /// [`crate::pitch_track_cc`]'s `periods_per_window` argument is not
    /// finite or not positive.
    InvalidPeriodsPerWindow(f64),
}

impl fmt::Display for PitchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTimeStep(step) => {
                write!(f, "time_step must be finite and positive, got {step}")
            }
            Self::InvalidFloor(floor_hz) => {
                write!(f, "floor_hz must be finite and positive, got {floor_hz}")
            }
            Self::CeilingNotAboveFloor {
                floor_hz,
                ceiling_hz,
            } => write!(
                f,
                "ceiling_hz must be finite and greater than floor_hz ({floor_hz}), got {ceiling_hz}"
            ),
            Self::ZeroCandidates => write!(f, "max_candidates must be at least 1"),
            Self::NonFiniteField { field, value } => {
                write!(f, "{field} must be finite, got {value}")
            }
            Self::InvalidPeriodsPerWindow(periods) => write!(
                f,
                "periods_per_window must be finite and positive, got {periods}"
            ),
        }
    }
}

impl Error for PitchError {}
