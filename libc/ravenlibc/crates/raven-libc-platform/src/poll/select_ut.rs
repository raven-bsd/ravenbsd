// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `select` tests.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_libc_abi::structs::{Timespec, Timeval};

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::poll::{FD_SETSIZE, MaskOps, pselect, select};

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

const MASK_A: u64 = 0x40;
const MASK_B: u64 = 0x02;

struct FakeMasks {
    current: u64,
    installs: u32,
    restores: u32,
}

impl FakeMasks {
    fn new(current: u64) -> Self {
        Self {
            current,
            installs: 0,
            restores: 0,
        }
    }
}

impl MaskOps for FakeMasks {
    fn install(&mut self, set: u64) -> Result<u64, Errno> {
        let previous = self.current;
        self.current = set;
        self.installs += 1;
        Ok(previous)
    }

    fn restore(&mut self, old: u64) -> Result<(), Errno> {
        self.current = old;
        self.restores += 1;
        Ok(())
    }
}

#[test]
fn pselect_restores_the_mask_after_the_timeout() {
    let platform = Platform::new(FakeSys::new());
    let mut masks = FakeMasks::new(MASK_A);
    let ready = pselect(
        &platform,
        0,
        &mut [],
        &mut [],
        &mut [],
        Some(&Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }),
        Some(&[MASK_B]),
        &mut masks,
    )
    .expect("pselect");
    assert_eq!(ready, 0);
    assert_eq!(masks.current, MASK_A);
    assert_eq!(masks.installs, 1);
    assert_eq!(masks.restores, 1);
}

#[test]
fn pselect_restores_the_mask_after_an_argument_error() {
    let platform = Platform::new(FakeSys::new());
    let mut masks = FakeMasks::new(MASK_A);
    let result = pselect(
        &platform,
        -1,
        &mut [],
        &mut [],
        &mut [],
        Some(&Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }),
        Some(&[MASK_B]),
        &mut masks,
    );
    assert_eq!(result, Err(Errno::EINVAL));
    assert_eq!(masks.current, MASK_A);
    assert_eq!(masks.installs, 1);
    assert_eq!(masks.restores, 1);
}

#[test]
fn pselect_without_a_mask_does_not_touch_the_mask() {
    let platform = Platform::new(FakeSys::new());
    let mut masks = FakeMasks::new(MASK_A);
    let ready = pselect(
        &platform,
        0,
        &mut [],
        &mut [],
        &mut [],
        Some(&Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        }),
        None,
        &mut masks,
    )
    .expect("pselect");
    assert_eq!(ready, 0);
    assert_eq!(masks.current, MASK_A);
    assert_eq!(masks.installs, 0);
    assert_eq!(masks.restores, 0);
}
