// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the syscall-backed process identity and credentials.

use core::cell::Cell;

use crate::backend::ProcBackend;
use crate::errors::{EINVAL, ESRCH};
use crate::ids::SetCred;
use crate::syscall_backend::{CredInfo, SelfInfo, Syscall, SyscallBackend};

struct Fake {
    info: Cell<SelfInfo>,
    child: Cell<Option<SelfInfo>>,
    cred_info: Cell<CredInfo>,
    groups: Cell<[u32; 4]>,
    group_count: Cell<usize>,
    setpgid: Cell<Option<(i32, i32)>>,
    cred: Cell<Option<u32>>,
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
            child: Cell::new(Some(SelfInfo {
                pid: 7,
                ppid: 42,
                pgid: 7,
                sid: 1,
                uid: 0,
                euid: 0,
                gid: 0,
                egid: 0,
            })),
            cred_info: Cell::new(CredInfo {
                uid: 0,
                euid: 0,
                suid: 5,
                gid: 0,
                egid: 0,
                sgid: 6,
            }),
            groups: Cell::new([11, 12, 0, 0]),
            group_count: Cell::new(2),
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

    fn pid_info(&self, pid: i32) -> Result<SelfInfo, i32> {
        self.child
            .get()
            .filter(|child| child.pid == u64::try_from(pid).unwrap_or(u64::MAX))
            .ok_or(ESRCH)
    }

    fn cred_info(&self) -> Result<CredInfo, i32> {
        Ok(self.cred_info.get())
    }

    fn groups(&self, out: &mut [u32]) -> Result<usize, i32> {
        let count = self.group_count.get();
        if out.is_empty() {
            return Ok(count);
        }
        if out.len() < count {
            return Err(EINVAL);
        }
        for (slot, value) in out.iter_mut().zip(self.groups.get()) {
            *slot = value;
        }
        Ok(count)
    }

    fn setcred_ex(&self, request: &SetCred) -> Result<(), i32> {
        request.validate()?;
        self.cred.set(Some(request.kind));
        Ok(())
    }

    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), i32> {
        self.setpgid.set(Some((pid, pgid)));
        Ok(())
    }

    fn setsid(&self) -> Result<i32, i32> {
        Ok(self.sid)
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
    assert_eq!(backend.getpgid(7), Ok(7));
    assert_eq!(backend.getsid(0), Ok(1));
    assert_eq!(backend.getsid(7), Ok(1));
    assert_eq!(backend.getpgid(8), Err(ESRCH));
}

#[test]
fn process_group_and_session_operations_reach_the_seam() {
    let backend = SyscallBackend::new(Fake::new());
    backend.setpgid(0, 0).unwrap();
    assert_eq!(backend.sys.setpgid.get(), Some((0, 0)));
    assert_eq!(backend.setsid(), Ok(42));
}

#[test]
fn saved_ids_come_from_the_credential_query() {
    let backend = SyscallBackend::new(Fake::new());
    assert_eq!(backend.getresuid(), Ok((0, 0, 5)));
    assert_eq!(backend.getresgid(), Ok((0, 0, 6)));
}

#[test]
fn every_credential_kind_reaches_the_extended_seam() {
    let backend = SyscallBackend::new(Fake::new());
    backend.setcred(&SetCred::setuid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(1));
    backend.setcred(&SetCred::seteuid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(2));
    backend.setcred(&SetCred::setreuid(0, 0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(3));
    backend.setcred(&SetCred::setresuid(0, 0, 0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(4));
    backend.setcred(&SetCred::setgid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(5));
    backend.setcred(&SetCred::setegid(0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(6));
    backend.setcred(&SetCred::setregid(0, 0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(7));
    backend.setcred(&SetCred::setresgid(0, 0, 0)).unwrap();
    assert_eq!(backend.sys.cred.get(), Some(8));
    backend
        .setcred(&SetCred::setgroups(&[11, 12]).unwrap())
        .unwrap();
    assert_eq!(backend.sys.cred.get(), Some(9));
}

#[test]
fn groups_are_read_from_the_seam_and_not_invented() {
    let backend = SyscallBackend::new(Fake::new());
    let mut groups = [99u32; 4];
    assert_eq!(backend.getgroups(&mut groups), Ok(2));
    assert_eq!(groups[..2], [11, 12]);
    assert_eq!(backend.getgroups(&mut []), Ok(2));
}
