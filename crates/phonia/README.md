# phonia

Phonetic analysis in Rust. This crate gathers the Phonia libraries under one
name; each is also published on its own, so a project that needs only one can
depend on it directly.

| Module | Crate | Contents |
| --- | --- | --- |
| `audio` | [`phonia-audio`] | Planar audio buffers; WAV, AIFF, FLAC and MP3 decoding; resampling |
| `dsp` | [`phonia-dsp`] | Analysis windows, FFT plans, frame grids, sinc interpolation |
| `spectrogram` | [`phonia-spectrogram`] | Power spectral density in dB, in viewport-independent tiles |
| `pitch` | [`phonia-pitch`] | Autocorrelation and cross-correlation pitch tracking |
| `formant` | [`phonia-formant`] | Burg LPC formants and optional tracking |
| `intensity` | [`phonia-intensity`] | Intensity contours in dB |
| `voice` | [`phonia-voice`] | Pulses, jitter, shimmer, HNR, CPP, spectral moments |
| `annot` | [`phonia-annot`] | Interval and point tiers with invertible edits |
| `textgrid` | [`phonia-textgrid`] | Praat TextGrid reading and writing |

Every module has a cargo feature of the same name, all on by default:

```toml
[dependencies]
phonia = { version = "0.1", default-features = false, features = ["pitch", "textgrid"] }
```

The analyses follow Praat's documented algorithms. Pitch, formants, intensity,
cross-correlation HNR and the voice report are compared against Praat's own
output, and the spectrogram against SciPy; CPP and CPPS have no reference
comparison yet. The Phonia application built on these crates runs at
<https://phonia.app>; the source is at <https://github.com/mkpoli/phonia>.

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.

[`phonia-audio`]: https://crates.io/crates/phonia-audio
[`phonia-dsp`]: https://crates.io/crates/phonia-dsp
[`phonia-spectrogram`]: https://crates.io/crates/phonia-spectrogram
[`phonia-pitch`]: https://crates.io/crates/phonia-pitch
[`phonia-formant`]: https://crates.io/crates/phonia-formant
[`phonia-intensity`]: https://crates.io/crates/phonia-intensity
[`phonia-voice`]: https://crates.io/crates/phonia-voice
[`phonia-annot`]: https://crates.io/crates/phonia-annot
[`phonia-textgrid`]: https://crates.io/crates/phonia-textgrid
