//! The one API both frontends and future bindings consume: commands with
//! explicit arguments, journaled unified undo, content-addressed analysis
//! cache.
//!
//! The mutation surface is journaled end to end: every change to session
//! state — audio import, annotation attachment, tier lifecycle, and the
//! boundary and label edits of the annotation loop — records an id-stable
//! inverse so [`Engine::undo`] and [`Engine::redo`] restore
//! state-hash-identical documents (design rule 5, `docs/plan/architecture.md`;
//! invariant 5, `docs/plan/validation.md`). Most of it goes through
//! [`Engine::apply`] against a [`Command`]; the three entry points that add
//! audio outside the command surface — [`Engine::import_audio_bytes`],
//! [`Engine::open_streaming_wav`], and [`Engine::finish_recording`] — record
//! the same kind of journal entry directly, since a streamed source's byte
//! reader and a recording's accumulated samples cannot ride inside a `Command`
//! (not `Clone`/`Serialize`). The journal is in memory; persisting a session's
//! history to the project file arrives with `phx-project` in phase 4. Analyses
//! (pitch, formants, intensity, spectrogram tiles) stay outside the journal —
//! they are pure functions of `(audio, params)` and never mutate a document.
#![warn(missing_docs)]

mod commands;
mod document;
mod error;
mod figure;
mod journal;
mod pyramid;
mod recording;
mod store;
mod stream_pyramid;
mod tile_cache;

use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use phx_spectrogram::{analysis_axes_dims, select_axis_indices};

use tile_cache::{BlockKey, TILE_COLS, TileCache, params_hash};

use phx_audio::{Audio, ResampleQuality};
use phx_dsp::{RealFftPlan, Window, window_samples};

use std::sync::Arc;

use stream_pyramid::StreamPyramid;

use document::DocumentStore;
use journal::{Journal, Reverse, Transition};

pub use commands::{Applied, Command, EngineHit};
pub use document::{AnnotationId, Document};
pub use error::EngineError;
pub use figure::{
    ExportBundle, FigureColormap, FigureFormat, FigurePitchUnit, FigureRequest, FigureTheme,
    FigureUnit, LayerToggles, default_figure_request, export_figure, figure_to_svg,
};
pub use phx_annot::{
    AlignMode, Annotation, AnnotationError, BoundaryId, BoundaryMove, Hit, IntegrityIssue,
    Interval, IntervalId, IntervalTier, LabelPattern, LabelQuery, LabelTarget, MatchSpan, Merged,
    Moved, Point, PointId, PointTier, Tier, TierId, TierKind, TierMerge, TierRelation, TierSlot,
};
pub use phx_audio::{
    AudioError, BitDepth, ByteReader, BytesReader, StreamSampleFormat, StreamingWav, WavStreamInfo,
};
pub use phx_figure::Figure;
pub use phx_formant::{FormantFrame, FormantParams, FormantPoint, FormantTrack};
pub use phx_intensity::{IntensityParams, IntensityTrack};
pub use phx_pitch::{PitchFrame, PitchParams, PitchTrack, TimeSpan};
pub use phx_render::{Colormap, DisplayMapping, Theme};
pub use phx_spectrogram::{SpectrogramParams, Tile, TileRequest};
pub use phx_voice::{
    CppParams, HarmonicityParams, JitterMeasures, Moments, PitchSummary, PointProcess, PulseParams,
    ShimmerMeasures, VoiceBreaks, VoiceReport,
};
pub use pyramid::MinMax;
pub use recording::{FinishedRecording, RecordingId};
pub use store::{AudioId, AudioInfo, AudioStore, SampleAccess};

use recording::RecordingStore;

/// Frame count at or below which audio stays on the eager whole-decode path.
///
/// Two minutes at 48 kHz. Below it, a per-sample pyramid and a resident buffer
/// cost little and keep every analysis a borrow away; above it, the streamed
/// path keeps opening and scrolling bounded. A frontend reads this through
/// [`Engine::eager_import_frame_limit`].
const EAGER_MAX_FRAMES: usize = 48_000 * 120;

/// The measurement readout for a time–frequency selection.
///
/// Geometry (`t0`/`t1`/`f0`/`f1`/`duration`) is the box in signal coordinates;
/// the remaining fields are engine queries over it, so a selection bar built
/// from this struct shows exactly what a script reading the same API would.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionReadout {
    /// Selection start time in seconds.
    pub t0: f64,
    /// Selection end time in seconds.
    pub t1: f64,
    /// Selection low frequency in hertz.
    pub f0: f64,
    /// Selection high frequency in hertz.
    pub f1: f64,
    /// Selection duration in seconds.
    pub duration: f64,
    /// Mean voiced fundamental over the span, in hertz.
    pub f0_mean_hz: Option<f64>,
    /// Minimum voiced fundamental over the span, in hertz.
    pub f0_min_hz: Option<f64>,
    /// Maximum voiced fundamental over the span, in hertz.
    pub f0_max_hz: Option<f64>,
    /// Sample standard deviation of the voiced fundamental over the span, in
    /// hertz, absent with fewer than two voiced frames.
    pub f0_sd_hz: Option<f64>,
    /// 5th-percentile voiced fundamental over the span, in hertz — the low bound
    /// robust to the octave dips the minimum catches.
    pub f0_p5_hz: Option<f64>,
    /// 95th-percentile voiced fundamental over the span, in hertz.
    pub f0_p95_hz: Option<f64>,
    /// Mean raw band energy inside the box, in decibels.
    pub band_energy_db: f64,
    /// Mean intensity over the span, in dB SPL, absent when the span is empty.
    pub intensity_mean_db: Option<f64>,
    /// Sample standard deviation of intensity over the span, in decibels, absent
    /// with fewer than two frames.
    pub intensity_sd_db: Option<f64>,
    /// Mean harmonics-to-noise ratio over the span, in decibels.
    pub hnr_mean_db: Option<f64>,
    /// Root-mean-square amplitude over the span, absent when the span is empty.
    pub rms: Option<f64>,
    /// Largest absolute sample amplitude over the span, absent when the span is
    /// empty — Praat's "Get absolute extremum", for clipping and level checks.
    pub peak: Option<f64>,
    /// Power-weighted spectral centre of gravity in hertz, from the spectral
    /// slice at the span midpoint.
    pub spectral_cog_hz: Option<f64>,
    /// Power-weighted spectral standard deviation in hertz.
    pub spectral_sd_hz: Option<f64>,
    /// Power-weighted spectral skewness.
    pub spectral_skewness: Option<f64>,
    /// Power-weighted spectral kurtosis.
    pub spectral_kurtosis: Option<f64>,
}

/// Session engine: the audio store plus the pure functions that read it.
///
/// Every method beyond store bookkeeping is stateless-by-arguments — the
/// same `(id, params)` pair always produces the same result, independent of
/// call order, viewport, or any other implicit state (rule 1,
/// `docs/plan/architecture.md`).
#[derive(Default)]
pub struct Engine {
    store: AudioStore,
    documents: DocumentStore,
    journal: Journal,
    recordings: RecordingStore,
    tiles: Mutex<TileCache>,
    /// FFT plans reused across band-filtered span renders, so the box-selection
    /// replay a frontend fires while a user drags does not rebuild a plan per
    /// call.
    filter_plan: RealFftPlan,
}

impl Engine {
    /// Creates an engine with an empty audio store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes a WAV, AIFF, or FLAC byte buffer and returns the id of the new
    /// store entry.
    ///
    /// The container is detected from its leading signature (see
    /// [`phx_audio::Audio::from_bytes`]); a caller does not choose the format
    /// ahead of time.
    ///
    /// Journaled exactly like [`Command::ImportAudio`]: [`Engine::undo`] drops
    /// the buffer and [`Engine::redo`] reinserts the clone captured here.
    ///
    /// # Errors
    /// Returns [`EngineError::Audio`] with [`AudioError::UnrecognizedFormat`]
    /// when the bytes match none of the three container signatures, or with a
    /// container-specific `Malformed*`/`Unsupported*` variant when the bytes
    /// are a recognized container this crate cannot decode (see
    /// [`phx_audio::Audio::from_bytes`]).
    pub fn import_audio_bytes(&mut self, bytes: &[u8]) -> Result<AudioId, EngineError> {
        let audio = Audio::from_bytes(bytes)?;
        Ok(self.journal_eager_import(audio))
    }

    /// Opens a WAV from a byte reader as a streamed source and returns its id.
    ///
    /// The header is parsed and the bounded waveform pyramid built in one
    /// streaming pass; the decoded signal is never held whole, so metadata is
    /// ready at header speed and the waveform scrolls without the full-decode
    /// footprint. This is the path for recordings past the eager comfort
    /// threshold ([`EAGER_MAX_FRAMES`]) — an hour-long take the desktop shell
    /// backs with a file handle or the web worker with an OPFS access handle.
    /// Whole-signal analysis of a streamed source still materializes it on
    /// demand ([`Engine::pitch_track`] and the other whole-signal contours);
    /// the streamed win is that opening and scrolling never pay that cost.
    ///
    /// `name` is the display name the metadata surface reports.
    ///
    /// Journaled like any other import: [`Engine::undo`] detaches the source
    /// (parking it, not dropping it) and [`Engine::redo`] reattaches the same
    /// parked source, so neither direction reopens the reader or rereads a
    /// byte. A reader is not `Clone`/`Serialize`, so it cannot ride in a
    /// [`Command`] the way [`Command::ImportAudio`] carries its byte buffer;
    /// detach/reattach is the id-stable inverse pair [`Command::DetachAudio`]
    /// already uses, reused here since a fresh import has no documents to
    /// cascade.
    ///
    /// # Errors
    /// Returns [`EngineError::Audio`] when the header is malformed, the sample
    /// format is unsupported, or the backing store fails during the pyramid
    /// pass.
    pub fn open_streaming_wav(
        &mut self,
        reader: impl ByteReader + Send + Sync + 'static,
        name: Option<String>,
    ) -> Result<AudioId, EngineError> {
        let source = Arc::new(StreamingWav::open(reader)?);
        let pyramid = StreamPyramid::build(&source)?;
        let id = self.store.insert_streamed(source, pyramid, name);
        self.journal_streamed_import(id);
        Ok(id)
    }

    /// Frame count at or below which an import stays eager (whole-signal
    /// decode). Two minutes of 48 kHz audio; longer takes belong on the
    /// streamed path so their decoded footprint and per-sample pyramid never
    /// enter memory. Frontends deciding between [`Engine::import_audio_bytes`]
    /// and [`Engine::open_streaming_wav`] read this bound from
    /// [`Engine::eager_import_frame_limit`].
    #[must_use]
    pub fn eager_import_frame_limit() -> usize {
        EAGER_MAX_FRAMES
    }

    /// Opens a streaming recording and returns its id.
    ///
    /// `sample_rate` is the true capture rate (the host reads it from the
    /// audio device, never assumes one) and `channels` its channel count.
    /// Sample chunks arrive through [`Engine::append_samples`]; the take stays
    /// out of the audio store until [`Engine::finish_recording`] materializes
    /// it, so nothing queries a half-captured buffer.
    ///
    /// # Errors
    /// Returns [`EngineError::InvalidRequest`] when `sample_rate` is not finite
    /// and positive, or when `channels` is zero.
    pub fn begin_recording(
        &mut self,
        sample_rate: f64,
        channels: usize,
    ) -> Result<RecordingId, EngineError> {
        self.recordings.begin(sample_rate, channels)
    }

    /// Appends one planar sample chunk to an open recording.
    ///
    /// `planar` carries every channel's samples for this chunk back to back,
    /// so its length must divide evenly by the take's channel count. Chunks
    /// accumulate in memory until the take is finished or aborted.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownRecordingId`] when `id` names no open
    /// take, and [`EngineError::InvalidRequest`] when `planar` does not divide
    /// evenly by the channel count.
    pub fn append_samples(&mut self, id: RecordingId, planar: &[f32]) -> Result<(), EngineError> {
        self.recordings.append(id, planar)
    }

    /// Finishes a recording, materializing it as a store entry and returning
    /// that id alongside the take encoded as WAV bytes.
    ///
    /// The store entry is the same kind of buffer an import produces, so every
    /// analysis reads a recorded take exactly as it reads an imported file. The
    /// WAV bytes (24-bit PCM, lossless for the captured `[-1, 1]` signal) let
    /// the host persist the take beside imported media through its own storage
    /// path. Finishing consumes the take; the id is invalid afterwards.
    ///
    /// Journaled exactly like [`Engine::import_audio_bytes`]: [`Engine::undo`]
    /// drops the materialized take and [`Engine::redo`] reinserts it.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownRecordingId`] when `id` names no open
    /// take, and [`EngineError::Audio`] when the accumulated samples cannot
    /// form an audio buffer (an empty take, or one too large to allocate).
    pub fn finish_recording(
        &mut self,
        id: RecordingId,
        name: String,
    ) -> Result<FinishedRecording, EngineError> {
        let take = self.recordings.finish(id)?;
        let audio = Audio::new(take.channels, take.sample_rate)?.with_name(name);
        let wav = audio.to_wav_bytes(BitDepth::Pcm24)?;
        let audio = self.journal_eager_import(audio);
        Ok(FinishedRecording { audio, wav })
    }

    /// Discards an open recording without materializing it.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownRecordingId`] when `id` names no open take.
    pub fn abort_recording(&mut self, id: RecordingId) -> Result<(), EngineError> {
        self.recordings.abort(id)
    }

    /// Returns duration, sample rate, channel count, and name for `id`.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a
    /// live store entry.
    pub fn audio_info(&self, id: AudioId) -> Result<AudioInfo, EngineError> {
        self.store.info(id)
    }

