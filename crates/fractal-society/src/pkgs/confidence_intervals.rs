//! Confidence-interval helper package.
//!
//! Provides deterministic percentile and bootstrap mean confidence intervals.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Percentile of a sample, returning `None` for empty samples or invalid percentiles.
pub fn percentile(sample: &[f64], pct: f64) -> Option<f64> {
    if sample.is_empty() || !pct.is_finite() || !(0.0..=100.0).contains(&pct) {
        return None;
    }
    let mut sorted = finite_sorted(sample)?;
    let index = if sorted.len() == 1 {
        0
    } else {
        ((pct / 100.0) * (sorted.len() - 1) as f64).round() as usize
    };
    Some(sorted.swap_remove(index))
}

/// Bootstrap mean confidence interval at `confidence`, deterministic for `seed`.
pub fn mean_ci(sample: &[f64], confidence: f64, trials: usize, seed: u64) -> Option<(f64, f64)> {
    if sample.is_empty()
        || trials == 0
        || !confidence.is_finite()
        || !(0.0..1.0).contains(&confidence)
        || finite_sorted(sample).is_none()
    {
        return None;
    }

    let mut rng = StdRng::seed_from_u64(seed);
    let mut means = Vec::with_capacity(trials);
    for _ in 0..trials {
        let mut sum = 0.0;
        for _ in 0..sample.len() {
            let index = rng.gen_range(0..sample.len());
            sum += sample[index];
        }
        means.push(sum / sample.len() as f64);
    }

    let tail_pct = (1.0 - confidence) / 2.0 * 100.0;
    let lower = percentile(&means, tail_pct)?;
    let upper = percentile(&means, 100.0 - tail_pct)?;
    Some((lower, upper))
}

fn finite_sorted(sample: &[f64]) -> Option<Vec<f64>> {
    let mut sorted = Vec::with_capacity(sample.len());
    for value in sample {
        if !value.is_finite() {
            return None;
        }
        sorted.push(*value);
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("finite values are comparable"));
    Some(sorted)
}
