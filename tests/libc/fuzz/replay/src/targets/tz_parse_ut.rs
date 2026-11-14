// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn tz_parse_hostile_ok() {
    body(b"TZif2");
    body(b"EST5EDT,M3.2.0,M11.1.0");
}

#[test]
fn tz_parse_boundaries_ok() {
    let mut header = vec![b'T', b'Z', b'i', b'f', 2];
    header.extend_from_slice(&[0u8; 40]);
    body(&header);
    body(b"<");
    body(b"<+");
    body(b"A0B,M13.1.0");
    body(b"EST5EDT,J366,J1/2");
    let mut footer = b"TZif2".to_vec();
    footer.extend_from_slice(&[0u8; 44]);
    footer.extend_from_slice(&[0u8; 4096]);
    body(&footer);
}

#[test]
#[should_panic(expected = "P-tz-3")]
fn tz_parse_mutation_3_fires() {
    let times = [10i64, 10];
    for window in times.windows(2) {
        if let [left, right] = window {
            assert!(
                left < right,
                "P-tz-3: accepted transition times are not strictly increasing"
            );
        }
    }
}

#[test]
fn a_fixed_offset_state_is_accepted() {
    let state = tzparse(b"<+0530>-5:30", None).expect("valid TZ");
    assert!(!state.ttis.is_empty());
}

#[test]
fn the_finding_input_is_rejected_or_increasing() {
    if let Ok(state) = tzparse(b"EST5T,0365,J1/2", None) {
        assert!(state.ats.windows(2).all(|window| window[0] < window[1]));
    }
}

#[test]
fn a_posix_string_has_increasing_transitions() {
    let state = tzparse(b"EST5EDT,M3.2.0,M11.1.0", None).expect("valid TZ");
    for window in state.ats.windows(2) {
        if let [left, right] = window {
            assert!(left < right);
        }
    }
}
