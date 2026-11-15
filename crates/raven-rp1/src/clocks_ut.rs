// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the clock tree.

use super::*;

fn tree() -> ClockTree {
    let mut tree = ClockTree::new();
    assert!(
        tree.insert(ClockSpec::new(0, 1_000_000_000, 1_500_000_000))
            .is_ok()
    );
    assert!(
        tree.insert(ClockSpec::new(1, 200_000_000, 200_000_000).with_parent(0))
            .is_ok()
    );
    assert!(
        tree.insert(ClockSpec::new(2, 50_000_000, 100_000_000).with_parent(1))
            .is_ok()
    );
    tree
}

#[test]
fn the_tree_holds_clocks_in_order() {
    let tree = tree();
    assert_eq!(tree.len(), 3);
    assert!(!tree.is_empty());
    assert_eq!(tree.get(0).map(|clock| clock.rate_hz), Some(1_000_000_000));
    assert_eq!(
        tree.iter().map(|clock| clock.id).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn duplicate_ids_and_a_full_tree_are_rejected() {
    let mut tree = tree();
    assert_eq!(
        tree.insert(ClockSpec::new(1, 1, 1)),
        Err(ClockError::Duplicate(1))
    );
    let mut full = ClockTree::new();
    for id in 0..MAX_CLOCKS as u8 {
        assert!(full.insert(ClockSpec::new(id, 1, 1)).is_ok());
    }
    assert_eq!(
        full.insert(ClockSpec::new(200, 1, 1)),
        Err(ClockError::Full)
    );
}

#[test]
fn a_rate_is_bounded_by_the_clock_and_its_parent() {
    let mut tree = tree();
    assert_eq!(tree.set_rate(1, 50_000_000), Ok(200_000_000));
    assert_eq!(tree.set_rate(1, 0), Err(ClockError::RateZero));
    assert_eq!(
        tree.set_rate(2, 200_000_000),
        Err(ClockError::RateTooHigh {
            id: 2,
            rate: 200_000_000,
            max: 100_000_000
        })
    );
    assert_eq!(
        tree.set_rate(2, 60_000_000),
        Err(ClockError::ParentTooSlow {
            id: 2,
            parent: 1,
            rate: 60_000_000
        })
    );
    assert_eq!(
        tree.set_rate(2, 40_000_000),
        Ok(50_000_000),
        "a child may run slower than its parent"
    );
}

#[test]
fn an_unknown_clock_or_parent_is_rejected() {
    let mut tree = tree();
    assert_eq!(tree.set_rate(9, 1), Err(ClockError::UnknownClock(9)));
    assert_eq!(tree.enable(9), Err(ClockError::UnknownClock(9)));
    assert_eq!(tree.disable(9), Err(ClockError::UnknownClock(9)));
    assert!(
        tree.insert(ClockSpec::new(3, 1, 1).with_parent(9)).is_ok(),
        "a parent link is not resolved at insertion time"
    );
    assert_eq!(tree.set_rate(3, 1), Err(ClockError::UnknownParent(9)));
    assert_eq!(tree.enable(3), Err(ClockError::UnknownParent(9)));
}

#[test]
fn a_child_needs_its_parent_enabled() {
    let mut tree = tree();
    assert!(tree.disable(2).is_ok());
    assert!(tree.disable(1).is_ok());
    assert!(tree.disable(0).is_ok());
    assert_eq!(tree.enable(1), Err(ClockError::ParentDisabled(0)));
    assert!(tree.enable(0).is_ok());
    assert!(tree.enable(1).is_ok());
    assert!(tree.enable(2).is_ok());
}

#[test]
fn a_parent_cannot_be_disabled_while_a_child_is_on() {
    let mut tree = tree();
    assert_eq!(tree.disable(0), Err(ClockError::ChildEnabled(1)));
    assert!(tree.disable(2).is_ok());
    assert!(tree.disable(1).is_ok());
    assert!(tree.disable(0).is_ok());
}
