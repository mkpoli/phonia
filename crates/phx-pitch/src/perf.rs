//! Ignored timing harness for the pitch hot path. Run with
//! `cargo test -p phx-pitch --release -- --ignored --nocapture perf_`.

use std::f64::consts::PI;
use std::time::Instant;

use phx_audio::Audio;
use phx_dsp::FrameGrid;

use crate::analysis::Layout;
use crate::candidates::CandidateFinder;
use crate::params::PitchParams;

const SAMPLE_RATE: f64 = 44_100.0;
const DURATION: f64 = 2.0;

fn beep_bursts(gap_dither: f64) -> Vec<f32> {
    let n = (DURATION * SAMPLE_RATE).round() as usize;
    let mut seed = 0x1234_5678_9abc_def0_u64;
    (0..n)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            // 120 ms tone burst, 80 ms gap.
            let phase = (t / 0.2).fract();
            if phase < 0.6 {
                0.5 * (2.0 * PI * 1000.0 * t).sin() as f32
            } else if gap_dither > 0.0 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let unit = (seed >> 11) as f64 / ((1_u64 << 53) as f64);
                (gap_dither * (2.0 * unit - 1.0)) as f32
            } else {
                0.0
            }
        })
        .collect()
}

fn clean_sine() -> Vec<f32> {
    let n = (DURATION * SAMPLE_RATE).round() as usize;
    (0..n)
        .map(|i| 0.5 * (2.0 * PI * 1000.0 * i as f64 / SAMPLE_RATE).sin() as f32)
        .collect()
}

fn silence() -> Vec<f32> {
    let n = (DURATION * SAMPLE_RATE).round() as usize;
    vec![0.0; n]
}

fn time_track(label: &str, signal: Vec<f32>, params: &PitchParams) {
    let audio = Audio::new(vec![signal], SAMPLE_RATE).expect("valid audio");
    let view = audio.slice_samples(0..audio.frames());
    // warm any lazy state
    let _ = crate::pitch_track(view.clone(), params);
    let start = Instant::now();
    let track = crate::pitch_track(view, params);
    let elapsed = start.elapsed();
    let voiced = track.frames().iter().filter(|f| f.f0.is_some()).count();
    println!(
        "[{label:>18}] pitch_track = {:8.2} ms  ({} frames, {voiced} voiced, {:.3} ms/frame)",
        elapsed.as_secs_f64() * 1e3,
        track.frames().len(),
        elapsed.as_secs_f64() * 1e3 / track.frames().len().max(1) as f64,
    );
}

/// Per-frame breakdown: time in the candidate stage (correlation plus
/// candidate search and refinement) against the path finder, and how many
/// voiced candidates each frame retains. The setup mirrors `pitch_track`
/// (grid, mean removal, global peak); keep the two in step.
fn breakdown(label: &str, signal: Vec<f32>, params: &PitchParams) {
    let audio = Audio::new(vec![signal], SAMPLE_RATE).expect("valid audio");
    let view = audio.slice_samples(0..audio.frames());
    let sample_rate = view.sample_rate();
    let step = params.resolved_step().unwrap();
    let layout = Layout::new(params, sample_rate).unwrap();
    let grid = FrameGrid::new(view.duration(), layout.window_seconds, step);
    let signal: Vec<f64> = view.mono_mix().iter().map(|&s| f64::from(s)).collect();
    let mean = signal.iter().sum::<f64>() / signal.len() as f64;
    let global_peak = signal
        .iter()
        .fold(0.0_f64, |peak, &s| peak.max((s - mean).abs()));
    let mut finder = CandidateFinder::new(layout, params, global_peak);

    let cand_start = Instant::now();
    let frames: Vec<_> = grid
        .centers()
        .map(|time| finder.frame(&signal, time))
        .collect();
    let cand_ms = cand_start.elapsed().as_secs_f64() * 1e3;
    let frame_count = frames.len();
    let voiced_candidates: usize = frames.iter().map(|f| f.candidates.len() - 1).sum();
    let max_candidates = frames
        .iter()
        .map(|f| f.candidates.len() - 1)
        .max()
        .unwrap_or(0);

    let path_start = Instant::now();
    let _ = crate::path::viterbi_track(frames, params, layout.ceiling_hz, step);
    let path_ms = path_start.elapsed().as_secs_f64() * 1e3;

    println!(
        "[{label:>18}] candidates(sum)={cand_ms:8.2}ms  path={path_ms:6.2}ms  | voiced candidates/frame: mean={:.1} max={max_candidates}  frames={frame_count}",
        voiced_candidates as f64 / frame_count.max(1) as f64,
    );
}

#[test]
#[ignore = "timing harness; run manually in release"]
fn perf_three_cases() {
    let defaults = PitchParams::default();
    let accurate = PitchParams {
        time_step: Some(0.02),
        floor_hz: 65.0,
        ceiling_hz: 500.0,
        very_accurate: true,
        voicing_threshold: 0.5,
        ..PitchParams::default()
    };
    println!();
    for (name, params) in [("defaults", &defaults), ("accurate 65-500", &accurate)] {
        println!("--- {name} ---");
        time_track("beep(zero-gap)", beep_bursts(0.0), params);
        time_track("beep(dither-gap)", beep_bursts(1e-4), params);
        time_track("clean-sine", clean_sine(), params);
        time_track("silence", silence(), params);
        breakdown("beep(zero-gap)", beep_bursts(0.0), params);
        breakdown("beep(dither-gap)", beep_bursts(1e-4), params);
        breakdown("clean-sine", clean_sine(), params);
        breakdown("silence", silence(), params);
    }
}
