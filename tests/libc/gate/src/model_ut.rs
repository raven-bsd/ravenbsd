// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Status aggregation and vocabulary tests.

use super::*;

#[test]
fn all_required_met_is_met() {
    assert_eq!(
        aggregate([Status::Met, Status::Met, Status::Info]),
        Status::Met
    );
}

#[test]
fn one_not_met_wins() {
    assert_eq!(
        aggregate([Status::Met, Status::NotImplemented, Status::Blocked]),
        Status::NotImplemented
    );
}

#[test]
fn blocked_without_not_met_is_blocked() {
    assert_eq!(aggregate([Status::Met, Status::Blocked]), Status::Blocked);
}

#[test]
fn info_never_gates() {
    assert_eq!(aggregate([Status::Info, Status::Info]), Status::Blocked);
    assert_eq!(aggregate([Status::Info, Status::Met]), Status::Met);
}

#[test]
fn status_spellings_are_exact() {
    assert_eq!(Status::Met.as_str(), "MET");
    assert_eq!(Status::NotImplemented.as_str(), "NOT MET");
    assert_eq!(Status::Blocked.as_str(), "BLOCKED");
    assert_eq!(Status::Info.as_str(), "INFO");
}

#[test]
fn verdict_requires_every_row() {
    let met = [Status::Met; 6];
    assert_eq!(verdict(met), Status::Met);
    let mut blocked = met;
    blocked[2] = Status::Blocked;
    assert_eq!(verdict(blocked), Status::Blocked);
    let mut failed = met;
    failed[4] = Status::NotImplemented;
    failed[5] = Status::Blocked;
    assert_eq!(verdict(failed), Status::NotImplemented);
}
