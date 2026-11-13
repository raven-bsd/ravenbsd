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
fn subset_is_the_observed_54_entries() {
    // Doc 14 3.A.4 and ticket C36 call this a 62-value subset; the contract crate as committed
    // has 54 `NAME = N, "text";` rows (OBSERVED). The number is asserted so a silent change is
    // caught, and the report records the discrepancy.
    let rows = parse_raven_abi(&raven_abi_source());
    assert_eq!(rows.len(), 54, "raven-abi errno subset changed size");
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
fn every_described_value_is_a_linux_value_in_our_table() {
    let rows = parse_raven_abi(&raven_abi_source());
    for value in rows.values() {
        assert!(
            raven_libc_abi::errno::Errno(*value).is_linux(),
            "raven-abi value {value} is not a Linux errno in the libc table"
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
fn the_four_missing_kernel_names_are_present() {
    for (name, value) in [
        ("ENOTBLK", 15),
        ("ETXTBSY", 26),
        ("EDEADLK", 35),
        ("ENOLCK", 37),
    ] {
        let found = raven_libc_abi::errno::ALL
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .unwrap_or_else(|| panic!("libc errno table lacks {name}"));
        assert_eq!(found.1, i128::from(value), "{name}");
    }
    let rows = parse_raven_abi(&raven_abi_source());
    for name in ["ENOTBLK", "ETXTBSY", "EDEADLK", "ENOLCK"] {
        assert!(
            !rows.contains_key(name),
            "raven-abi unexpectedly has {name}"
        );
    }
}

#[test]
fn the_raven_abi_type_still_agrees_on_the_common_values() {
    // The dev-dependency is used directly as well as through the source parse, so a source
    // reformat cannot hide a semantic change.
    for &(name, value) in raven_linux_abi::errno::ERRNOS {
        let found = raven_libc_abi::errno::ALL
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .unwrap_or_else(|| panic!("libc errno table lacks {name}"));
        assert_eq!(found.1, i128::from(value), "{name}");
    }
    let eperm = raven_abi::errno::Errno::EPERM;
    assert_eq!(eperm.0, 1);
    assert_eq!(raven_libc_abi::errno::Errno(eperm.0).name(), Some("EPERM"));
}
