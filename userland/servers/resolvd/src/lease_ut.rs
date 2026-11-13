// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Lease application tests.

use alloc::vec;

use super::*;

fn sample(renew_ms: u32, rebind_ms: u32, expires_ms: u32) -> Lease {
    Lease {
        addr: [10, 0, 2, 15],
        prefix: 24,
        router: Some([10, 0, 2, 2]),
        dns: vec![[10, 0, 2, 3]],
        renew_ms,
        rebind_ms,
        expires_ms,
    }
}

#[test]
fn converts_the_wire_record_without_change() {
    let info = DhcpLeaseInfo {
        addr: [192, 0, 2, 15],
        prefix: 25,
        router: Some([192, 0, 2, 1]),
        dns: vec![[192, 0, 2, 53], [198, 51, 100, 53]],
        renew_ms: 1_000,
        rebind_ms: 2_000,
        expires_ms: 3_000,
    };
    let lease = Lease::from(info.clone());
    assert_eq!(lease.addr, info.addr);
    assert_eq!(lease.prefix, info.prefix);
    assert_eq!(lease.router, info.router);
    assert_eq!(lease.dns, info.dns);
    assert_eq!(lease.renew_ms, info.renew_ms);
    assert_eq!(lease.rebind_ms, info.rebind_ms);
    assert_eq!(lease.expires_ms, info.expires_ms);
    assert_eq!(lease.config().servers, info.dns);
    assert!(lease.has_dns());
}

#[test]
fn earliest_timer_wins_with_a_margin() {
    let lease = sample(1_000, 2_000, 3_000);
    assert_eq!(lease.refresh_delay_ms(), 900);
    let lease = sample(50_000, 80_000, 100_000);
    assert_eq!(lease.refresh_delay_ms(), 49_000);
}

#[test]
fn unknown_timers_are_skipped() {
    let lease = sample(DHCP_TIMER_UNKNOWN, 2_000, 3_000);
    assert_eq!(lease.refresh_delay_ms(), 1_800);
    let lease = sample(DHCP_TIMER_UNKNOWN, DHCP_TIMER_UNKNOWN, 3_000);
    assert_eq!(lease.refresh_delay_ms(), 2_700);
    let lease = sample(DHCP_TIMER_UNKNOWN, DHCP_TIMER_UNKNOWN, DHCP_TIMER_UNKNOWN);
    assert_eq!(lease.refresh_delay_ms(), DEFAULT_REFRESH_MS);
}

#[test]
fn tiny_timers_clamp_to_the_floor() {
    let lease = sample(1, 2, 3);
    assert_eq!(lease.refresh_delay_ms(), MIN_REFRESH_MS);
    let lease = sample(0, 0, 0);
    assert_eq!(lease.refresh_delay_ms(), MIN_REFRESH_MS);
}

#[test]
fn a_refresh_with_the_same_config_does_not_apply_again() {
    let first = sample(1_000, 2_000, 3_000);
    let mut second = first.clone();
    second.renew_ms = 900;
    second.rebind_ms = 1_900;
    second.expires_ms = 2_900;
    assert!(first.same_config(&second));
    second.dns = vec![[192, 0, 2, 53]];
    assert!(!first.same_config(&second));
    second.dns = first.dns.clone();
    second.prefix = 25;
    assert!(!first.same_config(&second));
}

#[test]
fn a_lease_without_dns_is_detected() {
    let mut lease = sample(1_000, 2_000, 3_000);
    lease.dns.clear();
    assert!(!lease.has_dns());
    assert!(lease.config().servers.is_empty());
}
