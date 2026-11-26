// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the root pivot.

use super::*;

fn table() -> MountTable {
    let mut table = MountTable::new();
    table.mount_defaults().unwrap();
    table.mount("/root", "ufs:").unwrap();
    table
}

#[test]
fn pivot_replaces_the_root_and_keeps_every_other_mount() {
    let before = table();
    let after = pivot_root(&before, "ufs:").unwrap();
    assert_eq!(
        after
            .mounts()
            .find(|mount| mount.path == "/")
            .map(|mount| mount.scheme.as_str()),
        Some("ufs:")
    );
    for path in ["/tmp", "/dev/null", "/dev/zero", "/dev/random", "/root"] {
        assert!(
            after.mounts().any(|mount| mount.path == path),
            "{path} survived"
        );
    }
    assert_eq!(after.mounts().count(), before.mounts().count());
}

#[test]
fn pivoting_to_the_current_root_is_allowed() {
    let before = table();
    let after = pivot_root(&before, "initfs:").unwrap();
    assert_eq!(
        after
            .mounts()
            .find(|mount| mount.path == "/")
            .map(|mount| mount.scheme.as_str()),
        Some("initfs:")
    );
}

#[test]
fn an_invalid_scheme_is_rejected_without_a_table() {
    let before = table();
    assert!(pivot_root(&before, "ufs").is_err());
}
