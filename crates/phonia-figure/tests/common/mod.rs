//! Shared reference-figure fixture for the integration tests and examples.
//!
//! This lives outside the library (`cargo package` cannot ship an
//! `include_bytes!` reaching past the crate root) and reads the repo's
//! fixture WAV and TextGrid from disk at test/example run time instead of
//! embedding them at compile time.

use std::path::{Path, PathBuf};

use phonia_figure::{
    Axis, Figure, FigureBuilder, LayerKind, LengthUnit, LineStyle, Panel, PitchUnit,
    ProvenanceRecord, SizeSpec, TierStyle, pitch_layer, spectrogram_layer, tiers_layer,
    waveform_layer, waveform_minmax,
};
use phonia_pitch::{PitchParams, TimeSpan};
use phonia_render::{Colormap, DisplayMapping, Theme};
use phonia_spectrogram::{SpectrogramParams, TileRequest};

/// The repo's `tests/fixtures` directory, two levels above this crate's root.
fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn provenance(layer: LayerKind, params: &[(&str, String)]) -> ProvenanceRecord {
    ProvenanceRecord {
        layer,
        params: params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        smoothed: None,
    }
}

fn spectrogram_provenance(params: &SpectrogramParams) -> ProvenanceRecord {
    provenance(
        LayerKind::Spectrogram,
        &[
            ("window_length_s", format!("{}", params.window_length)),
            ("max_frequency_hz", format!("{}", params.max_frequency)),
            ("time_step_s", format!("{}", params.time_step)),
            ("frequency_step_hz", format!("{}", params.frequency_step)),
        ],
    )
}

fn pitch_provenance(params: &PitchParams) -> ProvenanceRecord {
    let step = params
        .time_step
        .map_or_else(|| "auto".to_owned(), |s| format!("{s}"));
    provenance(
        LayerKind::Pitch,
        &[
            ("floor_hz", format!("{}", params.floor_hz)),
            ("ceiling_hz", format!("{}", params.ceiling_hz)),
            ("time_step_s", step),
        ],
    )
}

fn tier_name(slot: &phonia_annot::TierSlot) -> &str {
    match &slot.tier {
        phonia_annot::Tier::Interval(tier) => &tier.name,
        phonia_annot::Tier::Point(tier) => &tier.name,
    }
}

/// Builds the roadmap gate figure: waveform, spectrogram, pitch, and one tier
/// from `arctic_bdl_a0001` and its fixture TextGrid.
///
/// Every export backend reuses this figure as its gate input. The figure runs
/// the default spectrogram and pitch analyses over the whole clip, envelopes
/// the mono mix, and embeds the TextGrid's `words` interval tier.
///
/// # Panics
/// Panics if the bundled fixture WAV or TextGrid is missing or fails to
/// decode, or if the hardcoded tile request or default pitch parameters are
/// rejected; any of these means the fixtures, the repo layout or this code
/// is broken.
#[must_use]
pub fn reference_figure() -> Figure {
    use phonia_audio::Audio;
    use phonia_pitch::pitch_track;
    use phonia_spectrogram::compute_tile;

    let wav = std::fs::read(fixtures_dir().join("audio/arctic_bdl_a0001.wav"))
        .expect("reference fixture WAV must exist");
    let textgrid =
        std::fs::read(fixtures_dir().join("textgrids/arctic_bdl_a0001_long_utf8.TextGrid"))
            .expect("reference fixture TextGrid must exist");

    let audio = Audio::from_wav_bytes(&wav).expect("reference fixture WAV must decode");
    let frames = audio.frames();
    let duration = audio.duration();

    let spec_params = SpectrogramParams::default();
    let tile = compute_tile(
        audio.slice_samples(0..frames),
        &TileRequest {
            t0: 0.0,
            t1: duration,
            f0: 0.0,
            f1: spec_params.max_frequency,
            width_px: 800,
            height_px: 256,
            params: spec_params,
        },
    )
    .expect("reference tile request is valid");

    let pitch_params = PitchParams::default();
    let pitch = pitch_track(audio.slice_samples(0..frames), &pitch_params)
        .expect("default pitch params are valid");

    let mono = audio.mono_mix();
    let envelope = waveform_minmax(&mono, 1000);
    let span = TimeSpan::new(0.0, duration);

    let (annotation, _) =
        phonia_textgrid::read(&textgrid).expect("reference fixture TextGrid must parse");
    let words = annotation
        .tiers()
        .iter()
        .find(|slot| tier_name(slot) == "words")
        .or_else(|| annotation.tiers().first())
        .expect("reference TextGrid must have at least one tier");

    let time_axis = || Axis::linear(0.0, duration, Some("Time"), Some("s"));

    let waveform_panel = Panel {
        layers: vec![waveform_layer(envelope, span, LineStyle::default())],
        time_axis: time_axis(),
        value_axis: Axis::linear(-1.0, 1.0, Some("Amplitude"), None),
        height_share: 0.22,
    };

    let spectrogram_panel = Panel {
        layers: vec![spectrogram_layer(
            &tile,
            DisplayMapping::default(),
            Colormap::Viridis,
        )],
        time_axis: time_axis(),
        value_axis: Axis::linear(
            0.0,
            spec_params.max_frequency,
            Some("Frequency"),
            Some("Hz"),
        ),
        height_share: 0.44,
    };

    let pitch_panel = Panel {
        layers: vec![pitch_layer(&pitch, PitchUnit::Hertz, LineStyle::default())],
        time_axis: time_axis(),
        value_axis: Axis::linear(
            pitch_params.floor_hz,
            pitch_params.ceiling_hz,
            Some("Pitch"),
            Some("Hz"),
        ),
        height_share: 0.22,
    };

    let tier_panel = Panel {
        layers: vec![tiers_layer(
            std::slice::from_ref(words),
            TierStyle::default(),
        )],
        time_axis: time_axis(),
        value_axis: Axis::linear(0.0, 1.0, None, None),
        height_share: 0.12,
    };

    FigureBuilder::new(SizeSpec::new(16.0, 12.0, LengthUnit::Cm), Theme::Dark)
        .panel(waveform_panel)
        .panel(spectrogram_panel)
        .panel(pitch_panel)
        .panel(tier_panel)
        .source(spectrogram_provenance(&spec_params))
        .source(pitch_provenance(&pitch_params))
        .build()
}
