//! Assembling figures, including from the committed analysis types.
//!
//! [`tier_data`] and [`tiers_layer`] convert annotation [`TierSlot`]s into
//! embedded tiers and are always available. Behind the default-off
//! `analysis` feature, every conversion from a live analysis result to an
//! embedded [`Layer`] lives here too: a spectrogram `Tile` becomes a
//! raw-decibel layer, a `PitchTrack` becomes a point series, a
//! `FormantTrack` becomes a speckle, and an `IntensityTrack` becomes a
//! contour. The functions copy data out of the analysis types, so the
//! resulting [`Figure`] holds no reference back to them.

use phonia_annot::{Tier, TierSlot};
#[cfg(feature = "analysis")]
use phonia_formant::FormantTrack;
#[cfg(feature = "analysis")]
use phonia_intensity::IntensityTrack;
#[cfg(feature = "analysis")]
use phonia_pitch::PitchTrack;
use phonia_render::Theme;
#[cfg(feature = "analysis")]
use phonia_render::{Colormap, DisplayMapping};
#[cfg(feature = "analysis")]
use phonia_spectrogram::{Slice, Tile};

use crate::model::{
    CaptionMeta, Figure, IntervalData, Layer, MinMax, Panel, PointData, ProvenanceRecord, SizeSpec,
    TierContent, TierData, TimeSpan,
};
#[cfg(feature = "analysis")]
use crate::model::{PitchUnit, SpeckleFrame, SpecklePoint};
#[cfg(feature = "analysis")]
use crate::style::SpeckleStyle;
use crate::style::{LineStyle, TierStyle};

/// Fluent assembler for a [`Figure`].
///
/// Panels stack top to bottom in the order they are added; provenance records
/// accumulate in the caption in the order they are added.
#[derive(Debug, Clone)]
pub struct FigureBuilder {
    size: SizeSpec,
    theme: Theme,
    panels: Vec<Panel>,
    caption_meta: CaptionMeta,
    axis_color: Option<[u8; 3]>,
    show_grid: bool,
}

impl FigureBuilder {
    /// Starts a builder for a figure of physical `size` described against
    /// `theme`.
    #[must_use]
    pub fn new(size: SizeSpec, theme: Theme) -> Self {
        Self {
            size,
            theme,
            panels: Vec::new(),
            caption_meta: CaptionMeta::default(),
            axis_color: None,
            show_grid: true,
        }
    }

    /// Appends a panel below the previously added panels.
    #[must_use]
    pub fn panel(mut self, panel: Panel) -> Self {
        self.panels.push(panel);
        self
    }

    /// Appends a caption provenance record.
    #[must_use]
    pub fn source(mut self, record: ProvenanceRecord) -> Self {
        self.caption_meta.sources.push(record);
        self
    }

    /// Overrides the axis and tick colour; `None` keeps the theme default.
    #[must_use]
    pub fn axis_color(mut self, color: Option<[u8; 3]>) -> Self {
        self.axis_color = color;
        self
    }

    /// Sets whether interior grid lines are drawn.
    #[must_use]
    pub fn show_grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    /// Finishes the figure.
    #[must_use]
    pub fn build(self) -> Figure {
        Figure {
            size: self.size,
            theme: self.theme,
            panels: self.panels,
            caption_meta: self.caption_meta,
            axis_color: self.axis_color,
            show_grid: self.show_grid,
        }
    }
}

/// A waveform envelope layer over `span`.
#[must_use]
pub fn waveform_layer(minmax: Vec<MinMax>, span: TimeSpan, style: LineStyle) -> Layer {
    Layer::Waveform {
        minmax,
        span,
        style,
    }
}

/// Computes a waveform min/max envelope of `buckets` columns from `samples`.
///
/// Each bucket spans a contiguous, near-equal share of the samples and holds
/// the exact minimum and maximum over its range. Returns an empty envelope
/// when `samples` is empty or `buckets` is zero.
#[must_use]
pub fn waveform_minmax(samples: &[f32], buckets: usize) -> Vec<MinMax> {
    if samples.is_empty() || buckets == 0 {
        return Vec::new();
    }
    let buckets = buckets.min(samples.len());
    let mut out = Vec::with_capacity(buckets);
    for i in 0..buckets {
        let lo = i * samples.len() / buckets;
        let hi = (i + 1) * samples.len() / buckets;
        let span = &samples[lo..hi];
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for &s in span {
            min = min.min(s);
            max = max.max(s);
        }
        out.push(MinMax { min, max });
    }
    out
}

