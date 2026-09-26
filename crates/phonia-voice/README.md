# phonia-voice

Voice-quality measures for phonetics: glottal pulse extraction from pitch and
waveform, jitter and shimmer families, harmonics-to-noise ratio, cepstral peak
prominence (CPP/CPPS), spectral moments, and an aggregate voice report.

`voice_report` runs pitch tracking, pulse extraction, and the jitter, shimmer,
HNR and CPP measures over a requested time span and returns them together.

Compared with Praat 6.1.38 through parselmouth: jitter, shimmer and mean HNR
match on sustained vowels, the use a voice report is defined for. On running
speech the pulses fall differently and the measures differ by up to 43.5%
(shimmer APQ11). `hnr_track_cc` matches on every test recording to within 0.5
dB per frame. `hnr_track` (autocorrelation), `cpp`, `cpps`, `cpp_track` and
`cepstrum_slice` have not been compared with Praat yet.

## Example

```rust
use phonia_audio::Audio;
use phonia_pitch::{PitchParams, TimeSpan};
use phonia_voice::voice_report;
use std::f64::consts::PI;

let sample_rate = 16_000.0;
let f0 = 150.0;
let duration = 0.5;
let frames = (sample_rate * duration) as usize;
let samples: Vec<f32> = (0..frames)
    .map(|i| (2.0 * PI * f0 * i as f64 / sample_rate).sin() as f32 * 0.5)
    .collect();
let audio = Audio::new(vec![samples], sample_rate)?;

let span = TimeSpan::new(0.0, duration);
let report = voice_report(
    audio.slice_samples(0..audio.frames()),
    span,
    &PitchParams::default(),
)?;
assert!(report.pitch.mean_hz.is_some());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
