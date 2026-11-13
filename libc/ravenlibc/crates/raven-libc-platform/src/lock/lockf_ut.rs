// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `lockf` tests: the OpenBSD `F_TEST` choice and the `EACCES` adaptation.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_libc_abi::structs::Flock as FlockRecord;

use crate::lock::lockf::{RecordLock, lockf};

struct Fake {
    calls: Vec<(i32, FlockRecord)>,
    pid: i32,
    result: Option<(i16, i32)>,
}

impl Fake {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            pid: 7,
            result: None,
        }
    }
}

impl RecordLock for Fake {
    fn fcntl(&mut self, command: i32, lock: &mut FlockRecord) -> Result<(), Errno> {
        self.calls.push((command, *lock));
        if command == fcntl::F_GETLK
            && let Some((kind, pid)) = self.result
        {
            lock.l_type = kind;
            lock.l_pid = pid;
        }
        Ok(())
    }

    fn pid(&self) -> i32 {
        self.pid
    }
}

#[test]
fn f_lock_is_a_blocking_write_lock() {
    let mut fake = Fake::new();
    lockf(3, fcntl::F_LOCK, 0, &mut fake).expect("lock");
    assert_eq!(fake.calls.len(), 1);
    assert_eq!(fake.calls[0].0, fcntl::F_SETLKW);
    assert_eq!(fake.calls[0].1.l_type, fcntl::F_WRLCK as i16);
}

#[test]
fn f_tlock_is_a_non_blocking_write_lock() {
    let mut fake = Fake::new();
    lockf(3, fcntl::F_TLOCK, 10, &mut fake).expect("lock");
    assert_eq!(fake.calls[0].0, fcntl::F_SETLK);
    assert_eq!(fake.calls[0].1.l_type, fcntl::F_WRLCK as i16);
    assert_eq!(fake.calls[0].1.l_len, 10);
}

#[test]
fn f_ulock_unlocks() {
    let mut fake = Fake::new();
    lockf(3, fcntl::F_ULOCK, 0, &mut fake).expect("unlock");
    assert_eq!(fake.calls[0].0, fcntl::F_SETLK);
    assert_eq!(fake.calls[0].1.l_type, fcntl::F_UNLCK as i16);
}

#[test]
fn f_test_uses_a_write_lock_probe() {
    let mut fake = Fake::new();
    fake.result = Some((fcntl::F_UNLCK as i16, 0));
    lockf(3, fcntl::F_TEST, 0, &mut fake).expect("free");
    assert_eq!(fake.calls[0].0, fcntl::F_GETLK);
    assert_eq!(
        fake.calls[0].1.l_type,
        fcntl::F_WRLCK as i16,
        "A-sys-12: ask for a write lock so shared locks are seen"
    );
}

#[test]
fn f_test_reports_a_foreign_lock_as_eacces() {
    let mut fake = Fake::new();
    fake.result = Some((fcntl::F_WRLCK as i16, 99));
    assert_eq!(lockf(3, fcntl::F_TEST, 0, &mut fake), Err(Errno::EACCES));
}

#[test]
fn f_test_ignores_our_own_lock() {
    let mut fake = Fake::new();
    fake.result = Some((fcntl::F_WRLCK as i16, 7));
    assert_eq!(lockf(3, fcntl::F_TEST, 0, &mut fake), Ok(()));
}

#[test]
fn unknown_function_is_einval() {
    let mut fake = Fake::new();
    assert_eq!(lockf(3, 99, 0, &mut fake), Err(Errno::EINVAL));
    assert!(fake.calls.is_empty());
}