/// A spectrogram layer built from a [`Tile`], storing raw decibels for
/// export-time colorization.
///
/// The time and frequency ranges come from the tile's snapped axes; `display`
/// and `colormap` are carried unevaluated so a backend calls
/// [`phonia_render::colorize`] at export.
#[cfg(feature = "analysis")]
#[must_use]
pub fn spectrogram_layer(tile: &Tile, display: DisplayMapping, colormap: Colormap) -> Layer {
    let width = tile.t_axis.len() as u32;
    let height = tile.f_axis.len() as u32;
    let t0 = tile.t_axis.first().copied().unwrap_or(0.0);
    let t1 = tile.t_axis.last().copied().unwrap_or(0.0);
    let f0 = tile.f_axis.first().copied().unwrap_or(0.0);
    let f1 = tile.f_axis.last().copied().unwrap_or(0.0);
    Layer::Spectrogram {
        db: tile.db.clone(),
        width,
        height,
        t: [t0, t1],
        f: [f0, f1],
        display,
        colormap,
    }
}

/// A pitch contour layer in `unit`.
///
/// Only voiced frames contribute points; unvoiced frames are omitted, so the
/// point series carries no non-finite values.
#[cfg(feature = "analysis")]
#[must_use]
pub fn pitch_layer(track: &PitchTrack, unit: PitchUnit, style: LineStyle) -> Layer {
    let points = track
        .frames()
        .iter()
        .filter_map(|frame| {
            frame.f0.map(|hz| {
                let value = match unit {
                    PitchUnit::Hertz => hz,
                    // Semitones re 1 Hz, Praat's Hertz-to-semitone reference.
                    PitchUnit::Semitones => 12.0 * hz.log2(),
                };
                (frame.time, value)
            })
        })
        .collect();
    Layer::PitchLine {
        points,
        unit,
        style,
    }
}

/// A formant speckle layer.
///
/// `smoothed` records whether `track` carries Viterbi-smoothed slots or raw
/// candidates, per the formant-tracking caveat.
#[cfg(feature = "analysis")]
#[must_use]
pub fn formant_layer(track: &FormantTrack, smoothed: bool, style: SpeckleStyle) -> Layer {
    let frames = track
        .frames
        .iter()
        .map(|frame| SpeckleFrame {
            time: frame.time,
            points: frame
                .formants
                .iter()
                .map(|p| SpecklePoint {
                    frequency: p.frequency,
                    bandwidth: p.bandwidth,
                })
                .collect(),
        })
        .collect();
    Layer::FormantSpeckle {
        frames,
        smoothed,
        style,
    }
}

/// An intensity contour layer, one point per analysis frame.
#[cfg(feature = "analysis")]
#[must_use]
pub fn intensity_layer(track: &IntensityTrack, style: LineStyle) -> Layer {
    Layer::IntensityLine {
        points: track.iter().collect(),
        style,
    }
}

/// A harmonicity (HNR) contour layer from `(time, hnr_db)` frames, in dB.
/// Unvoiced frames (no HNR) are dropped so the line spans only voiced regions.
#[must_use]
pub fn harmonicity_layer(frames: &[(f64, Option<f64>)], style: LineStyle) -> Layer {
    Layer::HarmonicityLine {
        points: frames
            .iter()
            .filter_map(|&(time, db)| db.map(|value| (time, value)))
            .collect(),
        style,
    }
}

/// Converts one annotation [`TierSlot`] into embedded [`TierData`].
#[must_use]
pub fn tier_data(slot: &TierSlot) -> TierData {
    match &slot.tier {
        Tier::Interval(tier) => TierData {
            name: tier.name.clone(),
            content: TierContent::Intervals(
                tier.intervals
                    .iter()
                    .map(|iv| IntervalData {
                        xmin: iv.xmin,
                        xmax: iv.xmax,
                        label: iv.label.clone(),
                    })
                    .collect(),
            ),
        },
        Tier::Point(tier) => TierData {
            name: tier.name.clone(),
            content: TierContent::Points(
                tier.points
                    .iter()
                    .map(|p| PointData {
                        time: p.time,
                        label: p.label.clone(),
                    })
                    .collect(),
            ),
        },
    }
}

/// A tiers layer embedding `slots` in order.
#[must_use]
pub fn tiers_layer(slots: &[TierSlot], style: TierStyle) -> Layer {
    Layer::Tiers {
        tiers: slots.iter().map(tier_data).collect(),
        style,
    }
}

/// A spectral-slice layer of `(frequency, dB)` bins built from a [`Slice`].
#[cfg(feature = "analysis")]
#[must_use]
pub fn spectral_slice_layer(slice: &Slice, style: LineStyle) -> Layer {
    let bins = slice
        .f_axis
        .iter()
        .zip(slice.db.iter())
        .map(|(&f, &db)| (f, f64::from(db)))
        .collect();
    Layer::SpectralSlice { bins, style }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harmonicity_layer_drops_unvoiced_frames() {
        let frames = [(0.0, Some(10.0)), (0.01, None), (0.02, Some(12.0))];
        match harmonicity_layer(&frames, LineStyle::default()) {
            Layer::HarmonicityLine { points, .. } => {
                assert_eq!(points, vec![(0.0, 10.0), (0.02, 12.0)]);
            }
            other => panic!("expected a HarmonicityLine layer, got {other:?}"),
        }
    }
}
