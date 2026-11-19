// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the RavenBSD `/etc/mounts` reader.

use super::*;

#[test]
fn parses_the_real_mount_table_shape() {
    let mounts = parse_ravenbsd_mounts("# comment\n/ initfs:\n/tmp ramfs:\n/dev/null null:\n");
    assert_eq!(mounts.len(), 3);
    assert_eq!(mounts[0].mount_dir, OsString::from("/"));
    assert_eq!(mounts[0].dev_name, "initfs:");
    assert_eq!(mounts[0].fs_type, "initfs");
    assert_eq!(mounts[0].dev_id, "/");
    assert_eq!(mounts[1].mount_dir, OsString::from("/tmp"));
    assert_eq!(mounts[1].fs_type, "ramfs");
    assert_eq!(mounts[2].mount_dir, OsString::from("/dev/null"));
    assert_eq!(mounts[2].fs_type, "null");
}

#[test]
fn empty_and_malformed_lines_are_skipped() {
    assert!(parse_ravenbsd_mounts("").is_empty());
    assert!(parse_ravenbsd_mounts("# only a comment\n").is_empty());
    assert!(parse_ravenbsd_mounts("/\n").is_empty());
}

#[test]
fn zero_fs_type_has_a_stable_unknown_label() {
    assert_eq!("UNKNOWN (0x0)", pretty_fstype(0));
}
