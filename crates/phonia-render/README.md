# phonia-render

Colours spectrogram tiles. `colorize` takes a row-major tile of dB values and
returns 8-bit RGBA pixels: values are clipped to a dynamic range below a fixed
or autoscaled ceiling (`DisplayMapping`), mapped linearly onto a colormap, and
written fully opaque. Silent frames (`NaN`, `-inf`) render as the floor colour.

The built-in ramps are fixed tables: viridis, magma, inferno, plasma, cividis,
turbo, cubehelix, CMRmap, gnuplot, ocean, two grayscale ramps (ink on paper
and dark-floor), and Phonia's own two ramps. Each file under `src/data/`
records where its table comes from. Any ramp can be reversed with `invert`,
and `colorize_with_lut` accepts a custom 256-entry table. The crate has no
dependencies.

## Example

```rust
use phonia_render::{Colormap, DisplayMapping, colorize};

// A 2 × 2 tile, row-major, in dB.
let tile = [-60.0_f32, -30.0, -10.0, f32::NEG_INFINITY];
let pixels = colorize(&tile, 2, 2, &DisplayMapping::default(), Colormap::Viridis, false);

assert_eq!(pixels.len(), 4 * 2 * 2);
assert!(pixels.chunks_exact(4).all(|px| px[3] == 255));
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
