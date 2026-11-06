//! A long-only, box-constrained mean-variance optimiser. Weights and returns
//! are plain fractions (f64); no money passes through here.

use crate::store::{Error, Result};

/// Result of one optimisation.
#[derive(Debug, Clone, PartialEq)]
pub struct MvoResult {
    pub weights: Vec<f64>,
    pub expected_return: f64,
    pub volatility: f64,
}

/// A covariance matrix from volatilities and a correlation matrix.
pub fn covariance_from(vols: &[f64], corr: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = vols.len();
    (0..n)
        .map(|i| (0..n).map(|j| corr[i][j] * vols[i] * vols[j]).collect())
        .collect()
}

pub fn portfolio_return(w: &[f64], mu: &[f64]) -> f64 {
    w.iter().zip(mu).map(|(a, b)| a * b).sum()
}

pub fn portfolio_variance(w: &[f64], cov: &[Vec<f64>]) -> f64 {
    let mut v = 0.0;
    for i in 0..w.len() {
        for j in 0..w.len() {
            v += w[i] * w[j] * cov[i][j];
        }
    }
    v
}

/// Projects `v` onto {w : sum(w) = 1, lo <= w <= hi} (Euclidean), by
/// bisection on the shift tau in w_i = clamp(v_i - tau, lo_i, hi_i).
pub fn project_box_simplex(v: &[f64], lo: &[f64], hi: &[f64]) -> Vec<f64> {
    let n = v.len();
    let sum_at = |tau: f64| -> f64 { (0..n).map(|i| (v[i] - tau).clamp(lo[i], hi[i])).sum() };
    let (mut a, mut b) = (-10.0_f64, 10.0_f64);
    // sum_at is non-increasing in tau: widen until the root is bracketed.
    while sum_at(a) < 1.0 && a > -1e6 {
        a *= 2.0;
    }
    while sum_at(b) > 1.0 && b < 1e6 {
        b *= 2.0;
    }
    for _ in 0..100 {
        let m = (a + b) / 2.0;
        if sum_at(m) > 1.0 {
            a = m;
        } else {
            b = m;
        }
    }
    let tau = (a + b) / 2.0;
    (0..n).map(|i| (v[i] - tau).clamp(lo[i], hi[i])).collect()
}

/// Optional settings for [`optimise`]; the defaults match the prototype.
#[derive(Debug, Clone)]
pub struct Options<'a> {
    pub lower: Option<&'a [f64]>,
    pub upper: Option<&'a [f64]>,
    pub max_iter: usize,
    pub tol: f64,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Options {
            lower: None,
            upper: None,
            max_iter: 20_000,
            tol: 1e-12,
        }
    }
}

/// maximise w'mu - (lambda/2) w'Σw  subject to sum(w) = 1, lo <= w <= hi.
///
/// Convex; solved by projected gradient with a 1/L step. A larger
/// `risk_aversion` (lambda) gives a safer portfolio. Bounds that cannot sum
/// to one are refused.
pub fn optimise(
    mu: &[f64],
    cov: &[Vec<f64>],
    risk_aversion: f64,
    opts: &Options,
) -> Result<MvoResult> {
    let n = mu.len();
    let zeros = vec![0.0; n];
    let ones = vec![1.0; n];
    let lo = opts.lower.unwrap_or(&zeros);
    let hi = opts.upper.unwrap_or(&ones);
    if lo.len() != n || hi.len() != n || cov.len() != n || cov.iter().any(|r| r.len() != n) {
        return Err(Error::Invalid("optimiser inputs differ in size".into()));
    }
    if lo.iter().sum::<f64>() > 1.0 + 1e-9 || hi.iter().sum::<f64>() < 1.0 - 1e-9 {
        return Err(Error::Invalid("infeasible weight bounds".into()));
    }
    // Gradient Lipschitz constant <= lambda * trace(cov) for a PSD cov.
    let trace: f64 = (0..n).map(|i| cov[i][i]).sum();
    let step = 1.0 / (risk_aversion * trace + 1e-12);
    let mut w = project_box_simplex(&vec![1.0 / n as f64; n], lo, hi);
    for _ in 0..opts.max_iter {
        let moved: Vec<f64> = (0..n)
            .map(|i| {
                let cw: f64 = (0..n).map(|j| cov[i][j] * w[j]).sum();
                w[i] + step * (mu[i] - risk_aversion * cw)
            })
            .collect();
        let next = project_box_simplex(&moved, lo, hi);
        let diff: f64 = next.iter().zip(&w).map(|(a, b)| (a - b).abs()).sum();
        w = next;
        if diff < opts.tol {
            break;
        }
    }
    Ok(MvoResult {
        expected_return: portfolio_return(&w, mu),
        volatility: portfolio_variance(&w, cov).sqrt(),
        weights: w,
    })
}

