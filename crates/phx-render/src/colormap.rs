//! Colormap selection and dB→color sampling.

use crate::data::{
    cividis::CIVIDIS_DATA, cmrmap::CMRMAP_DATA, cubehelix::CUBEHELIX_DATA, gnuplot::GNUPLOT_DATA,
    golden::GOLDEN_DATA, inferno::INFERNO_DATA, magma::MAGMA_DATA, ocean::OCEAN_DATA,
    phonia::PHONIA_DATA, plasma::PLASMA_DATA, turbo::TURBO_DATA, viridis::VIRIDIS_DATA,
};

/// Perceptual colormap used to render a normalized dB tile.
///
/// Every ramp is a fixed table: none of them reshapes itself against the UI
/// theme, so a tile looks the same on a light and a dark background. Two
/// achromatic ramps cover the two reading directions explicitly, and any
/// ramp can be reversed at render time with the `invert` flag of
/// [`crate::colorize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Colormap {
    /// The app's default ramp: warm charcoal floor through the teal identity
    /// ramp into a warm soft-yellow paper highlight, drawn from the Phonia
    /// brand family. Monotonically increasing in relative luminance.
    Phonia,
    /// Perceptually uniform purple→teal→yellow ramp (matplotlib default
    /// since 2.0). Monotonically increasing in relative luminance.
    Viridis,
    /// Perceptually uniform black→purple→orange→pale-yellow ramp.
    /// Monotonically increasing in relative luminance.
    Magma,
    /// Perceptually uniform black→purple→orange→pale-yellow ramp, a warmer
    /// sibling of magma. Monotonically increasing in relative luminance.
    Inferno,
    /// Perceptually uniform dark-blue→purple→orange→yellow ramp.
    /// Monotonically increasing in relative luminance.
    Plasma,
    /// Perceptually uniform dark-blue→gray→yellow ramp optimized for
    /// color-vision deficiency. Monotonically increasing in relative
    /// luminance.
    Cividis,
    /// Warm sibling of Phonia: the same charcoal floor through a
    /// burnt-umber and amber midtone into a golden-cream highlight, more
    /// saturated than Phonia's paper cream. Monotonically increasing in
    /// relative luminance.
    Golden,
    /// High-contrast blue→cyan→green→yellow→red rainbow (Google's turbo).
    /// Luminance peaks mid-ramp; quiet and loud separate by hue.
    Turbo,
    /// Black→green→purple→white helix (Green 2011). Hue rotates while
    /// luminance increases monotonically.
    Cubehelix,
    /// Black→indigo→red→yellow→white ramp (Carey Rappaport's CMRmap).
    Cmrmap,
    /// The classic gnuplot pm3d ramp: black→purple→red→orange→yellow.
    Gnuplot,
    /// Cool sequential ramp: deep green-navy→mid blue→pale blue-white.
    Ocean,
    /// Achromatic print ramp: white silence through black peak, ink density
    /// on a page.
    Grayscale,
    /// Achromatic dark-floor ramp: dark neutral gray silence through soft
    /// white peak. The endpoints stop short of pure black and pure white,
    /// so low-energy regions merge with a dark panel background and peak
    /// brightness is reserved for genuinely loud content.
    GrayscaleDark,
}

impl Colormap {
    /// Sample the colormap at normalized position `t` (clamped to
    /// `[0, 1]`, where `0` is the display floor and `1` is `max_db`),
    /// returning 8-bit sRGB channel values.
    pub(crate) fn sample(self, t: f32) -> [u8; 3] {
        match self {
            Colormap::Phonia => sample_lut(&PHONIA_DATA, t),
            Colormap::Viridis => sample_lut(&VIRIDIS_DATA, t),
            Colormap::Magma => sample_lut(&MAGMA_DATA, t),
            Colormap::Inferno => sample_lut(&INFERNO_DATA, t),
            Colormap::Plasma => sample_lut(&PLASMA_DATA, t),
            Colormap::Cividis => sample_lut(&CIVIDIS_DATA, t),
            Colormap::Golden => sample_lut(&GOLDEN_DATA, t),
            Colormap::Turbo => sample_lut(&TURBO_DATA, t),
            Colormap::Cubehelix => sample_lut(&CUBEHELIX_DATA, t),
            Colormap::Cmrmap => sample_lut(&CMRMAP_DATA, t),
            Colormap::Gnuplot => sample_lut(&GNUPLOT_DATA, t),
            Colormap::Ocean => sample_lut(&OCEAN_DATA, t),
            Colormap::Grayscale => sample_grayscale(t, GRAYSCALE_PRINT),
            Colormap::GrayscaleDark => sample_grayscale(t, GRAYSCALE_DARK),
        }
    }
}

