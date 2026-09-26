# phonia-figure

A figure model for phonetics plots and the exporters that draw it. A `Figure`
stacks panels over a shared time axis; each panel holds layers: waveform
envelopes, spectrograms (kept as raw dB and coloured at export time), pitch
and intensity series, formant speckles, and annotation tiers. All data is
embedded, so a figure serialises to JSON deterministically.

Every exporter reads the same model:

- `to_svg` always; `to_png` with the `raster` feature and `to_pdf` with the
  `pdf` feature, both derived from the SVG
- `to_tikz` (PGFPlots), `to_typst`, `to_vega` (Vega-Lite v5), `to_graphml`
- `to_code` for matplotlib, ggplot2 and Makie scripts that redraw the figure
  from exported data

With the `analysis` feature, `spectrogram_layer`, `pitch_layer`,
`formant_layer`, `intensity_layer` and `spectral_slice_layer` build layers
straight from the Phonia analysis crates. Without it, the crate depends on none
of them.

PNG and PDF text is set in the bundled DejaVu Sans, whose licence is in
`assets/fonts/LICENSE-DejaVu.txt`.

## Example

```rust
use phonia_figure::{
    Axis, FigureBuilder, LengthUnit, LineStyle, Panel, SizeSpec, Theme, TimeSpan,
    to_svg, waveform_layer, waveform_minmax,
};

let rate = 16_000.0;
let samples: Vec<f32> = (0..16_000)
    .map(|n| (2.0 * std::f32::consts::PI * 220.0 * n as f32 / rate).sin())
    .collect();

let figure = FigureBuilder::new(SizeSpec::new(16.0, 6.0, LengthUnit::Cm), Theme::Light)
    .panel(Panel {
        layers: vec![waveform_layer(
            waveform_minmax(&samples, 400),
            TimeSpan::new(0.0, 1.0),
            LineStyle::default(),
        )],
        time_axis: Axis::linear(0.0, 1.0, Some("Time"), Some("s")),
        value_axis: Axis::linear(-1.0, 1.0, Some("Amplitude"), None),
        height_share: 1.0,
    })
    .build();

let svg = to_svg(&figure);
assert!(svg.starts_with("<svg"));
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

The code is licensed under either of MIT (LICENSE-MIT) or Apache-2.0
(LICENSE-APACHE) at your option. The bundled DejaVu Sans font is under the
Bitstream Vera licence, with a notice in the same terms for glyphs imported
from Arev fonts.
