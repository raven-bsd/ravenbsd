// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `*64` alias seed.

use super::*;

#[test]
fn every_seed_alias_has_a_base() {
    for (alias, base) in ALIASES {
        assert!(!alias.is_empty());
        assert!(!base.is_empty());
        assert!(alias.ends_with("64") || alias.contains("64"));
        assert_eq!(crate::alias64::base(alias), Some(*base));
    }
}

#[test]
fn no_duplicate_aliases() {
    for (index, (alias, _)) in ALIASES.iter().enumerate() {
        for (other, _) in ALIASES.iter().skip(index + 1) {
            assert_ne!(alias, other);
        }
    }
}

#[test]
fn pending_names_are_seed_rows() {
    for name in PENDING {
        assert!(ALIASES.iter().any(|(alias, _)| alias == name), "{name}");
        assert!(!active(name));
    }
}

#[test]
fn active_rows_are_not_pending() {
    for (alias, _) in ALIASES {
        assert_eq!(
            active(alias),
            !PENDING.contains(alias) && !EXISTING.contains(alias),
            "{alias}"
        );
    }
}

#[test]
fn seed_matches_ticket_5f() {
    let expected = [
        "stat64",
        "fstat64",
        "lstat64",
        "fstatat64",
        "statfs64",
        "fstatfs64",
        "open64",
        "openat64",
        "creat64",
        "truncate64",
        "ftruncate64",
        "pread64",
        "pwrite64",
        "lseek64",
        "mmap64",
        "sendfile64",
        "lockf64",
        "posix_fadvise64",
        "posix_fallocate64",
        "fopen64",
        "fseeko64",
        "readdir64",
        "getdents64",
    ];
    let actual: alloc::vec::Vec<&str> = ALIASES.iter().map(|(alias, _)| *alias).collect();
    assert_eq!(actual, expected);
}
