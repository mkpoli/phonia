# phonia-pitch

Praat-compatible pitch tracking: window-corrected autocorrelation (Boersma
1993) or forward cross-correlation candidates, plus a Viterbi path finder,
with Praat-documented parameter defaults.

`pitch_track` follows Praat's "Sound: To Pitch (raw autocorrelation)...";
`pitch_track_cc` follows "Sound: To Pitch (cc)...". Frames are centred on a
grid that spans the signal symmetrically, and each frame's candidates carry
raw correlation strengths; the path finder applies the octave cost and
transition costs when it picks the final `f0` per frame. Audio shorter than
one analysis window is not an error: it yields an empty, valid `PitchTrack`.

## Example

```rust
use phonia_audio::Audio;
use phonia_pitch::{PitchParams, pitch_track};
use std::f64::consts::PI;

let sample_rate = 16_000.0;
let f0 = 150.0;
let frames = (sample_rate * 0.5) as usize;
let samples: Vec<f32> = (0..frames)
    .map(|i| (2.0 * PI * f0 * i as f64 / sample_rate).sin() as f32)
    .collect();
let audio = Audio::new(vec![samples], sample_rate)?;

let params = PitchParams::default();
let track = pitch_track(audio.slice_samples(0..audio.frames()), &params)?;
let voiced_frame = track
    .frames()
    .iter()
    .find(|frame| frame.f0.is_some())
    .expect("a pure tone above the pitch floor is voiced");
assert!((voiced_frame.f0.unwrap() - f0).abs() < 5.0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