/// Linearly interpolate a 256-entry `[0, 1]` control-point table at `t`
/// and quantize to 8-bit sRGB channels.
fn sample_lut(lut: &[[f32; 3]; 256], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let pos = t * (lut.len() - 1) as f32;
    let i0 = pos.floor() as usize;
    let i1 = (i0 + 1).min(lut.len() - 1);
    let frac = pos - i0 as f32;
    let c0 = lut[i0];
    let c1 = lut[i1];
    [
        to_u8(c0[0] + (c1[0] - c0[0]) * frac),
        to_u8(c0[1] + (c1[1] - c0[1]) * frac),
        to_u8(c0[2] + (c1[2] - c0[2]) * frac),
    ]
}

/// Achromatic endpoints, `(floor_color, ceiling_color)`.
///
/// The print ramp runs white (silence) to black (loudest), matching ink
/// density on a page. The dark-floor ramp is not that ramp inverted: pure
/// black at the floor would read as a hole punched through a dark panel
/// background, and pure white at the ceiling would blow out against it, so
/// its endpoints are tuned in from both extremes.
const GRAYSCALE_PRINT: ([u8; 3], [u8; 3]) = ([255, 255, 255], [0, 0, 0]);
const GRAYSCALE_DARK: ([u8; 3], [u8; 3]) = ([30, 30, 30], [235, 235, 235]);

/// Sample an achromatic ramp between `endpoints` at normalized position `t`.
fn sample_grayscale(t: f32, endpoints: ([u8; 3], [u8; 3])) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let (floor, ceiling) = endpoints;
    [
        lerp_u8(floor[0], ceiling[0], t),
        lerp_u8(floor[1], ceiling[1], t),
        lerp_u8(floor[2], ceiling[2], t),
    ]
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    to_u8(a as f32 / 255.0 + (b as f32 - a as f32) / 255.0 * t)
}

