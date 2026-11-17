// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn schema_is_one() {
    assert_eq!(SCHEMA, 1);
}

#[test]
fn default_paths_are_repository_relative() {
    assert!(!DEFAULT_WORLD.starts_with('/'));
    assert!(!DEFAULT_RESULTS.starts_with('/'));
    assert!(!DEFAULT_OUT.starts_with('/'));
    assert!(DEFAULT_WORLD.ends_with("world.toml"));
}
