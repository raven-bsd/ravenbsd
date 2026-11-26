// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `raven-abi` contract: every kernel `errno` name is a libc `errno` name with the same
//! value, and the 62-value subset is exactly the values the kernel contract describes.

use std::collections::BTreeMap;

fn raven_abi_source() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../crates/raven-abi/src/errno.rs"
    );
    std::fs::read_to_string(path).expect("read raven-abi errno.rs")
}

/// Parse `NAME = N, "text";` rows out of the `errnos!` macro.
fn parse_raven_abi(text: &str) -> BTreeMap<String, u16> {
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with(char::is_uppercase) {
            continue;
        }
        let Some((name, rest)) = line.split_once(" = ") else {
            continue;
        };
        let Some(value) = rest.split(',').next() else {
            continue;
        };
        let Ok(value) = value.trim().parse::<u16>() else {
            continue;
        };
        if !line.ends_with(';')
            || !name
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            continue;
        }
        rows.insert(name.to_string(), value);
    }
    rows
}

#[test]
fn subset_is_the_observed_97_entries() {
    // The FreeBSD donor `sys/sys/errno.h` defines 1..97; S722 makes raven-abi carry that full
    // set, so the parsed row count is exactly the donor's error range.
    let rows = parse_raven_abi(&raven_abi_source());
    assert_eq!(rows.len(), 97, "raven-abi errno subset changed size");
}

#[test]
fn every_raven_abi_value_matches_the_libc_table() {
    let rows = parse_raven_abi(&raven_abi_source());
    for (name, value) in &rows {
        let found = raven_libc_abi::errno::ALL
            .iter()
            .find(|(candidate, _)| candidate == name)
            .unwrap_or_else(|| panic!("libc errno table lacks {name}"));
        assert_eq!(
            found.1,
            i128::from(*value),
            "{name} differs between raven-abi and raven-libc-abi"
        );
    }
}

#[test]
fn description_counter_equals_the_parsed_count() {
    let rows = parse_raven_abi(&raven_abi_source());
    let described = (1u16..=4095)
        .filter(|value| raven_abi::errno::Errno(*value).description() != "unknown error")
        .count();
    assert_eq!(described, rows.len());
}

#[test]
fn every_described_value_is_a_native_value_in_our_table() {
    let rows = parse_raven_abi(&raven_abi_source());
    for value in rows.values() {
        assert!(
            raven_libc_abi::errno::Errno(*value).name().is_some(),
            "raven-abi value {value} has no native libc name"
        );
        assert!(
            raven_libc_abi::errno::ALL
                .iter()
                .any(|(_, candidate)| *candidate == i128::from(*value)),
            "value {value} has no libc name"
        );
    }
}

#[test]
fn the_four_former_kernel_gaps_are_now_present() {
    let rows = parse_raven_abi(&raven_abi_source());
    for (name, value) in [
        ("ENOTBLK", 15),
        ("ETXTBSY", 26),
        ("EDEADLK", 11),
        ("ENOLCK", 77),
    ] {
        let found = raven_libc_abi::errno::ALL
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .unwrap_or_else(|| panic!("libc errno table lacks {name}"));
        assert_eq!(found.1, i128::from(value), "{name}");
        assert_eq!(rows.get(name), Some(&value), "raven-abi lacks {name}");
    }
}

#[test]
fn the_native_type_still_agrees_on_the_native_values() {
    // The dev-dependency is used directly as well as through the source parse, so a source
    // reformat cannot hide a semantic change.
    let rows = parse_raven_abi(&raven_abi_source());
    for &(name, value) in raven_libc_abi::errno::ALL {
        // The two alias rows have no separate `errnos!` entry in raven-abi.
        if let Some(native) = rows.get(name) {
            assert_eq!(i128::from(*native), value, "{name}");
        }
    }
    let eperm = raven_abi::errno::Errno::EPERM;
    assert_eq!(eperm.0, 1);
    assert_eq!(raven_libc_abi::errno::Errno(eperm.0).name(), Some("EPERM"));
    let again = raven_abi::errno::Errno::EAGAIN;
    assert_eq!(again.0, 35);
    assert_eq!(raven_libc_abi::errno::Errno(again.0).name(), Some("EAGAIN"));
}
