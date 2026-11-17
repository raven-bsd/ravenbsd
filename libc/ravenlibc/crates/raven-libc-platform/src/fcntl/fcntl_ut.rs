// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the record-lock translation.

use super::*;
use crate::backend::FakeSys;
use raven_libc_abi::fcntl;

fn platform() -> Platform<FakeSys> {
    let fake = FakeSys::new();
    fake.dir(b"/");
    fake.file(b"/a", b"0123456789");
    Platform::new(fake)
}

fn lock(kind: i16, start: i64, len: i64) -> Flock {
    Flock {
        l_type: kind,
        l_whence: 0,
        l_start: start,
        l_len: len,
        l_pid: 0,
    }
}

#[test]
fn setlk_forwards_the_handle_and_wire_fields() {
    let platform = platform();
    let fd = platform.open(b"/a", fcntl::O_RDWR as u32, 0).expect("open");
    let handle = platform.get(fd).expect("fd").handle();
    let mut value = lock(F_WRLCK as i16, 4, 8);
    assert_eq!(record_lock(&platform, fd, F_SETLK, &mut value), Ok(()));
    let calls = platform.backend().record_lock_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, handle);
    assert_eq!(calls[0].1, SETLK);
    assert_eq!(
        calls[0].2,
        RecordLock {
            l_type: WRLCK,
            l_whence: 0,
            l_start: 4,
            l_len: 8,
            l_pid: 0,
        }
    );
}

#[test]
fn getlk_copies_the_first_conflict_back() {
    let platform = platform();
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let reply = RecordLock {
        l_type: WRLCK,
        l_whence: 0,
        l_start: 3,
        l_len: 5,
        l_pid: 77,
    };
    platform.backend().set_record_lock_reply(reply);
    let mut value = lock(F_RDLCK as i16, 0, 10);
    assert_eq!(record_lock(&platform, fd, F_GETLK, &mut value), Ok(()));
    assert_eq!(
        value,
        Flock {
            l_type: F_WRLCK as i16,
            l_whence: 0,
            l_start: 3,
            l_len: 5,
            l_pid: 77,
        }
    );
    assert_eq!(platform.backend().record_lock_calls()[0].1, GETLK);
}

#[test]
fn conflict_and_interrupt_errno_propagate() {
    let platform = platform();
    let fd = platform.open(b"/a", fcntl::O_RDWR as u32, 0).expect("open");
    platform.backend().fail_record_lock(Errno::EAGAIN);
    let mut value = lock(F_WRLCK as i16, 0, 10);
    assert_eq!(
        record_lock(&platform, fd, F_SETLK, &mut value),
        Err(Errno::EAGAIN)
    );
    platform.backend().fail_record_lock(Errno::EINTR);
    assert_eq!(
        record_lock(&platform, fd, F_SETLKW, &mut value),
        Err(Errno::EINTR)
    );
}

#[test]
fn unknown_commands_and_types_are_einval() {
    let platform = platform();
    let fd = platform.open(b"/a", fcntl::O_RDWR as u32, 0).expect("open");
    let mut value = lock(9, 0, 10);
    assert_eq!(
        record_lock(&platform, fd, F_SETLK, &mut value),
        Err(Errno::EINVAL)
    );
    let mut value = lock(F_WRLCK as i16, 0, 10);
    assert_eq!(
        record_lock(&platform, fd, 999, &mut value),
        Err(Errno::EINVAL)
    );
    assert!(platform.backend().record_lock_calls().is_empty());
}

#[test]
fn bad_fd_is_ebadf() {
    let platform = platform();
    let mut value = lock(F_WRLCK as i16, 0, 10);
    assert_eq!(
        record_lock(&platform, 99, F_SETLK, &mut value),
        Err(Errno::EBADF)
    );
}
