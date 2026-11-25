//! Validation of reported distributions, not a repair or a statistical model.
use super::{CodecError, Number, Probability};

const ARITHMETIC_EPSILON: f64 = 1e-12;

/// Native report resolution. These profiles are qualified with selected live
/// observations; this is an admission envelope, not a provider precision claim.
pub(super) struct Precision {
    pub probability_half_step: f64,
    pub score_half_step: f64,
}

#[derive(Clone, Copy)]
struct Interval {
    low: f64,
    high: f64,
}

impl Precision {
    fn intervals(&self, probabilities: &[Probability]) -> Result<Vec<Interval>, CodecError> {
        let mut nonzero = false;
        let intervals = probabilities
            .iter()
            .map(|p| {
                let p = p.as_number().as_f64().ok_or_else(CodecError::invalid)?;
                nonzero |= p > 0.;
                Ok(Interval {
                    low: (p - self.probability_half_step).max(0.),
                    high: (p + self.probability_half_step).min(1.),
                })
            })
            .collect::<Result<Vec<_>, CodecError>>()?;
        let low: f64 = intervals.iter().map(|p| p.low).sum();
        let high: f64 = intervals.iter().map(|p| p.high).sum();
        if !nonzero || low > 1. + ARITHMETIC_EPSILON || high < 1. - ARITHMETIC_EPSILON {
            return Err(CodecError::invalid());
        }
        Ok(intervals)
    }

    pub fn choice(&self, probabilities: &[Probability], selected: usize) -> Result<(), CodecError> {
        let intervals = self.intervals(probabilities)?;
        let winner = intervals.get(selected).ok_or_else(CodecError::invalid)?;
        let other_min: f64 = intervals
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != selected)
            .map(|(_, interval)| interval.low)
            .sum();
        let top = winner.high.min(1. - other_min);
        let high: f64 = intervals.iter().map(|p| p.high.min(top)).sum();
        if intervals.iter().any(|p| p.low > top + ARITHMETIC_EPSILON)
            || high < 1. - ARITHMETIC_EPSILON
        {
            return Err(CodecError::invalid());
        }
        Ok(())
    }

    pub fn score(&self, probabilities: &[Probability], score: &Number) -> Result<(), CodecError> {
        let intervals = self.intervals(probabilities)?;
        let score = score.as_f64().ok_or_else(CodecError::invalid)?;
        let ceiling = (intervals.len() - 1) as f64;
        let low = weighted_extreme(&intervals, 0..intervals.len());
        let high = weighted_extreme(&intervals, (0..intervals.len()).rev());
        let tolerance = self.score_half_step + ARITHMETIC_EPSILON;
        if !(0. ..=ceiling).contains(&score) || score < low - tolerance || score > high + tolerance
        {
            return Err(CodecError::invalid());
        }
        Ok(())
    }
}

fn weighted_extreme(intervals: &[Interval], order: impl Iterator<Item = usize>) -> f64 {
    // Start at lower bounds, then assign remaining mass to the cheapest
    // (or most expensive) coordinates, retaining the simplex constraint.
    let mut remaining = 1. - intervals.iter().map(|p| p.low).sum::<f64>();
    let mut total: f64 = intervals
        .iter()
        .enumerate()
        .map(|(i, p)| i as f64 * p.low)
        .sum();
    for i in order {
        let p = intervals[i];
        let added = remaining.max(0.).min(p.high - p.low);
        total += i as f64 * added;
        remaining -= added;
    }
    total
}
