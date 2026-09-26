//! Regenerate the reference-figure exports for visual inspection.
//!
//! Renders [`common::reference_figure`] to SVG, PNG, and PDF in both the dark
//! and light themes and writes them into a target directory (default
//! `target/figure-reference`). This is the committed way to reproduce the
//! export artifacts; it reads the repo's fixtures from disk at run time.
//!
//! ```text
//! cargo run -p phonia-figure --example render_reference --features raster,pdf
//! ```

#[path = "../tests/common/mod.rs"]
mod common;

use std::path::PathBuf;

use common::reference_figure;
use phonia_figure::{to_pdf, to_png, to_svg};
use phonia_render::Theme;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("target/figure-reference"), PathBuf::from);
    std::fs::create_dir_all(&out)?;

    let base = reference_figure();
    for (tag, theme) in [("dark", Theme::Dark), ("light", Theme::Light)] {
        let mut fig = base.clone();
        fig.theme = theme;

        let svg = to_svg(&fig);
        std::fs::write(out.join(format!("reference-{tag}.svg")), &svg)?;

        let png = to_png(&fig, 192.0)?;
        std::fs::write(out.join(format!("reference-{tag}.png")), &png)?;

        let pdf = to_pdf(&fig)?;
        std::fs::write(out.join(format!("reference-{tag}.pdf")), &pdf)?;

        println!(
            "{tag}: svg {} B, png {} B, pdf {} B",
            svg.len(),
            png.len(),
            pdf.len()
        );
    }
    println!("wrote to {}", out.display());
    Ok(())
}
