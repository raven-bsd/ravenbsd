// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the file-backed POSIX shared memory seam.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::mem::shm::{path, shm_open, shm_unlink};
use crate::platform::Platform;

#[test]
fn names_map_into_the_tmp_prefix() {
    assert_eq!(path(b"/demo").expect("name"), b"/tmp/shm-demo".to_vec());
    assert_eq!(
        path(b"/a-b_c.1").expect("name"),
        b"/tmp/shm-a-b_c.1".to_vec()
    );
}

#[test]
fn bad_names_are_einval() {
    for name in [
        b"".as_slice(),
        b"demo",
        b"/",
        b"/a/b",
        b"/.",
        b"/..",
        b"/a\0b",
    ] {
        assert_eq!(path(name), Err(Errno::EINVAL), "name {name:?}");
    }
}

#[test]
fn create_reopen_and_unlink_round_trip() {
    let fake = FakeSys::new();
    fake.dir(b"/").dir(b"/tmp");
    let platform = Platform::new(fake);
    let create = (fcntl::O_RDWR | fcntl::O_CREAT) as u32;
    let fd = shm_open(&platform, b"/demo", create, 0o600).expect("create");
    assert!(fd >= 0);
    let reopen = shm_open(&platform, b"/demo", fcntl::O_RDWR as u32, 0).expect("reopen");
    assert!(reopen >= 0);
    assert_eq!(shm_unlink(&platform, b"/demo"), Ok(()));
    assert_eq!(
        shm_open(&platform, b"/demo", fcntl::O_RDWR as u32, 0),
        Err(Errno::ENOENT)
    );
}

#[test]
fn write_only_and_bad_names_are_rejected_before_open() {
    let fake = FakeSys::new();
    fake.dir(b"/tmp");
    let platform = Platform::new(fake);
    assert_eq!(
        shm_open(&platform, b"/demo", fcntl::O_WRONLY as u32, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        shm_open(
            &platform,
            b"bad",
            (fcntl::O_RDWR | fcntl::O_CREAT) as u32,
            0o600
        ),
        Err(Errno::EINVAL)
    );
}
