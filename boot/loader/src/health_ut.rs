// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the boot-health marker.

use super::{MARKER_BYTES, Marker, STATE_HEALTHY, STATE_TRIAL, decode, encode};
use crate::config::Slot;

#[test]
fn marker_round_trips_for_both_slots() {
    for slot in [Slot::A, Slot::B] {
        let sector = encode(slot, 0x1234_5678, STATE_HEALTHY);
        let marker = decode(&sector).unwrap();
        assert_eq!(
            marker,
            Marker {
                slot,
                token: 0x1234_5678,
                state: STATE_HEALTHY,
            }
        );
        assert!(marker.confirms(slot, 0x1234_5678));
        assert!(!marker.confirms(slot, 0x1234_5679));
        assert!(!marker.confirms(slot.other(), 0x1234_5678));
    }
}

#[test]
fn trial_state_does_not_confirm() {
    let sector = encode(Slot::B, 7, STATE_TRIAL);
    let marker = decode(&sector).unwrap();
    assert_eq!(marker.state, STATE_TRIAL);
    assert!(!marker.confirms(Slot::B, 7));
}

#[test]
fn bad_magic_version_slot_and_checksum_are_rejected() {
    let good = encode(Slot::A, 1, STATE_HEALTHY);

    let mut bad = good;
    bad[0] ^= 0xff;
    assert!(decode(&bad).is_none());

    let mut bad = good;
    bad[8] = 2;
    assert!(decode(&bad).is_none());

    let mut bad = good;
    bad[10] = 2;
    assert!(decode(&bad).is_none());

    let mut bad = good;
    bad[12] ^= 0xff;
    assert!(decode(&bad).is_none());

    assert!(decode(&good[..23]).is_none());
    assert!(decode(&[0u8; MARKER_BYTES]).is_none());
}
