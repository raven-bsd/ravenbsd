// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the P2a syscall-backed process identity.

use core::cell::Cell;

use crate::backend::ProcBackend;
use crate::errors::{ENOSYS, ESRCH};
use crate::ids::SetCred;
use crate::syscall_backend::{SelfInfo, Syscall, SyscallBackend};

struct Fake {
    info: Cell<SelfInfo>,
    setpgid: Cell<Option<(i32, i32)>>,
    cred: Cell<Option<(u32, u32)>>,
    sid: i32,
}

impl Fake {
    fn new() -> Self {
        Self {
            info: Cell::new(SelfInfo {
                pid: 42,
                ppid: 1,
                pgid: 1,
                sid: 1,
                uid: 0,
                euid: 0,
                gid: 0,
                egid: 0,
            }),
            setpgid: Cell::new(None),
            cred: Cell::new(None),
            sid: 42,
        }
    }
}

impl Syscall for Fake {
    fn self_info(&self) -> Result<SelfInfo, i32> {
        Ok(self.info.get())
    }

    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), i32> {
        self.setpgid.set(Some((pid, pgid)));
        Ok(())
    }

    fn setsid(&self) -> Result<i32, i32> {
        Ok(self.sid)
    }

    fn setcred(&self, which: u32, value: u32) -> Result<(), i32> {
        self.cred.set(Some((which, value)));
        Ok(())
    }
}

#[test]
fn identity_getters_use_self_info() {
    let backend = SyscallBackend::new(Fake::new());
    assert_eq!(backend.getpid(), 42);
    assert_eq!(backend.getppid(), 1);
    assert_eq!(backend.getuid(), 0);
    assert_eq!(backend.geteuid(), 0);
    assert_eq!(backend.getgid(), 0);
    assert_eq!(backend.getegid(), 0);
    assert_eq!(backend.getpgid(0), Ok(1));
    assert_eq!(backend.getpgid(42), Ok(1));
    assert_eq!(backend.getpgid(7), Err(ESRCH));
    assert_eq!(backend.getsid(0), Ok(1));
    assert_eq!(backend.getsid(7), Err(ESRCH));
}

#[test]
fn process_group_and_session_operations_reach_the_seam() {
    let fake = Fake::new();
    let backend = SyscallBackend::new(fake);
    backend.setpgid(0, 0).unwrap();
    assert_eq!(backend.sys.setpgid.get(), Some((0, 0)));
    assert_eq!(backend.setsid(), Ok(42));
}

#[test]
fn credential_requests_map_to_the_kernel_which_values() {
    let fake = Fake::new();
    let backend = SyscallBackend::new(fake);
    backend.setcred(&SetCred::setuid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some((0, 0)));
    backend.setcred(&SetCred::seteuid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some((1, 0)));
    backend.setcred(&SetCred::setgid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some((2, 0)));
    backend.setcred(&SetCred::setegid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some((3, 0)));
    assert_eq!(backend.setcred(&SetCred::setreuid(0, 0)), Err(ENOSYS));
}

#[test]
fn no_supplementary_groups_are_invented() {
    let backend = SyscallBackend::new(Fake::new());
    let mut groups = [99u32; 2];
    assert_eq!(backend.getgroups(&mut groups), Ok(0));
    assert_eq!(groups, [99, 99]);
}