/// Quantize a `[0, 1]` linear channel value to an 8-bit integer, clamping
/// out-of-range input defensively.
fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2.x relative luminance of a linear-`[0, 1]` control-point triple
    /// (no 8-bit quantization). This is the property the sequential colormap
    /// data was designed to satisfy; testing it pre-quantization avoids
    /// spurious failures from the same 1-LSB rounding jitter that shows up
    /// as a handful of near-zero luminance dips once the ramp is rounded to
    /// 8-bit sRGB (an imperceptible quantization artifact, not a property
    /// of the ramp itself).
    fn relative_luminance_linear([r, g, b]: [f32; 3]) -> f64 {
        fn channel(c: f32) -> f64 {
            let c = c as f64;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    #[test]
    fn phonia_endpoints_are_the_charcoal_floor_and_warm_highlight() {
        assert_eq!(sample_lut(&PHONIA_DATA, 0.0), [23, 22, 15]);
        assert_eq!(sample_lut(&PHONIA_DATA, 1.0), [247, 237, 203]);
    }

    #[test]
    fn phonia_luminance_is_monotonic() {
        assert_monotonic_luminance(&PHONIA_DATA, "phonia");
    }

    #[test]
    fn viridis_endpoints_match_published_hex() {
        assert_eq!(sample_lut(&VIRIDIS_DATA, 0.0), [68, 1, 84]);
        assert_eq!(sample_lut(&VIRIDIS_DATA, 1.0), [253, 231, 37]);
    }

    #[test]
    fn magma_endpoints_match_published_hex() {
        assert_eq!(sample_lut(&MAGMA_DATA, 0.0), [0, 0, 4]);
        assert_eq!(sample_lut(&MAGMA_DATA, 1.0), [252, 253, 191]);
    }

    #[test]
    fn inferno_endpoints_match_published_hex() {
        assert_eq!(sample_lut(&INFERNO_DATA, 0.0), [0, 0, 4]);
        assert_eq!(sample_lut(&INFERNO_DATA, 1.0), [252, 255, 164]);
    }

    #[test]
    fn plasma_endpoints_match_published_hex() {
        assert_eq!(sample_lut(&PLASMA_DATA, 0.0), [13, 8, 135]);
        assert_eq!(sample_lut(&PLASMA_DATA, 1.0), [240, 249, 33]);
    }

    #[test]
    fn cividis_endpoints_match_published_hex() {
        assert_eq!(sample_lut(&CIVIDIS_DATA, 0.0), [0, 34, 78]);
        assert_eq!(sample_lut(&CIVIDIS_DATA, 1.0), [254, 232, 56]);
    }

    #[test]
    fn golden_endpoints_are_the_charcoal_floor_and_golden_highlight() {
        assert_eq!(sample_lut(&GOLDEN_DATA, 0.0), [23, 22, 15]);
        assert_eq!(sample_lut(&GOLDEN_DATA, 1.0), [245, 214, 138]);
    }

    #[test]
    fn golden_luminance_is_monotonic() {
        assert_monotonic_luminance(&GOLDEN_DATA, "golden");
    }

    #[test]
    fn turbo_endpoints_match_published_hex() {
        // The published turbo table runs #30123b to #7a0403.
        assert_eq!(sample_lut(&TURBO_DATA, 0.0), [48, 18, 59]);
        assert_eq!(sample_lut(&TURBO_DATA, 1.0), [122, 4, 3]);
    }

    #[test]
    fn cubehelix_endpoints_are_black_and_white() {
        assert_eq!(sample_lut(&CUBEHELIX_DATA, 0.0), [0, 0, 0]);
        assert_eq!(sample_lut(&CUBEHELIX_DATA, 1.0), [255, 255, 255]);
    }

    #[test]
    fn cmrmap_endpoints_are_black_and_white() {
        assert_eq!(sample_lut(&CMRMAP_DATA, 0.0), [0, 0, 0]);
        assert_eq!(sample_lut(&CMRMAP_DATA, 1.0), [255, 255, 255]);
    }

    #[test]
    fn gnuplot_endpoints_are_black_and_yellow() {
        assert_eq!(sample_lut(&GNUPLOT_DATA, 0.0), [0, 0, 0]);
        assert_eq!(sample_lut(&GNUPLOT_DATA, 1.0), [255, 255, 0]);
    }

    #[test]
    fn ocean_endpoints_are_deep_green_and_white() {
        assert_eq!(sample_lut(&OCEAN_DATA, 0.0), [0, 128, 0]);
        assert_eq!(sample_lut(&OCEAN_DATA, 1.0), [255, 255, 255]);
    }

    #[test]
    fn grayscale_endpoints_run_white_to_black() {
        assert_eq!(Colormap::Grayscale.sample(0.0), [255, 255, 255]);
        assert_eq!(Colormap::Grayscale.sample(1.0), [0, 0, 0]);
    }

    #[test]
    fn grayscale_dark_endpoints_stay_off_pure_black_and_white() {
        // Never a naive inversion of the print ramp.
        assert_ne!(Colormap::GrayscaleDark.sample(0.0), [0, 0, 0]);
        assert_ne!(Colormap::GrayscaleDark.sample(1.0), [255, 255, 255]);
        assert_eq!(Colormap::GrayscaleDark.sample(0.0), [30, 30, 30]);
        assert_eq!(Colormap::GrayscaleDark.sample(1.0), [235, 235, 235]);
    }

    #[test]
    fn grayscale_dark_floor_is_darker_than_its_ceiling() {
        let (floor, ceiling) = GRAYSCALE_DARK;
        assert!(floor[0] < ceiling[0]);
    }

    fn assert_monotonic_luminance(data: &[[f32; 3]; 256], name: &str) {
        let mut prev = relative_luminance_linear(data[0]);
        for (i, &stop) in data.iter().enumerate().skip(1) {
            let lum = relative_luminance_linear(stop);
            assert!(
                lum + 1e-12 >= prev,
                "{name} luminance decreased at stop {i}: {prev} -> {lum}"
            );
            prev = lum;
        }
    }

    #[test]
    fn inferno_luminance_is_monotonic() {
        assert_monotonic_luminance(&INFERNO_DATA, "inferno");
    }

    #[test]
    fn plasma_luminance_is_monotonic() {
        assert_monotonic_luminance(&PLASMA_DATA, "plasma");
    }

    #[test]
    fn cividis_luminance_is_monotonic() {
        assert_monotonic_luminance(&CIVIDIS_DATA, "cividis");
    }

    #[test]
    fn cubehelix_luminance_is_monotonic() {
        assert_monotonic_luminance(&CUBEHELIX_DATA, "cubehelix");
    }

    #[test]
    fn viridis_luminance_is_monotonic() {
        assert_monotonic_luminance(&VIRIDIS_DATA, "viridis");
    }

    #[test]
    fn magma_luminance_is_monotonic() {
        assert_monotonic_luminance(&MAGMA_DATA, "magma");
    }
}
