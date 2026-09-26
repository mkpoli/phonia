# phonia-spectrogram

Gaussian-window STFT spectrograms as power spectral density in decibels,
computed in viewport-independent tiles.

Every frame is windowed (Gaussian, Hanning, or Kaiser), transformed, and
divided by the window's own energy, so the result is a power *spectral
density* (`Pa²/Hz`) rather than a raw bin power: broadband noise reads at the
same level whatever the window, a pure tone does not. `Tile` rows run
low-to-high frequency; `ColumnBlock` is column-major instead, so a viewport's
bytes stay stable while it scrolls across whole columns. `compute_tile`'s raw
PSD dB is checked against a SciPy-computed fixture
(`scipy.signal.ShortTimeFFT`).

## Example

```rust
use phonia_audio::Audio;
use phonia_spectrogram::{SpectrogramParams, TileRequest, compute_tile};
use std::f64::consts::PI;

let sample_rate = 16_000.0;
let frames = (sample_rate * 0.2) as usize;
let samples: Vec<f32> = (0..frames)
    .map(|i| (2.0 * PI * 440.0 * i as f64 / sample_rate).sin() as f32)
    .collect();
let audio = Audio::new(vec![samples], sample_rate)?;

let request = TileRequest {
    t0: 0.0,
    t1: audio.duration(),
    f0: 0.0,
    f1: 2_000.0,
    width_px: 64,
    height_px: 32,
    params: SpectrogramParams::default(),
};
let tile = compute_tile(audio.slice_samples(0..audio.frames()), &request)?;
assert_eq!(tile.db.len(), tile.t_axis.len() * tile.f_axis.len());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
