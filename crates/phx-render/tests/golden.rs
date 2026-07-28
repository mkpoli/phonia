//! Golden-image tests: one fixed 4-pixel tile, colorized with every
//! `Colormap`, compared byte-for-byte against a raw RGBA fixture under
//! `tests/golden/`.
//!
//! The tile carries dB values `[-100, -50, -25, 0]` under
//! `DisplayMapping { dynamic_range_db: 50.0, max_db: Some(0.0) }`, giving
//! normalized positions `[0.0, 0.0, 0.5, 1.0]` — the display floor
//! (clipped and exact), the ramp midpoint, and the ceiling. Fixture bytes
//! were derived from the published control-point data and the documented
//! grayscale endpoints with the sampler's own rounding rule, not captured
//! from this crate's own output.

use phx_render::{Colormap, DisplayMapping, colorize};

const TILE_DB: [f32; 4] = [-100.0, -50.0, -25.0, 0.0];
const MAPPING: DisplayMapping = DisplayMapping {
    dynamic_range_db: 50.0,
    max_db: Some(0.0),
};

fn check(colormap: Colormap, fixture_bytes: &[u8]) {
    let out = colorize(&TILE_DB, 4, 1, &MAPPING, colormap, false);
    assert_eq!(
        out, fixture_bytes,
        "{colormap:?} tile does not match golden fixture"
    );
}

#[test]
fn phonia_matches_golden() {
    check(Colormap::Phonia, include_bytes!("golden/phonia.rgba"));
}

#[test]
fn viridis_matches_golden() {
    check(Colormap::Viridis, include_bytes!("golden/viridis.rgba"));
}

#[test]
fn magma_matches_golden() {
    check(Colormap::Magma, include_bytes!("golden/magma.rgba"));
}

#[test]
fn inferno_matches_golden() {
    check(Colormap::Inferno, include_bytes!("golden/inferno.rgba"));
}

#[test]
fn plasma_matches_golden() {
    check(Colormap::Plasma, include_bytes!("golden/plasma.rgba"));
}

#[test]
fn cividis_matches_golden() {
    check(Colormap::Cividis, include_bytes!("golden/cividis.rgba"));
}

#[test]
fn golden_matches_golden() {
    check(Colormap::Golden, include_bytes!("golden/golden.rgba"));
}

#[test]
fn turbo_matches_golden() {
    check(Colormap::Turbo, include_bytes!("golden/turbo.rgba"));
}

#[test]
fn cubehelix_matches_golden() {
    check(Colormap::Cubehelix, include_bytes!("golden/cubehelix.rgba"));
}

#[test]
fn cmrmap_matches_golden() {
    check(Colormap::Cmrmap, include_bytes!("golden/cmrmap.rgba"));
}

#[test]
fn gnuplot_matches_golden() {
    check(Colormap::Gnuplot, include_bytes!("golden/gnuplot.rgba"));
}

#[test]
fn ocean_matches_golden() {
    check(Colormap::Ocean, include_bytes!("golden/ocean.rgba"));
}

#[test]
fn grayscale_matches_golden() {
    check(Colormap::Grayscale, include_bytes!("golden/grayscale.rgba"));
}

#[test]
fn grayscale_dark_matches_golden() {
    check(
        Colormap::GrayscaleDark,
        include_bytes!("golden/grayscale_dark.rgba"),
    );
}

/// Inverting the print grayscale swaps its endpoints: the floor renders
/// black, the ceiling white, and the midpoint is the same gray from both
/// directions.
#[test]
fn grayscale_inverted_is_the_endpoint_swap() {
    let out = colorize(&TILE_DB, 4, 1, &MAPPING, Colormap::Grayscale, true);
    assert_eq!(&out[0..4], &[0, 0, 0, 255]);
    assert_eq!(&out[4..8], &[0, 0, 0, 255]);
    assert_eq!(&out[12..16], &[255, 255, 255, 255]);
    let plain = colorize(&TILE_DB, 4, 1, &MAPPING, Colormap::Grayscale, false);
    assert_eq!(out[8..11], plain[8..11]);
}

/// The two achromatic ramps are independent designs, not endpoint swaps of
/// each other: the dark-floor ramp keeps both ends off the pure extremes.
#[test]
fn grayscale_dark_is_not_the_print_ramp_inverted() {
    let print_flipped = colorize(&TILE_DB, 4, 1, &MAPPING, Colormap::Grayscale, true);
    let dark = colorize(&TILE_DB, 4, 1, &MAPPING, Colormap::GrayscaleDark, false);
    assert_ne!(print_flipped, dark);
    assert_ne!(&dark[0..3], &[0, 0, 0]);
    assert_ne!(&dark[12..15], &[255, 255, 255]);
}
