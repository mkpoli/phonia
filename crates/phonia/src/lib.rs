//! Phonetic analysis in Rust.
//!
//! This crate gathers the Phonia libraries under one name. Each library is a
//! module here and a crate of its own on crates.io, so a project that needs
//! only one of them can depend on it directly.
//!
//! | Module | Crate | Contents |
//! | --- | --- | --- |
//! | [`audio`] | `phonia-audio` | Planar audio buffers; WAV, AIFF, FLAC and MP3 decoding; resampling |
//! | [`dsp`] | `phonia-dsp` | Analysis windows, FFT plans, frame grids, sinc interpolation |
//! | [`spectrogram`] | `phonia-spectrogram` | Power spectral density in dB, in viewport-independent tiles |
//! | [`pitch`] | `phonia-pitch` | Autocorrelation and cross-correlation pitch tracking |
//! | [`formant`] | `phonia-formant` | Burg LPC formants and optional tracking |
//! | [`intensity`] | `phonia-intensity` | Intensity contours in dB |
//! | [`voice`] | `phonia-voice` | Pulses, jitter, shimmer, HNR, CPP, spectral moments |
//! | [`annot`] | `phonia-annot` | Interval and point tiers with invertible edits |
//! | [`textgrid`] | `phonia-textgrid` | Praat TextGrid reading and writing |
//!
//! Every module has a cargo feature of the same name, all on by default. To
//! pick a subset, set `default-features = false` and list the modules;
//! `textgrid` brings in `annot`, `voice` brings in `pitch`, and the analysis
//! modules bring in `audio`.
//!
//! The analyses follow Praat's documented algorithms and are checked against
//! Praat itself; each crate's documentation states how closely its output
//! matches. The Phonia application built on these crates runs at
//! <https://phonia.app>.
#![no_std]

#[cfg(feature = "annot")]
#[doc(inline)]
pub use phonia_annot as annot;
#[cfg(feature = "audio")]
#[doc(inline)]
pub use phonia_audio as audio;
#[cfg(feature = "dsp")]
#[doc(inline)]
pub use phonia_dsp as dsp;
#[cfg(feature = "formant")]
#[doc(inline)]
pub use phonia_formant as formant;
#[cfg(feature = "intensity")]
#[doc(inline)]
pub use phonia_intensity as intensity;
#[cfg(feature = "pitch")]
#[doc(inline)]
pub use phonia_pitch as pitch;
#[cfg(feature = "spectrogram")]
#[doc(inline)]
pub use phonia_spectrogram as spectrogram;
#[cfg(feature = "textgrid")]
#[doc(inline)]
pub use phonia_textgrid as textgrid;
#[cfg(feature = "voice")]
#[doc(inline)]
pub use phonia_voice as voice;
