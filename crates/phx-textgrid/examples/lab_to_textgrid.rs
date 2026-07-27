//! Converts an HTK-style forced-alignment label file into a canonical
//! TextGrid.
//!
//! Reads lines of `<end_time> <state> <label>` (the format CMU ARCTIC
//! publishes under each voice's `lab/` directory) and writes an interval
//! tier whose boundaries are exactly the file's cumulative end times: the
//! source is preserved verbatim. The whole-document `xmax` is the
//! caller-supplied audio duration, and the final interval's boundary is
//! extended to meet it, since a forced aligner's last labeled frame commonly
//! falls short of the file's true sample count by a few milliseconds of
//! untranscribed trailing silence. No boundary is invented, merged, or moved
//! beyond that single required extension.
//!
//! Two optional enrichments, both fully determined by the label file plus a
//! plain-text spec, so neither can introduce a boundary or a label the
//! alignment does not attest:
//!
//! `--map <file>` relabels phones through a notation table (e.g. ARPABET →
//! IPA), one `<from> <to>` pair per line; a line with only `<from>` maps to
//! the empty label (Praat's convention for silence). Every phone in the lab
//! file must appear in the table.
//!
//! `--words <file>` prepends a `words` interval tier derived by grouping the
//! phone sequence: each line is `<word> <phone> <phone>…`, consumed in order
//! against the lab file's non-silence phones. A word's interval spans its
//! first phone's start to its last phone's end, so every word boundary is an
//! attested phone boundary; `pau` stretches become empty word intervals
//! (consecutive `pau` segments merge into one). A mismatch anywhere between
//! the spec and the alignment aborts the conversion.
//!
//! ```text
//! cargo run -p phx-textgrid --example lab_to_textgrid -- \
//!   input.lab 3.2350625 phones output.TextGrid \
//!   [--words words.txt] [--map map.txt]
//! ```

use phx_annot::{Annotation, IntervalId, LabelTarget, Tier, TierRelation};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::ExitCode;

const SILENCE: &str = "pau";

fn parse_lab(text: &str) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "#" {
            continue;
        }
        let mut parts = line.split_whitespace();
        let end: f64 = parts
            .next()
            .expect("lab line has an end-time field")
            .parse()
            .expect("end-time field is a finite number");
        let _state = parts.next().expect("lab line has a state field");
        let label = parts
            .next()
            .expect("lab line has a label field")
            .to_string();
        out.push((end, label));
    }
    out
}

/// One `<from> <to>` pair per line; a lone `<from>` maps to the empty label.
fn parse_map(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let from = parts
            .next()
            .expect("map line has a source label")
            .to_string();
        let to = parts.next().unwrap_or("").to_string();
        out.insert(from, to);
    }
    out
}

/// One `<word> <phone> <phone>…` per line, in utterance order.
fn parse_words(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let word = parts.next().expect("words line has a word").to_string();
        let phones: Vec<String> = parts.map(str::to_string).collect();
        assert!(!phones.is_empty(), "word {word:?} lists no phones");
        out.push((word, phones));
    }
    out
}

/// Groups the aligned phone stream into word intervals per the spec,
/// validating the phone sequence exactly. `segments` are `(start, end,
/// label)`; the result covers the same domain contiguously.
fn group_words(
    segments: &[(f64, f64, String)],
    spec: &[(String, Vec<String>)],
) -> Vec<(f64, f64, String)> {
    let mut out: Vec<(f64, f64, String)> = Vec::new();
    let mut words = spec.iter();
    let mut current: Option<(&str, std::slice::Iter<String>, f64)> = None;
    for (start, end, label) in segments {
        if label == SILENCE {
            assert!(
                current.is_none(),
                "alignment has {SILENCE:?} inside a word; the words spec does not match"
            );
            match out.last_mut() {
                // Consecutive silence stretches merge into one empty interval.
                Some((_, prev_end, prev_label)) if prev_label.is_empty() && *prev_end == *start => {
                    *prev_end = *end;
                }
                _ => out.push((*start, *end, String::new())),
            }
            continue;
        }
        let (word, mut expected, word_start) = match current.take() {
            Some(state) => state,
            None => {
                let (word, phones) = words.next().unwrap_or_else(|| {
                    panic!("alignment continues past the last spec word at {label:?}")
                });
                (word.as_str(), phones.iter(), *start)
            }
        };
        let want = expected
            .next()
            .expect("a word in progress always has a pending phone");
        assert_eq!(
            want, label,
            "word {word:?} expects phone {want:?} but the alignment has {label:?}"
        );
        if expected.len() == 0 {
            out.push((word_start, *end, word.to_string()));
        } else {
            current = Some((word, expected, word_start));
        }
    }
    assert!(current.is_none(), "alignment ended mid-word");
    assert!(
        words.next().is_none(),
        "words spec lists more words than the alignment attests"
    );
    out
}

