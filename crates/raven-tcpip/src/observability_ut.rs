// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the counter and property types.

use super::*;
use alloc::vec::Vec;

#[test]
fn counters_default_to_zero() {
    let counters = Counters::default();
    for name in Counters::names() {
        assert_eq!(counters.get(name), Some(0), "{name}");
    }
}

#[test]
fn counters_get_unknown_name_is_none() {
    assert_eq!(Counters::default().get("tcp.not_a_counter"), None);
}

#[test]
fn counters_names_are_unique() {
    let mut names = Counters::names().to_vec();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count);
}

#[test]
fn counters_reset_zeroes_a_moved_counter() {
    let mut counters = Counters {
        active_opens: 7,
        in_bytes: 99,
        ..Counters::default()
    };
    counters.reset();
    assert_eq!(counters.active_opens, 0);
    assert_eq!(counters.in_bytes, 0);
}

#[test]
fn counters_names_cover_the_whole_struct() {
    assert_eq!(Counters::names().len(), 84);
}

#[test]
fn property_names_are_unique() {
    let mut names: Vec<&str> = PROPERTIES.iter().map(|info| info.name).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count);
}

#[test]
fn property_table_is_non_empty() {
    assert_eq!(PROPERTIES.len(), 58);
}

#[test]
fn property_ranges_are_sane() {
    for info in PROPERTIES {
        assert!(info.min <= info.max, "{}", info.name);
        assert!(
            info.min <= info.default && info.default <= info.max,
            "{}",
            info.name
        );
    }
}

#[test]
fn bool_properties_are_zero_or_one() {
    for info in PROPERTIES {
        if info.kind == PropKind::Bool {
            assert_eq!(info.min, 0, "{}", info.name);
            assert_eq!(info.max, 1, "{}", info.name);
        }
    }
}

#[test]
fn unit_names_render_as_spec_spellings() {
    assert_eq!(Unit::Ms.as_str(), "ms");
    assert_eq!(Unit::Bytes.as_str(), "bytes");
    assert_eq!(Unit::Port.as_str(), "port");
}

#[test]
fn stability_markers_are_exhaustive() {
    assert_ne!(Stability::Committed, Stability::Private);
}

#[test]
fn tunables_default_matches_table_defaults() {
    let tunables = Tunables::default();
    for info in PROPERTIES {
        assert_eq!(tunables.get(info.name), Some(info.default), "{}", info.name);
    }
}

#[test]
fn tunables_set_and_get_round_trip() {
    let mut tunables = Tunables::default();
    assert!(tunables.set("tcp.time_wait_interval", 1234));
    assert_eq!(tunables.get("tcp.time_wait_interval"), Some(1234));
}

#[test]
fn tunables_reject_unknown_name() {
    let mut tunables = Tunables::default();
    assert!(!tunables.set("tcp.nope", 1));
    assert_eq!(tunables.get("tcp.nope"), None);
}

#[test]
fn option_flags_default_to_nothing_negotiated() {
    let flags = OptionFlags::default();
    assert_eq!(flags.wscale, None);
    assert!(!flags.timestamps);
    assert!(!flags.sack);
    assert!(!flags.ecn);
    assert_eq!(flags.ts_recent_age_ms, 0);
}
