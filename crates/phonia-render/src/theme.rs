//! Background a figure is drawn against.

/// Light or dark background.
///
/// Themes style chrome (axes, labels, panel backgrounds), never the
/// spectrogram ramps: every [`crate::Colormap`] is a fixed table that renders
/// identically on both backgrounds. Inverting a ramp is an explicit choice
/// ([`crate::colorize`]'s `invert`), not a side effect of the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    /// Light background.
    Light,
    /// Dark background.
    Dark,
}
