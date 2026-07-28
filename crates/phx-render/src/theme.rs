//! UI theme a figure is rendered against.

/// Light or dark UI theme.
///
/// Themes style chrome — axes, labels, panel backgrounds — never the
/// spectrogram ramps: every [`crate::Colormap`] is a fixed table that
/// renders identically on both backgrounds, per `docs/plan/ux.md`
/// ("palettes are defined against both backgrounds, never inverted").
/// Inverting a ramp is an explicit user choice ([`crate::colorize`]'s
/// `invert`), not a side effect of the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Theme {
    /// Light application background.
    Light,
    /// Dark application background.
    Dark,
}
