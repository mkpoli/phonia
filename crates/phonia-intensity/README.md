# phonia-intensity

Praat-compatible intensity contours: the squared signal convolved with a
Kaiser-20 analysis window, reported in dB SPL re 2×10⁻⁵ Pa.

`intensity_track` follows Praat's "Sound: To Intensity...": the window's
effective duration is `3.2 / pitch_floor_hz`, and by default the frame's
windowed mean is subtracted before squaring so a non-zero recording offset
does not inflate the reported level. A `Sound` object's samples are documented
as air pressure in pascal, so this crate treats every input sample as a pascal
value the same way; integer PCM decoded to `±1.0` full scale therefore yields
dB relative to full scale offset by a fixed constant, not a calibrated sound
pressure level, unless the recording chain maps a sample of `1.0` to 1 Pa at
the microphone.

## Example

```rust
use phonia_audio::Audio;
use phonia_intensity::{IntensityParams, intensity_track};
use std::f64::consts::PI;

let sample_rate = 16_000.0;
let frames = (sample_rate * 0.5) as usize;
let samples: Vec<f32> = (0..frames)
    .map(|i| (2.0 * PI * 150.0 * i as f64 / sample_rate).sin() as f32 * 0.5)
    .collect();
let audio = Audio::new(vec![samples], sample_rate)?;

let params = IntensityParams::default();
let track = intensity_track(audio.slice_samples(0..audio.frames()), &params)?;
assert!(!track.is_empty());
assert!(track.db(0).unwrap().is_finite());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
