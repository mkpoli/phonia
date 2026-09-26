# phonia-formant

Praat-compatible formant analysis: pre-emphasis, Burg LPC, polynomial roots
converted to frequency and bandwidth, and optional Viterbi tracking.

`formant_track` reproduces Praat's "Sound: To Formant (burg)..." pipeline —
resampling, the Gaussian window, pre-emphasis, Burg LPC, and root-to-formant
conversion. Compared with Praat through parselmouth, it agrees on all but 8 of
6717 checked points, with a median difference of 0.3 Hz. Its output is the
crate's raw, per-frame candidate list: each frame's frequency-gated LPC roots,
sorted by frequency, with no correspondence enforced between a slot in one
frame and the same slot in the next. `track_smoothed` is a provisional step on
top of it: a Viterbi reassignment of those candidates to formant slots, after
Xia & Espy-Wilson (2000); its local-cost and transition-cost weights are this
crate's own empirical choice, unvalidated against Praat's separate `Formant:
Track...` command, so callers presenting the smoothed track should label it as
provisional.

## Example

```rust
use phonia_audio::Audio;
use phonia_formant::{FormantParams, formant_track};
use std::f64::consts::PI;

let sample_rate = 11_000.0;
let f0 = 100.0;
let frames = (sample_rate * 0.3) as usize;
// A pulse train through a resonator would give cleaner formants; a rich
// harmonic buzz is enough to exercise the pipeline here.
let samples: Vec<f32> = (0..frames)
    .map(|i| {
        let t = i as f64 / sample_rate;
        let mut s = 0.0;
        for harmonic in 1..20 {
            s += (2.0 * PI * f0 * harmonic as f64 * t).sin() / harmonic as f64;
        }
        (0.2 * s) as f32
    })
    .collect();
let audio = Audio::new(vec![samples], sample_rate)?;

let params = FormantParams::default();
let track = formant_track(audio.slice_samples(0..audio.frames()), &params)?;
assert!(!track.frames.is_empty());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
