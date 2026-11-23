// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the inherited-fd map restore.

use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::fd::inherit::{FdMap, FdMapEntry};
use crate::fd::ofd::OfdKind;
use crate::platform::Platform;

/// The fake device tree and two allocated handles. FakeSys reports `st_ino = handle + 100`.
fn platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    Platform::new(fake)
}

fn restore(platform: &Platform<FakeSys>, kind: OfdKind, path: Option<&[u8]>) -> i32 {
    let _first = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("first handle");
    let second = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("slave handle");
    let handle = platform.get(second).expect("fd").handle();
    let mut entry = FdMapEntry::new(2, handle.0, kind, 0, fcntl::O_RDWR as u32);
    entry.path = path.map(<[u8]>::to_vec);
    let mut block = FdMap::empty();
    block.entries.push(entry);
    platform.restore_fdmap(&block).expect("restore");
    2
}

#[test]
fn restore_keeps_the_spawn_tty_kind_and_path() {
    // The spawn map carries the parent's kind and resolved slave path, so the child needs no
    // scheme call at startup (FreeBSD fdcopy copies the file object; ttyname reads its name).
    let platform = platform();
    let fd = restore(&platform, OfdKind::Tty, Some(b"/dev/pts/7"));
    assert_eq!(platform.get(fd).expect("fd").kind(), OfdKind::Tty);
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Ok(b"/dev/pts/7".to_vec())
    );
}

#[test]
fn restore_keeps_a_pathless_tty_unnamed() {
    // A pathless terminal stays unnamed and untouched: no FSTAT/probe of an inherited handle
    // happens during startup.
    let platform = platform();
    let fd = restore(&platform, OfdKind::Tty, None);
    assert_eq!(platform.get(fd).expect("fd").kind(), OfdKind::Tty);
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Err(crate::Errno::ENOTTY)
    );
}

#[test]
fn restore_leaves_an_unknown_handle_unprobed() {
    let platform = platform();
    let fd = restore(&platform, OfdKind::Unknown, None);
    assert_eq!(platform.get(fd).expect("fd").kind(), OfdKind::Unknown);
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Err(crate::Errno::ENOTTY)
    );
}
