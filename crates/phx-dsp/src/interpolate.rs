//! Windowed-sinc interpolation and sub-sample peak location.

use std::f64::consts::PI;

/// Value of the sampled sequence `y` at the continuous index `x`, by
/// Hanning-windowed `sin(x)/x` interpolation.
///
/// This is Boersma (1993) eq. 22: the ideal sinc reconstruction truncated to
/// `depth` samples on each side of `x` and tapered by a raised cosine so the
/// interpolation falls to zero at its edges. With `nl = ⌊x⌋`, `φ_l = x − nl`,
/// and `φ_r = 1 − φ_l`,
///
/// ```text
/// y(x) ≈ Σ_{k=1}^{depth} y[nl+1−k] · sinc(φ_l+k−1) · ½(1 + cos(π(φ_l+k−1)/(φ_l+depth)))
///      + Σ_{k=1}^{depth} y[nl+k]   · sinc(φ_r+k−1) · ½(1 + cos(π(φ_r+k−1)/(φ_r+depth)))
/// ```
///
/// where `sinc(d) = sin(πd)/(πd)`. The depth is reduced on both sides to the
/// number of samples available, so the taper always ends one step past the
/// outermost sample used and no sample outside `y` is ever needed. `x` below
/// the first sample returns `y[0]`, `x` past the last returns the last sample,
/// and an integer `x` returns `y[x]` exactly.
///
/// A depth of two or less, whether asked for or forced by the edges, is not a
/// sinc at all: one sample per side interpolates linearly and two per side
/// with the cubic through the four neighbours (the Hermite form whose end
/// slopes are the central differences), as Praat's interpolation does at
/// those depths. The switch was settled against the oracle (a resampled
/// signal's first samples).
///
/// Each side is summed by [`tapered_sinc_sum`], which generates both the sinc
/// and the taper by recurrence.
#[must_use]
pub fn sinc_interpolate(y: &[f64], x: f64, depth: usize) -> f64 {
    let n = y.len();
    if n == 0 {
        return 0.0;
    }
    let last = (n - 1) as f64;
    if x <= 0.0 {
        return y[0];
    }
    if x >= last {
        return y[n - 1];
    }
    let nl = x.floor();
    if x == nl {
        return y[nl as usize];
    }
    let nl = nl as usize;
    let nr = nl + 1;
    let depth = depth.min(nl + 1).min(n - nr);
    let phi = x - nl as f64;
    match depth {
        0 => return y[(x + 0.5).floor() as usize],
        1 => return y[nl] + phi * (y[nr] - y[nl]),
        2 => {
            let (yl, yr) = (y[nl], y[nr]);
            let dyl = 0.5 * (yr - y[nl - 1]);
            let dyr = 0.5 * (y[nr + 1] - yl);
            let fir = 1.0 - phi;
            return yl * fir + yr * phi
                - phi * fir * (0.5 * (dyr - dyl) + (phi - 0.5) * (dyl + dyr - 2.0 * (yr - yl)));
        }
        _ => {}
    }

    // Left side: samples nl, nl−1, …, nl+1−depth at distances φ, φ+1, ….
    let left = tapered_sinc_sum(y[nr - depth..=nl].iter().rev(), phi, depth);
    // Right side: samples nr, nr+1, …, nl+depth at distances 1−φ, 2−φ, ….
    let right = tapered_sinc_sum(y[nr..=nl + depth].iter(), 1.0 - phi, depth);
    left + right
}

/// One side of eq. 22: `Σ_k y_k · sinc(φ + k) · ½(1 + cos(π(φ + k)/(φ + depth)))`
/// for `k = 0, 1, …`, with `sinc(d) = sin(πd)/(πd)`.
///
/// `sin(π(φ + k))` is `(−1)^k · sin(πφ)`, and the taper angle advances by the
/// constant `π/(φ + depth)` per term, so its cosine comes from a rotation:
/// one sine and two sine–cosine pairs per side, no trigonometry per term
/// (the rotation is four multiplications and two additions). The rotated
/// pair drifts in phase and magnitude by about one ulp per term, and each
/// side turns through less than π in total, so the sum's error stays near
/// `depth · ε` — around 1e-13 at depth 700, the deepest any caller asks for;
/// a far deeper caller would need to renormalise. A rotation rather than the
/// cheaper three-term cosine recurrence, which amplifies rounding by about
/// `span/π` and would spend most of the tests' 1e-11 budget at depth 700.
fn tapered_sinc_sum<'a>(samples: impl Iterator<Item = &'a f64>, phi: f64, depth: usize) -> f64 {
    let span = phi + depth as f64;
    let mut signed_sine = (PI * phi).sin();
    let (mut taper_sin, mut taper_cos) = (PI * phi / span).sin_cos();
    let (step_sin, step_cos) = (PI / span).sin_cos();
    let mut result = 0.0;
    for (k, &sample) in samples.enumerate() {
        let d = phi + k as f64;
        result += sample * signed_sine / (PI * d) * 0.5 * (1.0 + taper_cos);
        signed_sine = -signed_sine;
        let rotated_cos = taper_cos * step_cos - taper_sin * step_sin;
        taper_sin = taper_sin * step_cos + taper_cos * step_sin;
        taper_cos = rotated_cos;
    }
    result
}

