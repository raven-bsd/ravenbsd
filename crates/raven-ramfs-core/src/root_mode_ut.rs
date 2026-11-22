// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Root-mode tests for the ramfs model.

use super::*;

#[test]
fn explicit_root_mode_is_applied() {
    let fs = Ramfs::with_root_mode(Limits::default(), 100, 0o1777);

    let stat = fs.stat(ROOT_INO).unwrap();

    assert_eq!(stat.mode & 0o170000, S_IFDIR);
    assert_eq!(stat.mode & 0o7777, 0o1777);
}

#[test]
fn default_root_mode_remains_0755() {
    let fs = Ramfs::new(100);

    let stat = fs.stat(ROOT_INO).unwrap();

    assert_eq!(stat.mode & 0o7777, 0o755);
}
