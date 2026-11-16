// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the shared validation helpers.

use super::*;

#[test]
fn names_follow_the_fixture_contract() {
    for name in ["root", "alice", "_svc", "a.b-c", "A1"] {
        assert!(valid_name(name), "{name}");
    }
    for name in [
        "",
        "-bad",
        ".bad",
        "bad name",
        "bad:name",
        "bad\nname",
        "0123",
    ] {
        assert!(!valid_name(name), "{name}");
    }
    assert!(!valid_name(&"a".repeat(MAX_NAME + 1)));
}

#[test]
fn gecos_is_limited_and_colon_free() {
    assert!(valid_gecos(""));
    assert!(valid_gecos("Alice Example"));
    assert!(!valid_gecos("a:b"));
    assert!(!valid_gecos("a\nb"));
    assert!(!valid_gecos(&"g".repeat(MAX_GECOS + 1)));
}

#[test]
fn paths_are_absolute_and_cannot_escape() {
    assert!(valid_path(""));
    assert!(valid_path("/home/alice"));
    assert!(valid_path("/"));
    for path in [
        "home/alice",
        "/home/../etc",
        "/home/./alice",
        "/a:b",
        "/a\nb",
    ] {
        assert!(!valid_path(path), "{path}");
    }
}

#[test]
fn hashes_are_six_or_locked() {
    assert!(valid_hash(""));
    assert!(valid_hash("*"));
    assert!(valid_hash("!"));
    assert!(valid_hash("!$6$salt$digest"));
    assert!(valid_hash("$6$salt$digest"));
    assert!(!valid_hash("plaintext"));
    assert!(!valid_hash("$5$salt$digest"));
    assert!(!valid_hash("$6$salt$digest\n"));
}
