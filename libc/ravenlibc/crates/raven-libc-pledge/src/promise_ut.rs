// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the promise names and parser.

use super::*;

#[test]
fn there_are_thirty_five_promises() {
    assert_eq!(Promise::ALL.len(), 35);
}

#[test]
fn every_name_round_trips() {
    for promise in Promise::ALL {
        assert_eq!(Promise::from_name(promise.name().as_bytes()), Some(promise));
    }
}

#[test]
fn every_index_is_unique_and_in_order() {
    for (index, promise) in Promise::ALL.iter().enumerate() {
        assert_eq!(promise.index() as usize, index);
    }
}

#[test]
fn the_order_matches_the_donor_when_present() {
    let path = "/home/bo/work/ravenos-workspace/ravenbsd-resources/openbsd/sys/sys/pledge.h";
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("SKIP: openbsd checkout absent");
        return;
    };
    let mut names = Vec::new();
    for line in text.lines() {
        let Some(start) = line.find("PLEDGE_") else {
            continue;
        };
        let rest = &line[start..];
        let Some(open) = rest.find('"') else {
            continue;
        };
        let rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else {
            continue;
        };
        names.push(rest[..close].to_string());
    }
    assert_eq!(names.len(), 35);
    for (index, name) in names.iter().enumerate() {
        assert_eq!(Promise::ALL[index].name(), name);
    }
}

#[test]
fn the_empty_string_is_the_empty_set() {
    assert_eq!(parse_promises(b""), Ok(PromiseSet(0)));
}

#[test]
fn one_name_parses() {
    let set = parse_promises(b"stdio").expect("stdio");
    assert!(set.contains(Promise::Stdio));
    assert_eq!(set.0, 1);
}

#[test]
fn several_names_parse() {
    let set = parse_promises(b"stdio rpath wpath").expect("set");
    assert!(set.contains(Promise::Stdio));
    assert!(set.contains(Promise::Rpath));
    assert!(set.contains(Promise::Wpath));
}

#[test]
fn duplicate_names_are_fine() {
    let set = parse_promises(b"stdio stdio stdio").expect("set");
    assert_eq!(set.0, 1);
}

#[test]
fn interior_runs_of_spaces_are_skipped() {
    let set = parse_promises(b"stdio    rpath").expect("set");
    assert!(set.contains(Promise::Stdio));
    assert!(set.contains(Promise::Rpath));
}

#[test]
fn a_leading_space_is_invalid() {
    assert_eq!(parse_promises(b" stdio"), Err(EINVAL));
}

#[test]
fn a_tab_is_part_of_the_name() {
    assert_eq!(parse_promises(b"std\tio"), Err(EINVAL));
}

#[test]
fn trailing_spaces_are_fine() {
    let set = parse_promises(b"stdio   ").expect("set");
    assert!(set.contains(Promise::Stdio));
}

#[test]
fn an_unknown_name_is_invalid() {
    assert_eq!(parse_promises(b"stdio unknown"), Err(EINVAL));
}

#[test]
fn a_string_at_the_limit_is_accepted() {
    let mut text = alloc::vec![b' '; MAX_PLEDGE_STRING];
    text[0] = b's';
    // A single long unknown token below the length limit is still EINVAL, not ENAMETOOLONG.
    assert_eq!(parse_promises(&text), Err(EINVAL));
}

#[test]
fn a_string_over_the_limit_is_nametoolong() {
    let text = alloc::vec![b'x'; MAX_PLEDGE_STRING + 1];
    assert_eq!(parse_promises(&text), Err(ENAMETOOLONG));
}

#[test]
fn a_space_only_string_is_invalid() {
    assert_eq!(parse_promises(b" "), Err(EINVAL));
}

#[test]
fn underscore_names_parse() {
    assert!(
        parse_promises(b"prot_exec")
            .expect("set")
            .contains(Promise::ProtExec)
    );
    assert!(
        parse_promises(b"getpw")
            .expect("set")
            .contains(Promise::Getpw)
    );
}

#[test]
fn set_operations_work() {
    let a = parse_promises(b"stdio rpath").expect("a");
    let b = parse_promises(b"rpath inet").expect("b");
    assert!(a.intersects(b));
    assert!(a.union(b).contains(Promise::Inet));
    assert_eq!(a.intersection(b), parse_promises(b"rpath").expect("rpath"));
    assert_eq!(a.difference(b), parse_promises(b"stdio").expect("stdio"));
    assert!(PromiseSet::empty().is_empty());
}

#[test]
fn a_known_name_is_not_confused_with_a_prefix() {
    assert_eq!(Promise::from_name(b"pro"), None);
    assert_eq!(Promise::from_name(b"prot_exec"), Some(Promise::ProtExec));
}
