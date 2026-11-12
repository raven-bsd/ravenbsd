// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic virtual-time assertions; wall-clock values are never gated.

use raven_tcpip_bench::{raven_transfer, raven_ttfb, smoltcp_transfer, smoltcp_ttfb};

#[test]
fn raven_transfer_is_deterministic_on_a_reliable_link() {
    let first = raven_transfer(64 * 1024, 20, 0.0, 0.0);
    let second = raven_transfer(64 * 1024, 20, 0.0, 0.0);
    assert_eq!(first.bytes, 64 * 1024);
    assert_eq!(first.virtual_ms, second.virtual_ms);
    assert_eq!(first.retrans_bytes, Some(0));
}

#[test]
fn smoltcp_transfer_is_deterministic_on_a_reliable_link() {
    let first = smoltcp_transfer(64 * 1024, 20, 0, 0);
    let second = smoltcp_transfer(64 * 1024, 20, 0, 0);
    assert_eq!(first.bytes, 64 * 1024);
    assert_eq!(first.virtual_ms, second.virtual_ms);
}

#[test]
fn time_to_first_byte_is_positive_and_deterministic() {
    let first = raven_ttfb(20);
    let second = raven_ttfb(20);
    assert!(first.virtual_ms > 0);
    assert_eq!(first.virtual_ms, second.virtual_ms);
    let baseline = smoltcp_ttfb(20);
    assert!(baseline.virtual_ms > 0);
}

#[test]
fn raven_transfer_survives_loss_with_an_intact_stream() {
    let run = raven_transfer(64 * 1024, 20, 5.0, 0.0);
    assert_eq!(run.bytes, 64 * 1024);
}

#[test]
fn smoltcp_transfer_survives_loss_with_an_intact_stream() {
    let run = smoltcp_transfer(64 * 1024, 20, 10, 0);
    assert_eq!(run.bytes, 64 * 1024);
}