/// Locates the local maximum of `y` near index `around` to sub-sample precision.
///
/// The sampled sequence is reconstructed with [`sinc_interpolate`] (Boersma
/// 1993 eq. 22) and the continuous maximum is sought in `[around−1, around+1]`
/// with Brent's method, as Boersma describes. Returns `(position, value)` with
/// `position` in fractional sample units. Sinc interpolation — rather than
/// parabolic — is what recovers the peak height accurately, which downstream
/// HNR estimation needs.
///
/// `depth` is the one-sided interpolation width in samples; larger is more
/// accurate and slower, and it is bounded by the samples available on either
/// side. A `depth` of `0` is treated as `1`.
#[must_use]
pub fn sinc_interpolate_max(y: &[f64], around: usize, depth: usize) -> (f64, f64) {
    let depth = depth.max(1);
    if y.is_empty() {
        return (0.0, 0.0);
    }
    let last = (y.len() - 1) as f64;
    let lo = (around as f64 - 1.0).max(0.0);
    let hi = (around as f64 + 1.0).min(last);
    if lo >= hi {
        let x = (around as f64).clamp(0.0, last);
        return (x, sinc_interpolate(y, x, depth));
    }
    brent_maximize(|x| sinc_interpolate(y, x, depth), lo, hi, 1e-10)
}

