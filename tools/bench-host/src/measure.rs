// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Timed-loop helper.

use std::time::Instant;

pub(crate) fn measure<F: FnMut()>(
    runs: usize,
    iterations: usize,
    warmup_iterations: usize,
    mut operation: F,
) -> Vec<f64> {
    for _ in 0..warmup_iterations {
        operation();
    }
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        let elapsed = start.elapsed().as_nanos() as f64;
        samples.push(elapsed / iterations as f64);
    }
    samples
}
