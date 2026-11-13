// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for tag mapping and the ring rule.

use super::*;

fn assignment(name: &str, ring: u8, reason: Option<&str>) -> Assignment {
    Assignment {
        name: name.to_string(),
        ring: Some(Ring::Number(ring)),
        ring_override: reason.map(str::to_string),
        ..Assignment::default()
    }
}

#[test]
fn every_listed_tag_maps_as_the_contract_says() {
    let cases = [
        ("ISO", "iso-c"),
        ("C90", "iso-c"),
        ("C99", "iso-c"),
        ("C11", "iso-c11"),
        ("C23", "iso-c23"),
        ("POSIX", "posix"),
        ("POSIX.1", "posix"),
        ("POSIX.1b", "posix"),
        ("POSIX.1-2008", "posix"),
        ("POSIX.2", "posix"),
        ("POSIX.1-2024", "posix-2024"),
        ("POSIX-1.2024", "posix-2024"),
        ("Unix98", "xsi"),
        ("X/Open", "xsi"),
        ("XPG4.2", "xsi"),
        ("SVID", "svid"),
        ("BSD", "bsd"),
        ("GNU", "gnu"),
        ("Linux", "linux"),
        ("Linux???", "linux"),
        ("TS 18661-4:2015", "ts18661"),
    ];
    for (tag, expected) in cases {
        assert_eq!(map_tag(tag), expected, "tag {tag}");
    }
}

#[test]
fn unknown_tags_lowercase_and_replace_spaces() {
    assert_eq!(map_tag("TS 18661-3:2015"), "ts-18661-3:2015");
    assert_eq!(map_tag("Some New Tag"), "some-new-tag");
}

#[test]
fn several_tags_are_sorted_and_deduplicated() {
    let tags = map_tags(["POSIX.1", "ISO", "POSIX", "GNU"]);
    assert_eq!(tags, vec!["gnu", "iso-c", "posix"]);
}

#[test]
fn a_standard_name_is_ring_zero_even_with_one_family() {
    let tags = vec!["posix".to_string()];
    let ring = compute_ring(&tags, &["linux".to_string()]);
    assert_eq!(ring.ring, 0);
    assert_eq!(ring.personality, None);
}

#[test]
fn two_families_are_ring_one() {
    let ring = compute_ring(&[], &["bsd".to_string(), "linux".to_string()]);
    assert_eq!(ring.ring, 1);
    assert_eq!(ring.personality, None);
}

#[test]
fn one_family_is_ring_two_with_a_personality() {
    let ring = compute_ring(&[], &["solaris".to_string()]);
    assert_eq!(ring.ring, 2);
    assert_eq!(ring.personality.as_deref(), Some("solaris"));
}

#[test]
fn no_family_is_ring_three() {
    let ring = compute_ring(&["gnu".to_string()], &[]);
    assert_eq!(ring.ring, 3);
    assert_eq!(ring.personality, None);
}

#[test]
fn families_of_maps_and_deduplicates_donors() {
    let families = families_of(&[
        "glibc".to_string(),
        "musl".to_string(),
        "illumos".to_string(),
        "mlibc".to_string(),
    ]);
    assert_eq!(families, vec!["linux", "solaris"]);
}

#[test]
fn an_assignment_with_the_same_ring_needs_no_reason() {
    let computed = ComputedRing {
        ring: 2,
        personality: Some("linux".to_string()),
    };
    let decision = decide_ring(&computed, Some(&assignment("epoll_create", 2, None))).unwrap();
    assert_eq!(decision.ring, Ring::Number(2));
    assert_eq!(decision.source, "assignment");
    assert_eq!(decision.ring_override, None);
}

#[test]
fn a_differing_assignment_requires_a_reason() {
    let computed = ComputedRing {
        ring: 1,
        personality: None,
    };
    let result = decide_ring(&computed, Some(&assignment("strlcpy", 2, None)));
    assert!(result.is_err());
    let result = decide_ring(&computed, Some(&assignment("strlcpy", 2, Some("because"))));
    assert_eq!(result.unwrap().ring_override.as_deref(), Some("because"));
}

#[test]
fn without_an_assignment_the_computed_ring_wins() {
    let computed = ComputedRing {
        ring: 3,
        personality: None,
    };
    let decision = decide_ring(&computed, None).unwrap();
    assert_eq!(decision.ring, Ring::Number(3));
    assert_eq!(decision.source, "computed");
}

#[test]
fn private_rings_pass_validation() {
    let assignment = Assignment {
        name: "x".to_string(),
        ring: Some(Ring::private()),
        ..Assignment::default()
    };
    let computed = ComputedRing::default();
    let decision = decide_ring(&computed, Some(&assignment)).unwrap();
    assert_eq!(decision.ring, Ring::private());
}
