// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for inherited-fd adoption at `restore_fdmap`.

use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::fd::inherit::{FdMap, FdMapEntry};
use crate::fd::ofd::OfdKind;
use crate::platform::Platform;

/// The fake device tree: `/dev/tty` is a pty slave endpoint; `/dev/console` is a legacy console.
///
/// FakeSys reports `st_ino = handle + 100`. Handles start at 5, so the second open is handle 6
/// and its inode 106 is the pty slave identity `2*52 + 2` -> `/dev/pts/52`.
fn platform_with_pty(legacy_console: bool) -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    if legacy_console {
        fake.tty_legacy(b"/dev/console");
    }
    Platform::new(fake)
}

fn restore_pathless(platform: &Platform<FakeSys>, node: &[u8], kind: OfdKind) -> i32 {
    let _first = platform
        .open(node, fcntl::O_RDWR as u32, 0)
        .expect("first handle");
    let second = platform
        .open(node, fcntl::O_RDWR as u32, 0)
        .expect("slave handle");
    let handle = platform.get(second).expect("fd").handle();
    let mut block = FdMap::empty();
    block
        .entries
        .push(FdMapEntry::new(2, handle.0, kind, 0, fcntl::O_RDWR as u32));
    platform.restore_fdmap(&block).expect("restore");
    2
}

#[test]
fn restore_adopts_a_spawn_pty_slave_from_its_endpoint() {
    // The version-5 spawn map carries the parent's real kind but no path. The child recovers the
    // public name from the endpoint (FreeBSD fdcopy keeps the file object; ttyname uses fstat).
    let platform = platform_with_pty(false);
    let fd = restore_pathless(&platform, b"/dev/tty", OfdKind::Tty);
    assert_eq!(platform.get(fd).expect("fd").kind(), OfdKind::Tty);
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Ok(b"/dev/pts/52".to_vec())
    );
}

#[test]
fn restore_leaves_an_unknown_handle_unprobed() {
    // An entry the parent did not classify must not enter its server: the startup path is
    // exercised for every inherited descriptor, including sockets.
    let platform = platform_with_pty(false);
    let fd = restore_pathless(&platform, b"/dev/tty", OfdKind::Unknown);
    assert_eq!(platform.get(fd).expect("fd").kind(), OfdKind::Unknown);
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Err(crate::Errno::ENOTTY)
    );
}

#[test]
fn restore_adopts_a_pathless_exec_tty_slave() {
    // The exec map carries the kind but the path can be missing; the device identity is still
    // the correct source of the name.
    let platform = platform_with_pty(false);
    let fd = restore_pathless(&platform, b"/dev/tty", OfdKind::Tty);
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Ok(b"/dev/pts/52".to_vec())
    );
}

#[test]
fn restore_does_not_name_a_legacy_console_like_a_pty() {
    // The console answers GET_MODE only (GET_TERMIOS is ENOSYS), so it is not a pty endpoint and
    // must not acquire a made-up /dev/pts name from its inode.
    let platform = platform_with_pty(true);
    let fd = restore_pathless(&platform, b"/dev/console", OfdKind::Tty);
    assert_eq!(
        crate::compat::ttyname(&platform, fd),
        Err(crate::Errno::ENOTTY)
    );
}