/// Points along the efficient frontier, from very safe (lambda 50) to
/// aggressive (lambda 0.5).
pub fn efficient_frontier(
    mu: &[f64],
    cov: &[Vec<f64>],
    lower: Option<&[f64]>,
    upper: Option<&[f64]>,
    points: usize,
) -> Result<Vec<MvoResult>> {
    let opts = Options {
        lower,
        upper,
        max_iter: 5000,
        ..Options::default()
    };
    let last = points.saturating_sub(1).max(1) as f64;
    (0..points)
        .map(|k| {
            let lambda = 50.0 * (0.5_f64 / 50.0).powf(k as f64 / last);
            optimise(mu, cov, lambda, &opts)
        })
        .collect()
}

/// Ported from the prototype's `test/core/engine_test.dart` (projection and
/// MVO groups).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Assumptions;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} is not within {tol} of {b}");
    }

    #[test]
    fn projection_sums_to_one_and_respects_bounds() {
        let w = project_box_simplex(&[0.9, 0.9, -0.5], &[0.0, 0.0, 0.0], &[0.6, 1.0, 1.0]);
        close(w.iter().sum(), 1.0, 1e-9);
        assert!(w[0] <= 0.6 + 1e-12);
        assert!(w.iter().all(|x| *x >= 0.0));
    }

    #[test]
    fn two_asset_global_minimum_variance_matches_closed_form() {
        let (s1, s2, rho) = (0.2, 0.1, 0.3);
        let cov = covariance_from(&[s1, s2], &[vec![1.0, rho], vec![rho, 1.0]]);
        let r = optimise(&[0.05, 0.05], &cov, 10.0, &Options::default()).unwrap();
        let expected = (s2 * s2 - rho * s1 * s2) / (s1 * s1 + s2 * s2 - 2.0 * rho * s1 * s2);
        close(r.weights[0], expected, 1e-4);
    }

    #[test]
    fn equal_vol_uncorrelated_equal_return_is_50_50() {
        let cov = covariance_from(&[0.15, 0.15], &[vec![1.0, 0.0], vec![0.0, 1.0]]);
        let r = optimise(&[0.06, 0.06], &cov, 3.0, &Options::default()).unwrap();
        close(r.weights[0], 0.5, 1e-5);
    }

    #[test]
    fn two_asset_interior_optimum_matches_closed_form() {
        let (s1, s2, rho, m1, m2, lam) = (0.18, 0.06, 0.2, 0.08, 0.03, 4.0);
        let c = rho * s1 * s2;
        let cov = covariance_from(&[s1, s2], &[vec![1.0, rho], vec![rho, 1.0]]);
        let r = optimise(&[m1, m2], &cov, lam, &Options::default()).unwrap();
        let expected = ((m1 - m2) / lam + s2 * s2 - c) / (s1 * s1 + s2 * s2 - 2.0 * c);
        close(r.weights[0], expected, 1e-4);
    }

    #[test]
    fn upper_bound_binds_and_weights_still_sum_to_one() {
        let cov = covariance_from(
            &[0.2, 0.1, 0.05],
            &[
                vec![1.0, 0.2, 0.0],
                vec![0.2, 1.0, 0.0],
                vec![0.0, 0.0, 1.0],
            ],
        );
        let upper = [0.3, 1.0, 1.0];
        let opts = Options {
            upper: Some(&upper),
            ..Options::default()
        };
        let r = optimise(&[0.1, 0.05, 0.02], &cov, 0.5, &opts).unwrap();
        close(r.weights[0], 0.3, 1e-6);
        close(r.weights.iter().sum(), 1.0, 1e-9);
    }

    #[test]
    fn infeasible_bounds_are_refused() {
        let cov = covariance_from(&[0.1, 0.1], &[vec![1.0, 0.0], vec![0.0, 1.0]]);
        let upper = [0.3, 0.3];
        let opts = Options {
            upper: Some(&upper),
            ..Options::default()
        };
        assert!(optimise(&[0.1, 0.1], &cov, 1.0, &opts).is_err());
    }

    #[test]
    fn efficient_frontier_is_monotone_in_risk_and_return() {
        let a = Assumptions::builtin();
        let cov = covariance_from(&a.vols(), &a.correlations);
        let f = efficient_frontier(&a.mu(), &cov, None, None, 8).unwrap();
        assert_eq!(f.len(), 8);
        for i in 1..f.len() {
            assert!(f[i].volatility >= f[i - 1].volatility - 1e-6);
            assert!(f[i].expected_return >= f[i - 1].expected_return - 1e-6);
        }
    }
}
