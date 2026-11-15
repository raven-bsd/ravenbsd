// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the debounced PHY link tracker.

use super::*;
use crate::phy::{AUXSTS_RES_100FD, AUXSTS_RES_1000FD, BMSR_ACOMP, BMSR_LINK};

fn up_1000() -> (u16, u16) {
    (BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_1000FD)
}

#[test]
fn a_new_tracker_is_unknown() {
    let tracker = LinkTracker::new(2);
    assert_eq!(tracker.state(), LinkState::Unknown);
    assert_eq!(tracker.debounce(), 2);
}

#[test]
fn down_needs_the_debounce_samples_before_it_is_reported() {
    let mut tracker = LinkTracker::new(2);
    assert_eq!(tracker.observe(0, 0), Ok(None));
    assert_eq!(tracker.state(), LinkState::Unknown);
    assert_eq!(tracker.observe(0, 0), Ok(Some(LinkEvent::Down)));
    assert_eq!(tracker.state(), LinkState::Down);
}

#[test]
fn up_needs_the_debounce_samples_before_it_is_reported() {
    let mut tracker = LinkTracker::new(2);
    tracker.observe(0, 0).expect("down sample");
    tracker.observe(0, 0).expect("down sample");
    let (bmsr, auxsts) = up_1000();
    assert_eq!(tracker.observe(bmsr, auxsts), Ok(None));
    assert_eq!(tracker.state(), LinkState::Down);
    assert_eq!(
        tracker.observe(bmsr, auxsts),
        Ok(Some(LinkEvent::Up {
            speed: Speed::Thousand,
            duplex: Duplex::Full,
        }))
    );
    assert_eq!(
        tracker.state(),
        LinkState::Up {
            speed: Speed::Thousand,
            duplex: Duplex::Full,
        }
    );
}

#[test]
fn a_single_flap_does_not_change_the_stable_state() {
    let mut tracker = LinkTracker::new(2);
    let (bmsr, auxsts) = up_1000();
    tracker.observe(bmsr, auxsts).expect("up 1");
    tracker.observe(bmsr, auxsts).expect("up 2");
    assert!(matches!(tracker.state(), LinkState::Up { .. }));

    assert_eq!(tracker.observe(0, 0), Ok(None));
    assert_eq!(tracker.observe(bmsr, auxsts), Ok(None));
    assert_eq!(
        tracker.state(),
        LinkState::Up {
            speed: Speed::Thousand,
            duplex: Duplex::Full
        }
    );
}

#[test]
fn a_speed_change_reports_changed() {
    let mut tracker = LinkTracker::new(2);
    let (bmsr, auxsts) = up_1000();
    tracker.observe(bmsr, auxsts).expect("up 1");
    tracker.observe(bmsr, auxsts).expect("up 2");
    let changed = (BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_100FD);
    assert_eq!(tracker.observe(changed.0, changed.1), Ok(None));
    assert_eq!(
        tracker.observe(changed.0, changed.1),
        Ok(Some(LinkEvent::Changed {
            speed: Speed::Hundred,
            duplex: Duplex::Full,
        }))
    );
}

#[test]
fn an_unknown_resolution_does_not_change_the_state() {
    let mut tracker = LinkTracker::new(1);
    tracker.observe(0, 0).expect("down");
    assert_eq!(
        tracker.observe(BMSR_LINK, 0),
        Err(LinkError::UnknownResolution(0))
    );
    assert_eq!(tracker.state(), LinkState::Down);
}

#[test]
fn a_debounce_of_one_is_immediate() {
    let mut tracker = LinkTracker::new(1);
    assert_eq!(tracker.observe(0, 0), Ok(Some(LinkEvent::Down)));
    let (bmsr, auxsts) = up_1000();
    assert_eq!(
        tracker.observe(bmsr, auxsts),
        Ok(Some(LinkEvent::Up {
            speed: Speed::Thousand,
            duplex: Duplex::Full,
        }))
    );
}

#[test]
fn a_zero_debounce_is_clamped_to_one() {
    let tracker = LinkTracker::new(0);
    assert_eq!(tracker.debounce(), 1);
    let mut tracker = tracker;
    assert_eq!(tracker.observe(0, 0), Ok(Some(LinkEvent::Down)));
}

#[test]
fn an_unchanged_sample_returns_none() {
    let mut tracker = LinkTracker::new(1);
    tracker.observe(0, 0).expect("down");
    assert_eq!(tracker.observe(0, 0), Ok(None));
    assert_eq!(tracker.observe(0, 0), Ok(None));
}

#[test]
fn default_tracker_is_immediate() {
    let tracker = LinkTracker::default();
    assert_eq!(tracker.debounce(), 1);
    assert_eq!(tracker.state(), LinkState::Unknown);
}

#[test]
fn a_down_after_up_reports_down() {
    let mut tracker = LinkTracker::new(1);
    let (bmsr, auxsts) = up_1000();
    tracker.observe(bmsr, auxsts).expect("up");
    assert_eq!(tracker.observe(0, 0), Ok(Some(LinkEvent::Down)));
    assert_eq!(tracker.state(), LinkState::Down);
}
