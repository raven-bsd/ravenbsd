// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the error diagnostics.

use super::*;

#[test]
fn every_variant_has_a_description() {
    let cases = [
        (DbError::UnknownUser, "unknown user"),
        (DbError::UnknownGroup, "unknown group"),
        (DbError::NoFreeId, "no free id in range"),
        (DbError::TooManyGroups, "too many supplementary groups"),
        (DbError::InvalidName, "invalid account name"),
        (DbError::InvalidHash, "invalid password hash"),
        (DbError::Busy, "account database is locked"),
        (DbError::CommitFailed, "account database update failed"),
        (DbError::Io(Errno::ENOENT), "no such file or directory"),
    ];
    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn file_names_are_stable() {
    assert_eq!(DbFile::Passwd.name(), "passwd");
    assert_eq!(DbFile::Group.name(), "group");
    assert_eq!(DbFile::Shadow.name(), "shadow");
}
