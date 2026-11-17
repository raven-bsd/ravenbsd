// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the sampling decision.

use crate::{Sampling, TraceId};

#[test]
fn never_and_always() {
    assert!(!Sampling::Never.decide(TraceId::ZERO));
    assert!(!Sampling::Never.decide(TraceId::from_raw(1)));
    assert!(Sampling::Always.decide(TraceId::ZERO));
    assert!(Sampling::Always.decide(TraceId::from_raw(123)));
}

#[test]
fn every_samples_one_in_n_by_id() {
    let policy = Sampling::Every(4);
    assert!(policy.decide(TraceId::from_raw(0)));
    assert!(policy.decide(TraceId::from_raw(4)));
    assert!(policy.decide(TraceId::from_raw(8)));
    assert!(!policy.decide(TraceId::from_raw(1)));
    assert!(!policy.decide(TraceId::from_raw(3)));
}

#[test]
fn every_zero_never_samples() {
    assert!(!Sampling::Every(0).decide(TraceId::from_raw(0)));
    assert!(!Sampling::Every(0).decide(TraceId::from_raw(4)));
}

#[test]
fn decision_is_stable_per_id() {
    // Same id -> same decision, so every span of a trace (sharing the id) agrees.
    let policy = Sampling::Every(5);
    let id = TraceId::from_raw(25);
    assert_eq!(policy.decide(id), policy.decide(id));
    assert!(policy.decide(id));
}
