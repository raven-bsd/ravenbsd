// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic virtual-time assertions for the native stack; wall-clock values are never
//! gated.

use raven_tcpip_bench::{raven_transfer, raven_ttfb};

#[test]
fn a_reliable_transfer_is_deterministic_and_needs_no_retransmission() {
    let first = raven_transfer(64 * 1024, 20, 0.0, 0.0);
    let second = raven_transfer(64 * 1024, 20, 0.0, 0.0);
    assert_eq!(first.bytes, 64 * 1024);
    assert_eq!(first.virtual_ms, second.virtual_ms);
    assert_eq!(first.retrans_bytes, Some(0));
}

#[test]
fn a_lossy_transfer_is_deterministic_and_intact() {
    let first = raven_transfer(64 * 1024, 20, 5.0, 0.0);
    let second = raven_transfer(64 * 1024, 20, 5.0, 0.0);
    assert_eq!(first.bytes, 64 * 1024);
    assert_eq!(first.virtual_ms, second.virtual_ms);
    assert!(first.retrans_bytes.is_some_and(|bytes| bytes > 0));
}

#[test]
fn a_reordered_transfer_is_intact() {
    let run = raven_transfer(64 * 1024, 20, 0.0, 10.0);
    assert_eq!(run.bytes, 64 * 1024);
}

#[test]
fn time_to_first_byte_is_positive_and_deterministic() {
    let first = raven_ttfb(20);
    let second = raven_ttfb(20);
    assert!(first.virtual_ms > 0);
    assert_eq!(first.virtual_ms, second.virtual_ms);
}

#[test]
fn larger_round_trips_take_proportionally_longer() {
    let short = raven_transfer(256 * 1024, 5, 0.0, 0.0).virtual_ms;
    let long = raven_transfer(256 * 1024, 50, 0.0, 0.0).virtual_ms;
    assert!(
        long > short * 3,
        "5 ms RTT took {short} ms, 50 ms RTT {long} ms"
    );
}
