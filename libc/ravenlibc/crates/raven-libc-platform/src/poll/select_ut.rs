// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `select` tests.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_libc_abi::structs::Timeval;

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::poll::{FD_SETSIZE, pselect, select};

fn set_bit(set: &mut [u64], fd: usize) {
    if let Some(word) = set.get_mut(fd / 64) {
        *word |= 1u64 << (fd % 64);
    }
}

fn bit(set: &[u64], fd: usize) -> bool {
    set.get(fd / 64)
        .is_some_and(|word| word & (1u64 << (fd % 64)) != 0)
}

#[test]
fn readable_file_sets_the_bit() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let mut read = [0u64; 16];
    set_bit(&mut read, fd as usize);
    let mut write = [0u64; 16];
    let mut except = [0u64; 16];
    let ready = select(
        &platform,
        fd + 1,
        &mut read,
        &mut write,
        &mut except,
        Some(&Timeval {
            tv_sec: 0,
            tv_usec: 0,
        }),
    )
    .expect("select");
    assert_eq!(ready, 1);
    assert!(bit(&read, fd as usize));
}

#[test]
fn writable_file_sets_the_bit() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform.open(b"/a", fcntl::O_RDWR as u32, 0).expect("open");
    let mut read = [0u64; 16];
    let mut write = [0u64; 16];
    set_bit(&mut write, fd as usize);
    let mut except = [0u64; 16];
    let ready = select(
        &platform,
        fd + 1,
        &mut read,
        &mut write,
        &mut except,
        Some(&Timeval {
            tv_sec: 0,
            tv_usec: 0,
        }),
    )
    .expect("select");
    assert_eq!(ready, 1);
    assert!(bit(&write, fd as usize));
}

#[test]
fn negative_nfds_is_einval() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        select(&platform, -1, &mut [], &mut [], &mut [], None),
        Err(Errno::EINVAL)
    );
}

#[test]
fn nfds_above_fd_setsize_is_einval() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        select(
            &platform,
            (FD_SETSIZE + 1) as i32,
            &mut [],
            &mut [],
            &mut [],
            None
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn invalid_timeout_is_einval() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        select(
            &platform,
            0,
            &mut [],
            &mut [],
            &mut [],
            Some(&Timeval {
                tv_sec: 0,
                tv_usec: 1_000_000,
            })
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn pselect_is_enosys_until_pg_26() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        pselect(&platform, 0, &mut [], &mut [], &mut [], None, None),
        Err(Errno::ENOSYS)
    );
}