/// Adds an interval tier holding `intervals` (contiguous `(start, end,
/// label)` spans over the whole document domain).
fn add_tier(doc: &mut Annotation, name: &str, intervals: &[(f64, f64, String)]) {
    let tier = doc
        .add_interval_tier(name, TierRelation::Independent)
        .expect("add tier");
    for (_, end, _) in &intervals[..intervals.len() - 1] {
        doc.insert_boundary(tier, *end).expect("insert boundary");
    }
    let slot = doc.tier(tier).expect("tier exists");
    let Tier::Interval(interval_tier) = &slot.tier else {
        unreachable!("add_interval_tier always creates an interval tier");
    };
    let ids: Vec<IntervalId> = interval_tier.intervals.iter().map(|iv| iv.id).collect();
    assert_eq!(
        ids.len(),
        intervals.len(),
        "interval/segment count mismatch"
    );
    for (id, (_, _, label)) in ids.iter().zip(intervals.iter()) {
        doc.set_label(
            LabelTarget::Interval {
                tier,
                interval: *id,
            },
            label,
        )
        .expect("set label");
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mut positional: Vec<&String> = Vec::new();
    let mut words_path: Option<&String> = None;
    let mut map_path: Option<&String> = None;
    let mut it = args.iter().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--words" => words_path = Some(it.next().expect("--words takes a file path")),
            "--map" => map_path = Some(it.next().expect("--map takes a file path")),
            _ => positional.push(arg),
        }
    }
    let [lab_path, duration, tier_name, out_path] = positional.as_slice() else {
        eprintln!(
            "usage: lab_to_textgrid <input.lab> <audio_duration_s> <tier_name> <output.TextGrid> \
             [--words <words.txt>] [--map <map.txt>]"
        );
        return ExitCode::FAILURE;
    };
    let wav_duration: f64 = duration.parse().expect("audio_duration_s is a number");

    let lab_text = fs::read_to_string(lab_path).expect("read lab file");
    let mut ends = parse_lab(&lab_text);
    assert!(!ends.is_empty(), "lab file has no segments");
    let last_end = ends.last_mut().expect("checked non-empty above");
    assert!(
        wav_duration >= last_end.0,
        "audio duration {wav_duration} is shorter than the lab file's last boundary {}",
        last_end.0
    );
    last_end.0 = wav_duration;

    let mut segments: Vec<(f64, f64, String)> = Vec::with_capacity(ends.len());
    let mut cursor = 0.0;
    for (end, label) in &ends {
        segments.push((cursor, *end, label.clone()));
        cursor = *end;
    }

    let mut doc = Annotation::new(0.0, wav_duration).expect("valid document domain");

    if let Some(path) = words_path {
        let spec = parse_words(&fs::read_to_string(path).expect("read words file"));
        let word_intervals = group_words(&segments, &spec);
        add_tier(&mut doc, "words", &word_intervals);
    }

    let phone_intervals: Vec<(f64, f64, String)> = match map_path {
        Some(path) => {
            let map = parse_map(&fs::read_to_string(path).expect("read map file"));
            segments
                .iter()
                .map(|(start, end, label)| {
                    let mapped = map
                        .get(label)
                        .unwrap_or_else(|| panic!("label {label:?} is missing from the map"));
                    (*start, *end, mapped.clone())
                })
                .collect()
        }
        None => segments.clone(),
    };
    add_tier(&mut doc, tier_name, &phone_intervals);

    let bytes = phx_textgrid::write(&doc).expect("write textgrid");
    fs::write(out_path, bytes).expect("write output file");
    println!("wrote {out_path}");
    ExitCode::SUCCESS
}
