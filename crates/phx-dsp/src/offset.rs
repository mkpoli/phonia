//! In-place DC-offset removal.

/// Subtracts the arithmetic mean from `x` in place, centring it on zero.
///
/// A constant DC bias (typically an ADC offset in a field recording) skews
/// intensity and low-frequency spectral measurements; removing it is Praat's
/// Sound "Subtract mean". An empty slice is left unchanged.
pub fn subtract_mean_in_place(x: &mut [f64]) {
    if x.is_empty() {
        return;
    }
    let mean = x.iter().sum::<f64>() / x.len() as f64;
    for sample in x.iter_mut() {
        *sample -= mean;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_a_constant_bias() {
        // A 1.0-amplitude alternation lifted by a +0.5 DC bias.
        let mut x: Vec<f64> = (0..64)
            .map(|i| 0.5 + if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        subtract_mean_in_place(&mut x);
        let mean = x.iter().sum::<f64>() / x.len() as f64;
        assert!(mean.abs() < 1e-12, "residual mean {mean}");
        // The waveform shape (sample-to-sample differences) is preserved.
        assert!((x[0] - 1.0).abs() < 1e-12 && (x[1] + 1.0).abs() < 1e-12);
    }

    #[test]
    fn empty_is_a_noop() {
        let mut empty: [f64; 0] = [];
        subtract_mean_in_place(&mut empty);
    }
}
