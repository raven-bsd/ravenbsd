// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the combined Pi 4 soak.

use super::*;

#[test]
fn the_default_soak_has_the_documented_counts() {
    let report = run(DEFAULT_ITERATIONS).expect("soak");
    assert_eq!(
        report,
        Report {
            iterations: 1024,
            rx_completed: 1024,
            tx_completed: 910,
            rx_errors: 128,
            tx_errors: 114,
            owned_checks: 147,
            link_events: 1024,
            irq_checks: 32,
            recovery_resets: 3,
            recovery_fault: true,
            audio_periods: 960,
            audio_frames: 983_040,
            audio_underruns: 64,
            audio_writes: 1032,
            audio_bytes: 8_421_504,
        }
    );
}

#[test]
fn a_zero_iteration_soak_is_rejected() {
    assert_eq!(run(0), Err(SoakError::ZeroIterations));
}

#[test]
fn an_oversized_soak_is_rejected() {
    assert_eq!(run(MAX_ITERATIONS + 1), Err(SoakError::TooManyIterations));
}

#[test]
fn one_iteration_is_all_underrun_and_still_checks_genet() {
    let report = run(1).expect("one iteration");
    assert_eq!(report.iterations, 1);
    assert_eq!(report.rx_completed, 1);
    assert_eq!(report.tx_completed, 0);
    assert_eq!(report.rx_errors, 1);
    assert_eq!(report.tx_errors, 1);
    assert_eq!(report.owned_checks, 1);
    assert_eq!(report.link_events, 1);
    assert_eq!(report.irq_checks, 1);
    assert_eq!(report.audio_periods, 0);
    assert_eq!(report.audio_frames, 0);
    assert_eq!(report.audio_underruns, 1);
    assert_eq!(report.audio_writes, 9);
    assert_eq!(report.audio_bytes, 41_088);
}