    /// Returns `px` [`MinMax`] buckets covering `[t0, t1)` seconds of `id`,
    /// read from its cached waveform pyramid.
    ///
    /// `t0`/`t1` may be given in either order and are clamped to the
    /// signal's duration; each bucket's min/max agrees exactly with a direct
    /// scan of the same underlying sample range (see the [`pyramid`] module
    /// doc for why the pyramid combine is exact, not approximate).
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a
    /// live store entry, and [`EngineError::InvalidRequest`] when `t0`/`t1`
    /// are not finite.
    pub fn waveform_slice(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        px: u32,
    ) -> Result<Vec<MinMax>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "waveform_slice t0/t1 must be finite".to_string(),
            });
        }
        self.store.waveform(id, t0, t1, px)
    }

    /// Computes a spectrogram tile for `id` and colorizes it to RGBA bytes.
    ///
    /// Composes [`phx_spectrogram::compute_tile`] (raw PSD-derived dB,
    /// snapped to the object-level frame grid so adjacent tile requests
    /// share columns exactly) with [`phx_render::colorize`] (linear-in-dB
    /// clip against `display`, then a perceptual colormap lookup tuned for
    /// `theme`). The whole audio buffer is always passed to
    /// `compute_tile` — never just the `[t0, t1)` window `req` names — so
    /// the frame grid stays a function of the signal alone, not the
    /// viewport.
    ///
    /// Returns `4 * req.width_px * req.height_px` bytes, `R, G, B, A` per
    /// pixel, row 0 first.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a
    /// live store entry, and [`EngineError::InvalidRequest`] when `req`
    /// carries a non-finite bound or a non-positive analysis parameter, or
    /// when the audio is too short for the requested window to produce a
    /// single analysis frame.
    pub fn spectrogram_tile_rgba(
        &self,
        id: AudioId,
        req: &TileRequest,
        display: &DisplayMapping,
        colormap: Colormap,
        invert: bool,
        preemphasis: bool,
    ) -> Result<Vec<u8>, EngineError> {
        validate_tile_request(req)?;
        let tile_db = self.spectrogram_tile_db(id, req, preemphasis)?;

        let expected_len = req.width_px as usize * req.height_px as usize;
        if tile_db.len() != expected_len {
            return Err(EngineError::InvalidRequest {
                reason: format!(
                    "tile produced {} values for a {}x{} request; the audio is likely too \
                     short, or the time/frequency range too narrow, to fit a single analysis \
                     frame",
                    tile_db.len(),
                    req.width_px,
                    req.height_px
                ),
            });
        }

        Ok(phx_render::colorize(
            &tile_db,
            req.width_px,
            req.height_px,
            display,
            colormap,
            invert,
        ))
    }

    /// Computes a spectrogram tile for `id` and colorizes it with a custom
    /// 256-entry 8-bit sRGB lookup table — a ramp built in the gradient editor
    /// rather than a built-in [`Colormap`].
    ///
    /// The raw dB is read from the same block cache
    /// [`Engine::spectrogram_tile_rgba`] uses, so switching between a built-in
    /// palette and a custom ramp, or between two custom ramps, re-colorizes
    /// cached dB and never recomputes the STFT.
    ///
    /// Returns `4 * req.width_px * req.height_px` bytes, `R, G, B, A` per
    /// pixel, row 0 first.
    ///
    /// # Errors
    /// Same conditions as [`Engine::spectrogram_tile_rgba`].
    pub fn spectrogram_tile_rgba_lut(
        &self,
        id: AudioId,
        req: &TileRequest,
        display: &DisplayMapping,
        lut: &[[u8; 3]; 256],
        invert: bool,
        preemphasis: bool,
    ) -> Result<Vec<u8>, EngineError> {
        validate_tile_request(req)?;
        let tile_db = self.spectrogram_tile_db(id, req, preemphasis)?;

        let expected_len = req.width_px as usize * req.height_px as usize;
        if tile_db.len() != expected_len {
            return Err(EngineError::InvalidRequest {
                reason: format!(
                    "tile produced {} values for a {}x{} request; the audio is likely too \
                     short, or the time/frequency range too narrow, to fit a single analysis \
                     frame",
                    tile_db.len(),
                    req.width_px,
                    req.height_px
                ),
            });
        }

        Ok(phx_render::colorize_with_lut(
            &tile_db,
            req.width_px,
            req.height_px,
            display,
            lut,
            invert,
        ))
    }

    /// Assembles the raw dB values for a tile from the block cache, in the
    /// row-major, lowest-frequency-first order [`phx_render::colorize`] expects.
    ///
    /// The frame columns the request selects are grouped into fixed
    /// [`tile_cache`] blocks aligned to the object-level frame grid; each block's
    /// STFT is computed once and reused, so a colormap, theme, or dynamic-range
    /// change re-colorizes cached dB without recomputing the transform. The
    /// values are bit-for-bit identical to a direct `compute_tile`, since both
    /// read the same frame centres off the same grid.
    fn spectrogram_tile_db(
        &self,
        id: AudioId,
        req: &TileRequest,
        preemphasis: bool,
    ) -> Result<Vec<f32>, EngineError> {
        // Axes come from the header dimensions alone, so a streamed source picks
        // the same tile columns without decoding a sample; each needed block is
        // then computed bounded (whole-buffer for eager, ranged read for
        // streamed) and cached, never materializing the whole signal.
        let info = self.store.info(id)?;
        let axes = analysis_axes_dims(info.sample_rate, info.duration, &req.params);
        let freq_len = axes.frequencies.len();

        let time_indices = select_axis_indices(
            &axes.times,
            req.t0.min(req.t1),
            req.t0.max(req.t1),
            req.width_px as usize,
        );
        let freq_indices = select_axis_indices(
            &axes.frequencies,
            req.f0.min(req.f1),
            req.f0.max(req.f1),
            req.height_px as usize,
        );
        if time_indices.is_empty() || freq_indices.is_empty() {
            return Ok(Vec::new());
        }

        let hash = params_hash(&req.params);
        let mut needed: Vec<usize> = time_indices.iter().map(|&t| t / TILE_COLS).collect();
        needed.sort_unstable();
        needed.dedup();

        let mut blocks: std::collections::HashMap<usize, phx_spectrogram::ColumnBlock> =
            std::collections::HashMap::with_capacity(needed.len());
        for &block_index in &needed {
            let key = BlockKey {
                audio: id.as_u64(),
                params_hash: hash,
                block_index,
            };
            let hit = self.tiles.lock().expect("tile cache poisoned").get(key);
            let block = match hit {
                Some(block) => block,
                None => {
                    let block = self.store.column_block(
                        id,
                        &req.params,
                        block_index * TILE_COLS,
                        TILE_COLS,
                    )?;
                    self.tiles
                        .lock()
                        .expect("tile cache poisoned")
                        .insert(key, block.clone());
                    block
                }
            };
            blocks.insert(block_index, block);
        }

        let mut db = Vec::with_capacity(freq_indices.len() * time_indices.len());
        for &f in &freq_indices {
            // Display pre-emphasis lifts the higher frequencies of the rendered
            // tile by row; it never touches the cached block dB, so the cache
            // key and the raw hover readouts stay unchanged.
            let freq = axes.frequencies[f];
            for &t in &time_indices {
                let block = &blocks[&(t / TILE_COLS)];
                let local = t - block.first_col;
                let value = block.db[local * freq_len + f];
                db.push(if preemphasis {
                    phx_spectrogram::apply_display_preemphasis_db(f64::from(value), freq) as f32
                } else {
                    value
                });
            }
        }
        Ok(db)
    }

    /// Number of raw dB spectrogram blocks currently held in the tile cache.
    ///
    /// Exposed for the frontend perf probe: a colormap or theme change must
    /// leave this count unchanged, since it re-colorizes cached dB rather than
    /// recomputing the STFT.
    #[must_use]
    pub fn spectrogram_cached_block_count(&self) -> usize {
        self.tiles.lock().expect("tile cache poisoned").len()
    }

    /// Computes the autocorrelation pitch track of `id` over its whole signal.
    ///
    /// The track sits on a frame grid derived from the audio duration alone,
    /// so a value queried at time *t* is the same at any zoom or scroll
    /// (rule 2, `docs/plan/architecture.md`). `phx_pitch::pitch_track` returns
    /// an empty track for parameters it cannot analyse rather than panicking,
    /// so this method never surfaces a parameter error of its own.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry.
    pub fn pitch_track(
        &self,
        id: AudioId,
        params: &PitchParams,
    ) -> Result<PitchTrack, EngineError> {
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let view = audio.slice_samples(0..audio.frames());
        Ok(phx_pitch::pitch_track(view, params))
    }

    /// Computes pitch over just the samples spanning `[t0, t1)` seconds,
    /// returning the track together with the absolute start time of the
    /// analysed slice.
    ///
    /// This is the fast preview a live parameter edit renders first: pitch is
    /// the one contour whose whole-signal cost grows with duration, so the
    /// visible window is analysed on its own before the full-signal
    /// [`Engine::pitch_track`] result (the authoritative, zoom-independent one)
    /// replaces it. Frame times are relative to the slice; add the returned
    /// start time to place them on the absolute timeline. Because the Viterbi
    /// path here sees only the windowed frames, the preview can differ from the
    /// full track near the window edges — the whole-signal result is the one
    /// callers keep.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when `t0`/`t1` are not
    /// finite.
    pub fn pitch_track_span(
        &self,
        id: AudioId,
        params: &PitchParams,
        t0: f64,
        t1: f64,
    ) -> Result<(PitchTrack, f64), EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "pitch_track_span t0/t1 must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let sample_rate = info.sample_rate;
        let frames = (info.duration * sample_rate).round() as usize;
        let lo = t0.min(t1).clamp(0.0, info.duration);
        let hi = t0.max(t1).clamp(0.0, info.duration);
        let start = ((lo * sample_rate).floor() as usize).min(frames);
        let end = ((hi * sample_rate).ceil() as usize).clamp(start, frames);
        // The span is a viewport window; decode only its samples so a streamed
        // source never materializes the whole signal for a preview.
        let window = self.store.range_owned(id, start, end)?;
        let track = phx_pitch::pitch_track(window.slice_samples(0..window.frames()), params);
        Ok((track, start as f64 / sample_rate))
    }

    /// Per-frame harmonics-to-noise ratio (dB) over the span `[t0, t1]` of `id`,
    /// as `(time, hnr_db)` pairs — Praat's Sound → To Harmonicity, one value per
    /// frame instead of a single mean. Silent or aperiodic frames carry `None`;
    /// times are absolute.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry and [`EngineError::InvalidRequest`] when a bound is not finite.
    pub fn harmonicity_track_span(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
    ) -> Result<Vec<(f64, Option<f64>)>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "harmonicity_track_span t0/t1 must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let sample_rate = info.sample_rate;
        let frames = (info.duration * sample_rate).round() as usize;
        let lo = t0.min(t1).clamp(0.0, info.duration);
        let hi = t0.max(t1).clamp(0.0, info.duration);
        let start = ((lo * sample_rate).floor() as usize).min(frames);
        let end = ((hi * sample_rate).ceil() as usize).clamp(start, frames);
        let window = self.store.range_owned(id, start, end)?;
        let offset = start as f64 / sample_rate;
        let track = phx_voice::hnr_track(
            window.slice_samples(0..window.frames()),
            &HarmonicityParams::default(),
        );
        Ok(track
            .frames
            .iter()
            .map(|frame| (offset + frame.time, frame.hnr_db))
            .collect())
    }

    /// Computes a per-frame cepstral-peak-prominence (CPP) track over `[t0, t1]`
    /// as `(time, cpp_db)` pairs — a voice-quality contour where a high value
    /// marks strong harmonic organization and a low value marks breathiness or
    /// aperiodicity. Frames without a usable cepstral peak carry `None`; times
    /// are absolute.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry and [`EngineError::InvalidRequest`] when a bound is not finite.
    pub fn cpp_track_span(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
    ) -> Result<Vec<(f64, Option<f64>)>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "cpp_track_span t0/t1 must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let sample_rate = info.sample_rate;
        let frames = (info.duration * sample_rate).round() as usize;
        let lo = t0.min(t1).clamp(0.0, info.duration);
        let hi = t0.max(t1).clamp(0.0, info.duration);
        let start = ((lo * sample_rate).floor() as usize).min(frames);
        let end = ((hi * sample_rate).ceil() as usize).clamp(start, frames);
        let window = self.store.range_owned(id, start, end)?;
        let offset = start as f64 / sample_rate;
        let track = phx_voice::cpp_track(
            window.slice_samples(0..window.frames()),
            &CppParams::default(),
        );
        Ok(track
            .frames
            .iter()
            .map(|frame| (offset + frame.time, frame.cpp_db))
            .collect())
    }

    /// Computes the raw Burg formant candidates of `id` over its whole signal.
    ///
    /// These are the frequency-gated LPC roots per frame, before any tracking
    /// reassigns them to formant slots — the display default while the
    /// tracking weights remain provisional (`docs/plan/tasks/phase-4.md`).
    /// Call [`Engine::formant_track_smoothed`] for the Viterbi-tracked view.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a formant
    /// parameter is outside the range `phx_formant` accepts (its analysis
    /// entry point asserts these, so the engine boundary checks them first).
    pub fn formant_track(
        &self,
        id: AudioId,
        params: &FormantParams,
    ) -> Result<FormantTrack, EngineError> {
        validate_formant_params(params)?;
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let view = audio.slice_samples(0..audio.frames());
        Ok(phx_formant::formant_track(view, params))
    }

    /// Computes Xia–Espy-Wilson smoothed formants of `id` over its whole
    /// signal, using the crate's default neutral references and cost weights.
    ///
    /// Those weights are documented as provisional
    /// (`docs/plan/tasks/phase-4.md`); the UI surfaces this track only behind
    /// an explicit toggle and marks it as such.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a formant
    /// parameter is outside the range `phx_formant` accepts.
    pub fn formant_track_smoothed(
        &self,
        id: AudioId,
        params: &FormantParams,
    ) -> Result<FormantTrack, EngineError> {
        let raw = self.formant_track(id, params)?;
        Ok(phx_formant::track_smoothed(
            &raw,
            &phx_formant::TrackingRefs::default(),
        ))
    }

    /// Computes the intensity contour of `id` over its whole signal.
    ///
    /// The contour sits on a frame grid derived from the audio duration
    /// alone (rule 2, `docs/plan/architecture.md`).
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when the pitch floor
    /// is not finite and positive.
    pub fn intensity_track(
        &self,
        id: AudioId,
        params: &IntensityParams,
    ) -> Result<IntensityTrack, EngineError> {
        if !(params.pitch_floor_hz.is_finite() && params.pitch_floor_hz > 0.0) {
            return Err(EngineError::InvalidRequest {
                reason: "intensity pitch_floor_hz must be finite and positive".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let view = audio.slice_samples(0..audio.frames());
        Ok(phx_intensity::intensity_track(view, params))
    }

    /// Returns the mean raw power spectral density inside a time–frequency box,
    /// in decibels.
    ///
    /// The box is the spectrogram selection: `[t0, t1]` seconds by `[f0, f1]`
    /// hertz, each pair accepted in either order and clamped to the signal. The
    /// value is the analysis grid's raw PSD (no display pre-emphasis), averaged
    /// as linear power over every snapped cell that falls inside the box, then
    /// converted back to decibels — a function of the signal and the box alone,
    /// so the readout equals this query at identical coordinates (the
    /// batch-equals-GUI invariant, `docs/plan/tasks/phase-4.md` T4.4).
    ///
    /// Returns `f64::NEG_INFINITY` for an empty box (no analysis cell falls
    /// inside it).
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn band_energy(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        f0: f64,
        f1: f64,
    ) -> Result<f64, EngineError> {
        if ![t0, t1, f0, f1].iter().all(|value| value.is_finite()) {
            return Err(EngineError::InvalidRequest {
                reason: "band_energy bounds must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
        let (flo, fhi) = ordered_clamped(f0, f1, 0.0, f64::INFINITY);
        if hi - lo <= 0.0 || fhi - flo <= 0.0 {
            return Ok(f64::NEG_INFINITY);
        }
        let params = SpectrogramParams {
            max_frequency: fhi.max(SpectrogramParams::default().max_frequency),
            ..SpectrogramParams::default()
        };
        // Match the tile resolution to the analysis grid so every snapped
        // frame and frequency bin inside the box contributes about once.
        let time_step = phx_spectrogram::effective_time_step(&params);
        let frequency_step = phx_spectrogram::effective_frequency_step(&params);
        let width = (((hi - lo) / time_step).ceil() as u32).clamp(1, 4096);
        let height = (((fhi - flo) / frequency_step).ceil() as u32).clamp(1, 4096);
        let req = TileRequest {
            t0: lo,
            t1: hi,
            f0: flo,
            f1: fhi,
            width_px: width,
            height_px: height,
            params,
        };
        let view = audio.slice_samples(0..audio.frames());
        let tile = phx_spectrogram::compute_tile(view, &req);
        if tile.db.is_empty() {
            return Ok(f64::NEG_INFINITY);
        }
        let mut sum = 0.0;
        for &db in &tile.db {
            sum += 10.0_f64.powf(f64::from(db) / 10.0);
        }
        Ok(10.0 * (sum / tile.db.len() as f64).log10())
    }

    /// Computes the measurement readout for a selection: its geometry plus the
    /// span statistics the selection bar shows.
    ///
    /// Every number is an engine query over the selection, so the bar displays
    /// exactly what a script reading this API would get for the same box (the
    /// batch-equals-GUI invariant). Pitch statistics cover voiced frames inside
    /// the span; band energy comes from [`Engine::band_energy`]; mean intensity
    /// and mean HNR are frame means over the span, absent when the span holds no
    /// frame.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    #[allow(clippy::too_many_arguments)]
    pub fn selection_readout(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        f0: f64,
        f1: f64,
        pitch_floor_hz: f64,
        pitch_ceiling_hz: f64,
        intensity_floor_hz: f64,
    ) -> Result<SelectionReadout, EngineError> {
        if ![t0, t1, f0, f1].iter().all(|value| value.is_finite()) {
            return Err(EngineError::InvalidRequest {
                reason: "selection_readout bounds must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
        let (flo, fhi) = ordered_clamped(f0, f1, 0.0, f64::INFINITY);
        let span = TimeSpan::new(lo, hi);
        let view = audio.slice_samples(0..audio.frames());

        // Spectral moments from a single windowed FFT over the whole selection,
        // plus the span's raw RMS and absolute-peak amplitude off the same slice.
        let (moments, rms, peak) = {
            let sr = audio.sample_rate();
            let start = (lo * sr).floor().max(0.0) as usize;
            let end = ((hi * sr).ceil() as usize).min(audio.frames());
            if end > start {
                let mono = audio.slice_samples(start..end).mono_mix();
                let moments = windowed_span_moments(mono.as_ref(), sr, 2.0);
                let sum_sq: f64 = mono.iter().map(|&s| f64::from(s) * f64::from(s)).sum();
                let rms = (sum_sq / mono.len() as f64).sqrt();
                let peak = mono
                    .iter()
                    .map(|&s| f64::from(s).abs())
                    .fold(0.0_f64, f64::max);
                (moments, Some(rms), Some(peak))
            } else {
                (windowed_span_moments(&[], sr, 2.0), None, None)
            }
        };

        let pitch_params = PitchParams {
            floor_hz: pitch_floor_hz,
            ceiling_hz: pitch_ceiling_hz,
            ..PitchParams::default()
        };
        let pitch = phx_pitch::pitch_track(view.clone(), &pitch_params);

        let intensity_params = IntensityParams {
            pitch_floor_hz: intensity_floor_hz,
            ..IntensityParams::default()
        };
        let intensity = phx_intensity::intensity_track(view.clone(), &intensity_params);
        let intensity_mean_db = mean_in_span(intensity.iter(), span);
        let intensity_sd_db = sd_in_span(intensity.iter(), span);

        let harmonicity_params = HarmonicityParams {
            floor_hz: pitch_floor_hz,
            ..HarmonicityParams::default()
        };
        let hnr = phx_voice::hnr_track(view, &harmonicity_params);
        let hnr_mean_db = hnr.mean_db(span);

        Ok(SelectionReadout {
            t0: lo,
            t1: hi,
            f0: flo,
            f1: fhi,
            duration: hi - lo,
            f0_mean_hz: pitch.mean_hz(span),
            f0_min_hz: pitch.min_hz(span),
            f0_max_hz: pitch.max_hz(span),
            f0_sd_hz: pitch.sd_hz(span),
            f0_p5_hz: pitch.quantile_hz(span, 0.05),
            f0_p95_hz: pitch.quantile_hz(span, 0.95),
            band_energy_db: self.band_energy(id, lo, hi, flo, fhi)?,
            intensity_mean_db,
            intensity_sd_db,
            hnr_mean_db,
            rms,
            peak,
            spectral_cog_hz: moments.centre_of_gravity_hz,
            spectral_sd_hz: moments.standard_deviation_hz,
            spectral_skewness: moments.skewness,
            spectral_kurtosis: moments.kurtosis,
        })
    }

    /// Returns the mean frequency of each formant slot over a time span, in
    /// hertz.
    ///
    /// Slot `j` is the `j`-th lowest candidate of each frame; its mean is taken
    /// over the frames inside `[t0, t1]` that carry that slot, or `None` when no
    /// frame does. These are the provisional tracked-formant means the readout
    /// marks while the tracking weights stay unvalidated
    /// (`docs/plan/tasks/phase-4.md`).
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a formant parameter
    /// is outside the range the analysis accepts, or a bound is not finite.
    pub fn formant_span_means(
        &self,
        id: AudioId,
        params: &FormantParams,
        smoothed: bool,
        t0: f64,
        t1: f64,
    ) -> Result<Vec<Option<f64>>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "formant_span_means t0/t1 must be finite".to_string(),
            });
        }
        let track = if smoothed {
            self.formant_track_smoothed(id, params)?
        } else {
            self.formant_track(id, params)?
        };
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        let mut sums = vec![0.0; params.max_formants];
        let mut counts = vec![0usize; params.max_formants];
        for frame in &track.frames {
            if frame.time < lo || frame.time > hi {
                continue;
            }
            for (slot, formant) in frame.formants.iter().enumerate().take(params.max_formants) {
                sums[slot] += formant.frequency;
                counts[slot] += 1;
            }
        }
        Ok(sums
            .into_iter()
            .zip(counts)
            .map(|(sum, count)| (count > 0).then(|| sum / count as f64))
            .collect())
    }

    /// Returns the mean bandwidth of each formant slot over a time span, in
    /// hertz — the resonance sharpness a phonetician reads beside each formant
    /// frequency. Slot `j` averages the `j`-th lowest candidate's bandwidth over
    /// the frames inside `[t0, t1]` that carry it, or `None` when none do.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a formant parameter
    /// is outside the range the analysis accepts, or a bound is not finite.
    pub fn formant_span_bandwidth_means(
        &self,
        id: AudioId,
        params: &FormantParams,
        smoothed: bool,
        t0: f64,
        t1: f64,
    ) -> Result<Vec<Option<f64>>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "formant_span_bandwidth_means t0/t1 must be finite".to_string(),
            });
        }
        let track = if smoothed {
            self.formant_track_smoothed(id, params)?
        } else {
            self.formant_track(id, params)?
        };
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        let mut sums = vec![0.0; params.max_formants];
        let mut counts = vec![0usize; params.max_formants];
        for frame in &track.frames {
            if frame.time < lo || frame.time > hi {
                continue;
            }
            for (slot, formant) in frame.formants.iter().enumerate().take(params.max_formants) {
                sums[slot] += formant.bandwidth;
                counts[slot] += 1;
            }
        }
        Ok(sums
            .into_iter()
            .zip(counts)
            .map(|(sum, count)| (count > 0).then(|| sum / count as f64))
            .collect())
    }

    /// Computes power-weighted spectral moments at the midpoint of a span.
    ///
    /// The slice is the raw spectrogram frame nearest `(t0 + t1) / 2`, its dB
    /// PSD converted to linear power before weighting. `power` is the moment
    /// weighting exponent (`2.0` weights by power, Praat's default).
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn spectral_moments_in_span(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        power: f64,
    ) -> Result<Moments, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "spectral_moments_in_span t0/t1 must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
        let sr = audio.sample_rate();
        let start = (lo * sr).floor().max(0.0) as usize;
        let end = ((hi * sr).ceil() as usize).min(audio.frames());
        if end <= start {
            return Ok(windowed_span_moments(&[], sr, power));
        }
        let mono = audio.slice_samples(start..end).mono_mix();
        Ok(windowed_span_moments(mono.as_ref(), sr, power))
    }

    /// The spectrum of the selection `[t0, t1]` as parallel `(frequencies_hz,
    /// db)` vectors — one Hann-windowed FFT over the whole span, the way Praat's
    /// `Spectrum` object takes it, for a dB-vs-Hz view. Longer selections give
    /// finer frequency resolution.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn spectrum_slice(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
    ) -> Result<(Vec<f64>, Vec<f32>), EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "spectrum_slice bounds must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
        let sr = audio.sample_rate();
        let start = (lo * sr).floor().max(0.0) as usize;
        let end = ((hi * sr).ceil() as usize).min(audio.frames());
        if end < start + 4 {
            return Ok((Vec::new(), Vec::new()));
        }
        let mono = audio.slice_samples(start..end).mono_mix();
        let n = mono.len();
        let window = window_samples(Window::Hanning, n);
        let mut buffer: Vec<f64> = mono
            .iter()
            .zip(&window)
            .map(|(&s, &w)| f64::from(s) * w)
            .collect();
        let mut plan = RealFftPlan::new();
        let spectrum = plan.rfft(&mut buffer);
        let mut frequencies_hz = Vec::with_capacity(spectrum.len());
        let mut db = Vec::with_capacity(spectrum.len());
        for (k, bin) in spectrum.iter().enumerate() {
            frequencies_hz.push(k as f64 * sr / n as f64);
            db.push((20.0 * bin.norm().max(1e-12).log10()) as f32);
        }
        Ok((frequencies_hz, db))
    }

    /// The span-averaged real cepstrum of `[t0, t1]` as parallel
    /// `(quefrency_seconds, amplitude)` vectors — the quefrency-domain curve
    /// whose rahmonic peak at `1/F0` is the periodicity CPP measures.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn cepstrum_slice(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
    ) -> Result<(Vec<f64>, Vec<f64>), EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "cepstrum_slice bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let sample_rate = info.sample_rate;
        let frames = (info.duration * sample_rate).round() as usize;
        let lo = t0.min(t1).clamp(0.0, info.duration);
        let hi = t0.max(t1).clamp(0.0, info.duration);
        let start = ((lo * sample_rate).floor() as usize).min(frames);
        let end = ((hi * sample_rate).ceil() as usize).clamp(start, frames);
        let window = self.store.range_owned(id, start, end)?;
        let view = window.slice_samples(0..window.frames());
        let span = TimeSpan::new(0.0, view.duration());
        Ok(phx_voice::cepstrum_slice(view, span, &CppParams::default()))
    }

    /// The LPC-smoothed spectral envelope of the selection `[t0, t1]` as
    /// parallel `(frequencies_hz, db)` vectors — an all-pole Burg model sampled
    /// across `0..Nyquist`, the way Praat's `To LPC` then `To Spectrum (slice)`
    /// traces the resonance peaks a phonetician reads as formants over the raw
    /// spectrum. The model order tracks the sample rate (`round(sr/1000) + 2`,
    /// clamped to `4..=40`), one pole pair per expected formant plus headroom.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn lpc_spectrum(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
    ) -> Result<(Vec<f64>, Vec<f32>), EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "lpc_spectrum bounds must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
        let sr = audio.sample_rate();
        let start = (lo * sr).floor().max(0.0) as usize;
        let end = ((hi * sr).ceil() as usize).min(audio.frames());
        let order = ((sr / 1000.0).round() as usize + 2).clamp(4, 40);
        if end < start + order + 1 {
            return Ok((Vec::new(), Vec::new()));
        }
        let samples = audio
            .slice_samples(start..end)
            .mono_mix()
            .iter()
            .map(|&sample| f64::from(sample))
            .collect::<Vec<_>>();
        let points = 512;
        let Some(envelope) = phx_formant::lpc_envelope_db(&samples, sr, order, points) else {
            return Ok((Vec::new(), Vec::new()));
        };
        let mut frequencies_hz = Vec::with_capacity(envelope.len());
        let mut db = Vec::with_capacity(envelope.len());
        for (frequency, decibels) in envelope {
            if !frequency.is_finite() || !decibels.is_finite() {
                return Ok((Vec::new(), Vec::new()));
            }
            frequencies_hz.push(frequency);
            db.push(decibels as f32);
        }
        Ok((frequencies_hz, db))
    }

    /// The long-term average spectrum of `[t0, t1]`: the mean power spectrum
    /// across overlapping 20 ms Hann frames, in dB vs Hz. Averaging over frames
    /// smooths the harmonic fine structure a single spectrum shows, leaving the
    /// spectral slope voice-quality and sociophonetic work reads.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn ltas(&self, id: AudioId, t0: f64, t1: f64) -> Result<(Vec<f64>, Vec<f32>), EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "ltas bounds must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
        let sr = audio.sample_rate();
        let start = (lo * sr).floor().max(0.0) as usize;
        let end = ((hi * sr).ceil() as usize).min(audio.frames());
        if end < start + 8 {
            return Ok((Vec::new(), Vec::new()));
        }
        let mono = audio.slice_samples(start..end).mono_mix();
        let total = mono.len();
        let frame = ((sr * 0.02).round() as usize).clamp(8, total);
        let hop = (frame / 2).max(1);
        let window = window_samples(Window::Hanning, frame);
        let bins = frame / 2 + 1;
        let mut power = vec![0.0_f64; bins];
        let mut plan = RealFftPlan::new();
        let mut frames = 0usize;
        let mut pos = 0usize;
        while pos + frame <= total {
            let mut buffer: Vec<f64> = (0..frame)
                .map(|i| f64::from(mono[pos + i]) * window[i])
                .collect();
            for (acc, bin) in power.iter_mut().zip(plan.rfft(&mut buffer)) {
                *acc += bin.norm_sqr();
            }
            frames += 1;
            pos += hop;
        }
        if frames == 0 {
            return Ok((Vec::new(), Vec::new()));
        }
        let frequencies_hz = (0..bins).map(|k| k as f64 * sr / frame as f64).collect();
        let db = power
            .iter()
            .map(|&p| (10.0 * (p / frames as f64).max(1e-20).log10()) as f32)
            .collect();
        Ok((frequencies_hz, db))
    }

    /// Segments the recording into sounding and silent intervals by thresholding
    /// the intensity contour at `threshold_db` below its peak, then removing
    /// silent runs shorter than `min_silent_s` and sounding runs shorter than
    /// `min_sounding_s`. Returns `(t0, t1, is_sounding)` triples partitioning
    /// `[0, duration]`, for a first-pass "annotate by silences" TextGrid tier.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a parameter is not
    /// finite.
    pub fn silence_intervals(
        &self,
        id: AudioId,
        threshold_db: f64,
        min_silent_s: f64,
        min_sounding_s: f64,
    ) -> Result<Vec<(f64, f64, bool)>, EngineError> {
        if ![threshold_db, min_silent_s, min_sounding_s]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err(EngineError::InvalidRequest {
                reason: "silence_intervals parameters must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let view = audio.slice_samples(0..audio.frames());
        let intensity = phx_intensity::intensity_track(view, &IntensityParams::default());
        let frames: Vec<(f64, f64)> = intensity.iter().collect();
        let whole = vec![(0.0, duration, true)];
        if frames.is_empty() {
            return Ok(whole);
        }
        let peak = frames
            .iter()
            .map(|&(_, db)| db)
            .fold(f64::NEG_INFINITY, f64::max);
        if !peak.is_finite() {
            return Ok(whole);
        }
        let cutoff = peak + threshold_db;

        // Runs of like-classified frames become segments; each boundary sits at
        // the frame where the classification flips.
        let mut segs: Vec<(f64, f64, bool)> = Vec::new();
        let mut seg_start = 0.0;
        let mut current = frames[0].1 >= cutoff;
        for pair in frames.windows(2) {
            let sounding = pair[1].1 >= cutoff;
            if sounding != current {
                segs.push((seg_start, pair[1].0, current));
                seg_start = pair[1].0;
                current = sounding;
            }
        }
        segs.push((seg_start, duration, current));
        segs = coalesce_segments(segs);

        // Remove too-short runs by flipping them into their neighbours; each
        // flip strictly reduces the segment count once coalesced, so this ends.
        loop {
            let short = segs.iter().position(|&(a, b, sounding)| {
                let min = if sounding {
                    min_sounding_s
                } else {
                    min_silent_s
                };
                (b - a) < min
            });
            match short {
                Some(i) if segs.len() > 1 => {
                    segs[i].2 = !segs[i].2;
                    segs = coalesce_segments(segs);
                }
                _ => break,
            }
        }
        Ok(segs)
    }

    /// Segments the whole signal into voiced and unvoiced runs as `(t0, t1,
    /// voiced)` tuples — Praat's Pitch → "To TextGrid (vuv)", classifying each
    /// pitch frame as voiced when it carries an F0 candidate, then coalescing
    /// runs shorter than the minimum voiced/unvoiced durations into their
    /// neighbours.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, and [`EngineError::InvalidRequest`] when a parameter is not finite.
    pub fn voicing_intervals(
        &self,
        id: AudioId,
        pitch_floor_hz: f64,
        pitch_ceiling_hz: f64,
        min_voiced_s: f64,
        min_unvoiced_s: f64,
    ) -> Result<Vec<(f64, f64, bool)>, EngineError> {
        if ![
            pitch_floor_hz,
            pitch_ceiling_hz,
            min_voiced_s,
            min_unvoiced_s,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return Err(EngineError::InvalidRequest {
                reason: "voicing_intervals parameters must be finite".to_string(),
            });
        }
        let duration = self.store.info(id)?.duration;
        let params = PitchParams {
            floor_hz: pitch_floor_hz,
            ceiling_hz: pitch_ceiling_hz,
            ..PitchParams::default()
        };
        let track = self.pitch_track(id, &params)?;
        let frames = track.frames();
        let whole = vec![(0.0, duration, false)];
        if frames.is_empty() {
            return Ok(whole);
        }

        // Runs of like-voicing frames become segments; each boundary sits at the
        // frame where the classification flips.
        let mut segs: Vec<(f64, f64, bool)> = Vec::new();
        let mut seg_start = 0.0;
        let mut current = frames[0].f0.is_some();
        for pair in frames.windows(2) {
            let voiced = pair[1].f0.is_some();
            if voiced != current {
                segs.push((seg_start, pair[1].time, current));
                seg_start = pair[1].time;
                current = voiced;
            }
        }
        segs.push((seg_start, duration, current));
        segs = coalesce_segments(segs);

        // Fold too-short runs into their neighbours until every run clears its
        // minimum; each flip strictly reduces the count once coalesced.
        loop {
            let short = segs.iter().position(|&(a, b, voiced)| {
                let min = if voiced { min_voiced_s } else { min_unvoiced_s };
                (b - a) < min
            });
            match short {
                Some(i) if segs.len() > 1 => {
                    segs[i].2 = !segs[i].2;
                    segs = coalesce_segments(segs);
                }
                _ => break,
            }
        }
        Ok(segs)
    }

    /// Computes the aggregate voice report over a selection span.
    ///
    /// Wraps [`phx_voice::voice_report`]: it tracks pitch, extracts pulses, and
    /// aggregates the jitter, shimmer, HNR, CPP, and voice-break measures over
    /// `[t0, t1]`, embedding the parameters used. The pitch floor and ceiling
    /// come from the selection's analysis parameters.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn voice_report(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        pitch_floor_hz: f64,
        pitch_ceiling_hz: f64,
    ) -> Result<VoiceReport, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "voice_report t0/t1 must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let duration = audio.duration();
        let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
        let view = audio.slice_samples(0..audio.frames());
        let pitch_params = PitchParams {
            floor_hz: pitch_floor_hz,
            ceiling_hz: pitch_ceiling_hz,
            ..PitchParams::default()
        };
        Ok(phx_voice::voice_report(
            view,
            TimeSpan::new(lo, hi),
            &pitch_params,
        ))
    }

    /// Glottal pulse instants across the whole signal, in seconds — the same
    /// point process the voice report is built on, exposed for the waveform
    /// overlay that lets a reader audit jitter, shimmer, and HNR by eye.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` does not name a live
    /// store entry, and [`EngineError::InvalidRequest`] when a bound is not
    /// finite.
    pub fn pulse_times(
        &self,
        id: AudioId,
        pitch_floor_hz: f64,
        pitch_ceiling_hz: f64,
    ) -> Result<Vec<f64>, EngineError> {
        if !pitch_floor_hz.is_finite() || !pitch_ceiling_hz.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "pulse_times floor/ceiling must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let view = audio.slice_samples(0..audio.frames());
        let pitch_params = PitchParams {
            floor_hz: pitch_floor_hz,
            ceiling_hz: pitch_ceiling_hz,
            ..PitchParams::default()
        };
        let pitch = phx_pitch::pitch_track(view.clone(), &pitch_params);
        let pulses = phx_voice::pulses(view, &pitch, &PulseParams::default());
        Ok(pulses.times().to_vec())
    }

    /// Renders a time span band-filtered to `[f_low, f_high]` as a mono buffer
    /// at the source sample rate, for audible playback of a box selection.
    ///
    /// The span `[t0, t1]` is decoded (only that range, through a streamed
    /// source's ranged reads), mixed to mono, and passed through the spectral
    /// band-pass filter in [`phx_dsp::band_pass_filter`]: unity gain inside the
    /// band, zero outside, half-cosine skirts of
    /// [`phx_dsp::PASS_BAND_SKIRT_HZ`] at each edge, and a
    /// [`phx_dsp::EDGE_TAPER_S`] taper on the reconstructed span's ends. The
    /// result is deterministic for a given `(id, t0, t1, f_low, f_high)`.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when a streamed span cannot be decoded.
    pub fn band_filtered_span(
        &mut self,
        id: AudioId,
        t0: f64,
        t1: f64,
        f_low: f64,
        f_high: f64,
    ) -> Result<Vec<f32>, EngineError> {
        if ![t0, t1, f_low, f_high]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(EngineError::InvalidRequest {
                reason: "band_filtered_span bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        let audio = self.store.range_owned(id, start, end)?;
        let mono = audio.mono_mix();
        Ok(phx_dsp::band_pass_filter(
            &mut self.filter_plan,
            &mono,
            info.sample_rate,
            f_low,
            f_high,
        ))
    }

    /// Returns the exact unfiltered mono samples of the time span `[t0, t1]` of
    /// `id`, at the source sample rate — the same range and mono mix
    /// [`Engine::band_filtered_span`] filters, without the filter. The waveform
    /// pane reads this at high zoom to draw a sample-accurate polyline instead of
    /// a min/max envelope.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when a streamed span cannot be decoded.
    pub fn span_samples(&self, id: AudioId, t0: f64, t1: f64) -> Result<Vec<f32>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "span_samples t0/t1 must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        let audio = self.store.range_owned(id, start, end)?;
        Ok(audio.mono_mix().into_owned())
    }

    /// Returns the time of the zero crossing nearest `t`, for snapping a
    /// selection edge so a cut leaves no click. The signal is read as the mean
    /// of its channels; a crossing lies where consecutive frames straddle zero,
    /// and its sub-sample position is linearly interpolated. The search expands
    /// outward from `t` and returns the first crossing it reaches; a signal with
    /// none in range (silence held at a constant, DC) leaves `t` unchanged.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry and [`EngineError::InvalidRequest`] when `t` is not finite.
    pub fn nearest_zero_crossing(&self, id: AudioId, t: f64) -> Result<f64, EngineError> {
        if !t.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "nearest_zero_crossing t must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let sr = audio.sample_rate();
        let frames = audio.frames();
        let channels = audio.channel_count();
        if frames < 2 || channels == 0 || !(sr > 0.0) {
            return Ok(t);
        }
        let duration = audio.duration();
        let target = t.clamp(0.0, duration);
        let center = ((target * sr).round() as i64).clamp(0, frames as i64 - 1) as usize;

        let sample_at = |i: usize| -> f64 {
            let mut sum = 0.0;
            for c in 0..channels {
                sum += f64::from(audio.channel(c)[i]);
            }
            sum / channels as f64
        };
        // The crossing between frames `i` and `i+1`, in seconds. An exact zero at
        // either end is that frame; otherwise the line between them hits zero at
        // `a / (a - b)` of the way across.
        let crossing_time = |i: usize| -> f64 {
            let a = sample_at(i);
            let b = sample_at(i + 1);
            if a == 0.0 {
                i as f64 / sr
            } else if b == 0.0 {
                (i + 1) as f64 / sr
            } else {
                (i as f64 + a / (a - b)) / sr
            }
        };
        let straddles = |i: usize| -> bool {
            let a = sample_at(i);
            let b = sample_at(i + 1);
            (a <= 0.0 && b >= 0.0) || (a >= 0.0 && b <= 0.0)
        };

        let max_i = frames - 2;
        let mut offset: i64 = 0;
        loop {
            let mut in_range = false;
            let mut best: Option<f64> = None;
            let mut best_dist = f64::INFINITY;
            for side in [center as i64 + offset, center as i64 - offset] {
                if side < 0 || side as usize > max_i {
                    continue;
                }
                in_range = true;
                let i = side as usize;
                if straddles(i) {
                    let ct = crossing_time(i);
                    let dist = (ct - target).abs();
                    if dist < best_dist {
                        best_dist = dist;
                        best = Some(ct);
                    }
                }
                if offset == 0 {
                    break; // both entries are the same index
                }
            }
            if let Some(ct) = best {
                return Ok(ct);
            }
            if !in_range {
                return Ok(t);
            }
            offset += 1;
        }
    }

    /// Encodes the time span `[t0, t1]` of `id` as WAV bytes at `bits`, with no
    /// filtering.
    ///
    /// The span is the exact sample slice — an eager buffer's samples are copied
    /// verbatim and a streamed source's are decoded from the same range — so an
    /// unfiltered export at [`BitDepth::Float32`] round-trips bit-for-bit with a
    /// direct slice of the signal.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when the span cannot be decoded or encoded.
    pub fn export_span_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "export_span_wav t0/t1 must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        let audio = self.store.range_owned(id, start, end)?;
        Ok(audio.to_wav_bytes(bits)?)
    }

    /// Encodes the time span `[t0, t1]` of `id` reversed in time as WAV bytes at
    /// `bits` — Praat's Sound "Reverse" over a selection, as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when the span cannot be decoded or encoded.
    pub fn reverse_span_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "reverse_span_wav t0/t1 must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        let mut audio = self.store.range_owned(id, start, end)?;
        audio.reverse();
        Ok(audio.to_wav_bytes(bits)?)
    }

    /// Encodes the pre-emphasized time span `[t0, t1]` of `id` as mono WAV bytes
    /// at `bits` — Praat's Sound "Filter (pre-emphasis)", the `+6` dB/octave
    /// first-difference high-pass with its `+3` dB corner at `from_hz` that lifts
    /// the higher formants before analysis, saved as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite or the
    /// span is empty, and [`EngineError::Audio`] when the span cannot be encoded.
    pub fn apply_preemphasis_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        from_hz: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() || !from_hz.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "apply_preemphasis_wav bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        if end <= start {
            return Err(EngineError::InvalidRequest {
                reason: "apply_preemphasis_wav span is empty".to_string(),
            });
        }
        let audio = self.store.range_owned(id, start, end)?;
        let sample_rate = audio.sample_rate();
        let mut samples: Vec<f64> = audio.mono_mix().iter().map(|&s| f64::from(s)).collect();
        phx_dsp::preemphasis_in_place(&mut samples, from_hz, sample_rate);
        let mono: Vec<f32> = samples.iter().map(|&s| s as f32).collect();
        Ok(Audio::new(vec![mono], sample_rate)?.to_wav_bytes(bits)?)
    }

    /// Encodes the de-emphasized time span `[t0, t1]` of `id` as mono WAV bytes
    /// at `bits` — Praat's Sound "De-emphasize (in-place)", the recursive
    /// integrator with its `+3` dB corner at `from_hz` that undoes a
    /// pre-emphasis tilt, saved as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite or the
    /// span is empty, and [`EngineError::Audio`] when the span cannot be encoded.
    pub fn apply_deemphasis_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        from_hz: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() || !from_hz.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "apply_deemphasis_wav bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        if end <= start {
            return Err(EngineError::InvalidRequest {
                reason: "apply_deemphasis_wav span is empty".to_string(),
            });
        }
        let audio = self.store.range_owned(id, start, end)?;
        let sample_rate = audio.sample_rate();
        let mut samples: Vec<f64> = audio.mono_mix().iter().map(|&s| f64::from(s)).collect();
        phx_dsp::deemphasis_in_place(&mut samples, from_hz, sample_rate);
        let mono: Vec<f32> = samples.iter().map(|&s| s as f32).collect();
        Ok(Audio::new(vec![mono], sample_rate)?.to_wav_bytes(bits)?)
    }

    /// Encodes the time span `[t0, t1]` of `id` with its DC offset removed as
    /// mono WAV bytes at `bits` — Praat's Sound "Subtract mean", centring the
    /// span on zero so a constant bias no longer skews intensity or spectral
    /// measurements, saved as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite or the
    /// span is empty, and [`EngineError::Audio`] when the span cannot be encoded.
    pub fn subtract_mean_span_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "subtract_mean_span_wav bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        if end <= start {
            return Err(EngineError::InvalidRequest {
                reason: "subtract_mean_span_wav span is empty".to_string(),
            });
        }
        let audio = self.store.range_owned(id, start, end)?;
        let sample_rate = audio.sample_rate();
        let mut samples: Vec<f64> = audio.mono_mix().iter().map(|&s| f64::from(s)).collect();
        phx_dsp::subtract_mean_in_place(&mut samples);
        let mono: Vec<f32> = samples.iter().map(|&s| s as f32).collect();
        Ok(Audio::new(vec![mono], sample_rate)?.to_wav_bytes(bits)?)
    }

    /// Encodes the whole of `id` with the span `[t0, t1]` set to silence as mono
    /// WAV bytes at `bits` — Praat's Sound "Set part to zero", which punches a
    /// hole (a click, a cough) out of a recording without changing its length,
    /// saved as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite or the
    /// span is empty, and [`EngineError::Audio`] when the audio cannot be encoded.
    pub fn zero_span_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "zero_span_wav bounds must be finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let sample_rate = audio.sample_rate();
        let frames = audio.frames();
        let (start, end) = span_frames(sample_rate, audio.duration(), t0, t1);
        if end <= start {
            return Err(EngineError::InvalidRequest {
                reason: "zero_span_wav span is empty".to_string(),
            });
        }
        let mut mono: Vec<f32> = audio.slice_samples(0..frames).mono_mix().to_vec();
        for sample in &mut mono[start..end] {
            *sample = 0.0;
        }
        Ok(Audio::new(vec![mono], sample_rate)?.to_wav_bytes(bits)?)
    }

    /// Encodes the time span `[t0, t1]` of `id` scaled to an average intensity of
    /// `target_db` as WAV bytes at `bits` — Praat's Sound "Scale intensity" over
    /// a selection, as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when the span cannot be decoded or encoded.
    pub fn scale_intensity_span_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        target_db: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() || !target_db.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "scale_intensity_span_wav bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        let mut audio = self.store.range_owned(id, start, end)?;
        audio.scale_intensity(target_db);
        Ok(audio.to_wav_bytes(bits)?)
    }

    /// Encodes the time span `[t0, t1]` of `id` scaled so its largest absolute
    /// sample reaches `target` as WAV bytes at `bits` — Praat's Sound "Scale
    /// peak" over a selection, as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when the span cannot be decoded or encoded.
    pub fn scale_peak_span_wav(
        &self,
        id: AudioId,
        t0: f64,
        t1: f64,
        target: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !t0.is_finite() || !t1.is_finite() || !target.is_finite() {
            return Err(EngineError::InvalidRequest {
                reason: "scale_peak_span_wav bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        let mut audio = self.store.range_owned(id, start, end)?;
        audio.scale_peak(target);
        Ok(audio.to_wav_bytes(bits)?)
    }

    /// Encodes the whole of `id` resampled to `target_hz` as WAV bytes at `bits`
    /// — Praat's Sound "Resample", as a new take. A source already at the target
    /// rate is re-encoded unchanged.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when `target_hz` is not positive
    /// and finite, and [`EngineError::Audio`] when resampling or encoding fails.
    pub fn resample_wav(
        &self,
        id: AudioId,
        target_hz: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if !(target_hz.is_finite() && target_hz > 0.0) {
            return Err(EngineError::InvalidRequest {
                reason: "resample_wav target_hz must be positive and finite".to_string(),
            });
        }
        let access = self.store.whole(id)?;
        let audio = access.audio();
        if audio.sample_rate() == target_hz {
            return Ok(audio.to_wav_bytes(bits)?);
        }
        Ok(audio
            .resampled(target_hz, ResampleQuality::Best)?
            .to_wav_bytes(bits)?)
    }

    /// Encodes the whole of `id` mixed down to a single channel as WAV bytes at
    /// `bits` — Praat's Sound "Convert to mono", the arithmetic mean of the
    /// channels, as a new take.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry and [`EngineError::Audio`] when the mix cannot be encoded.
    pub fn export_mono_wav(&self, id: AudioId, bits: BitDepth) -> Result<Vec<u8>, EngineError> {
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let mono = audio.mono_mix().into_owned();
        Ok(Audio::new(vec![mono], audio.sample_rate())?.to_wav_bytes(bits)?)
    }

    /// Joins `ids` end to end into one mono WAV at `bits` — Praat's Sound
    /// "Concatenate". The first source sets the sample rate; any source at a
    /// different rate is resampled to it first, and every source is mixed to
    /// mono before the runs are laid end to end.
    ///
    /// # Errors
    /// Returns [`EngineError::InvalidRequest`] when `ids` is empty,
    /// [`EngineError::UnknownAudioId`] when one names no live entry, and
    /// [`EngineError::Audio`] when a source cannot be resampled or the result
    /// cannot be encoded.
    pub fn concat_wav(&self, ids: &[AudioId], bits: BitDepth) -> Result<Vec<u8>, EngineError> {
        let Some((&first, _)) = ids.split_first() else {
            return Err(EngineError::InvalidRequest {
                reason: "concat_wav needs at least one recording".to_string(),
            });
        };
        let target_hz = self.store.info(first)?.sample_rate;
        let mut samples: Vec<f32> = Vec::new();
        for &id in ids {
            let access = self.store.whole(id)?;
            let audio = access.audio();
            if audio.sample_rate() == target_hz {
                samples.extend_from_slice(&audio.mono_mix());
            } else {
                let resampled = audio.resampled(target_hz, ResampleQuality::Best)?;
                samples.extend_from_slice(&resampled.mono_mix());
            }
        }
        Ok(Audio::new(vec![samples], target_hz)?.to_wav_bytes(bits)?)
    }

    /// Encodes `id_a` and `id_b` as the left and right channels of one stereo WAV
    /// at `bits` — Praat's Sound "Combine to stereo". Each source is mixed to
    /// mono first, `id_b` is resampled to `id_a`'s rate when they differ, and the
    /// shorter channel is zero-padded to the longer so nothing is truncated.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when either id names no live store
    /// entry, and [`EngineError::Audio`] when the result cannot be encoded.
    pub fn combine_stereo_wav(
        &self,
        id_a: AudioId,
        id_b: AudioId,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        let rate = self.store.info(id_a)?.sample_rate;
        let mut left: Vec<f32> = {
            let access = self.store.whole(id_a)?;
            access.audio().mono_mix().to_vec()
        };
        let mut right: Vec<f32> = {
            let access = self.store.whole(id_b)?;
            let audio = access.audio();
            if audio.sample_rate() == rate {
                audio.mono_mix().to_vec()
            } else {
                audio
                    .resampled(rate, ResampleQuality::Best)?
                    .mono_mix()
                    .to_vec()
            }
        };
        let len = left.len().max(right.len());
        left.resize(len, 0.0);
        right.resize(len, 0.0);
        Ok(Audio::new(vec![left, right], rate)?.to_wav_bytes(bits)?)
    }

    /// Encodes the band-filtered time span `[t0, t1]` of `id` as mono WAV bytes
    /// at `bits`.
    ///
    /// Filters the span through [`Engine::band_filtered_span`], then encodes the
    /// mono result — the "save selection as audio" path for a box selection the
    /// user is hearing filtered.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when the span cannot be decoded or encoded.
    pub fn export_band_filtered_span_wav(
        &mut self,
        id: AudioId,
        t0: f64,
        t1: f64,
        f_low: f64,
        f_high: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        let sample_rate = self.store.info(id)?.sample_rate;
        let filtered = self.band_filtered_span(id, t0, t1, f_low, f_high)?;
        let audio = Audio::new(vec![filtered], sample_rate)?;
        Ok(audio.to_wav_bytes(bits)?)
    }

    /// Encodes a single `channel` of `id` as its own mono WAV at `bits` — Praat's
    /// Sound "Extract one channel", the way one takes the left or right of a
    /// stereo take into a separate recording.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when `channel` is out of range, and
    /// [`EngineError::Audio`] when the channel cannot be encoded.
    pub fn export_channel_wav(
        &self,
        id: AudioId,
        channel: usize,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        let access = self.store.whole(id)?;
        let audio = access.audio();
        let channel_count = audio.channel_count();
        if channel >= channel_count {
            return Err(EngineError::InvalidRequest {
                reason: format!("channel {channel} is out of range for {channel_count} channels"),
            });
        }
        let samples = audio.channel(channel).to_vec();
        let mono = Audio::new(vec![samples], audio.sample_rate())?;
        Ok(mono.to_wav_bytes(bits)?)
    }

    /// Encodes the time span `[t0, t1]` of `id` with the `[f_low, f_high]` band
    /// attenuated as mono WAV bytes at `bits` — Praat's Sound "Filter (stop Hann
    /// band)", the complement of [`Engine::export_band_filtered_span_wav`].
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAudioId`] when `id` names no live store
    /// entry, [`EngineError::InvalidRequest`] when a bound is not finite, and
    /// [`EngineError::Audio`] when the span cannot be decoded or encoded.
    pub fn export_notch_filtered_span_wav(
        &mut self,
        id: AudioId,
        t0: f64,
        t1: f64,
        f_low: f64,
        f_high: f64,
        bits: BitDepth,
    ) -> Result<Vec<u8>, EngineError> {
        if ![t0, t1, f_low, f_high]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(EngineError::InvalidRequest {
                reason: "export_notch_filtered_span_wav bounds must be finite".to_string(),
            });
        }
        let info = self.store.info(id)?;
        let (start, end) = span_frames(info.sample_rate, info.duration, t0, t1);
        let audio = self.store.range_owned(id, start, end)?;
        let mono = audio.mono_mix();
        let filtered = phx_dsp::band_stop_filter(
            &mut self.filter_plan,
            &mono,
            info.sample_rate,
            f_low,
            f_high,
        );
        Ok(Audio::new(vec![filtered], info.sample_rate)?.to_wav_bytes(bits)?)
    }

    /// Applies one command through the journal and reports what changed.
    ///
    /// This is the only path that mutates a document: it runs the command,
    /// records an id-stable inverse for [`Engine::undo`], captures the id-stable
    /// redo, and clears the redo stack so the new command cannot be contradicted
    /// by pending redo history. On any error the state is left untouched — every
    /// underlying mutator commits only a fully validated result.
    ///
    /// # Errors
    /// Returns [`EngineError::Audio`] for an undecodable import,
    /// [`EngineError::UnknownAudioId`] / [`EngineError::UnknownAnnotationId`]
    /// for a missing target, [`EngineError::InvalidAnnotation`] for an attached
    /// document that fails validation, and [`EngineError::Annotation`] for a
    /// rejected annotation mutation (an out-of-range boundary, a control
    /// character in a label, a dangling relation left by a tier removal).
    pub fn apply(&mut self, cmd: Command) -> Result<Applied, EngineError> {
        let (applied, transition) = self.execute(cmd)?;
        self.journal.record(transition);
        Ok(applied)
    }

    /// Id of the entry [`Engine::undo`] would target right now, or `None` when
    /// there is nothing to undo.
    ///
    /// A caller that wants a later action to affect one specific command — an
    /// undo toast for a delete, say — captures this right after applying it,
    /// then compares it again before acting: a match means [`Engine::undo`]
    /// still targets that same entry; a mismatch means something else has
    /// been journaled (or undone, or redone) since, and a blind `undo()`
    /// would hit that instead.
    #[must_use]
    pub fn journal_head_id(&self) -> Option<u64> {
        self.journal.head_id()
    }

    /// Undoes the most recent command, restoring a state-hash-identical
    /// document, and reports what changed. Returns `None` when nothing is left
    /// to undo.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAnnotationId`] only if a document a stored
    /// inverse names has gone missing, which the journal's own bookkeeping
    /// prevents in practice.
    pub fn undo(&mut self) -> Result<Option<Applied>, EngineError> {
        let Some(entry) = self.journal.take_undo() else {
            return Ok(None);
        };
        match entry.undo.apply(&mut self.store, &mut self.documents) {
            Ok(applied) => {
                self.journal.park_redo(entry);
                Ok(Some(applied))
            }
            Err(err) => {
                self.journal.park_undo(entry);
                Err(err)
            }
        }
    }

    /// Redoes the most recently undone command and reports what changed.
    /// Returns `None` when nothing is left to redo.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAnnotationId`] only if a document a stored
    /// transition names has gone missing, which the journal's own bookkeeping
    /// prevents in practice.
    pub fn redo(&mut self) -> Result<Option<Applied>, EngineError> {
        let Some(entry) = self.journal.take_redo() else {
            return Ok(None);
        };
        match entry.redo.apply(&mut self.store, &mut self.documents) {
            Ok(applied) => {
                self.journal.park_undo(entry);
                Ok(Some(applied))
            }
            Err(err) => {
                self.journal.park_redo(entry);
                Err(err)
            }
        }
    }

    /// Number of commands that can still be undone.
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.journal.undo_depth()
    }

    /// Number of commands that can still be redone.
    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.journal.redo_depth()
    }

    /// Returns a hash of the whole document model — every stored audio buffer's
    /// identity and every annotation document's content.
    ///
    /// Two engines whose document models are equal produce the same value, and
    /// undoing a command restores the value it had before (invariant 5,
    /// `docs/plan/validation.md`). The fold visits ids in ascending order so the
    /// result never depends on hash-map iteration order. The value is stable
    /// within a process run, which is all a consistency assertion needs; it is
    /// not a persisted content address.
    #[must_use]
    pub fn state_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let audio_ids = self.store.ids_sorted();
        (audio_ids.len() as u64).hash(&mut hasher);
        for id in audio_ids {
            id.as_u64().hash(&mut hasher);
            // Fold buffer identity through the store's metadata surface so the
            // name — the store's own field for both eager and streamed entries —
            // enters the hash and a rename shifts it. Reading metadata never
            // decodes a streamed source.
            if let Ok(info) = self.store.info(id) {
                info.duration.to_bits().hash(&mut hasher);
                info.sample_rate.to_bits().hash(&mut hasher);
                (info.channels as u64).hash(&mut hasher);
                info.name.hash(&mut hasher);
            }
        }
        let doc_ids = self.documents.ids_sorted();
        (doc_ids.len() as u64).hash(&mut hasher);
        for id in doc_ids {
            id.as_u64().hash(&mut hasher);
            if let Ok(document) = self.documents.get(id) {
                document.audio.as_u64().hash(&mut hasher);
                hash_annotation(&document.annotation, &mut hasher);
            }
        }
        hasher.finish()
    }

    /// Searches interval and point labels across every attached document.
    ///
    /// Each hit is tagged with the document it was found in, so a cross-project
    /// search can navigate to the right document and then to the span within it.
    /// Documents are visited in ascending id order.
    #[must_use]
    pub fn search_labels(&self, query: &LabelQuery) -> Vec<EngineHit> {
        let mut hits = Vec::new();
        for id in self.documents.ids_sorted() {
            let Ok(document) = self.documents.get(id) else {
                continue;
            };
            for hit in document.annotation.search(query) {
                hits.push(EngineHit {
                    annotation: id,
                    hit,
                });
            }
        }
        hits
    }

    /// Returns the annotation content of a document for read-only rendering.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAnnotationId`] when `id` names no live
    /// document.
    pub fn annotation(&self, id: AnnotationId) -> Result<&Annotation, EngineError> {
        Ok(&self.documents.get(id)?.annotation)
    }

    /// Returns the audio a document annotates.
    ///
    /// # Errors
    /// Returns [`EngineError::UnknownAnnotationId`] when `id` names no live
    /// document.
    pub fn annotation_audio(&self, id: AnnotationId) -> Result<AudioId, EngineError> {
        Ok(self.documents.get(id)?.audio)
    }

    /// Returns every live annotation id in ascending order.
    #[must_use]
    pub fn annotation_ids(&self) -> Vec<AnnotationId> {
        self.documents.ids_sorted()
    }

    /// Inserts a fully decoded buffer and records the same undo/redo pair
    /// [`Command::ImportAudio`] would, so [`Engine::import_audio_bytes`] and
    /// [`Engine::finish_recording`] — the two paths that add a whole-decoded
    /// buffer outside the command surface — still leave a real journal entry.
    fn journal_eager_import(&mut self, audio: Audio) -> AudioId {
        let replay = audio.clone();
        let id = self.store.insert(audio);
        self.journal.record(Transition {
            undo: Reverse::RemoveAudio { id },
            redo: Reverse::ImportAudio {
                id,
                audio: Box::new(replay),
            },
        });
        id
    }

    /// Records the undo/redo pair for a freshly opened streamed source: undo
    /// detaches it (parking the source and pyramid, never dropping or
    /// rereading them) and redo reattaches the same parked entry. A fresh
    /// import has no documents to cascade, so both sides of the pair carry an
    /// empty document list.
    fn journal_streamed_import(&mut self, id: AudioId) {
        self.journal.record(Transition {
            undo: Reverse::DetachAudio { id },
            redo: Reverse::RestoreAudio {
                id,
                docs: Vec::new(),
            },
        });
    }

    /// Runs a command forward against live state, returning the report and the
    /// journal entry that reverses and reproduces it.
    fn execute(&mut self, cmd: Command) -> Result<(Applied, Transition), EngineError> {
        use phx_annot::InverseMutation;

        match cmd {
            Command::ImportAudio { bytes, name } => {
                let audio = Audio::from_bytes(&bytes)?.with_name(name);
                let replay = audio.clone();
                let id = self.store.insert(audio);
                Ok((
                    Applied::AudioImported { audio: id },
                    Transition {
                        undo: Reverse::RemoveAudio { id },
                        redo: Reverse::ImportAudio {
                            id,
                            audio: Box::new(replay),
                        },
                    },
                ))
            }
            Command::RenameAudio { id, name } => {
                let previous = self.store.set_name(id, Some(name.clone()))?;
                Ok((
                    Applied::AudioRenamed {
                        audio: id,
                        name: name.clone(),
                    },
                    Transition {
                        undo: Reverse::RenameAudio { id, name: previous },
                        redo: Reverse::RenameAudio {
                            id,
                            name: Some(name),
                        },
                    },
                ))
            }
            Command::DetachAudio { id } => {
                if !self.store.contains(id) {
                    return Err(EngineError::UnknownAudioId(id));
                }
                let annotations = self.documents.ids_referencing(id);
                let mut captured = Vec::with_capacity(annotations.len());
                for annotation in &annotations {
                    if let Some(document) = self.documents.detach(*annotation) {
                        captured.push((*annotation, document));
                    }
                }
                self.store.detach(id);
                Ok((
                    Applied::AudioDetached {
                        audio: id,
                        annotations,
                    },
                    Transition {
                        undo: Reverse::RestoreAudio { id, docs: captured },
                        redo: Reverse::DetachAudio { id },
                    },
                ))
            }
            Command::AttachAnnotation { audio, annotation } => {
                if !self.store.contains(audio) {
                    return Err(EngineError::UnknownAudioId(audio));
                }
                let issues = annotation.validate();
                if !issues.is_empty() {
                    return Err(EngineError::InvalidAnnotation(issues));
                }
                let document = Document {
                    audio,
                    annotation: annotation.clone(),
                };
                let id = self.documents.attach(audio, annotation);
                Ok((
                    Applied::AnnotationAttached {
                        annotation: id,
                        audio,
                    },
                    Transition {
                        undo: Reverse::Detach { id },
                        redo: Reverse::Attach {
                            id,
                            document: Box::new(document),
                        },
                    },
                ))
            }
            Command::AddIntervalTier {
                annotation,
                name,
                relation,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let tier = document.annotation.add_interval_tier(&name, relation)?;
                let (index, slot) = captured_tier(&document.annotation, tier)?;
                Ok((
                    Applied::TierAdded { annotation, tier },
                    Transition {
                        undo: Reverse::RemoveTier {
                            doc: annotation,
                            tier,
                        },
                        redo: Reverse::InsertTier {
                            doc: annotation,
                            index,
                            slot: Box::new(slot),
                        },
                    },
                ))
            }
            Command::AddPointTier {
                annotation,
                name,
                points,
                relation,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let tier = document
                    .annotation
                    .add_point_tier(&name, points, relation)?;
                let (index, slot) = captured_tier(&document.annotation, tier)?;
                Ok((
                    Applied::TierAdded { annotation, tier },
                    Transition {
                        undo: Reverse::RemoveTier {
                            doc: annotation,
                            tier,
                        },
                        redo: Reverse::InsertTier {
                            doc: annotation,
                            index,
                            slot: Box::new(slot),
                        },
                    },
                ))
            }
            Command::RenameTier {
                annotation,
                tier,
                name,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let change = document.annotation.rename_tier(tier, &name)?;
                Ok((
                    Applied::TierRenamed {
                        annotation,
                        tier,
                        name: change.new_name.clone(),
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RenameTier {
                                tier,
                                name: change.old_name,
                            },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RenameTier {
                                tier,
                                name: change.new_name,
                            },
                        },
                    },
                ))
            }
            Command::RemoveTier { annotation, tier } => {
                let document = self.documents.get_mut(annotation)?;
                let (index, slot) = captured_tier(&document.annotation, tier)?;
                let reduced = journal::remove_tier(&document.annotation, tier)?;
                document.annotation = reduced;
                Ok((
                    Applied::TierRemoved { annotation, tier },
                    Transition {
                        undo: Reverse::InsertTier {
                            doc: annotation,
                            index,
                            slot: Box::new(slot),
                        },
                        redo: Reverse::RemoveTier {
                            doc: annotation,
                            tier,
                        },
                    },
                ))
            }
            Command::DuplicateTier { annotation, tier } => {
                let document = self.documents.get_mut(annotation)?;
                let copy = document.annotation.duplicate_tier(tier)?;
                // Redo restores the exact copy — id, contents, and position — so
                // an undo/redo cycle keeps every stable identifier, exactly as a
                // fresh add does.
                let (index, slot) = captured_tier(&document.annotation, copy)?;
                Ok((
                    Applied::TierAdded {
                        annotation,
                        tier: copy,
                    },
                    Transition {
                        undo: Reverse::RemoveTier {
                            doc: annotation,
                            tier: copy,
                        },
                        redo: Reverse::InsertTier {
                            doc: annotation,
                            index,
                            slot: Box::new(slot),
                        },
                    },
                ))
            }
            Command::InsertBoundary {
                annotation,
                tier,
                at,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let boundary = document.annotation.insert_boundary(tier, at)?;
                // Capture the split as a restore-merge so redo re-creates the
                // same boundary id rather than allocating a fresh one.
                let mut probe = document.annotation.clone();
                let merged = probe.remove_boundary(boundary)?;
                Ok((
                    Applied::BoundaryInserted {
                        annotation,
                        tier,
                        boundary,
                        at,
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RemoveBoundary { boundary },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RestoreMergedBoundary { merged },
                        },
                    },
                ))
            }
            Command::MoveBoundary {
                annotation,
                boundary,
                to,
                mode,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let moved = document.annotation.move_boundary(boundary, to, mode)?;
                let redo_moves = moved
                    .moves
                    .iter()
                    .map(|m| BoundaryMove {
                        tier: m.tier,
                        boundary: m.boundary,
                        from: m.to,
                        to: m.from,
                    })
                    .collect();
                Ok((
                    Applied::BoundaryMoved {
                        annotation,
                        moves: moved.moves.clone(),
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::MoveBoundaries { moves: moved.moves },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::MoveBoundaries { moves: redo_moves },
                        },
                    },
                ))
            }
            Command::RemoveBoundary {
                annotation,
                boundary,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let merged = document.annotation.remove_boundary(boundary)?;
                Ok((
                    Applied::BoundaryRemoved {
                        annotation,
                        boundary,
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RestoreMergedBoundary { merged },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RemoveBoundary { boundary },
                        },
                    },
                ))
            }
            Command::SetLabel {
                annotation,
                target,
                text,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let change = document.annotation.set_label(target, &text)?;
                Ok((
                    Applied::LabelSet {
                        annotation,
                        target,
                        text: change.new_text.clone(),
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::SetLabel {
                                target,
                                text: change.old_text,
                            },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::SetLabel {
                                target,
                                text: change.new_text,
                            },
                        },
                    },
                ))
            }
            Command::InsertPoint {
                annotation,
                tier,
                time,
                label,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let insertion = document.annotation.insert_point(tier, time, &label)?;
                let point = insertion.point.clone();
                Ok((
                    Applied::PointInserted {
                        annotation,
                        tier,
                        point: point.id,
                        at: point.time,
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RemovePoint { point: point.id },
                        },
                        // Redo restores the exact point id rather than allocating
                        // a fresh one, keeping undo/redo state-hash-identical.
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RestorePoint { tier, point },
                        },
                    },
                ))
            }
            Command::MovePoint {
                annotation,
                point,
                to,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let moved = document.annotation.move_point(point, to)?;
                Ok((
                    Applied::PointMoved {
                        annotation,
                        point,
                        to: moved.to,
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::MovePoint {
                                point,
                                to: moved.from,
                            },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::MovePoint {
                                point,
                                to: moved.to,
                            },
                        },
                    },
                ))
            }
            Command::RemovePoint { annotation, point } => {
                let document = self.documents.get_mut(annotation)?;
                let removal = document.annotation.remove_point(point)?;
                Ok((
                    Applied::PointRemoved { annotation, point },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RestorePoint {
                                tier: removal.tier,
                                point: removal.point,
                            },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::RemovePoint { point },
                        },
                    },
                ))
            }
            Command::ReorderTier {
                annotation,
                tier,
                to_index,
            } => {
                let document = self.documents.get_mut(annotation)?;
                let reorder = document.annotation.reorder_tier(tier, to_index)?;
                Ok((
                    Applied::TierReordered {
                        annotation,
                        tier,
                        to_index: reorder.to_index,
                    },
                    Transition {
                        undo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::ReorderTier {
                                tier,
                                to_index: reorder.from_index,
                            },
                        },
                        redo: Reverse::Content {
                            doc: annotation,
                            mutation: InverseMutation::ReorderTier {
                                tier,
                                to_index: reorder.to_index,
                            },
                        },
                    },
                ))
            }
        }
    }
}

