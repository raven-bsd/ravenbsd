// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end process lifetime over the host fake kernel.

#[path = "support/fake_sys.rs"]
mod support;

use raven_libc_platform::fd::ofd::OfdKind;

#[test]
fn open_dup_read_close() {
    let world = support::ProcessWorld::new();
    assert_eq!(world.open_dup_read_close(), b"hello, process");
    assert_eq!(world.platform.open_count(), 0);
}

#[test]
fn mmap_and_nanosleep_cross_the_seam() {
    let world = support::ProcessWorld::new();
    let address = world.mmap_and_sleep();
    assert!(address != 0);
    assert_eq!(world.platform.backend().sleeps(), vec![1_000_000]);
}

#[test]
fn tty_kind_is_recorded_at_open() {
    let world = support::ProcessWorld::new();
    assert_eq!(world.tty_kind(), OfdKind::Tty);
}

#[test]
fn working_directory_survives_the_process() {
    let world = support::ProcessWorld::new();
    world.platform.chdir(b"/tmp").expect("chdir");
    assert_eq!(world.platform.cwd(), b"/tmp");
}