/// Brent's derivative-free maximisation of `f` on `[a, b]` (Brent 1973, ch. 5):
/// golden-section steps with parabolic interpolation whenever the parabola
/// through the three best points is trustworthy. Stops when the bracket has
/// shrunk to `tol` around the best point.
///
/// Returns `(x, f(x))`. On a smooth unimodal peak a dozen evaluations reach
/// `tol = 1e-10`.
fn brent_maximize<F: Fn(f64) -> f64>(f: F, mut a: f64, mut b: f64, tol: f64) -> (f64, f64) {
    const GOLDEN: f64 = 0.381_966_011_250_105_2; // (3 − √5) / 2
    const MAX_ITERATIONS: usize = 60;
    let sqrt_epsilon = f64::EPSILON.sqrt();

    let mut x = a + GOLDEN * (b - a);
    let mut w = x;
    let mut v = x;
    let mut fx = f(x);
    let mut fw = fx;
    let mut fv = fx;
    let mut step_before_last = 0.0_f64;
    let mut d = 0.0_f64;

    for _ in 0..MAX_ITERATIONS {
        let mid = 0.5 * (a + b);
        let tol_here = sqrt_epsilon * x.abs() + tol / 3.0;
        if (x - mid).abs() + 0.5 * (b - a) <= 2.0 * tol_here {
            break;
        }
        let mut golden = true;
        if step_before_last.abs() > tol_here {
            // Parabola through (v, fv), (w, fw), (x, fx). A maximiser negates
            // the ordinate differences of the textbook minimiser.
            let r = (x - w) * (fv - fx);
            let mut q = (x - v) * (fw - fx);
            let mut p = (x - v) * q - (x - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            } else {
                q = -q;
            }
            let previous = step_before_last;
            step_before_last = d;
            if p.abs() < (0.5 * q * previous).abs() && p > q * (a - x) && p < q * (b - x) {
                d = p / q;
                let u = x + d;
                if u - a < 2.0 * tol_here || b - u < 2.0 * tol_here {
                    d = if x < mid { tol_here } else { -tol_here };
                }
                golden = false;
            }
        }
        if golden {
            step_before_last = if x >= mid { a - x } else { b - x };
            d = GOLDEN * step_before_last;
        }
        let u = if d.abs() >= tol_here {
            x + d
        } else if d > 0.0 {
            x + tol_here
        } else {
            x - tol_here
        };
        let fu = f(u);
        // On an exactly flat top a tie moves to the newer probe; peaks of a
        // sinc-interpolated correlation are never flat, so this only decides
        // which of two equal points is reported.
        if fu >= fx {
            if u >= x {
                a = x;
            } else {
                b = x;
            }
            v = w;
            fv = fw;
            w = x;
            fw = fx;
            x = u;
            fx = fu;
        } else {
            if u < x {
                a = u;
            } else {
                b = u;
            }
            if fu >= fw || w == x {
                v = w;
                fv = fw;
                w = u;
                fw = fu;
            } else if fu >= fv || v == x || v == w {
                v = u;
                fv = fu;
            }
        }
    }
    (x, fx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovers_value_at_integer() {
        let y = [0.0, 1.0, 4.0, 9.0, 16.0, 25.0];
        for (i, &yi) in y.iter().enumerate() {
            let v = sinc_interpolate(&y, i as f64, 8);
            assert!((v - yi).abs() < 1e-9, "index {i}: {v} vs {yi}");
        }
    }

    #[test]
    fn matches_direct_kernel_sum() {
        // The recurrence form must equal the literal eq. 22 sum.
        let n = 64;
        let y: Vec<f64> = (0..n).map(|i| (0.37 * i as f64).sin()).collect();
        for &x in &[12.25_f64, 20.5, 31.999, 40.01] {
            let depth = 12usize;
            let nl = x.floor() as usize;
            let phi_l = x - nl as f64;
            let phi_r = 1.0 - phi_l;
            let mut expected = 0.0;
            for k in 1..=depth {
                let d = phi_l + (k - 1) as f64;
                expected += y[nl + 1 - k] * (PI * d).sin() / (PI * d)
                    * (0.5 + 0.5 * (PI * d / (phi_l + depth as f64)).cos());
                let d = phi_r + (k - 1) as f64;
                expected += y[nl + k] * (PI * d).sin() / (PI * d)
                    * (0.5 + 0.5 * (PI * d / (phi_r + depth as f64)).cos());
            }
            let got = sinc_interpolate(&y, x, depth);
            assert!((got - expected).abs() < 1e-12, "x={x}: {got} vs {expected}");
        }
    }

    #[test]
    fn reduces_depth_near_the_edges() {
        // Left of x = 2.3 three samples exist, so both sides run at depth 3,
        // the smallest sinc depth, and the taper spans φ + 3.
        let y: Vec<f64> = (0..32).map(|i| (0.41 * i as f64).cos()).collect();
        let x = 2.3_f64;
        let depth = 3usize;
        let nl = 2usize;
        let phi_l = x - nl as f64;
        let phi_r = 1.0 - phi_l;
        let mut expected = 0.0;
        for k in 1..=depth {
            let d = phi_l + (k - 1) as f64;
            expected += y[nl + 1 - k] * (PI * d).sin() / (PI * d)
                * (0.5 + 0.5 * (PI * d / (phi_l + depth as f64)).cos());
            let d = phi_r + (k - 1) as f64;
            expected += y[nl + k] * (PI * d).sin() / (PI * d)
                * (0.5 + 0.5 * (PI * d / (phi_r + depth as f64)).cos());
        }
        let got = sinc_interpolate(&y, x, 12);
        assert!((got - expected).abs() < 1e-12, "{got} vs {expected}");
    }

    #[test]
    fn shallow_depths_interpolate_linearly_and_cubically() {
        let y = [0.0, 1.0, 4.0, 9.0, 16.0, 25.0];
        // One sample per side: linear between y[0] and y[1].
        assert!((sinc_interpolate(&y, 0.25, 12) - 0.25).abs() < 1e-12);
        // Two per side: the cubic through y[0..4] reproduces the parabola.
        assert!((sinc_interpolate(&y, 1.5, 2) - 2.25).abs() < 1e-12);
        assert!((sinc_interpolate(&y, 1.5, 12) - 2.25).abs() < 1e-12);
        // Off the midpoint the Hermite form with central-difference slopes
        // differs from the Lagrange cubic through the same four points: at
        // x = 1.25 on [0, 1, 3, 2] it gives 1.5 where Lagrange gives 1.5625.
        let bumpy = [0.0, 1.0, 3.0, 2.0, 5.0, 4.0];
        assert!((sinc_interpolate(&bumpy, 1.25, 2) - 1.5).abs() < 1e-12);
    }

    #[test]
    fn recurrence_holds_at_large_depth() {
        for depth in [300usize, 700] {
            recurrence_matches_direct_sum(depth);
        }
    }

    fn recurrence_matches_direct_sum(depth: usize) {
        let n = 2048;
        let y: Vec<f64> = (0..n).map(|i| (0.013 * i as f64).sin()).collect();
        let x = 1023.37_f64;
        let nl = x.floor() as usize;
        let phi_l = x - nl as f64;
        let phi_r = 1.0 - phi_l;
        let mut expected = 0.0;
        for k in 1..=depth {
            let d = phi_l + (k - 1) as f64;
            expected += y[nl + 1 - k] * (PI * d).sin() / (PI * d)
                * (0.5 + 0.5 * (PI * d / (phi_l + depth as f64)).cos());
            let d = phi_r + (k - 1) as f64;
            expected += y[nl + k] * (PI * d).sin() / (PI * d)
                * (0.5 + 0.5 * (PI * d / (phi_r + depth as f64)).cos());
        }
        let got = sinc_interpolate(&y, x, depth);
        assert!(
            (got - expected).abs() < 1e-11,
            "depth {depth}: {got} vs {expected}"
        );
    }

    #[test]
    fn recovers_sinusoid_peak_to_subsample() {
        // A pure cosine is band-limited below Nyquist, so windowed-sinc
        // interpolation reconstructs it and locates its peak precisely.
        let period = 20.0;
        let true_peak = 64.37; // non-integer maximum location
        let n = 128;
        let y: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * (i as f64 - true_peak) / period).cos())
            .collect();
        let around = true_peak.round() as usize;
        let (pos, val) = sinc_interpolate_max(&y, around, 50);
        assert!(
            (pos - true_peak).abs() < 0.01,
            "peak at {pos}, want {true_peak}"
        );
        assert!((val - 1.0).abs() < 1e-3, "peak height {val}");
    }

    #[test]
    fn recovers_peak_for_several_offsets() {
        let period = 24.0;
        let n = 160;
        for &frac in &[0.05, 0.25, 0.5, 0.73, 0.9] {
            let true_peak = 80.0 + frac;
            let y: Vec<f64> = (0..n)
                .map(|i| (2.0 * PI * (i as f64 - true_peak) / period).cos())
                .collect();
            let (pos, _) = sinc_interpolate_max(&y, true_peak.round() as usize, 60);
            assert!((pos - true_peak).abs() < 0.01, "frac {frac}: got {pos}");
        }
    }

    #[test]
    fn brent_finds_off_centre_and_endpoint_maxima() {
        let (x, _) = brent_maximize(|x| -(x - 0.83).powi(2), 0.0, 1.0, 1e-10);
        assert!((x - 0.83).abs() < 1e-8, "x = {x}");
        let (x, fx) = brent_maximize(|x| 2.0 * x, 0.0, 1.0, 1e-10);
        assert!(x > 1.0 - 1e-6 && fx > 2.0 - 1e-5, "x = {x}");
        let (x, _) = brent_maximize(|x| -x, 0.0, 1.0, 1e-10);
        assert!(x < 1e-6, "x = {x}");
    }

    #[test]
    fn brent_converges_in_few_evaluations() {
        let calls = std::cell::Cell::new(0usize);
        let (x, fx) = brent_maximize(
            |x| {
                calls.set(calls.get() + 1);
                -(x - 0.3).powi(2)
            },
            -1.0,
            1.0,
            1e-10,
        );
        assert!((x - 0.3).abs() < 1e-8, "x = {x}");
        assert!(fx.abs() < 1e-15);
        assert!(calls.get() <= 30, "{} evaluations", calls.get());
    }

    #[test]
    fn handles_edges_and_degenerate_inputs() {
        assert_eq!(sinc_interpolate_max(&[], 0, 10), (0.0, 0.0));
        let (p, v) = sinc_interpolate_max(&[5.0], 0, 10);
        assert_eq!(p, 0.0);
        assert!((v - 5.0).abs() < 1e-12);
        // A peak at the first sample: bracket clamps to [0, 1].
        let y = [3.0, 1.0, 0.5, 0.2];
        let (p, _) = sinc_interpolate_max(&y, 0, 8);
        assert!((0.0..=1.0).contains(&p));
        assert_eq!(sinc_interpolate(&y, -2.0, 4), 3.0);
        assert_eq!(sinc_interpolate(&y, 9.0, 4), 0.2);
    }
}
