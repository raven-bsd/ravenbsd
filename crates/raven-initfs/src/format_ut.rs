// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Format path-rule tests.

use super::{NAME_MAX, PathError, validate_path};

#[test]
fn valid_paths_are_accepted() {
    for path in [
        "/init",
        "/bin/hello",
        "/sbin/init",
        "/a/b-c_d.ef",
        "/bin/日本語",
        "/a b",
    ] {
        assert_eq!(validate_path(path), Ok(()), "{path}");
    }
}

#[test]
fn path_must_be_absolute() {
    assert_eq!(validate_path("bin/hello"), Err(PathError::NotAbsolute));
    assert_eq!(validate_path(""), Err(PathError::NotAbsolute));
}

#[test]
fn root_alone_and_trailing_slash_are_rejected() {
    assert_eq!(validate_path("/"), Err(PathError::EmptyComponent));
    assert_eq!(validate_path("/bin/"), Err(PathError::EmptyComponent));
    assert_eq!(validate_path("//"), Err(PathError::EmptyComponent));
}

#[test]
fn empty_components_are_rejected() {
    assert_eq!(validate_path("/bin//sh"), Err(PathError::EmptyComponent));
    assert_eq!(validate_path("/a//b/c"), Err(PathError::EmptyComponent));
}

#[test]
fn dot_components_are_rejected() {
    assert_eq!(validate_path("/."), Err(PathError::InvalidComponent));
    assert_eq!(validate_path("/.."), Err(PathError::InvalidComponent));
    assert_eq!(validate_path("/a/./b"), Err(PathError::InvalidComponent));
    assert_eq!(validate_path("/a/../b"), Err(PathError::InvalidComponent));
}

#[test]
fn nul_bytes_are_rejected() {
    assert_eq!(validate_path("/a\0b"), Err(PathError::Nul));
}

#[test]
fn path_length_limit_is_bytes() {
    let max = format!("/{}", "a".repeat(NAME_MAX - 1));
    assert_eq!(max.len(), NAME_MAX);
    assert_eq!(validate_path(&max), Ok(()));

    let too_long = format!("/{}", "a".repeat(NAME_MAX));
    assert_eq!(too_long.len(), NAME_MAX + 1);
    assert_eq!(validate_path(&too_long), Err(PathError::TooLong));
}