/// Captures a tier's document position and full slot for the journal, so undo
/// or redo can reinstate it with every stable id intact.
fn captured_tier(annotation: &Annotation, tier: TierId) -> Result<(usize, TierSlot), EngineError> {
    let index = journal::tier_index(annotation, tier).ok_or(EngineError::Annotation(
        AnnotationError::UnknownTier { tier },
    ))?;
    let slot = annotation.tiers()[index].clone();
    Ok((index, slot))
}

/// Folds an annotation's content into `hasher` in document order.
///
/// Every field that distinguishes two documents contributes: the time domain,
/// tier order, relations, and each interval's or point's stable ids, times, and
/// label. Floats are hashed by bit pattern so the fold matches
/// [`phx_annot::Annotation`]'s own bitwise equality.
fn hash_annotation<H: Hasher>(annotation: &Annotation, hasher: &mut H) {
    annotation.xmin().to_bits().hash(hasher);
    annotation.xmax().to_bits().hash(hasher);
    (annotation.tiers().len() as u64).hash(hasher);
    for slot in annotation.tiers() {
        slot.id.get().hash(hasher);
        match slot.relation {
            TierRelation::Independent => 0u8.hash(hasher),
            TierRelation::AlignedBoundaries { with } => {
                1u8.hash(hasher);
                with.get().hash(hasher);
            }
            TierRelation::ChildOf { parent } => {
                2u8.hash(hasher);
                parent.get().hash(hasher);
            }
        }
        match &slot.tier {
            Tier::Interval(tier) => {
                0u8.hash(hasher);
                tier.name.hash(hasher);
                (tier.intervals.len() as u64).hash(hasher);
                for interval in &tier.intervals {
                    interval.id.get().hash(hasher);
                    interval.start_boundary.get().hash(hasher);
                    interval.end_boundary.get().hash(hasher);
                    interval.xmin.to_bits().hash(hasher);
                    interval.xmax.to_bits().hash(hasher);
                    interval.label.hash(hasher);
                }
            }
            Tier::Point(tier) => {
                1u8.hash(hasher);
                tier.name.hash(hasher);
                (tier.points.len() as u64).hash(hasher);
                for point in &tier.points {
                    point.id.get().hash(hasher);
                    point.time.to_bits().hash(hasher);
                    point.label.hash(hasher);
                }
            }
        }
    }
}

