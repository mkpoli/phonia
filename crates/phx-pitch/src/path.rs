//! Viterbi path finder over per-frame candidates (Boersma 1993 §1.4).

use crate::candidates::FrameCandidates;
use crate::params::PitchParams;
use crate::types::{PitchCandidate, PitchFrame, PitchTrack};

/// Chooses one candidate per frame so that the sum of candidate strengths less
/// the sum of transition costs (eq. 26–27) is largest.
///
/// A voiced candidate's strength on the path is its correlation less the
/// octave cost measured against the ceiling (the anchor shifts every voiced
/// candidate by one constant relative to the unvoiced one, so it sets the
/// voicing balance); a candidate at or above the ceiling counts as unvoiced. The two transition costs are defined for a 10 ms step and scale
/// with `0.01 / step`, so the cost of a jump per second of signal is the same
/// on any grid.
pub(crate) fn viterbi_track(
    frames: Vec<FrameCandidates>,
    params: &PitchParams,
    ceiling_hz: f64,
    step: f64,
) -> PitchTrack {
    if frames.is_empty() {
        return PitchTrack::new(Vec::new());
    }
    debug_assert!(
        frames
            .iter()
            .all(|frame| frame.candidates.first().is_some_and(|c| c.frequency == 0.0)),
        "every frame lists its unvoiced candidate first"
    );
    let correction = 0.01 / step;
    let octave_jump_cost = params.octave_jump_cost * correction;
    let voiced_unvoiced_cost = params.voiced_unvoiced_cost * correction;
    let voiceless = |frequency: f64| frequency <= 0.0 || frequency >= ceiling_hz;
    // The unvoiced candidate comes first in every frame and carries eq. 23;
    // a maximum above the ceiling is scored like it.
    let path_strength = |frame: &FrameCandidates, candidate: &PitchCandidate| {
        if voiceless(candidate.frequency) {
            frame.candidates[0].strength
        } else {
            candidate.strength - params.octave_cost * (ceiling_hz / candidate.frequency).log2()
        }
    };

    let mut scores: Vec<Vec<f64>> = Vec::with_capacity(frames.len());
    let mut back: Vec<Vec<usize>> = Vec::with_capacity(frames.len());
    scores.push(
        frames[0]
            .candidates
            .iter()
            .map(|candidate| path_strength(&frames[0], candidate))
            .collect(),
    );
    back.push(vec![0; frames[0].candidates.len()]);

    for index in 1..frames.len() {
        let previous = &frames[index - 1].candidates;
        let frame = &frames[index];
        let current = &frame.candidates;
        let mut frame_scores = Vec::with_capacity(current.len());
        let mut frame_back = Vec::with_capacity(current.len());
        for candidate in current {
            let f2 = candidate.frequency;
            let mut best = f64::NEG_INFINITY;
            let mut from = 0;
            for (previous_index, previous_candidate) in previous.iter().enumerate() {
                let f1 = previous_candidate.frequency;
                let transition = match (voiceless(f1), voiceless(f2)) {
                    (true, true) => 0.0,
                    (true, false) | (false, true) => voiced_unvoiced_cost,
                    (false, false) => octave_jump_cost * (f1 / f2).log2().abs(),
                };
                let score = scores[index - 1][previous_index] - transition;
                if score > best {
                    best = score;
                    from = previous_index;
                }
            }
            frame_scores.push(best + path_strength(frame, candidate));
            frame_back.push(from);
        }
        scores.push(frame_scores);
        back.push(frame_back);
    }

    // Ties go to the earlier candidate, here and in the transition loop.
    let last = frames.len() - 1;
    let mut chosen = vec![0; frames.len()];
    chosen[last] = scores[last]
        .iter()
        .enumerate()
        .fold(
            (0, f64::NEG_INFINITY),
            |(best_index, best), (index, &score)| {
                if score > best {
                    (index, score)
                } else {
                    (best_index, best)
                }
            },
        )
        .0;
    for index in (1..frames.len()).rev() {
        chosen[index - 1] = back[index][chosen[index]];
    }

    PitchTrack::new(
        frames
            .into_iter()
            .zip(chosen)
            .map(|(frame, selected)| {
                let candidate = &frame.candidates[selected];
                let voiced = !voiceless(candidate.frequency);
                PitchFrame {
                    time: frame.time,
                    f0: voiced.then_some(candidate.frequency),
                    strength: if voiced { candidate.strength } else { 0.0 },
                    candidates: frame.candidates,
                }
            })
            .collect(),
    )
}
