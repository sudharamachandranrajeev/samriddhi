//! A classical forecast (no ML, no LLM): simple exponential smoothing with
//! the smoothing constant chosen by least one-step squared error. The band
//! comes from the in-sample residual spread and widens with the horizon.
//! Every screen that shows a [`Forecast`] must show its confidence, its
//! [`TrackRecord`] and the model disclosure: it is a model, not a promise.
//! Series are returns or rates (f64), never money.

use serde::Serialize;

use crate::store::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Forecast {
    pub mean: Vec<f64>,
    pub lower: Vec<f64>,
    pub upper: Vec<f64>,
    /// Nominal coverage of the band, e.g. 0.8.
    pub confidence: f64,
    pub alpha: f64,
    pub track: TrackRecord,
}

/// Rolling-origin back-test of the forecaster on its own history.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRecord {
    pub trials: usize,
    /// Times the realised value fell inside the predicted band.
    pub hits: usize,
    pub mae_model: f64,
    /// Mean absolute error of the "same as last value" forecast.
    pub mae_naive: f64,
}

impl TrackRecord {
    pub fn hit_rate(&self) -> f64 {
        if self.trials == 0 {
            0.0
        } else {
            self.hits as f64 / self.trials as f64
        }
    }
}

/// Normal quantile for the supported confidence levels (0.8, 0.9, 0.95).
pub fn z_for(confidence: f64) -> Result<f64> {
    [(0.8, 1.2816), (0.9, 1.6449), (0.95, 1.96)]
        .into_iter()
        .find(|(c, _)| *c == confidence)
        .map(|(_, z)| z)
        .ok_or_else(|| Error::Invalid("confidence must be 0.8, 0.9 or 0.95".into()))
}

fn ses_level(y: &[f64], alpha: f64) -> f64 {
    y[1..]
        .iter()
        .fold(y[0], |l, v| alpha * v + (1.0 - alpha) * l)
}

/// (alpha, sigma) of the best fit on a 0.05 grid.
fn fit_ses(y: &[f64]) -> (f64, f64) {
    let mut best_a = 0.5;
    let mut best_sse = f64::INFINITY;
    for k in 1..=20 {
        let a = f64::from(k) * 0.05;
        let mut l = y[0];
        let mut sse = 0.0;
        for v in &y[1..] {
            let e = v - l;
            sse += e * e;
            l = a * v + (1.0 - a) * l;
        }
        if sse < best_sse {
            best_sse = sse;
            best_a = a;
        }
    }
    (best_a, (best_sse / (y.len() - 1) as f64).sqrt())
}

/// Moving-average level over the last `window` points (baseline method).
pub fn moving_average(y: &[f64], window: usize) -> Result<f64> {
    let w = window.min(y.len());
    if w == 0 {
        return Err(Error::Invalid("a moving average needs data".into()));
    }
    Ok(y[y.len() - w..].iter().sum::<f64>() / w as f64)
}

/// The prototype's defaults: one step, 80% band, at least 8 points.
pub const MIN_HISTORY: usize = 8;

/// Forecast `horizon` steps ahead of `series`. Needs at least `min_history`
/// points (and at least two).
pub fn forecast_ses(
    series: &[f64],
    horizon: usize,
    confidence: f64,
    min_history: usize,
) -> Result<Forecast> {
    if series.len() < min_history.max(2) {
        return Err(Error::Invalid(format!(
            "need at least {} observations",
            min_history.max(2)
        )));
    }
    if series.iter().any(|v| !v.is_finite()) {
        return Err(Error::Invalid("every observation must be a number".into()));
    }
    let z = z_for(confidence)?;
    let (alpha, sigma) = fit_ses(series);
    let level = ses_level(series, alpha);
    let (mut mean, mut lower, mut upper) = (vec![], vec![], vec![]);
    for h in 1..=horizon {
        let sd = sigma * (1.0 + (h - 1) as f64 * alpha * alpha).sqrt();
        mean.push(level);
        lower.push(level - z * sd);
        upper.push(level + z * sd);
    }
    // Rolling origin: refit on a growing prefix, score one step ahead.
    let (mut trials, mut hits) = (0, 0);
    let (mut err_model, mut err_naive) = (0.0, 0.0);
    for t in min_history.max(2)..series.len() {
        let prefix = &series[..t];
        let (a, s) = fit_ses(prefix);
        let l = ses_level(prefix, a);
        let actual = series[t];
        trials += 1;
        if (actual - l).abs() <= z * s {
            hits += 1;
        }
        err_model += (actual - l).abs();
        err_naive += (actual - prefix[t - 1]).abs();
    }
    let per = |e: f64| if trials == 0 { 0.0 } else { e / trials as f64 };
    Ok(Forecast {
        mean,
        lower,
        upper,
        confidence,
        alpha,
        track: TrackRecord {
            trials,
            hits,
            mae_model: per(err_model),
            mae_naive: per(err_naive),
        },
    })
}

/// Ported from the prototype's `test/core/rebalance_test.dart` ("forecast"
/// group).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::{AssetClass, Assumptions};
    use crate::synthetic::{annualise, sample_monthly_returns};

    #[test]
    fn constant_series_forecasts_the_constant_with_a_tight_band() {
        let f = forecast_ses(&[0.05; 20], 1, 0.8, MIN_HISTORY).unwrap();
        assert_eq!(f.mean.len(), 1);
        assert!((f.mean[0] - 0.05).abs() < 1e-9);
        assert!((f.upper[0] - f.lower[0]).abs() < 1e-9);
    }

    #[test]
    fn band_widens_with_horizon_and_brackets_the_mean() {
        let y: Vec<f64> = (0..30)
            .map(|i| 0.05 + if i % 2 == 0 { 0.04 } else { -0.04 })
            .collect();
        let f = forecast_ses(&y, 5, 0.8, MIN_HISTORY).unwrap();
        assert_eq!(f.mean.len(), 5);
        for i in 0..5 {
            assert!(f.lower[i] < f.mean[i]);
            assert!(f.upper[i] > f.mean[i]);
        }
        assert!(f.upper[4] - f.lower[4] > f.upper[0] - f.lower[0]);
        assert_eq!(f.track.trials, 22);
    }

    #[test]
    fn rejects_short_history_and_unsupported_confidence() {
        assert!(forecast_ses(&[1.0, 2.0, 3.0], 1, 0.8, MIN_HISTORY).is_err());
        assert!(forecast_ses(&[1.0; 20], 1, 0.5, MIN_HISTORY).is_err());
    }

    #[test]
    fn moving_average_of_last_window() {
        assert_eq!(moving_average(&[1.0, 2.0, 3.0, 4.0], 2).unwrap(), 3.5);
        assert!(moving_average(&[], 2).is_err());
    }

    #[test]
    fn track_record_hit_rate_is_near_nominal_on_sample_data() {
        let data = sample_monthly_returns(&Assumptions::builtin(), 7);
        let annual = annualise(&data[&AssetClass::Equity]);
        let f = forecast_ses(&annual, 1, 0.8, MIN_HISTORY).unwrap();
        assert_eq!(f.track.trials, annual.len() - 8);
        assert!((0.5..=1.0).contains(&f.track.hit_rate()));
    }
}