/// Orders a pair and clamps it to `[min, max]`, returning `(low, high)`.
fn ordered_clamped(a: f64, b: f64, min: f64, max: f64) -> (f64, f64) {
    let lo = a.min(b).clamp(min, max);
    let hi = a.max(b).clamp(min, max);
    (lo, hi)
}

/// Converts a time span to a half-open frame range `[start, end)`.
///
/// Times are ordered and clamped to `[0, duration]`, then the start floors and
/// the end ceils to whole frames so the range covers every sample the span
/// touches. [`crate::store::AudioStore::range_owned`] clamps the end to the
/// signal's frame count.
fn span_frames(sample_rate: f64, duration: f64, t0: f64, t1: f64) -> (usize, usize) {
    let (lo, hi) = ordered_clamped(t0, t1, 0.0, duration);
    let start = (lo * sample_rate).floor().max(0.0) as usize;
    let end = (hi * sample_rate).ceil().max(0.0) as usize;
    (start, end.max(start))
}

/// Mean of the values whose time falls inside `span`, or `None` when none do.
/// Spectral moments from a single Hann-windowed FFT over the whole span, the
/// way Praat's `Spectrum` takes them — one transform of the selection, weighted
/// by magnitude to the `power` (2.0 is the power spectrum). This supersedes the
/// earlier single-frame slice at the span midpoint, which biased fricative
/// centre-of-gravity toward whichever frame the midpoint happened to land on.
fn windowed_span_moments(mono: &[f32], sample_rate: f64, power: f64) -> Moments {
    let n = mono.len();
    if n < 4 || sample_rate <= 0.0 {
        return phx_voice::spectral_moments(
            &phx_voice::SpectrumSlice {
                frequencies_hz: Vec::new(),
                values: Vec::new(),
            },
            power,
        );
    }
    let window = window_samples(Window::Hanning, n);
    let mut buffer: Vec<f64> = mono
        .iter()
        .zip(&window)
        .map(|(&s, &w)| f64::from(s) * w)
        .collect();
    let mut plan = RealFftPlan::new();
    let spectrum = plan.rfft(&mut buffer);
    let mut frequencies_hz = Vec::with_capacity(spectrum.len());
    let mut values = Vec::with_capacity(spectrum.len());
    for (k, bin) in spectrum.iter().enumerate() {
        frequencies_hz.push(k as f64 * sample_rate / n as f64);
        values.push(bin.norm());
    }
    phx_voice::spectral_moments(
        &phx_voice::SpectrumSlice {
            frequencies_hz,
            values,
        },
        power,
    )
}

