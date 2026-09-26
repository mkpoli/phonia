# phonia-audio

Audio buffers for speech analysis: planar `f32` channels with an `f64` sample
rate, decoding for WAV, AIFF, FLAC and MP3, WAV encoding, and Praat-compatible
resampling.

Decoding runs through `symphonia` for AIFF, FLAC and MP3, and `hound` for WAV;
`Audio::from_bytes` sniffs the container and picks the right decoder. WAV
writing supports 8/16/24/32-bit PCM and 32-bit float. Resampling uses
`ResampleQuality::BandLimitedSinc`, matching Praat's "Sound: Resample..." at
its default precision, or a windowed-sinc anti-aliasing filter for general
use.

## Example

```rust
use phonia_audio::{Audio, BitDepth, ResampleQuality};
use std::f64::consts::PI;

let sample_rate = 8_000.0;
let frames = 800;
let samples: Vec<f32> = (0..frames)
    .map(|i| (2.0 * PI * 220.0 * i as f64 / sample_rate).sin() as f32)
    .collect();
let tone = Audio::new(vec![samples], sample_rate)?;

// Round-trip through a WAV byte stream.
let wav_bytes = tone.to_wav_bytes(BitDepth::Pcm16)?;
let decoded = Audio::from_wav_bytes(&wav_bytes)?;
assert_eq!(decoded.frames(), tone.frames());

// Resample down to a lower rate.
let resampled = decoded.resampled(4_000.0, ResampleQuality::PRAAT)?;
assert_eq!(resampled.sample_rate(), 4_000.0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Compatibility

Requires Rust 1.88 or newer (edition 2024).

## License

Licensed under either of MIT (LICENSE-MIT) or Apache-2.0 (LICENSE-APACHE) at
your option.
