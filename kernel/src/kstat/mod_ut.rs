// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kstat model tests: stable names, typed values, bounded snapshots and checked aggregates.

use super::*;

#[test]
fn name_is_module_instance_name() {
    let name = KstatName::parse("dtrace:cpu0:produced").expect("name");
    assert_eq!(name.module().as_str(), "dtrace");
    assert_eq!(name.instance().as_str(), "cpu0");
    assert_eq!(name.name().as_str(), "produced");
    assert_eq!(name.to_string(), "dtrace:cpu0:produced");
}

#[test]
fn name_rejects_missing_and_extra_components() {
    assert_eq!(KstatName::parse("dtrace:cpu0"), Err(Error::InvalidName));
    assert_eq!(
        KstatName::parse("dtrace:cpu0:produced:extra"),
        Err(Error::InvalidName)
    );
    assert_eq!(
        KstatName::parse("dtrace::produced"),
        Err(Error::EmptyComponent)
    );
}

#[test]
fn name_rejects_too_long_or_invalid_components() {
    let long = "a".repeat(KSTAT_STRLEN + 1);
    assert_eq!(
        KstatName::parse(&format!("dtrace:cpu0:{long}")),
        Err(Error::NameTooLong)
    );
    assert_eq!(
        KstatName::parse("Dtrace:cpu0:produced"),
        Err(Error::InvalidCharacter)
    );
    assert_eq!(
        KstatName::parse("dtrace:cpu 0:produced"),
        Err(Error::InvalidCharacter)
    );
}

#[test]
fn text_values_allow_printable_ascii() {
    let text = Text::parse_value("a b@c").expect("text");
    assert_eq!(text.as_str(), "a b@c");
    assert_eq!(text.len(), 5);
    assert!(!text.is_empty());
    assert_eq!(Text::parse_value(""), Err(Error::EmptyComponent));
    assert_eq!(Text::parse_value("line\n"), Err(Error::InvalidCharacter));
}

#[test]
fn values_expose_their_donor_type() {
    assert_eq!(Value::Char(b'x').kind(), KstatType::Char);
    assert_eq!(Value::Int32(-1).kind(), KstatType::Int32);
    assert_eq!(Value::Uint32(1).kind(), KstatType::Uint32);
    assert_eq!(Value::Int64(-1).kind(), KstatType::Int64);
    assert_eq!(Value::Uint64(1).kind(), KstatType::Uint64);
    assert_eq!(
        Value::Text(Text::parse_value("ok").expect("text")).kind(),
        KstatType::Text
    );
    assert_eq!(KstatType::Text.as_str(), "string");
    assert_eq!(KstatUnit::Nanoseconds.as_str(), "ns");
}

#[test]
fn snapshot_push_is_bounded() {
    let name = KstatName::parse("dtrace:cpu0:produced").expect("name");
    let record = Kstat::new(name, Value::Uint64(1), KstatUnit::Count);
    let mut snapshot = Snapshot::<2>::new();
    assert!(snapshot.is_empty());
    snapshot.push(record).expect("first");
    snapshot.push(record).expect("second");
    assert_eq!(snapshot.push(record), Err(Error::Full));
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot.get(0), Some(&record));
    snapshot.clear();
    assert!(snapshot.is_empty());
}

#[test]
fn snapshot_iterates_in_insertion_order() {
    let mut snapshot = Snapshot::<4>::new();
    for value in [3u64, 1, 2] {
        let name = KstatName::parse("dtrace:cpu0:produced").expect("name");
        snapshot
            .push(Kstat::new(name, Value::Uint64(value), KstatUnit::Count))
            .expect("push");
    }
    let values: [u64; 3] = [3, 1, 2];
    for (record, expected) in snapshot.iter().zip(values) {
        assert_eq!(record.value(), Value::Uint64(expected));
    }
}

#[test]
fn checked_sum_rejects_overflow() {
    assert_eq!(checked_sum([1u64, 2, 3]), Ok(6));
    assert_eq!(checked_sum([u64::MAX, 1]), Err(Error::Overflow));
}