/// Merges adjacent segments of the same class, so a run stays one interval.
fn coalesce_segments(segs: Vec<(f64, f64, bool)>) -> Vec<(f64, f64, bool)> {
    let mut out: Vec<(f64, f64, bool)> = Vec::new();
    for (a, b, sounding) in segs {
        match out.last_mut() {
            Some(last) if last.2 == sounding => last.1 = b,
            _ => out.push((a, b, sounding)),
        }
    }
    out
}

fn mean_in_span(frames: impl Iterator<Item = (f64, f64)>, span: TimeSpan) -> Option<f64> {
    let mut sum = 0.0;
    let mut count = 0usize;
    for (time, value) in frames {
        if span.contains(time) {
            sum += value;
            count += 1;
        }
    }
    (count > 0).then(|| sum / count as f64)
}

/// Sample standard deviation of the frame values inside `span`, absent when
/// fewer than two frames fall in it.
fn sd_in_span(frames: impl Iterator<Item = (f64, f64)>, span: TimeSpan) -> Option<f64> {
    let values: Vec<f64> = frames
        .filter(|(time, _)| span.contains(*time))
        .map(|(_, value)| value)
        .collect();
    if values.len() < 2 {
        return None;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    Some(variance.sqrt())
}

/// Validates a [`FormantParams`] before it reaches `phx_formant`.
///
/// `phx_formant::formant_track` asserts these same properties and panics on
/// violation. The engine is the boundary untrusted callers reach, so it
/// re-checks them here and turns a would-be panic into a typed error.
fn validate_formant_params(params: &FormantParams) -> Result<(), EngineError> {
    let invalid = |reason: &str| {
        Err(EngineError::InvalidRequest {
            reason: reason.to_string(),
        })
    };
    if !(params.ceiling_hz.is_finite() && params.ceiling_hz > 100.0) {
        return invalid("params.ceiling_hz must be finite and greater than 100 Hz");
    }
    if params.max_formants == 0 {
        return invalid("params.max_formants must be positive");
    }
    if !(params.window_length.is_finite() && params.window_length > 0.0) {
        return invalid("params.window_length must be finite and positive");
    }
    if let Some(step) = params.time_step
        && !(step.is_finite() && step > 0.0)
    {
        return invalid("params.time_step must be finite and positive when set");
    }
    if !params.preemphasis_from_hz.is_finite() {
        return invalid("params.preemphasis_from_hz must be finite");
    }
    Ok(())
}

/// Validates a [`TileRequest`] before it reaches `phx_spectrogram`.
///
/// `phx_spectrogram::compute_tile` asserts these same properties and panics
/// on violation, which is the right contract for a pure math crate calling
/// itself internally with already-validated data. The engine is the
/// boundary that untrusted callers reach, so it re-checks the same
/// properties here and turns a would-be panic into a typed error.
fn validate_tile_request(req: &TileRequest) -> Result<(), EngineError> {
    let invalid = |reason: &str| {
        Err(EngineError::InvalidRequest {
            reason: reason.to_string(),
        })
    };

    if !req.t0.is_finite() || !req.t1.is_finite() {
        return invalid("t0/t1 must be finite");
    }
    if !req.f0.is_finite() || !req.f1.is_finite() {
        return invalid("f0/f1 must be finite");
    }
    let params = &req.params;
    if !(params.window_length.is_finite() && params.window_length > 0.0) {
        return invalid("params.window_length must be finite and positive");
    }
    if !(params.max_frequency.is_finite() && params.max_frequency >= 0.0) {
        return invalid("params.max_frequency must be finite and non-negative");
    }
    if !(params.time_step.is_finite() && params.time_step > 0.0) {
        return invalid("params.time_step must be finite and positive");
    }
    if !(params.frequency_step.is_finite() && params.frequency_step > 0.0) {
        return invalid("params.frequency_step must be finite and positive");
    }
    if let Window::Gaussian {
        effective_len_factor,
    } = params.window
        && !(effective_len_factor.is_finite() && effective_len_factor > 0.0)
    {
        return invalid("params.window Gaussian effective_len_factor must be finite and positive");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{PI, TAU};

    const FIXTURE_WAV: &[u8] = include_bytes!("../../../tests/fixtures/audio/arctic_bdl_a0001.wav");
    const VOWEL_WAV: &[u8] = include_bytes!("../../../tests/fixtures/audio/synth_vowel_a.wav");
    const FIXTURE_AIFF: &[u8] =
        include_bytes!("../../phx-audio/tests/fixtures/aiff_stereo_16_44100.aiff");
    const FIXTURE_AIFF_WAV_TWIN: &[u8] =
        include_bytes!("../../phx-audio/tests/fixtures/aiff_stereo_16_44100.wav");
    const FIXTURE_FLAC: &[u8] = include_bytes!("../../phx-audio/tests/fixtures/flac_level5.flac");
    const FIXTURE_FLAC_WAV_TWIN: &[u8] =
        include_bytes!("../../phx-audio/tests/fixtures/flac_base16.wav");

    fn sine_wav_bytes(sample_rate: u32, seconds: f64, frequency: f64) -> Vec<u8> {
        let frames = (sample_rate as f64 * seconds).round() as u32;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
            for i in 0..frames {
                let t = i as f64 / sample_rate as f64;
                let sample = (2.0 * PI * frequency * t).sin();
                writer.write_sample((sample * 32_000.0) as i16).unwrap();
            }
            writer.finalize().unwrap();
        }
        cursor.into_inner()
    }

    #[test]
    fn band_energy_is_finite_over_a_voiced_box_and_errors_on_nan() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(VOWEL_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let value = engine
            .band_energy(id, info.duration * 0.3, info.duration * 0.6, 0.0, 4000.0)
            .unwrap();
        assert!(value.is_finite(), "band energy over a vowel box is finite");
        // Order-independence: swapping the bounds names the same box.
        let swapped = engine
            .band_energy(id, info.duration * 0.6, info.duration * 0.3, 4000.0, 0.0)
            .unwrap();
        assert_eq!(value.to_bits(), swapped.to_bits());
        assert!(matches!(
            engine.band_energy(id, f64::NAN, 0.1, 0.0, 4000.0),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn selection_readout_band_energy_equals_the_direct_query() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(VOWEL_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let (t0, t1, f0, f1) = (info.duration * 0.3, info.duration * 0.6, 0.0, 4000.0);
        let readout = engine
            .selection_readout(id, t0, t1, f0, f1, 75.0, 600.0, 100.0)
            .unwrap();
        let direct = engine.band_energy(id, t0, t1, f0, f1).unwrap();
        // The batch-equals-GUI invariant: the readout's band energy is the same
        // engine query a script would run for the same box.
        assert_eq!(readout.band_energy_db.to_bits(), direct.to_bits());
        assert!((readout.duration - (t1 - t0)).abs() < 1.0e-12);
        assert!(readout.f0_mean_hz.is_some(), "vowel span should be voiced");
    }

    #[test]
    fn formant_span_bandwidth_means_are_present_and_positive() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(VOWEL_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let params = FormantParams::default();
        let bandwidths = engine
            .formant_span_bandwidth_means(
                id,
                &params,
                false,
                info.duration * 0.3,
                info.duration * 0.6,
            )
            .unwrap();
        let first = bandwidths
            .iter()
            .flatten()
            .next()
            .copied()
            .expect("a voiced vowel span carries at least one formant");
        assert!(
            first > 0.0,
            "formant bandwidth {first} Hz should be positive"
        );
    }

    #[test]
    fn selection_readout_reports_rms_and_peak() {
        // A full-scale 200 Hz sine: absolute peak ≈ 1.0, RMS ≈ 1/√2.
        let sr = 16_000.0;
        let samples: Vec<f32> = (0..8_000)
            .map(|i| (TAU * 200.0 * i as f64 / sr).sin() as f32)
            .collect();
        let mut engine = Engine::new();
        let id = engine.store.insert(Audio::new(vec![samples], sr).unwrap());

        let readout = engine
            .selection_readout(id, 0.0, 0.5, 0.0, 8000.0, 75.0, 600.0, 100.0)
            .unwrap();
        let peak = readout.peak.expect("non-empty span has a peak");
        let rms = readout.rms.expect("non-empty span has an rms");
        assert!((peak - 1.0).abs() < 0.01, "peak {peak}");
        assert!((rms - 0.5_f64.sqrt()).abs() < 0.02, "rms {rms}");

        // A steady sine holds a near-constant F0 and intensity, so both spreads
        // are present and small.
        let f0_sd = readout
            .f0_sd_hz
            .expect("steady sine is voiced across frames");
        let intensity_sd = readout
            .intensity_sd_db
            .expect("steady sine has intensity frames");
        assert!(f0_sd < 5.0, "steady F0 SD {f0_sd} should be small");
        assert!(
            intensity_sd < 3.0,
            "steady intensity SD {intensity_sd} should be small"
        );
    }

    #[test]
    fn colormap_change_recolorizes_cached_db_without_recomputing() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(VOWEL_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let req = TileRequest {
            t0: 0.0,
            t1: info.duration,
            f0: 0.0,
            f1: 5000.0,
            width_px: 220,
            height_px: 128,
            params: SpectrogramParams::default(),
        };
        let display = DisplayMapping::default();
        let viridis = engine
            .spectrogram_tile_rgba(id, &req, &display, Colormap::Viridis, false, false)
            .unwrap();
        let after_first = engine.spectrogram_cached_block_count();
        assert!(after_first > 0, "first tile populates the block cache");

        let magma = engine
            .spectrogram_tile_rgba(id, &req, &display, Colormap::Magma, false, false)
            .unwrap();
        // Re-colorizing the same viewport reuses every cached block: no new STFT.
        assert_eq!(engine.spectrogram_cached_block_count(), after_first);
        assert_ne!(
            viridis, magma,
            "different palettes produce different pixels"
        );

        // A custom LUT re-colorizes the same cached dB: still no new STFT.
        let mut lut = [[0u8; 3]; 256];
        for (i, entry) in lut.iter_mut().enumerate() {
            *entry = [i as u8, (255 - i) as u8, i as u8];
        }
        let custom = engine
            .spectrogram_tile_rgba_lut(id, &req, &display, &lut, false, false)
            .unwrap();
        assert_eq!(engine.spectrogram_cached_block_count(), after_first);
        assert_ne!(custom, magma, "a custom ramp produces its own pixels");

        // Deterministic through the cache: the same request twice is identical.
        let viridis_again = engine
            .spectrogram_tile_rgba(id, &req, &display, Colormap::Viridis, false, false)
            .unwrap();
        assert_eq!(viridis, viridis_again);
        let custom_again = engine
            .spectrogram_tile_rgba_lut(id, &req, &display, &lut, false, false)
            .unwrap();
        assert_eq!(custom, custom_again);
    }

    #[test]
    fn display_preemphasis_recolorizes_without_recomputing() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(VOWEL_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let req = TileRequest {
            t0: 0.0,
            t1: info.duration,
            f0: 0.0,
            f1: 5000.0,
            width_px: 220,
            height_px: 128,
            params: SpectrogramParams::default(),
        };
        let display = DisplayMapping::default();
        let plain = engine
            .spectrogram_tile_rgba(id, &req, &display, Colormap::Viridis, false, false)
            .unwrap();
        let blocks = engine.spectrogram_cached_block_count();
        let lifted = engine
            .spectrogram_tile_rgba(id, &req, &display, Colormap::Viridis, false, true)
            .unwrap();
        // Pre-emphasis is a post-cache display tilt: the rendered pixels change,
        // but no new STFT block is computed.
        assert_ne!(plain, lifted, "pre-emphasis changes the rendered tile");
        assert_eq!(
            engine.spectrogram_cached_block_count(),
            blocks,
            "pre-emphasis re-colorizes cached dB rather than recomputing"
        );
    }

    #[test]
    fn changing_analysis_params_keys_new_blocks() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(VOWEL_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let display = DisplayMapping::default();
        let base = TileRequest {
            t0: 0.0,
            t1: info.duration,
            f0: 0.0,
            f1: 5000.0,
            width_px: 200,
            height_px: 120,
            params: SpectrogramParams::default(),
        };
        engine
            .spectrogram_tile_rgba(id, &base, &display, Colormap::Viridis, false, false)
            .unwrap();
        let after_base = engine.spectrogram_cached_block_count();

        let widened = TileRequest {
            params: SpectrogramParams {
                window_length: 0.01,
                ..SpectrogramParams::default()
            },
            ..base.clone()
        };
        engine
            .spectrogram_tile_rgba(id, &widened, &display, Colormap::Viridis, false, false)
            .unwrap();
        // A different analysis parameter hashes to a different key, so the new
        // blocks sit alongside the old rather than colliding with them.
        assert!(engine.spectrogram_cached_block_count() > after_base);
    }

    #[test]
    fn voice_report_on_clean_vowel_has_low_perturbation_and_high_hnr() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(VOWEL_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let report = engine
            .voice_report(id, info.duration * 0.2, info.duration * 0.8, 75.0, 600.0)
            .unwrap();
        let jitter = report.jitter.local.expect("local jitter over the vowel");
        let shimmer = report.shimmer.local.expect("local shimmer over the vowel");
        let hnr = report.mean_hnr_db.expect("mean HNR over the vowel");
        assert!(
            jitter < 0.05,
            "clean vowel local jitter {jitter} should be small"
        );
        assert!(
            shimmer < 0.2,
            "clean vowel local shimmer {shimmer} should be small"
        );
        assert!(hnr > 10.0, "clean vowel HNR {hnr} dB should be high");
    }

    #[test]
    fn import_then_info_reports_the_decoded_buffer() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(16_000, 0.5, 440.0);
        let id = engine.import_audio_bytes(&bytes).unwrap();
        let info = engine.audio_info(id).unwrap();
        assert_eq!(info.sample_rate, 16_000.0);
        assert_eq!(info.channels, 1);
        assert!((info.duration - 0.5).abs() < 1.0e-9);
    }

    #[test]
    fn streaming_recording_equals_a_one_shot_buffer_bit_for_bit() {
        let sample_rate = 16_000.0;
        let samples: Vec<f32> = (0..2_000)
            .map(|i| (2.0 * PI * 220.0 * i as f64 / sample_rate).sin() as f32)
            .collect();

        // Stream the same samples through three uneven chunks.
        let mut engine = Engine::new();
        let rec = engine.begin_recording(sample_rate, 1).unwrap();
        for chunk in samples.chunks(517) {
            engine.append_samples(rec, chunk).unwrap();
        }
        let finished = engine.finish_recording(rec, "take".to_string()).unwrap();

        // The materialized buffer matches one built from the whole sample vector
        // in a single call, sample for sample.
        let streamed = engine.store.audio(finished.audio).unwrap();
        assert_eq!(streamed.sample_rate(), sample_rate);
        assert_eq!(streamed.name(), Some("take"));
        let one_shot = Audio::new(vec![samples.clone()], sample_rate).unwrap();
        assert_eq!(streamed.frames(), one_shot.frames());
        for (a, b) in streamed.channel(0).iter().zip(one_shot.channel(0)) {
            assert_eq!(a.to_bits(), b.to_bits());
        }

        // The finished id is spent; a second finish is a typed error.
        assert!(matches!(
            engine.finish_recording(rec, "again".to_string()),
            Err(EngineError::UnknownRecordingId(_))
        ));

        // The WAV bytes round-trip back to the same signal.
        let reloaded = Audio::from_wav_bytes(&finished.wav).unwrap();
        assert_eq!(reloaded.frames(), samples.len());
    }

    #[test]
    fn streaming_recording_interleaves_planar_channels() {
        let mut engine = Engine::new();
        let rec = engine.begin_recording(8_000.0, 2).unwrap();
        // Two frames per chunk, planar: [L0, L1, R0, R1].
        engine.append_samples(rec, &[0.1, 0.2, -0.1, -0.2]).unwrap();
        engine.append_samples(rec, &[0.3, 0.4, -0.3, -0.4]).unwrap();
        let finished = engine.finish_recording(rec, "stereo".to_string()).unwrap();
        let audio = engine.store.audio(finished.audio).unwrap();
        assert_eq!(audio.channel_count(), 2);
        assert_eq!(audio.channel(0), &[0.1, 0.2, 0.3, 0.4]);
        assert_eq!(audio.channel(1), &[-0.1, -0.2, -0.3, -0.4]);
    }

    #[test]
    fn aborted_recording_leaves_no_store_entry_and_a_typed_error() {
        let mut engine = Engine::new();
        let before = engine.store.ids_sorted().len();
        let rec = engine.begin_recording(16_000.0, 1).unwrap();
        engine.append_samples(rec, &[0.0; 256]).unwrap();
        engine.abort_recording(rec).unwrap();
        // Aborting materializes nothing.
        assert_eq!(engine.store.ids_sorted().len(), before);
        // The take is gone: appending, finishing, or aborting again all reject.
        assert!(matches!(
            engine.append_samples(rec, &[0.0; 4]),
            Err(EngineError::UnknownRecordingId(_))
        ));
        assert!(matches!(
            engine.abort_recording(rec),
            Err(EngineError::UnknownRecordingId(_))
        ));
    }

    #[test]
    fn recording_rejects_bad_parameters_and_ragged_chunks() {
        let mut engine = Engine::new();
        assert!(matches!(
            engine.begin_recording(0.0, 1),
            Err(EngineError::InvalidRequest { .. })
        ));
        assert!(matches!(
            engine.begin_recording(16_000.0, 0),
            Err(EngineError::InvalidRequest { .. })
        ));
        let rec = engine.begin_recording(16_000.0, 2).unwrap();
        // An odd chunk cannot split across two channels.
        assert!(matches!(
            engine.append_samples(rec, &[0.1, 0.2, 0.3]),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn unknown_id_is_a_typed_error_everywhere() {
        let engine = Engine::new();
        let bogus = AudioId::from_u64(999);
        assert!(matches!(
            engine.audio_info(bogus),
            Err(EngineError::UnknownAudioId(_))
        ));
        assert!(matches!(
            engine.waveform_slice(bogus, 0.0, 1.0, 8),
            Err(EngineError::UnknownAudioId(_))
        ));
        assert!(matches!(
            engine.spectrogram_tile_rgba(
                bogus,
                &TileRequest {
                    t0: 0.0,
                    t1: 0.1,
                    f0: 0.0,
                    f1: 5000.0,
                    width_px: 4,
                    height_px: 4,
                    params: SpectrogramParams::default(),
                },
                &DisplayMapping::default(),
                Colormap::Viridis,
                false,
                false,
            ),
            Err(EngineError::UnknownAudioId(_))
        ));
    }

    #[test]
    fn unrecognized_bytes_are_a_typed_error_not_a_panic() {
        let mut engine = Engine::new();
        assert!(matches!(
            engine.import_audio_bytes(b"not any audio container"),
            Err(EngineError::Audio(AudioError::UnrecognizedFormat))
        ));
    }

    #[test]
    fn truncated_known_container_is_distinguished_from_unrecognized_bytes() {
        let mut engine = Engine::new();
        // A RIFF/WAVE signature with the rest of the file cut off: recognized
        // as WAV, but too short to decode, so the error names the corrupt
        // container instead of falling back to "unrecognized format".
        let err = engine.import_audio_bytes(&FIXTURE_WAV[..16]).unwrap_err();
        assert!(
            matches!(err, EngineError::Audio(AudioError::MalformedWav(_))),
            "truncated WAV bytes should report MalformedWav, got {err:?}"
        );

        let err = engine.import_audio_bytes(&FIXTURE_AIFF[..16]).unwrap_err();
        assert!(
            matches!(err, EngineError::Audio(AudioError::MalformedAiff(_))),
            "truncated AIFF bytes should report MalformedAiff, got {err:?}"
        );

        let err = engine.import_audio_bytes(&FIXTURE_FLAC[..16]).unwrap_err();
        assert!(
            matches!(err, EngineError::Audio(AudioError::MalformedFlac(_))),
            "truncated FLAC bytes should report MalformedFlac, got {err:?}"
        );
    }

    #[test]
    fn import_audio_bytes_decodes_aiff_and_flac() {
        let mut engine = Engine::new();

        let aiff_id = engine.import_audio_bytes(FIXTURE_AIFF).unwrap();
        let wav_twin = Audio::from_wav_bytes(FIXTURE_AIFF_WAV_TWIN).unwrap();
        let aiff_info = engine.audio_info(aiff_id).unwrap();
        assert_eq!(aiff_info.sample_rate, wav_twin.sample_rate());
        assert_eq!(aiff_info.channels, wav_twin.channel_count());

        let flac_id = engine.import_audio_bytes(FIXTURE_FLAC).unwrap();
        let wav_twin = Audio::from_wav_bytes(FIXTURE_FLAC_WAV_TWIN).unwrap();
        let flac_info = engine.audio_info(flac_id).unwrap();
        assert_eq!(flac_info.sample_rate, wav_twin.sample_rate());
        assert_eq!(flac_info.channels, wav_twin.channel_count());
    }

    #[test]
    fn command_import_audio_also_accepts_aiff_and_flac() {
        let mut engine = Engine::new();

        let applied = engine
            .apply(Command::ImportAudio {
                bytes: FIXTURE_AIFF.to_vec(),
                name: "aiff take".to_string(),
            })
            .unwrap();
        let Applied::AudioImported { audio } = applied else {
            panic!("expected AudioImported, got {applied:?}");
        };
        assert_eq!(
            engine.audio_info(audio).unwrap().name.as_deref(),
            Some("aiff take")
        );

        let applied = engine
            .apply(Command::ImportAudio {
                bytes: FIXTURE_FLAC.to_vec(),
                name: "flac take".to_string(),
            })
            .unwrap();
        let Applied::AudioImported { audio } = applied else {
            panic!("expected AudioImported, got {applied:?}");
        };
        assert_eq!(
            engine.audio_info(audio).unwrap().name.as_deref(),
            Some("flac take")
        );
    }

    #[test]
    fn non_finite_tile_request_bounds_are_a_typed_error_not_a_panic() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let req = TileRequest {
            t0: f64::NAN,
            t1: 0.1,
            f0: 0.0,
            f1: 5000.0,
            width_px: 4,
            height_px: 4,
            params: SpectrogramParams::default(),
        };
        assert!(matches!(
            engine.spectrogram_tile_rgba(
                id,
                &req,
                &DisplayMapping::default(),
                Colormap::Viridis,
                false,
                false,
            ),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn non_finite_waveform_bounds_are_a_typed_error_not_a_panic() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        assert!(matches!(
            engine.waveform_slice(id, f64::NAN, 1.0, 8),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn waveform_pyramid_agrees_with_direct_min_max_on_fixture_audio() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let audio = Audio::from_wav_bytes(FIXTURE_WAV).unwrap();
        let mono = audio.mono_mix().into_owned();
        let sample_rate = audio.sample_rate();
        let duration = audio.duration();

        let px = 50;
        let t0 = duration * 0.1;
        let t1 = duration * 0.6;
        let slice = engine.waveform_slice(id, t0, t1, px).unwrap();
        assert_eq!(slice.len() as u32, px);

        for (i, bucket) in slice.iter().enumerate() {
            let frac0 = i as f64 / px as f64;
            let frac1 = (i + 1) as f64 / px as f64;
            let start = ((t0 + frac0 * (t1 - t0)) * sample_rate)
                .round()
                .clamp(0.0, mono.len() as f64) as usize;
            let mut end = ((t0 + frac1 * (t1 - t0)) * sample_rate)
                .round()
                .clamp(0.0, mono.len() as f64) as usize;
            end = end.max(start);
            if end == start && start < mono.len() {
                end = start + 1;
            }
            let expected_min = mono[start..end]
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
            let expected_max = mono[start..end]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            assert_eq!(
                bucket.min.to_bits(),
                expected_min.to_bits(),
                "bucket {i} min"
            );
            assert_eq!(
                bucket.max.to_bits(),
                expected_max.to_bits(),
                "bucket {i} max"
            );
        }
    }

    #[test]
    fn waveform_pyramid_agrees_with_direct_min_max_on_synthetic_audio() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let id = engine.import_audio_bytes(&bytes).unwrap();
        let audio = Audio::from_wav_bytes(&bytes).unwrap();
        let mono = audio.mono_mix().into_owned();

        let px = 64;
        let slice = engine.waveform_slice(id, 0.0, 1.0, px).unwrap();
        for (i, bucket) in slice.iter().enumerate() {
            let start = (mono.len() * i / px as usize).min(mono.len());
            let end = (mono.len() * (i + 1) / px as usize).min(mono.len());
            if start == end {
                continue;
            }
            let expected_min = mono[start..end]
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
            let expected_max = mono[start..end]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(bucket.min <= expected_min + f32::EPSILON);
            assert!(bucket.max >= expected_max - f32::EPSILON);
        }
    }

    #[test]
    fn spectrogram_tile_has_expected_dimensions_and_is_deterministic() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let req = TileRequest {
            t0: 0.05,
            t1: 0.35,
            f0: 0.0,
            f1: 5000.0,
            width_px: 40,
            height_px: 30,
            params: SpectrogramParams::default(),
        };
        let display = DisplayMapping::default();
        let first = engine
            .spectrogram_tile_rgba(id, &req, &display, Colormap::Viridis, false, false)
            .unwrap();
        let second = engine
            .spectrogram_tile_rgba(id, &req, &display, Colormap::Viridis, false, false)
            .unwrap();
        assert_eq!(first.len(), 40 * 30 * 4);
        assert_eq!(first, second);
    }

    #[test]
    fn pitch_track_on_fixture_is_voiced_in_the_male_speech_band() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let track = engine.pitch_track(id, &PitchParams::default()).unwrap();
        let voiced: Vec<f64> = track.frames().iter().filter_map(|frame| frame.f0).collect();
        assert!(!voiced.is_empty(), "male speech fixture should be voiced");
        // bdl is an adult male speaker; every voiced frame stays inside the
        // analysis band, and the median must sit in the 70-300 Hz range Praat
        // reports for this corpus. Individual frames can reach the ceiling on
        // octave slips, so the band claim rests on the median, not each frame.
        for f0 in &voiced {
            assert!(
                *f0 > 50.0 && *f0 <= PitchParams::default().ceiling_hz,
                "F0 {f0} Hz outside the analysis band"
            );
        }
        let mut sorted = voiced.clone();
        sorted.sort_by(f64::total_cmp);
        let median = sorted[sorted.len() / 2];
        assert!(
            (70.0..=300.0).contains(&median),
            "median F0 {median} Hz outside male band"
        );
    }

    #[test]
    fn pitch_track_span_places_frames_on_the_absolute_timeline() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let info = engine.audio_info(id).unwrap();
        let t0 = info.duration * 0.3;
        let t1 = info.duration * 0.6;
        let (track, start_time) = engine
            .pitch_track_span(id, &PitchParams::default(), t0, t1)
            .unwrap();
        assert!(!track.frames().is_empty());
        assert!(start_time >= t0 - 1.0e-3 && start_time <= t1);
        // Every frame, shifted onto the absolute timeline, lands inside the
        // requested window (allowing the leading half-window margin).
        for frame in track.frames() {
            let abs = start_time + frame.time;
            assert!(
                abs >= t0 - 1.0e-3 && abs <= t1 + 1.0e-3,
                "abs {abs} out of span"
            );
        }
    }

    #[test]
    fn formant_track_raw_and_smoothed_share_the_frame_grid() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let params = FormantParams::default();
        let raw = engine.formant_track(id, &params).unwrap();
        let smoothed = engine.formant_track_smoothed(id, &params).unwrap();
        assert!(!raw.frames.is_empty());
        assert_eq!(raw.frames.len(), smoothed.frames.len());
        assert_eq!(raw.frame_grid, smoothed.frame_grid);
        let has_formants = raw.frames.iter().any(|frame| !frame.formants.is_empty());
        assert!(
            has_formants,
            "speech fixture should yield formant candidates"
        );
    }

    #[test]
    fn bad_formant_ceiling_is_a_typed_error_not_a_panic() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let params = FormantParams {
            ceiling_hz: 10.0,
            ..FormantParams::default()
        };
        assert!(matches!(
            engine.formant_track(id, &params),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn intensity_track_on_fixture_is_non_empty_and_finite() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let track = engine
            .intensity_track(id, &IntensityParams::default())
            .unwrap();
        assert!(!track.is_empty());
        assert!(track.values().iter().all(|db| db.is_finite()));
    }

    #[test]
    fn bad_intensity_floor_is_a_typed_error_not_a_panic() {
        let mut engine = Engine::new();
        let id = engine.import_audio_bytes(FIXTURE_WAV).unwrap();
        let params = IntensityParams {
            pitch_floor_hz: 0.0,
            ..IntensityParams::default()
        };
        assert!(matches!(
            engine.intensity_track(id, &params),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn analysis_on_unknown_id_is_a_typed_error() {
        let engine = Engine::new();
        let bogus = AudioId::from_u64(4242);
        assert!(matches!(
            engine.pitch_track(bogus, &PitchParams::default()),
            Err(EngineError::UnknownAudioId(_))
        ));
        assert!(matches!(
            engine.formant_track(bogus, &FormantParams::default()),
            Err(EngineError::UnknownAudioId(_))
        ));
        assert!(matches!(
            engine.intensity_track(bogus, &IntensityParams::default()),
            Err(EngineError::UnknownAudioId(_))
        ));
    }

    #[test]
    fn tile_request_too_short_for_a_frame_is_a_typed_error() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 0.001, 440.0);
        let id = engine.import_audio_bytes(&bytes).unwrap();
        let req = TileRequest {
            t0: 0.0,
            t1: 0.001,
            f0: 0.0,
            f1: 4000.0,
            width_px: 4,
            height_px: 4,
            params: SpectrogramParams::default(),
        };
        assert!(matches!(
            engine.spectrogram_tile_rgba(
                id,
                &req,
                &DisplayMapping::default(),
                Colormap::Viridis,
                false,
                false,
            ),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    // --- Journal and annotation surface ---

    /// Small deterministic xorshift generator; the property test needs a
    /// reproducible command stream without pulling in an rng dependency.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Self {
            Self(seed | 1)
        }

        fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next_u64() % n as u64) as usize
        }

        fn frac(&mut self) -> f64 {
            (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn annotation_with_tier(xmax: f64) -> Annotation {
        let mut annotation = Annotation::new(0.0, xmax).unwrap();
        annotation
            .add_interval_tier("phones", TierRelation::Independent)
            .unwrap();
        annotation
    }

    fn base_engine() -> (Engine, AudioId, AnnotationId) {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 2.0, 220.0);
        let audio = match engine
            .apply(Command::ImportAudio {
                bytes,
                name: "base".to_string(),
            })
            .unwrap()
        {
            Applied::AudioImported { audio } => audio,
            other => panic!("expected AudioImported, got {other:?}"),
        };
        let annotation = annotation_with_tier(2.0);
        let doc = match engine
            .apply(Command::AttachAnnotation { audio, annotation })
            .unwrap()
        {
            Applied::AnnotationAttached { annotation, .. } => annotation,
            other => panic!("expected AnnotationAttached, got {other:?}"),
        };
        (engine, audio, doc)
    }

    fn interval_tiers(annotation: &Annotation) -> Vec<(TierId, Vec<Interval>)> {
        annotation
            .tiers()
            .iter()
            .filter_map(|slot| match &slot.tier {
                Tier::Interval(tier) => Some((slot.id, tier.intervals.clone())),
                Tier::Point(_) => None,
            })
            .collect()
    }

    #[test]
    fn rename_audio_reports_and_reads_back_and_undo_redo_is_hash_stable() {
        let (mut engine, audio, _doc) = base_engine();
        let before = engine.state_hash();
        assert_eq!(
            engine.audio_info(audio).unwrap().name.as_deref(),
            Some("base")
        );

        let applied = engine
            .apply(Command::RenameAudio {
                id: audio,
                name: "renamed".to_string(),
            })
            .unwrap();
        assert!(matches!(
            applied,
            Applied::AudioRenamed { audio: a, ref name } if a == audio && name == "renamed"
        ));
        assert_eq!(
            engine.audio_info(audio).unwrap().name.as_deref(),
            Some("renamed")
        );
        let renamed_hash = engine.state_hash();
        assert_ne!(renamed_hash, before, "rename must shift the state hash");

        engine.undo().unwrap();
        assert_eq!(
            engine.audio_info(audio).unwrap().name.as_deref(),
            Some("base")
        );
        assert_eq!(engine.state_hash(), before);

        engine.redo().unwrap();
        assert_eq!(
            engine.audio_info(audio).unwrap().name.as_deref(),
            Some("renamed")
        );
        assert_eq!(engine.state_hash(), renamed_hash);
    }

    fn tier_name(engine: &Engine, doc: AnnotationId, tier: TierId) -> String {
        match &engine.annotation(doc).unwrap().tier(tier).unwrap().tier {
            Tier::Interval(t) => t.name.clone(),
            Tier::Point(t) => t.name.clone(),
        }
    }

    #[test]
    fn rename_tier_reports_and_reads_back_and_undo_redo_is_hash_stable() {
        let (mut engine, _audio, doc) = base_engine();
        let tier = interval_tiers(engine.annotation(doc).unwrap())[0].0;
        let before = engine.state_hash();
        assert_eq!(tier_name(&engine, doc, tier), "phones");

        let applied = engine
            .apply(Command::RenameTier {
                annotation: doc,
                tier,
                name: "segments".to_string(),
            })
            .unwrap();
        assert!(matches!(
            applied,
            Applied::TierRenamed { annotation, tier: t, ref name }
                if annotation == doc && t == tier && name == "segments"
        ));
        assert_eq!(tier_name(&engine, doc, tier), "segments");
        let renamed_hash = engine.state_hash();
        assert_ne!(renamed_hash, before, "rename must shift the state hash");

        engine.undo().unwrap();
        assert_eq!(tier_name(&engine, doc, tier), "phones");
        assert_eq!(engine.state_hash(), before);

        engine.redo().unwrap();
        assert_eq!(tier_name(&engine, doc, tier), "segments");
        assert_eq!(engine.state_hash(), renamed_hash);
    }

    #[test]
    fn rename_unknown_tier_rejects() {
        let (mut engine, _audio, doc) = base_engine();
        assert!(
            engine
                .apply(Command::RenameTier {
                    annotation: doc,
                    tier: TierId::new(9_999),
                    name: "x".to_string(),
                })
                .is_err()
        );
    }

    #[test]
    fn rename_unknown_audio_rejects() {
        let mut engine = Engine::new();
        assert!(matches!(
            engine.apply(Command::RenameAudio {
                id: AudioId::from_u64(7),
                name: "x".to_string(),
            }),
            Err(EngineError::UnknownAudioId(_))
        ));
    }

    #[test]
    fn detach_audio_cascades_to_documents_and_undo_redo_is_hash_stable() {
        let (mut engine, audio, doc) = base_engine();
        let before = engine.state_hash();
        assert!(engine.annotation(doc).is_ok());

        let applied = engine.apply(Command::DetachAudio { id: audio }).unwrap();
        match applied {
            Applied::AudioDetached {
                audio: a,
                annotations,
            } => {
                assert_eq!(a, audio);
                assert_eq!(annotations, vec![doc]);
            }
            other => panic!("expected AudioDetached, got {other:?}"),
        }
        // Audio and its cascaded document are both gone.
        assert!(engine.audio_info(audio).is_err());
        assert!(engine.annotation(doc).is_err());
        let detached_hash = engine.state_hash();
        assert_ne!(detached_hash, before);

        let undone = engine.undo().unwrap().unwrap();
        match undone {
            Applied::AudioRestored {
                audio: a,
                annotations,
            } => {
                assert_eq!(a, audio);
                assert_eq!(annotations, vec![doc]);
            }
            other => panic!("expected AudioRestored, got {other:?}"),
        }
        assert!(engine.audio_info(audio).is_ok());
        assert_eq!(engine.annotation_audio(doc).unwrap(), audio);
        assert_eq!(engine.state_hash(), before);

        engine.redo().unwrap();
        assert!(engine.audio_info(audio).is_err());
        assert!(engine.annotation(doc).is_err());
        assert_eq!(engine.state_hash(), detached_hash);
    }

    #[test]
    fn import_audio_bytes_is_journaled_and_undo_redo_is_hash_stable() {
        let (mut engine, _audio, _doc) = base_engine();
        let before = engine.state_hash();
        let before_depth = engine.undo_depth();

        let imported = engine
            .import_audio_bytes(&sine_wav_bytes(8_000, 0.3, 330.0))
            .unwrap();
        assert!(engine.audio_info(imported).is_ok());
        assert_eq!(engine.undo_depth(), before_depth + 1);
        let after_import = engine.state_hash();
        assert_ne!(after_import, before, "import must shift the state hash");

        let undone = engine.undo().unwrap().unwrap();
        assert!(matches!(undone, Applied::AudioRemoved { audio } if audio == imported));
        assert!(engine.audio_info(imported).is_err());
        assert_eq!(engine.state_hash(), before);

        let redone = engine.redo().unwrap().unwrap();
        assert!(matches!(redone, Applied::AudioImported { audio } if audio == imported));
        assert!(engine.audio_info(imported).is_ok());
        assert_eq!(engine.state_hash(), after_import);
    }

    #[test]
    fn open_streaming_wav_is_journaled_and_undo_redo_is_hash_stable() {
        let (mut engine, _audio, _doc) = base_engine();
        let before = engine.state_hash();
        let before_depth = engine.undo_depth();

        let bytes = sine_wav_bytes(8_000, 0.3, 330.0);
        let streamed = engine
            .open_streaming_wav(BytesReader::new(bytes), Some("streamed-take".to_string()))
            .unwrap();
        assert!(engine.audio_info(streamed).is_ok());
        assert_eq!(engine.undo_depth(), before_depth + 1);
        let after_open = engine.state_hash();
        assert_ne!(
            after_open, before,
            "streamed open must shift the state hash"
        );

        // Undo detaches (parks) the streamed source rather than dropping it, so
        // the reader is never reopened and no byte is reread on redo.
        let undone = engine.undo().unwrap().unwrap();
        assert!(
            matches!(undone, Applied::AudioDetached { audio, ref annotations } if audio == streamed && annotations.is_empty())
        );
        assert!(engine.audio_info(streamed).is_err());
        assert_eq!(engine.state_hash(), before);

        let redone = engine.redo().unwrap().unwrap();
        assert!(
            matches!(redone, Applied::AudioRestored { audio, ref annotations } if audio == streamed && annotations.is_empty())
        );
        assert!(engine.audio_info(streamed).is_ok());
        assert_eq!(engine.state_hash(), after_open);
    }

    #[test]
    fn finish_recording_is_journaled_and_undo_redo_is_hash_stable() {
        let (mut engine, _audio, _doc) = base_engine();
        let before = engine.state_hash();
        let before_depth = engine.undo_depth();

        let rec = engine.begin_recording(8_000.0, 1).unwrap();
        engine.append_samples(rec, &[0.1, 0.2, -0.1, -0.2]).unwrap();
        let finished = engine.finish_recording(rec, "take".to_string()).unwrap();
        assert!(engine.audio_info(finished.audio).is_ok());
        assert_eq!(engine.undo_depth(), before_depth + 1);
        let after_finish = engine.state_hash();
        assert_ne!(
            after_finish, before,
            "a finished recording must shift the state hash"
        );

        let undone = engine.undo().unwrap().unwrap();
        assert!(matches!(undone, Applied::AudioRemoved { audio } if audio == finished.audio));
        assert!(engine.audio_info(finished.audio).is_err());
        assert_eq!(engine.state_hash(), before);

        let redone = engine.redo().unwrap().unwrap();
        assert!(matches!(redone, Applied::AudioImported { audio } if audio == finished.audio));
        assert!(engine.audio_info(finished.audio).is_ok());
        assert_eq!(engine.state_hash(), after_finish);
    }

    /// Extends the phase-3 hash-stability gate ([`random_fifty_command_undo_stack_is_hash_stable`])
    /// to the three entry points that journal outside a [`Command`]: a raw WAV
    /// import, a streamed open, and a finished recording, interleaved with
    /// ordinary content commands and undone/redone in full.
    #[test]
    fn mixed_raw_audio_imports_interleave_with_commands_and_undo_redo_is_hash_stable() {
        let (mut engine, audio, doc) = base_engine();
        let initial = engine.state_hash();
        let mut applied_hashes = Vec::new();

        // A content edit, so the sequence is not audio-only.
        let tier = engine.annotation(doc).unwrap().tiers()[0].id;
        let interval = interval_tiers(engine.annotation(doc).unwrap())
            .into_iter()
            .find(|(t, _)| *t == tier)
            .unwrap()
            .1[0]
            .id;
        engine
            .apply(Command::SetLabel {
                annotation: doc,
                target: LabelTarget::Interval { tier, interval },
                text: "vowel".to_string(),
            })
            .unwrap();
        applied_hashes.push(engine.state_hash());

        // A raw eager import, outside the command surface.
        let imported = engine
            .import_audio_bytes(&sine_wav_bytes(8_000, 0.2, 440.0))
            .unwrap();
        applied_hashes.push(engine.state_hash());

        // Rename it through the ordinary command path — the two paths must
        // compose in one journal.
        engine
            .apply(Command::RenameAudio {
                id: imported,
                name: "clip".to_string(),
            })
            .unwrap();
        applied_hashes.push(engine.state_hash());

        // A raw streamed open.
        let streamed = engine
            .open_streaming_wav(
                BytesReader::new(sine_wav_bytes(8_000, 0.2, 550.0)),
                Some("streamed".to_string()),
            )
            .unwrap();
        applied_hashes.push(engine.state_hash());

        // A raw finished recording.
        let rec = engine.begin_recording(8_000.0, 1).unwrap();
        engine.append_samples(rec, &[0.3, -0.3, 0.2, -0.2]).unwrap();
        let finished = engine
            .finish_recording(rec, "recorded".to_string())
            .unwrap();
        applied_hashes.push(engine.state_hash());

        // Detach the original audio, cascading its document off the session.
        engine.apply(Command::DetachAudio { id: audio }).unwrap();
        applied_hashes.push(engine.state_hash());

        assert_eq!(engine.undo_depth(), applied_hashes.len() + 2); // + import + attach from base_engine
        assert!(engine.audio_info(imported).is_ok());
        assert!(engine.audio_info(streamed).is_ok());
        assert!(engine.audio_info(finished.audio).is_ok());

        let final_hash = engine.state_hash();

        for expected in applied_hashes.iter().rev().skip(1) {
            engine.undo().unwrap();
            assert_eq!(engine.state_hash(), *expected);
        }
        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), initial);
        assert!(engine.audio_info(imported).is_err());
        assert!(engine.audio_info(streamed).is_err());
        assert!(engine.audio_info(finished.audio).is_err());

        for expected in &applied_hashes {
            engine.redo().unwrap();
            assert_eq!(engine.state_hash(), *expected);
        }
        assert_eq!(engine.state_hash(), final_hash);
        assert!(engine.audio_info(imported).is_ok());
        assert!(engine.audio_info(streamed).is_ok());
        assert!(engine.audio_info(finished.audio).is_ok());
    }

    #[test]
    fn export_span_wav_equals_a_direct_sample_slice_bit_for_bit() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();
        let info = engine.audio_info(audio).unwrap();
        let sr = info.sample_rate;

        let (t0, t1) = (0.25, 0.75);
        let wav = engine
            .export_span_wav(audio, t0, t1, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();

        // The exact half-open frame range the export covers.
        let start = (t0 * sr).floor() as usize;
        let end = (t1 * sr).ceil() as usize;
        let reference = engine.store.range_owned(audio, start, end).unwrap();
        assert_eq!(decoded.frames(), reference.frames());
        for (a, b) in decoded.channel(0).iter().zip(reference.channel(0)) {
            assert_eq!(a.to_bits(), b.to_bits());
        }
    }

    #[test]
    fn zero_span_wav_silences_the_span_and_keeps_the_rest() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();
        let info = engine.audio_info(audio).unwrap();
        let sr = info.sample_rate;

        let (t0, t1) = (0.25, 0.75);
        let wav = engine
            .zero_span_wav(audio, t0, t1, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();

        // The whole recording is returned, its length unchanged.
        assert_eq!(decoded.frames(), 8_000);
        let start = (t0 * sr).floor() as usize;
        let end = (t1 * sr).ceil() as usize;
        let channel = decoded.channel(0);
        // Every sample inside the span is exactly zero.
        assert!(channel[start..end].iter().all(|&s| s == 0.0));
        // The sine outside the span is untouched (some energy remains).
        let outside_energy: f32 = channel[..start].iter().map(|s| s.abs()).sum();
        assert!(
            outside_energy > 1.0,
            "audio outside the span should survive"
        );
    }

    #[test]
    fn reverse_span_wav_is_the_time_reverse_of_the_slice() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();
        let info = engine.audio_info(audio).unwrap();
        let sr = info.sample_rate;

        let (t0, t1) = (0.25, 0.75);
        let wav = engine
            .reverse_span_wav(audio, t0, t1, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();

        let start = (t0 * sr).floor() as usize;
        let end = (t1 * sr).ceil() as usize;
        let reference = engine.store.range_owned(audio, start, end).unwrap();
        assert_eq!(decoded.frames(), reference.frames());

        // Each sample equals its mirror in the unreversed slice, bit-for-bit.
        let n = reference.frames();
        for i in 0..n {
            assert_eq!(
                decoded.channel(0)[i].to_bits(),
                reference.channel(0)[n - 1 - i].to_bits()
            );
        }
    }

    #[test]
    fn scale_intensity_span_wav_hits_the_target_intensity() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();

        let target = 70.0;
        let wav = engine
            .scale_intensity_span_wav(audio, 0.0, 1.0, target, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();

        let ch = decoded.channel(0);
        let mean_sq =
            ch.iter().map(|&s| f64::from(s) * f64::from(s)).sum::<f64>() / ch.len() as f64;
        let db = 10.0 * (mean_sq / (2e-5_f64).powi(2)).log10();
        assert!(
            (db - target).abs() < 1e-2,
            "intensity {db} dB != target {target} dB"
        );
    }

    #[test]
    fn scale_peak_span_wav_hits_the_target_peak() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();

        let target = 0.99;
        let wav = engine
            .scale_peak_span_wav(audio, 0.0, 1.0, target, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();

        let peak = decoded
            .channel(0)
            .iter()
            .map(|&s| s.abs())
            .fold(0.0_f32, f32::max);
        assert!(
            (f64::from(peak) - target).abs() < 1e-4,
            "peak {peak} != target {target}"
        );
    }

    #[test]
    fn nearest_zero_crossing_lands_on_a_sine_zero() {
        let mut engine = Engine::new();
        let freq = 220.0;
        let bytes = sine_wav_bytes(8_000, 1.0, freq);
        let audio = engine.import_audio_bytes(&bytes).unwrap();

        let sr = 8_000.0;
        // sin(2π f t) is zero at t = k / (2f); probe just past one of them.
        let zero_t = 100.0 / (2.0 * freq);
        let probe = zero_t + 0.3 / sr;
        let got = engine.nearest_zero_crossing(audio, probe).unwrap();
        assert!(
            (got - zero_t).abs() < 2.0 / sr,
            "zero crossing {got} not near {zero_t}"
        );
    }

    #[test]
    fn resample_wav_changes_the_sample_rate_and_keeps_duration() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();

        let wav = engine
            .resample_wav(audio, 16_000.0, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();

        assert_eq!(decoded.sample_rate(), 16_000.0);
        // A one-second source stays about one second across the rate change.
        assert!(
            (decoded.duration() - 1.0).abs() < 0.01,
            "duration {}",
            decoded.duration()
        );
    }

    #[test]
    fn harmonicity_track_span_reports_high_hnr_for_a_sine() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 1.0, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();

        let frames = engine.harmonicity_track_span(audio, 0.2, 0.8).unwrap();
        let voiced: Vec<f64> = frames.iter().filter_map(|&(_, db)| db).collect();
        assert!(!voiced.is_empty(), "no voiced frames");
        // A clean sine is near-perfectly periodic, so its HNR runs very high.
        let peak = voiced.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(peak > 20.0, "peak HNR {peak} dB too low for a clean sine");
    }

    #[test]
    fn cpp_track_span_reports_positive_prominence_for_a_sine() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(44_100, 1.0, 150.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();

        let frames = engine.cpp_track_span(audio, 0.2, 0.8).unwrap();
        let values: Vec<f64> = frames.iter().filter_map(|&(_, db)| db).collect();
        assert!(!values.is_empty(), "no cpp frames");
        // A periodic sine places a cepstral rahmonic at 1/f0 that clears the
        // regression baseline; noise would leave the peak near zero.
        let peak = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!(peak > 2.0, "peak CPP {peak} dB too low for a clean sine");
    }

    #[test]
    fn lpc_spectrum_spans_zero_to_nyquist_with_finite_levels() {
        let mut engine = Engine::new();
        let bytes = sine_wav_bytes(8_000, 0.5, 220.0);
        let audio = engine.import_audio_bytes(&bytes).unwrap();

        let (freqs, db) = engine.lpc_spectrum(audio, 0.1, 0.4).unwrap();
        assert_eq!(freqs.len(), 512);
        assert_eq!(db.len(), 512);
        assert!(freqs.first().copied().unwrap() == 0.0);
        assert!((freqs.last().copied().unwrap() - 4_000.0).abs() < 1e-6);
        assert!(freqs.windows(2).all(|pair| pair[1] > pair[0]));
        assert!(db.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn concat_wav_lays_sources_end_to_end() {
        let mut engine = Engine::new();
        let a = engine
            .import_audio_bytes(&sine_wav_bytes(8_000, 0.5, 220.0))
            .unwrap();
        let b = engine
            .import_audio_bytes(&sine_wav_bytes(8_000, 0.5, 330.0))
            .unwrap();

        let wav = engine.concat_wav(&[a, b], BitDepth::Float32).unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();
        assert_eq!(decoded.sample_rate(), 8_000.0);
        assert!(
            (decoded.duration() - 1.0).abs() < 0.01,
            "duration {}",
            decoded.duration()
        );
    }

    #[test]
    fn combine_stereo_wav_lays_sources_on_two_channels() {
        let sr = 8_000.0;
        let left: Vec<f32> = (0..4_000).map(|i| 0.5 * (i as f32 * 0.01).sin()).collect();
        let right: Vec<f32> = (0..4_000).map(|i| -0.3 * (i as f32 * 0.02).cos()).collect();
        let mut engine = Engine::new();
        let a = engine
            .store
            .insert(Audio::new(vec![left.clone()], sr).unwrap());
        let b = engine
            .store
            .insert(Audio::new(vec![right.clone()], sr).unwrap());

        let wav = engine.combine_stereo_wav(a, b, BitDepth::Float32).unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();
        assert_eq!(decoded.channel_count(), 2);
        assert_eq!(decoded.channel(0), left.as_slice());
        assert_eq!(decoded.channel(1), right.as_slice());
    }

    #[test]
    fn concat_wav_resamples_mismatched_rates_to_the_first() {
        let mut engine = Engine::new();
        let a = engine
            .import_audio_bytes(&sine_wav_bytes(8_000, 0.5, 220.0))
            .unwrap();
        let b = engine
            .import_audio_bytes(&sine_wav_bytes(16_000, 0.5, 220.0))
            .unwrap();

        let wav = engine.concat_wav(&[a, b], BitDepth::Float32).unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();
        // The first source (8 kHz) sets the rate; the total stays the sum.
        assert_eq!(decoded.sample_rate(), 8_000.0);
        assert!(
            (decoded.duration() - 1.0).abs() < 0.01,
            "duration {}",
            decoded.duration()
        );
    }

    #[test]
    fn band_filtered_span_suppresses_out_of_band_energy() {
        // A 300 Hz + 3000 Hz two-tone; a band around 300 Hz should keep the low
        // tone and cut the high one, so the filtered span carries far less energy
        // than the raw span.
        let sr = 16_000_u32;
        let seconds = 0.5;
        let frames = (f64::from(sr) * seconds) as usize;
        let planar: Vec<f32> = (0..frames)
            .map(|i| {
                let t = i as f64 / f64::from(sr);
                (0.4 * (TAU * 300.0 * t).sin() + 0.4 * (TAU * 3000.0 * t).sin()) as f32
            })
            .collect();
        let mut engine = Engine::new();
        let audio = engine
            .store
            .insert(Audio::new(vec![planar], f64::from(sr)).unwrap());

        let raw: f64 = engine
            .band_filtered_span(audio, 0.0, seconds, 0.0, f64::from(sr) / 2.0)
            .unwrap()
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum();
        let low_band: f64 = engine
            .band_filtered_span(audio, 0.0, seconds, 150.0, 450.0)
            .unwrap()
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum();
        // Cutting the 3 kHz tone removes about half the energy.
        assert!(low_band < 0.7 * raw, "low_band {low_band} vs raw {raw}");
    }

    #[test]
    fn notch_filter_removes_the_stopped_band() {
        // The same 300 + 3000 Hz two-tone; a notch around 3 kHz keeps the low
        // tone and cuts the high one, so the result loses about half its energy.
        let sr = 16_000_u32;
        let seconds = 0.5;
        let frames = (f64::from(sr) * seconds) as usize;
        let planar: Vec<f32> = (0..frames)
            .map(|i| {
                let t = i as f64 / f64::from(sr);
                (0.4 * (TAU * 300.0 * t).sin() + 0.4 * (TAU * 3000.0 * t).sin()) as f32
            })
            .collect();
        let mut engine = Engine::new();
        let audio = engine
            .store
            .insert(Audio::new(vec![planar], f64::from(sr)).unwrap());

        let raw: f64 = engine
            .span_samples(audio, 0.0, seconds)
            .unwrap()
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum();
        let wav = engine
            .export_notch_filtered_span_wav(audio, 0.0, seconds, 2700.0, 3300.0, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();
        let notched: f64 = decoded
            .channel(0)
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum();
        assert!(notched < 0.7 * raw, "notched {notched} vs raw {raw}");
    }

    #[test]
    fn preemphasis_lifts_the_high_tone_over_the_low_tone() {
        // A 200 Hz + 3000 Hz two-tone; pre-emphasis is a +6 dB/octave high-pass,
        // so the high tone's amplitude grows relative to the low one.
        let sr = 16_000.0;
        let seconds = 0.5;
        let frames = (sr * seconds) as usize;
        let low = 200.0;
        let high = 3000.0;
        let raw: Vec<f32> = (0..frames)
            .map(|i| {
                let t = i as f64 / sr;
                (0.4 * (TAU * low * t).sin() + 0.4 * (TAU * high * t).sin()) as f32
            })
            .collect();
        let mut engine = Engine::new();
        let audio = engine
            .store
            .insert(Audio::new(vec![raw.clone()], sr).unwrap());

        let wav = engine
            .apply_preemphasis_wav(audio, 0.0, seconds, 50.0, BitDepth::Float32)
            .unwrap();
        let emphasized = Audio::from_wav_bytes(&wav).unwrap();

        let mag = |samples: &[f32], freq: f64| {
            let (mut re, mut im) = (0.0_f64, 0.0_f64);
            for (i, &s) in samples.iter().enumerate() {
                let w = TAU * freq * i as f64 / sr;
                re += f64::from(s) * w.cos();
                im += f64::from(s) * w.sin();
            }
            re.hypot(im)
        };
        let raw_ratio = mag(&raw, high) / mag(&raw, low);
        let post_ratio = mag(emphasized.channel(0), high) / mag(emphasized.channel(0), low);
        assert!(
            post_ratio > raw_ratio * 2.0,
            "high/low ratio {post_ratio} should rise well above raw {raw_ratio}"
        );
    }

    #[test]
    fn subtract_mean_centres_a_biased_span() {
        // A 200 Hz sine lifted by a +0.3 DC bias: the output mean drops to ~0.
        let sr = 16_000.0;
        let frames = (0.5 * sr) as usize;
        let samples: Vec<f32> = (0..frames)
            .map(|i| (0.3 + 0.4 * (TAU * 200.0 * i as f64 / sr).sin()) as f32)
            .collect();
        let mut engine = Engine::new();
        let id = engine.store.insert(Audio::new(vec![samples], sr).unwrap());

        let wav = engine
            .subtract_mean_span_wav(id, 0.0, 0.5, BitDepth::Float32)
            .unwrap();
        let decoded = Audio::from_wav_bytes(&wav).unwrap();
        let mean = decoded
            .channel(0)
            .iter()
            .map(|&s| f64::from(s))
            .sum::<f64>()
            / decoded.channel(0).len() as f64;
        assert!(mean.abs() < 1e-3, "residual DC {mean}");
    }

    #[test]
    fn export_channel_wav_splits_a_stereo_take_into_its_channels() {
        let sr = 8_000.0;
        let left: Vec<f32> = (0..800).map(|i| 0.5 * (i as f32 * 0.01).sin()).collect();
        let right: Vec<f32> = (0..800).map(|i| -0.3 * (i as f32 * 0.02).cos()).collect();
        let mut engine = Engine::new();
        let audio = engine
            .store
            .insert(Audio::new(vec![left.clone(), right.clone()], sr).unwrap());

        let ch0 = Audio::from_wav_bytes(
            &engine
                .export_channel_wav(audio, 0, BitDepth::Float32)
                .unwrap(),
        )
        .unwrap();
        let ch1 = Audio::from_wav_bytes(
            &engine
                .export_channel_wav(audio, 1, BitDepth::Float32)
                .unwrap(),
        )
        .unwrap();

        assert_eq!(ch0.channel_count(), 1);
        assert_eq!(ch1.channel_count(), 1);
        assert_eq!(ch0.channel(0), left.as_slice());
        assert_eq!(ch1.channel(0), right.as_slice());
        assert!(matches!(
            engine.export_channel_wav(audio, 2, BitDepth::Float32),
            Err(EngineError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn export_mono_wav_averages_the_channels() {
        let sr = 8_000.0;
        let left: Vec<f32> = (0..400).map(|i| 0.4 * (i as f32 * 0.03).sin()).collect();
        let right: Vec<f32> = (0..400).map(|i| -0.2 * (i as f32 * 0.05).cos()).collect();
        let mut engine = Engine::new();
        let audio = engine
            .store
            .insert(Audio::new(vec![left.clone(), right.clone()], sr).unwrap());

        let mono =
            Audio::from_wav_bytes(&engine.export_mono_wav(audio, BitDepth::Float32).unwrap())
                .unwrap();
        assert_eq!(mono.channel_count(), 1);
        assert_eq!(mono.channel(0).len(), left.len());
        for (i, &m) in mono.channel(0).iter().enumerate() {
            let expected = 0.5 * (left[i] + right[i]);
            assert!((m - expected).abs() < 1e-6, "sample {i}: {m} vs {expected}");
        }
    }

    #[test]
    fn voicing_intervals_marks_the_voiced_middle() {
        // Silence, a 150 Hz sine, then silence: the middle reads voiced, the
        // edges unvoiced.
        let sr = 16_000.0;
        let quiet = (0.2 * sr) as usize;
        let tone = (0.4 * sr) as usize;
        let mut samples = vec![0.0_f32; quiet];
        for i in 0..tone {
            samples.push(0.5 * (TAU * 150.0 * i as f64 / sr).sin() as f32);
        }
        samples.extend(std::iter::repeat_n(0.0_f32, quiet));
        let mut engine = Engine::new();
        let id = engine.store.insert(Audio::new(vec![samples], sr).unwrap());

        let segs = engine
            .voicing_intervals(id, 75.0, 600.0, 0.02, 0.02)
            .unwrap();
        let voiced_at = |t: f64| {
            segs.iter()
                .find(|&&(a, b, _)| t >= a && t < b)
                .map(|&(_, _, v)| v)
        };
        assert_eq!(voiced_at(0.4), Some(true), "the sine's midpoint is voiced");
        assert!(
            segs.iter().any(|&(_, _, voiced)| !voiced),
            "the silent edges leave at least one unvoiced run"
        );
        // The runs tile the whole signal without gaps.
        for pair in segs.windows(2) {
            assert!((pair[1].0 - pair[0].1).abs() < 1e-9, "runs are contiguous");
        }
    }

    #[test]
    fn attach_reports_incremental_changes_and_reads_back() {
        let (mut engine, audio, doc) = base_engine();
        let (tier, intervals) = {
            let annotation = engine.annotation(doc).unwrap();
            interval_tiers(annotation).remove(0)
        };
        assert_eq!(intervals.len(), 1);
        assert_eq!(engine.annotation_audio(doc).unwrap(), audio);

        let at = 1.0;
        let boundary = match engine
            .apply(Command::InsertBoundary {
                annotation: doc,
                tier,
                at,
            })
            .unwrap()
        {
            Applied::BoundaryInserted { boundary, .. } => boundary,
            other => panic!("expected BoundaryInserted, got {other:?}"),
        };
        let intervals = interval_tiers(engine.annotation(doc).unwrap()).remove(0).1;
        assert_eq!(intervals.len(), 2);

        let applied = engine
            .apply(Command::SetLabel {
                annotation: doc,
                target: LabelTarget::Interval {
                    tier,
                    interval: intervals[0].id,
                },
                text: "aː".to_string(),
            })
            .unwrap();
        assert!(matches!(applied, Applied::LabelSet { .. }));
        let _ = boundary;
    }

    #[test]
    fn undo_redo_restores_state_hash_against_untouched_engine() {
        let (mut engine, _audio, doc) = base_engine();
        let (mut untouched, _, _) = base_engine();
        let baseline = engine.state_hash();
        assert_eq!(baseline, untouched.state_hash());
        // Both engines are independent constructions; the "untouched" one is
        // the never-mutated reference the invariant compares against.
        assert!(untouched.undo().unwrap().is_some());
        assert!(untouched.redo().unwrap().is_some());
        assert_eq!(untouched.state_hash(), baseline);

        let (tier, intervals) = {
            let annotation = engine.annotation(doc).unwrap();
            interval_tiers(annotation).remove(0)
        };
        engine
            .apply(Command::InsertBoundary {
                annotation: doc,
                tier,
                at: 0.5,
            })
            .unwrap();
        engine
            .apply(Command::InsertBoundary {
                annotation: doc,
                tier,
                at: 1.5,
            })
            .unwrap();
        engine
            .apply(Command::SetLabel {
                annotation: doc,
                target: LabelTarget::Interval {
                    tier,
                    interval: intervals[0].id,
                },
                text: "x".to_string(),
            })
            .unwrap();
        let final_hash = engine.state_hash();
        assert_ne!(final_hash, baseline);

        for _ in 0..3 {
            assert!(engine.undo().unwrap().is_some());
        }
        assert_eq!(engine.state_hash(), baseline);
        assert_eq!(engine.state_hash(), untouched.state_hash());

        for _ in 0..3 {
            assert!(engine.redo().unwrap().is_some());
        }
        assert_eq!(engine.state_hash(), final_hash);
    }

    #[test]
    fn redo_stack_clears_on_new_command() {
        let (mut engine, _audio, doc) = base_engine();
        let tier = interval_tiers(engine.annotation(doc).unwrap()).remove(0).0;
        engine
            .apply(Command::InsertBoundary {
                annotation: doc,
                tier,
                at: 1.0,
            })
            .unwrap();
        engine.undo().unwrap();
        assert_eq!(engine.redo_depth(), 1);
        // A fresh command discards the pending redo.
        engine
            .apply(Command::AddIntervalTier {
                annotation: doc,
                name: "words".to_string(),
                relation: TierRelation::Independent,
            })
            .unwrap();
        assert_eq!(engine.redo_depth(), 0);
        assert!(engine.redo().unwrap().is_none());
    }

    /// Reproduces the delete/undo-toast race a captured head id has to guard
    /// against: a toast captures the id of its own delete, another journaled
    /// operation lands inside the toast's window, and the id no longer
    /// matches the journal head — telling the caller a blind `undo()` would
    /// hit the wrong entry.
    #[test]
    fn journal_head_id_detects_an_intervening_command() {
        let (mut engine, audio, doc) = base_engine();
        let after_setup = engine.journal_head_id();
        assert!(
            after_setup.is_some(),
            "base_engine leaves an import+attach on the journal"
        );

        // The "delete": detach the audio, and capture the id of that entry.
        engine.apply(Command::DetachAudio { id: audio }).unwrap();
        let delete_id = engine.journal_head_id();
        assert!(delete_id.is_some());
        assert_ne!(delete_id, after_setup);

        // Nothing else has happened yet: the toast's captured id still names
        // the journal head, so `undo()` would target the delete.
        assert_eq!(engine.journal_head_id(), delete_id);

        // Another journaled operation lands inside the toast's window.
        let intervening = engine
            .import_audio_bytes(&sine_wav_bytes(8_000, 0.1, 500.0))
            .unwrap();

        // The captured id no longer names the head: a blind `undo()` would
        // now undo the *import*, not the delete the toast promised to undo.
        assert_ne!(engine.journal_head_id(), delete_id);
        let undone = engine.undo().unwrap().unwrap();
        assert!(matches!(undone, Applied::AudioRemoved { audio: a } if a == intervening));
        assert_ne!(
            undone,
            Applied::AudioDetached {
                audio,
                annotations: vec![doc],
            },
            "a mismatched head id must never be trusted to undo the delete"
        );
    }

    #[test]
    fn tier_lifecycle_undo_restores_ids() {
        let (mut engine, _audio, doc) = base_engine();
        let before = engine.state_hash();
        let tier = match engine
            .apply(Command::AddPointTier {
                annotation: doc,
                name: "tones".to_string(),
                points: vec![(0.5, "H".to_string()), (1.5, "L".to_string())],
                relation: TierRelation::Independent,
            })
            .unwrap()
        {
            Applied::TierAdded { tier, .. } => tier,
            other => panic!("expected TierAdded, got {other:?}"),
        };
        let added = engine.state_hash();
        engine
            .apply(Command::RemoveTier {
                annotation: doc,
                tier,
            })
            .unwrap();
        assert_eq!(engine.state_hash(), before);
        engine.undo().unwrap(); // undo the removal
        assert_eq!(engine.state_hash(), added);
        engine.undo().unwrap(); // undo the addition
        assert_eq!(engine.state_hash(), before);
    }

    #[test]
    fn duplicate_tier_copies_and_undo_redo_restores_it() {
        let (mut engine, _audio, doc) = base_engine();
        let source = match engine
            .apply(Command::AddPointTier {
                annotation: doc,
                name: "tones".to_string(),
                points: vec![(0.5, "H".to_string()), (1.5, "L".to_string())],
                relation: TierRelation::Independent,
            })
            .unwrap()
        {
            Applied::TierAdded { tier, .. } => tier,
            other => panic!("expected TierAdded, got {other:?}"),
        };
        let before_dup = engine.state_hash();
        let tiers_before = engine.annotation(doc).unwrap().tiers().len();

        let copy = match engine
            .apply(Command::DuplicateTier {
                annotation: doc,
                tier: source,
            })
            .unwrap()
        {
            Applied::TierAdded { tier, .. } => tier,
            other => panic!("expected TierAdded, got {other:?}"),
        };
        assert_ne!(copy, source);
        let after_dup = engine.state_hash();
        assert_eq!(
            engine.annotation(doc).unwrap().tiers().len(),
            tiers_before + 1
        );

        // Undo drops the copy; redo restores it byte-for-byte, same id included.
        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), before_dup);
        assert_eq!(engine.annotation(doc).unwrap().tiers().len(), tiers_before);
        engine.redo().unwrap();
        assert_eq!(engine.state_hash(), after_dup);
        assert!(engine.annotation(doc).unwrap().tier(copy).is_some());
    }

    #[test]
    fn search_labels_spans_all_documents() {
        let (mut engine, audio, first) = base_engine();
        let tier = interval_tiers(engine.annotation(first).unwrap())
            .remove(0)
            .0;
        let interval = interval_tiers(engine.annotation(first).unwrap())
            .remove(0)
            .1[0]
            .id;
        engine
            .apply(Command::SetLabel {
                annotation: first,
                target: LabelTarget::Interval { tier, interval },
                text: "vowel".to_string(),
            })
            .unwrap();

        let second_annotation = {
            let mut a = annotation_with_tier(2.0);
            let (tier_id, intervals) = interval_tiers(&a).remove(0);
            a.set_label(
                LabelTarget::Interval {
                    tier: tier_id,
                    interval: intervals[0].id,
                },
                "vowel space",
            )
            .unwrap();
            a
        };
        let second = match engine
            .apply(Command::AttachAnnotation {
                audio,
                annotation: second_annotation,
            })
            .unwrap()
        {
            Applied::AnnotationAttached { annotation, .. } => annotation,
            other => panic!("expected AnnotationAttached, got {other:?}"),
        };

        let hits = engine.search_labels(&LabelQuery::substring("vowel"));
        assert_eq!(hits.len(), 2);
        let docs: Vec<AnnotationId> = hits.iter().map(|hit| hit.annotation).collect();
        assert!(docs.contains(&first));
        assert!(docs.contains(&second));
    }

    /// Roadmap phase-3 gate: a random 50-command mix undone in full returns to
    /// the initial state hash, and redone in full returns to the final one.
    #[test]
    fn random_fifty_command_undo_stack_is_hash_stable() {
        let (mut engine, audio, doc) = base_engine();
        let initial = engine.state_hash();

        let mut rng = Rng::new(0x9E37_79B9_7F4A_7C15);
        let mut name_counter = 0_u32;
        let mut applied_hashes = Vec::new();
        let mut guard = 0;

        while applied_hashes.len() < 50 {
            guard += 1;
            assert!(guard < 20_000, "generator failed to reach 50 commands");
            let Some(cmd) = gen_command(&engine, audio, doc, &mut rng, &mut name_counter) else {
                continue;
            };
            if engine.apply(cmd).is_ok() {
                applied_hashes.push(engine.state_hash());
            }
        }

        let final_hash = engine.state_hash();
        assert_eq!(engine.undo_depth(), 52); // 50 + import + attach

        // Undo the 50 generated commands; each step matches the hash recorded
        // just before it was applied.
        for expected in applied_hashes.iter().rev().skip(1) {
            engine.undo().unwrap();
            assert_eq!(engine.state_hash(), *expected);
        }
        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), initial);

        // Redo the 50 commands; each step reproduces its recorded hash.
        for expected in &applied_hashes {
            engine.redo().unwrap();
            assert_eq!(engine.state_hash(), *expected);
        }
        assert_eq!(engine.state_hash(), final_hash);
    }

    /// Chooses a state-valid command from the current engine, or `None` when
    /// the roll cannot be satisfied (the caller retries). Content targets are
    /// read from live state so the generated command almost always applies.
    fn gen_command(
        engine: &Engine,
        audio: AudioId,
        doc: AnnotationId,
        rng: &mut Rng,
        name_counter: &mut u32,
    ) -> Option<Command> {
        let annotation = engine.annotation(doc).ok()?;
        let tiers = interval_tiers(annotation);
        let roll = rng.below(100);

        match roll {
            39 => {
                // Rename the audio buffer; always applies against a live id.
                Some(Command::RenameAudio {
                    id: audio,
                    name: format!("r{}", rng.below(1000)),
                })
            }
            0..=39 => {
                // Set a label on a random interval.
                if tiers.is_empty() {
                    return None;
                }
                let (tier, intervals) = &tiers[rng.below(tiers.len())];
                let interval = &intervals[rng.below(intervals.len())];
                let text = format!("l{}", rng.below(1000));
                Some(Command::SetLabel {
                    annotation: doc,
                    target: LabelTarget::Interval {
                        tier: *tier,
                        interval: interval.id,
                    },
                    text,
                })
            }
            40..=57 => {
                // Split a wide interval at an interior fraction.
                let wide: Vec<&(TierId, Vec<Interval>)> = tiers
                    .iter()
                    .filter(|(_, ivs)| ivs.iter().any(|iv| iv.xmax - iv.xmin > 1.0e-3))
                    .collect();
                if wide.is_empty() {
                    return None;
                }
                let (tier, intervals) = wide[rng.below(wide.len())];
                let candidates: Vec<&Interval> = intervals
                    .iter()
                    .filter(|iv| iv.xmax - iv.xmin > 1.0e-3)
                    .collect();
                let interval = candidates[rng.below(candidates.len())];
                let frac = 0.2 + 0.6 * rng.frac();
                let at = interval.xmin + frac * (interval.xmax - interval.xmin);
                if at.to_bits() == interval.xmin.to_bits()
                    || at.to_bits() == interval.xmax.to_bits()
                {
                    return None;
                }
                Some(Command::InsertBoundary {
                    annotation: doc,
                    tier: *tier,
                    at,
                })
            }
            58..=69 => {
                // Move an interior boundary within its neighbours.
                let (_tier, intervals) = pick_multi_interval(&tiers, rng)?;
                let i = rng.below(intervals.len() - 1);
                let lo = intervals[i].xmin;
                let hi = intervals[i + 1].xmax;
                let frac = 0.2 + 0.6 * rng.frac();
                let at = lo + frac * (hi - lo);
                if at.to_bits() == intervals[i].xmax.to_bits() {
                    return None;
                }
                Some(Command::MoveBoundary {
                    annotation: doc,
                    boundary: intervals[i].end_boundary,
                    to: at,
                    mode: AlignMode::Linked,
                })
            }
            70..=77 => {
                // Remove an interior boundary.
                let (_tier, intervals) = pick_multi_interval(&tiers, rng)?;
                let i = rng.below(intervals.len() - 1);
                Some(Command::RemoveBoundary {
                    annotation: doc,
                    boundary: intervals[i].end_boundary,
                })
            }
            78..=82 => {
                *name_counter += 1;
                Some(Command::AddIntervalTier {
                    annotation: doc,
                    name: format!("tier{name_counter}"),
                    relation: TierRelation::Independent,
                })
            }
            83..=85 => {
                // Remove a tier other than the primary one, when one exists.
                if annotation.tiers().len() < 2 {
                    return None;
                }
                let slot = &annotation.tiers()[1 + rng.below(annotation.tiers().len() - 1)];
                Some(Command::RemoveTier {
                    annotation: doc,
                    tier: slot.id,
                })
            }
            86..=88 => {
                *name_counter += 1;
                Some(Command::AddPointTier {
                    annotation: doc,
                    name: format!("pts{name_counter}"),
                    points: vec![(0.4, "a".to_string()), (1.2, "b".to_string())],
                    relation: TierRelation::Independent,
                })
            }
            89..=91 => {
                // Insert a point in a free slot of a random point tier.
                let point_tiers = point_tiers(annotation);
                if point_tiers.is_empty() {
                    return None;
                }
                let (tier, points) = &point_tiers[rng.below(point_tiers.len())];
                let mut fence = vec![annotation.xmin()];
                fence.extend(points.iter().map(|point| point.time));
                fence.push(annotation.xmax());
                let mids: Vec<f64> = fence
                    .windows(2)
                    .filter_map(|pair| {
                        let mid = (pair[0] + pair[1]) / 2.0;
                        (mid.to_bits() != pair[0].to_bits() && mid.to_bits() != pair[1].to_bits())
                            .then_some(mid)
                    })
                    .collect();
                if mids.is_empty() {
                    return None;
                }
                Some(Command::InsertPoint {
                    annotation: doc,
                    tier: *tier,
                    time: mids[rng.below(mids.len())],
                    label: format!("p{}", rng.below(1000)),
                })
            }
            92..=93 => {
                // Move a point within its immediate neighbours.
                let movable = movable_points(annotation);
                if movable.is_empty() {
                    return None;
                }
                let (point, lower, upper) = movable[rng.below(movable.len())];
                let frac = 0.2 + 0.6 * rng.frac();
                let to = lower + frac * (upper - lower);
                if to.to_bits() == lower.to_bits() || to.to_bits() == upper.to_bits() {
                    return None;
                }
                Some(Command::MovePoint {
                    annotation: doc,
                    point,
                    to,
                })
            }
            94..=95 => {
                // Remove a random point.
                let ids: Vec<PointId> = point_tiers(annotation)
                    .into_iter()
                    .flat_map(|(_tier, points)| points.into_iter().map(|point| point.id))
                    .collect();
                if ids.is_empty() {
                    return None;
                }
                Some(Command::RemovePoint {
                    annotation: doc,
                    point: ids[rng.below(ids.len())],
                })
            }
            96..=97 => {
                // Move a tier to a random index.
                let count = annotation.tiers().len();
                if count < 2 {
                    return None;
                }
                let tier = annotation.tiers()[rng.below(count)].id;
                Some(Command::ReorderTier {
                    annotation: doc,
                    tier,
                    to_index: rng.below(count),
                })
            }
            98 => Some(Command::AttachAnnotation {
                audio,
                annotation: annotation_with_tier(2.0),
            }),
            _ => {
                let bytes = sine_wav_bytes(8_000, 0.2, 300.0);
                Some(Command::ImportAudio {
                    bytes,
                    name: format!("clip{name_counter}"),
                })
            }
        }
    }

    fn pick_multi_interval<'a>(
        tiers: &'a [(TierId, Vec<Interval>)],
        rng: &mut Rng,
    ) -> Option<(TierId, &'a Vec<Interval>)> {
        let multi: Vec<&(TierId, Vec<Interval>)> =
            tiers.iter().filter(|(_, ivs)| ivs.len() >= 2).collect();
        if multi.is_empty() {
            return None;
        }
        let (tier, intervals) = multi[rng.below(multi.len())];
        Some((*tier, intervals))
    }

    fn point_tiers(annotation: &Annotation) -> Vec<(TierId, Vec<Point>)> {
        annotation
            .tiers()
            .iter()
            .filter_map(|slot| match &slot.tier {
                Tier::Point(tier) => Some((slot.id, tier.points.clone())),
                Tier::Interval(_) => None,
            })
            .collect()
    }

    fn movable_points(annotation: &Annotation) -> Vec<(PointId, f64, f64)> {
        let mut out = Vec::new();
        for (_tier, points) in point_tiers(annotation) {
            for (index, point) in points.iter().enumerate() {
                let lower = if index > 0 {
                    points[index - 1].time
                } else {
                    annotation.xmin()
                };
                let upper = if index + 1 < points.len() {
                    points[index + 1].time
                } else {
                    annotation.xmax()
                };
                if upper > lower {
                    out.push((point.id, lower, upper));
                }
            }
        }
        out
    }

    /// A shared boundary on an aligned tier pair moves both tiers atomically as
    /// one journal entry, and a single undo restores both.
    #[test]
    fn aligned_tier_pair_moves_and_undoes_as_one_entry() {
        let (mut engine, _audio, doc) = base_engine();
        let primary = interval_tiers(engine.annotation(doc).unwrap()).remove(0).0;

        let aligned = match engine
            .apply(Command::AddIntervalTier {
                annotation: doc,
                name: "words".to_string(),
                relation: TierRelation::AlignedBoundaries { with: primary },
            })
            .unwrap()
        {
            Applied::TierAdded { tier, .. } => tier,
            other => panic!("expected TierAdded, got {other:?}"),
        };

        // Undo the tier add cleanly, then redo it back.
        let with_pair = engine.state_hash();
        engine.undo().unwrap();
        assert!(engine.annotation(doc).unwrap().tier(aligned).is_none());
        engine.redo().unwrap();
        assert_eq!(engine.state_hash(), with_pair);

        // Insert a boundary on the primary tier; it propagates to the aligned
        // peer, and the whole propagation is one undoable entry.
        let depth_before = engine.undo_depth();
        let boundary = match engine
            .apply(Command::InsertBoundary {
                annotation: doc,
                tier: primary,
                at: 1.0,
            })
            .unwrap()
        {
            Applied::BoundaryInserted { boundary, .. } => boundary,
            other => panic!("expected BoundaryInserted, got {other:?}"),
        };
        assert_eq!(engine.undo_depth(), depth_before + 1);
        for (_id, intervals) in interval_tiers(engine.annotation(doc).unwrap()) {
            assert_eq!(intervals.len(), 2, "both tiers gained the boundary");
        }

        // Move the shared boundary linked; both tiers move in one entry.
        let before_move = engine.state_hash();
        let moves = match engine
            .apply(Command::MoveBoundary {
                annotation: doc,
                boundary,
                to: 1.3,
                mode: AlignMode::Linked,
            })
            .unwrap()
        {
            Applied::BoundaryMoved { moves, .. } => moves,
            other => panic!("expected BoundaryMoved, got {other:?}"),
        };
        assert_eq!(moves.len(), 2, "both aligned tiers moved");
        for (_id, intervals) in interval_tiers(engine.annotation(doc).unwrap()) {
            assert_eq!(intervals[0].xmax.to_bits(), 1.3_f64.to_bits());
        }

        // A single undo restores both tiers to the shared boundary at 1.0.
        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), before_move);
        for (_id, intervals) in interval_tiers(engine.annotation(doc).unwrap()) {
            assert_eq!(intervals[0].xmax.to_bits(), 1.0_f64.to_bits());
        }
    }

    /// A point round-trip through the journal: insert, move, remove, then undo
    /// each back to the initial hash and redo to the final one.
    #[test]
    fn point_commands_undo_redo_are_hash_stable() {
        let (mut engine, _audio, doc) = base_engine();
        let tier = match engine
            .apply(Command::AddPointTier {
                annotation: doc,
                name: "tones".to_string(),
                points: Vec::new(),
                relation: TierRelation::Independent,
            })
            .unwrap()
        {
            Applied::TierAdded { tier, .. } => tier,
            other => panic!("expected TierAdded, got {other:?}"),
        };
        let empty = engine.state_hash();

        let point = match engine
            .apply(Command::InsertPoint {
                annotation: doc,
                tier,
                time: 1.0,
                label: "H".to_string(),
            })
            .unwrap()
        {
            Applied::PointInserted { point, .. } => point,
            other => panic!("expected PointInserted, got {other:?}"),
        };
        let inserted = engine.state_hash();

        engine
            .apply(Command::MovePoint {
                annotation: doc,
                point,
                to: 1.4,
            })
            .unwrap();
        let moved = engine.state_hash();
        assert_ne!(moved, inserted);

        engine
            .apply(Command::RemovePoint {
                annotation: doc,
                point,
            })
            .unwrap();
        assert_eq!(
            engine.state_hash(),
            empty,
            "removal returns to the empty tier"
        );

        // Undo removal, move, insertion.
        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), moved);
        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), inserted);
        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), empty);

        // Redo insertion, move, removal back to the empty tier.
        engine.redo().unwrap();
        assert_eq!(engine.state_hash(), inserted);
        engine.redo().unwrap();
        assert_eq!(engine.state_hash(), moved);
        engine.redo().unwrap();
        assert_eq!(engine.state_hash(), empty);
    }

    /// Reordering a tier is invertible and hash-stable through the journal.
    #[test]
    fn reorder_tier_undo_restores_order() {
        let (mut engine, _audio, doc) = base_engine();
        engine
            .apply(Command::AddIntervalTier {
                annotation: doc,
                name: "words".to_string(),
                relation: TierRelation::Independent,
            })
            .unwrap();
        let order_before: Vec<TierId> = engine
            .annotation(doc)
            .unwrap()
            .tiers()
            .iter()
            .map(|slot| slot.id)
            .collect();
        let hash_before = engine.state_hash();

        let last = *order_before.last().unwrap();
        engine
            .apply(Command::ReorderTier {
                annotation: doc,
                tier: last,
                to_index: 0,
            })
            .unwrap();
        let reordered: Vec<TierId> = engine
            .annotation(doc)
            .unwrap()
            .tiers()
            .iter()
            .map(|slot| slot.id)
            .collect();
        assert_eq!(reordered[0], last);
        assert_ne!(engine.state_hash(), hash_before);

        engine.undo().unwrap();
        assert_eq!(engine.state_hash(), hash_before);
        let restored: Vec<TierId> = engine
            .annotation(doc)
            .unwrap()
            .tiers()
            .iter()
            .map(|slot| slot.id)
            .collect();
        assert_eq!(restored, order_before);
    }

    // --- Streamed path: equal to the eager path on the same bytes -----------

    fn eager_and_streamed(bytes: &[u8]) -> (Engine, AudioId, Engine, AudioId) {
        let mut eager = Engine::new();
        let eager_id = eager.import_audio_bytes(bytes).unwrap();
        let mut streamed = Engine::new();
        let streamed_id = streamed
            .open_streaming_wav(BytesReader::new(bytes.to_vec()), Some("take".to_string()))
            .unwrap();
        (eager, eager_id, streamed, streamed_id)
    }

    #[test]
    fn streamed_audio_info_matches_the_eager_decode() {
        let (eager, eid, streamed, sid) = eager_and_streamed(FIXTURE_WAV);
        let a = eager.audio_info(eid).unwrap();
        let b = streamed.audio_info(sid).unwrap();
        assert_eq!(a.duration.to_bits(), b.duration.to_bits());
        assert_eq!(a.sample_rate.to_bits(), b.sample_rate.to_bits());
        assert_eq!(a.channels, b.channels);
    }

    #[test]
    fn streamed_waveform_slices_are_bit_identical_to_the_eager_pyramid() {
        let (eager, eid, streamed, sid) = eager_and_streamed(FIXTURE_WAV);
        let duration = eager.audio_info(eid).unwrap().duration;
        for &px in &[1u32, 13, 128, 777, 2000] {
            for &(a, b) in &[
                (0.0, duration),
                (0.0, duration * 0.5),
                (duration * 0.25, duration * 0.85),
                (duration * 0.499, duration * 0.501),
                (0.0, 0.003),
            ] {
                let expected = eager.waveform_slice(eid, a, b, px).unwrap();
                let actual = streamed.waveform_slice(sid, a, b, px).unwrap();
                assert_eq!(actual.len(), expected.len(), "px {px} span {a}..{b}");
                for (i, (x, y)) in actual.iter().zip(&expected).enumerate() {
                    assert_eq!(
                        x.min.to_bits(),
                        y.min.to_bits(),
                        "px {px} {a}..{b} bucket {i}"
                    );
                    assert_eq!(
                        x.max.to_bits(),
                        y.max.to_bits(),
                        "px {px} {a}..{b} bucket {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn streamed_spectrogram_tile_db_is_bit_identical_to_the_eager_tile() {
        let (eager, eid, streamed, sid) = eager_and_streamed(FIXTURE_WAV);
        let duration = eager.audio_info(eid).unwrap().duration;
        let req = TileRequest {
            t0: duration * 0.2,
            t1: duration * 0.7,
            f0: 0.0,
            f1: 5000.0,
            width_px: 220,
            height_px: 90,
            params: SpectrogramParams::default(),
        };
        let expected = eager.spectrogram_tile_db(eid, &req, false).unwrap();
        let actual = streamed.spectrogram_tile_db(sid, &req, false).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (i, (x, y)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(x.to_bits(), y.to_bits(), "db cell {i}");
        }
    }

    #[test]
    fn streamed_pitch_track_span_is_bit_identical_to_the_eager_span() {
        let (eager, eid, streamed, sid) = eager_and_streamed(FIXTURE_WAV);
        let duration = eager.audio_info(eid).unwrap().duration;
        let params = PitchParams::default();
        let (a, at) = eager
            .pitch_track_span(eid, &params, duration * 0.3, duration * 0.6)
            .unwrap();
        let (b, bt) = streamed
            .pitch_track_span(sid, &params, duration * 0.3, duration * 0.6)
            .unwrap();
        assert_eq!(at.to_bits(), bt.to_bits());
        assert_eq!(a.frames().len(), b.frames().len());
        for (i, (x, y)) in a.frames().iter().zip(b.frames()).enumerate() {
            assert_eq!(x.time.to_bits(), y.time.to_bits(), "frame {i} time");
            match (x.f0, y.f0) {
                (Some(fx), Some(fy)) => assert_eq!(fx.to_bits(), fy.to_bits(), "frame {i} f0"),
                (None, None) => {}
                _ => panic!("frame {i} voicing differs"),
            }
        }
    }

    #[test]
    fn streamed_whole_signal_pitch_track_matches_eager() {
        let (eager, eid, streamed, sid) = eager_and_streamed(VOWEL_WAV);
        let params = PitchParams::default();
        let a = eager.pitch_track(eid, &params).unwrap();
        let b = streamed.pitch_track(sid, &params).unwrap();
        assert_eq!(a.frames().len(), b.frames().len());
        for (i, (x, y)) in a.frames().iter().zip(b.frames()).enumerate() {
            assert_eq!(x.time.to_bits(), y.time.to_bits(), "frame {i}");
            assert_eq!(
                x.f0.map(f64::to_bits),
                y.f0.map(f64::to_bits),
                "frame {i} f0"
            );
        }
    }
}
