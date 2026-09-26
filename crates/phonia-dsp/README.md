# phonia-dsp

Shared DSP primitives for speech analysis: analysis windows (Hanning,
Gaussian, Kaiser), an absolute-time frame grid, cached real-FFT plans,
in-place pre-emphasis, and windowed-sinc peak interpolation.

Every Phonia analysis crate frames its signal through `FrameGrid`, so a value
queried at a given time is identical regardless of zoom, scroll, or which
caller asked for it. Audio arrives as `f32` and is promoted to `f64` before
windowing and transforms.

## Example

```rust
use phonia_dsp::{FrameGrid, RealFftPlan, Window, window_samples};
use std::f64::consts::PI;

let sample_rate: f64 = 8_000.0;

// One 40 ms frame, centred on a 1 s signal by the frame grid.
let grid = FrameGrid::new(1.0, 0.040, 0.010);
let center = grid.center(0).expect("non-empty grid");

let frame_len = (0.040 * sample_rate).round() as usize;
let window = window_samples(Window::Hanning, frame_len);
let mut frame: Vec<f64> = (0..frame_len)
    .map(|i| {
        let t = center + (i as f64 - frame_len as f64 / 2.0) / sample_rate;
        (2.0 * PI * 440.0 * t).sin() * window[i]
    })
    .collect();

let mut plan = RealFftPlan::new();
let spectrum = plan.rfft(&mut frame);
assert_eq!(spectrum.len(), frame_len / 2 + 1);
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
