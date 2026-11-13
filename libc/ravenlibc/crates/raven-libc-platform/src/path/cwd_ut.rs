// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! cwd cell tests.

use crate::path::DEFAULT_LIMITS;
use crate::path::cwd::Cwd;

#[test]
fn starts_at_root() {
    assert_eq!(Cwd::new().get(), b"/");
}

#[test]
fn pwd_absolute_is_used() {
    let cwd = Cwd::from_pwd(Some(b"/usr/local"));
    assert_eq!(cwd.get(), b"/usr/local");
}

#[test]
fn pwd_relative_is_ignored() {
    let cwd = Cwd::from_pwd(Some(b"usr/local"));
    assert_eq!(cwd.get(), b"/");
}

#[test]
fn pwd_is_normalised() {
    let cwd = Cwd::from_pwd(Some(b"/a//b/../c"));
    assert_eq!(cwd.get(), b"/a/c");
}

#[test]
fn resolve_uses_the_cwd() {
    let cwd = Cwd::from_pwd(Some(b"/tmp"));
    let resolved = cwd
        .resolve(b"a/b", false, &DEFAULT_LIMITS)
        .expect("resolved");
    assert_eq!(resolved.abs, b"/tmp/a/b");
}

#[test]
fn set_replaces_the_path() {
    let cwd = Cwd::new();
    cwd.set(b"/home/user");
    assert_eq!(cwd.get(), b"/home/user");
}
