// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the channel limits.

use crate::{Channel, Limits, MAX_CAPACITY};

#[test]
fn default_limits_are_sixty_four() {
    let limits = Limits::default();
    assert_eq!(limits.receive_queue, 64);
    assert_eq!(limits.in_flight, 64);
}

#[test]
fn zero_limits_clamp_to_one() {
    let channel = Channel::with_limits(Limits::new(0, 0));
    assert_eq!(channel.limits(), Limits::new(1, 1));
}

#[test]
fn oversized_limits_clamp_to_max() {
    let channel = Channel::with_limits(Limits::new(usize::MAX, MAX_CAPACITY + 1));
    assert_eq!(channel.limits(), Limits::new(MAX_CAPACITY, MAX_CAPACITY));
}

#[test]
fn limits_within_range_are_kept() {
    let channel = Channel::with_limits(Limits::new(2, 3));
    assert_eq!(channel.limits(), Limits::new(2, 3));
}
