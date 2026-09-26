use std::f64::consts::PI;

use crate::burg::burg_lpc;

/// The all-pole (LPC) spectral envelope of `samples` in decibels.
///
/// Fits a Burg all-pole model `H(z) = g / A(z)` of the given `order`, where
/// `A(z) = 1 + a_1 z^-1 + ... + a_p z^-p` and the gain `g` is the root-mean
/// square of the residual `A(z)` leaves on the signal. The transfer magnitude
/// `|H(e^{jω})| = g / |A(e^{jω})|` is sampled at `points` frequencies spanning
/// `0` to Nyquist inclusive, each returned as `(frequency_hz, decibels)`.
///
/// The envelope smooths away the harmonic fine structure a raw spectrum shows,
/// leaving the resonance peaks a phonetician reads as formants. Returns `None`
/// when the model cannot be fitted — `order` is zero, the request is degenerate
/// (`points < 2`, non-finite `sample_rate`), or the signal is too short or
/// silent for the Burg recursion.
#[must_use]
pub fn lpc_envelope_db(
    samples: &[f64],
    sample_rate: f64,
    order: usize,
    points: usize,
) -> Option<Vec<(f64, f64)>> {
    if order == 0 || points < 2 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return None;
    }

    let coeffs = burg_lpc(samples, order)?;
    if coeffs.iter().any(|value| !value.is_finite()) {
        return None;
    }

    let mut residual_power = 0.0;
    for (n, &sample) in samples.iter().enumerate() {
        let mut error = sample;
        for (k, &coeff) in coeffs.iter().enumerate() {
            if let Some(&past) = samples.get(n.wrapping_sub(k + 1)).filter(|_| n > k) {
                error += coeff * past;
            }
        }
        residual_power += error * error;
    }
    let gain = (residual_power / samples.len() as f64).sqrt();
    if !gain.is_finite() || gain <= 0.0 {
        return None;
    }

    let nyquist = sample_rate / 2.0;
    let envelope = (0..points)
        .map(|index| {
            let frequency = nyquist * index as f64 / (points - 1) as f64;
            let omega = 2.0 * PI * frequency / sample_rate;
            let mut real = 1.0;
            let mut imag = 0.0;
            for (k, &coeff) in coeffs.iter().enumerate() {
                let angle = omega * (k + 1) as f64;
                real += coeff * angle.cos();
                imag -= coeff * angle.sin();
            }
            let magnitude = gain / real.hypot(imag).max(1e-12);
            (frequency, 20.0 * magnitude.max(1e-12).log10())
        })
        .collect();

    Some(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse_train(sample_rate: f64, duration: f64, fundamental: f64) -> Vec<f64> {
        let frames = (sample_rate * duration).round() as usize;
        let period = (sample_rate / fundamental).round() as usize;
        (0..frames)
            .map(|i| if i.is_multiple_of(period) { 1.0 } else { 0.0 })
            .collect()
    }

    fn resonate(input: Vec<f64>, sample_rate: f64, frequency: f64, bandwidth: f64) -> Vec<f64> {
        let radius = (-PI * bandwidth / sample_rate).exp();
        let theta = 2.0 * PI * frequency / sample_rate;
        let c1 = 2.0 * radius * theta.cos();
        let c2 = -(radius * radius);
        let mut y1 = 0.0;
        let mut y2 = 0.0;
        input
            .into_iter()
            .map(|x| {
                let y = c1 * y1 + c2 * y2 + x;
                y2 = y1;
                y1 = y;
                y
            })
            .collect()
    }

    fn local_maxima(envelope: &[(f64, f64)]) -> Vec<f64> {
        envelope
            .windows(3)
            .filter(|window| window[1].1 > window[0].1 && window[1].1 > window[2].1)
            .map(|window| window[1].0)
            .collect()
    }

    #[test]
    fn envelope_peaks_sit_on_the_synthetic_formants() {
        let sample_rate = 10_000.0;
        let targets = [(700.0, 80.0), (1800.0, 120.0)];
        let mut signal = impulse_train(sample_rate, 0.25, 110.0);
        for &(frequency, bandwidth) in &targets {
            signal = resonate(signal, sample_rate, frequency, bandwidth);
        }

        let envelope = lpc_envelope_db(&signal, sample_rate, 12, 512).expect("envelope fits");
        let peaks = local_maxima(&envelope);

        for &(frequency, _) in &targets {
            let nearest = peaks
                .iter()
                .copied()
                .min_by(|left, right| {
                    (left - frequency)
                        .abs()
                        .total_cmp(&(right - frequency).abs())
                })
                .expect("envelope has a resonance peak");
            assert!(
                (nearest - frequency).abs() <= 0.05 * frequency,
                "peak {nearest} Hz should sit near the {frequency} Hz formant"
            );
        }
    }

    #[test]
    fn rejects_degenerate_requests() {
        let signal = impulse_train(8_000.0, 0.1, 120.0);
        assert!(lpc_envelope_db(&signal, 8_000.0, 0, 512).is_none());
        assert!(lpc_envelope_db(&signal, 8_000.0, 10, 1).is_none());
        assert!(lpc_envelope_db(&signal, 0.0, 10, 512).is_none());
        assert!(lpc_envelope_db(&[0.0; 32], 8_000.0, 10, 512).is_none());
    }
}
