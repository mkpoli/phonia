"""Named oracle cases: a measure, a parameter set, and a default corpus.

Case names and the `oracle run --case <name> --audio <path>` shape follow
`docs/plan/validation.md`. `tests/fixtures/audio/long_scroll_test.wav` (10
minutes, built by concatenating the other fixtures) is excluded from every
default corpus — it adds no case coverage beyond its five source clips and
is expensive to re-run.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

from oracle.params import (
    FormantParams,
    HarmonicityParams,
    IntensityParams,
    PitchParams,
    ResampleParams,
)

Measure = Literal[
    "pitch", "pitch-cc", "formant", "intensity", "harmonicity", "resample", "spectrogram", "voice"
]

SPEECH_AND_VOWEL_CORPUS = (
    "arctic_bdl_a0001.wav",
    "arctic_slt_a0001.wav",
    "librispeech_2277-149896-0005.wav",
    "synth_vowel_a.wav",
)

VOICE_REPORT_CORPUS = (
    "synth_vowel_a.wav",
    "synth_vowel_perturbed.wav",
    "arctic_bdl_a0001.wav",
)

# Time span each `voice-report-defaults` fixture is scored over. Not a
# `PitchParams` field -- it is intrinsic to which stretch of each recording
# is being reported on, so it lives here rather than in `oracle.params`.
# `tools/oracle-bridge/src/main.rs::voice_report_span` mirrors this table
# verbatim.
VOICE_REPORT_SPANS: dict[str, tuple[float, float]] = {
    # Whole analyzed domain for both synthetic sustained-vowel fixtures.
    "synth_vowel_a.wav": (0.0, 2.0),
    "synth_vowel_perturbed.wav": (0.0, 2.0),
    # The longest contiguous voiced run in `arctic_bdl_a0001.wav` with F0
    # spread under 15% (found by scanning
    # `pitch-defaults__arctic_bdl_a0001.json`): 31 glottal pulses in span,
    # enough for every point-perturbation quotient up to APQ11.
    "arctic_bdl_a0001.wav": (0.723, 0.943),
}


@dataclass(frozen=True)
class Case:
    name: str
    measure: Measure
    params: (
        PitchParams | IntensityParams | FormantParams | HarmonicityParams | ResampleParams | None
    )
    default_audio: tuple[str, ...]
    description: str


CASES: dict[str, Case] = {
    "pitch-defaults": Case(
        name="pitch-defaults",
        measure="pitch",
        params=PitchParams(),
        default_audio=SPEECH_AND_VOWEL_CORPUS,
        description="Boersma raw-autocorrelation pitch, Praat's documented defaults.",
    ),
    "pitch-accurate-speech": Case(
        name="pitch-accurate-speech",
        measure="pitch",
        params=PitchParams(
            time_step=0.02,
            floor_hz=65.0,
            ceiling_hz=500.0,
            very_accurate=True,
            voicing_threshold=0.5,
        ),
        default_audio=SPEECH_AND_VOWEL_CORPUS,
        description=(
            "Boersma autocorrelation pitch with the Gaussian very-accurate "
            "window, a 20 ms step, a 65-500 Hz search range, and voicing "
            "threshold 0.5: the setting a speech-comparison tool uses. Covers "
            "the window-ACF normalisation, the path finder's time-step "
            "correction, and a non-default floor, none of which "
            "`pitch-defaults` exercises."
        ),
    ),
    "pitch-cc-defaults": Case(
        name="pitch-cc-defaults",
        measure="pitch-cc",
        params=PitchParams(),
        default_audio=SPEECH_AND_VOWEL_CORPUS,
        description="Forward cross-correlation pitch, Praat's documented defaults.",
    ),
    "harmonicity-cc-defaults": Case(
        name="harmonicity-cc-defaults",
        measure="harmonicity",
        params=HarmonicityParams(),
        default_audio=SPEECH_AND_VOWEL_CORPUS,
        description="Cross-correlation harmonicity, Praat's documented defaults.",
    ),
    "harmonicity-cc-speech": Case(
        name="harmonicity-cc-speech",
        measure="harmonicity",
        params=HarmonicityParams(
            time_step=0.02,
            floor_hz=65.0,
            periods_per_window=4.5,
        ),
        default_audio=SPEECH_AND_VOWEL_CORPUS,
        description=(
            "Cross-correlation harmonicity with a 20 ms step, a 65 Hz floor and "
            "4.5 periods per window: the setting a speech-comparison tool uses."
        ),
    ),
    "resample-down": Case(
        name="resample-down",
        measure="resample",
        params=ResampleParams(target_hz=11000.0),
        default_audio=("synth_vowel_a.wav", "arctic_slt_a0001.wav"),
        description=(
            "Sound.resample to 11 kHz at precision 50: the band-limiting cut and the "
            "centred output grid, sample for sample."
        ),
    ),
    "resample-half": Case(
        name="resample-half",
        measure="resample",
        params=ResampleParams(target_hz=8000.0),
        default_audio=("arctic_slt_a0001.wav",),
        description="Sound.resample to 8 kHz at precision 50: an integer rate ratio.",
    ),
    "resample-up": Case(
        name="resample-up",
        measure="resample",
        params=ResampleParams(target_hz=22050.0),
        default_audio=("synth_vowel_a.wav", "arctic_bdl_a0001.wav"),
        description=(
            "Sound.resample to 22.05 kHz at precision 50: interpolation alone at a "
            "rate ratio that is not a binary fraction, sample for sample. An exact "
            "doubling is deliberately not a case: Praat takes another path there "
            "(algorithms report §6.1)."
        ),
    ),
    "formant-defaults": Case(
        name="formant-defaults",
        measure="formant",
        params=FormantParams(),
        default_audio=SPEECH_AND_VOWEL_CORPUS,
        description="Burg LPC formants, Praat's documented defaults.",
    ),
    "intensity-defaults": Case(
        name="intensity-defaults",
        measure="intensity",
        params=IntensityParams(),
        default_audio=SPEECH_AND_VOWEL_CORPUS,
        description="Intensity contour, Praat's documented defaults.",
    ),
    "spectrogram-slice-defaults": Case(
        name="spectrogram-slice-defaults",
        measure="spectrogram",
        params=None,
        default_audio=("synth_vowel_a.wav",),
        description=(
            "Single power-spectrum slice at t=1.0s, Praat's documented "
            "spectrogram defaults. Not oracle-diffed (validation.md: "
            "spectrogram parity is checked against scipy STFT instead); "
            "`oracle diff` refuses this measure."
        ),
    ),
    "voice-report-defaults": Case(
        name="voice-report-defaults",
        measure="voice",
        params=PitchParams(),
        default_audio=VOICE_REPORT_CORPUS,
        description=(
            "Voice-report scalars (jitter local/rap/ppq5/ddp, shimmer "
            "local/apq3/apq5/apq11/dda, mean HNR, F0 mean/median/min/max) "
            "over each fixture's `VOICE_REPORT_SPANS` span, via Praat's "
            "PointProcess/Pitch/Harmonicity \"Get ...\" commands -- the "
            "same pitch parameters `phx_voice::voice_report` takes."
        ),
    ),
}
