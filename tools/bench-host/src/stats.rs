// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Small-sample statistics used by every measurement.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Stats {
    pub(crate) n: usize,
    pub(crate) median: f64,
    pub(crate) p99: f64,
    pub(crate) min: f64,
    pub(crate) max: f64,
}

pub(crate) fn summarize(samples: &[f64]) -> Stats {
    if samples.is_empty() {
        return Stats::default();
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let median = if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    };
    let rank = ((n as f64 * 0.99).ceil() as usize)
        .saturating_sub(1)
        .min(n - 1);
    Stats {
        n,
        median,
        p99: sorted[rank],
        min: sorted[0],
        max: sorted[n - 1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_handles_odd_and_even_counts() {
        assert_eq!(summarize(&[3.0, 1.0, 2.0]).median, 2.0);
        assert_eq!(summarize(&[4.0, 1.0, 3.0, 2.0]).median, 2.5);
        assert_eq!(summarize(&[7.0]).median, 7.0);
    }

    #[test]
    fn p99_uses_nearest_rank() {
        let samples: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(summarize(&samples).p99, 99.0);
        let samples: Vec<f64> = (1..=1000).map(f64::from).collect();
        assert_eq!(summarize(&samples).p99, 990.0);
    }

    #[test]
    fn min_max_and_empty() {
        let stats = summarize(&[5.0, -2.0, 9.0]);
        assert_eq!(stats.min, -2.0);
        assert_eq!(stats.max, 9.0);
        assert_eq!(summarize(&[]), Stats::default());
    }
}
