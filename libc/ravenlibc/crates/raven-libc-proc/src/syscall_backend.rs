// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The P2a syscall seam for real process identity.
//!
//! The pure [`ProcBackend`] implementation calls a small [`Syscall`] trait. The target shell
//! supplies a raw-syscall implementation; host tests supply a deterministic fake. Non-identity
//! operations continue through [`LocalBackend`], the early fallback for callers that have not
//! wired the kernel path yet.

use alloc::vec::Vec;

use crate::backend::{EarlySpawn, ProcBackend, WaitResult};
use crate::ids::SetCred;
use crate::local_backend::LocalBackend;
use crate::request::{ExecRequest, ForkRequest, SpawnRequest};

/// Raw `proc_self_info(out, count)`.
pub const PROC_SELF_INFO: usize = 0x35;
/// Raw `proc_setpgid(pid, pgid)`.
pub const PROC_SETPGID: usize = 0x36;
/// Raw `proc_setsid()`.
pub const PROC_SETSID: usize = 0x37;
/// Raw `proc_setcred(which, value)`.
pub const PROC_SETCRED: usize = 0x38;
/// Raw `proc_pid_info(pid, out, count)`.
pub const PROC_PID_INFO: usize = 0x3A;
/// Raw `proc_setcred_ex(which, a, b, c, groups, count)`.
pub const PROC_SETCRED_EX: usize = 0x3B;
/// Raw `proc_cred_info(out, count)`.
pub const PROC_CRED_INFO: usize = 0x3C;
/// Raw `proc_getgroups(out, size)`.
pub const PROC_GETGROUPS: usize = 0x3D;
/// Raw `proc_getrusage(who, out, count)` (kernel `process_ctl.rs`).
pub const PROC_GETRUSAGE: usize = 0x4D;
/// Raw `proc_getrlimit(resource, out)` (kernel `process_ctl.rs`).
pub const PROC_GETRLIMIT: usize = 0x4E;
/// Raw `proc_setrlimit(resource, cur, max)` (kernel `process_ctl.rs`).
pub const PROC_SETRLIMIT: usize = 0x4F;

/// The six credential words returned by `PROC_CRED_INFO`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CredInfo {
    /// Real user id.
    pub uid: u32,
    /// Effective user id.
    pub euid: u32,
    /// Saved user id.
    pub suid: u32,
    /// Real group id.
    pub gid: u32,
    /// Effective group id.
    pub egid: u32,
    /// Saved group id.
    pub sgid: u32,
}

/// The eight identity words returned by `PROC_SELF_INFO`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelfInfo {
    /// Process id.
    pub pid: u64,
    /// Parent process id.
    pub ppid: u64,
    /// Process group id.
    pub pgid: u64,
    /// Session id.
    pub sid: u64,
    /// Real user id.
    pub uid: u32,
    /// Effective user id.
    pub euid: u32,
    /// Real group id.
    pub gid: u32,
    /// Effective group id.
    pub egid: u32,
}

/// The raw syscall seam used by [`SyscallBackend`].
pub trait Syscall {
    /// Read the caller's identity.
    fn self_info(&self) -> Result<SelfInfo, i32>;
    /// Read another live process's identity.
    fn pid_info(&self, pid: i32) -> Result<SelfInfo, i32>;
    /// Read the caller's saved ids.
    fn cred_info(&self) -> Result<CredInfo, i32>;
    /// Read the caller's supplementary groups into `out`; returns the stored count.
    fn groups(&self, out: &mut [u32]) -> Result<usize, i32>;
    /// Apply one extended credential request.
    fn setcred_ex(&self, request: &SetCred) -> Result<(), i32>;
    /// Set a process group.
    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), i32>;
    /// Create a session and return its id.
    fn setsid(&self) -> Result<i32, i32>;
}

/// A `ProcBackend` whose identity methods execute the P2a syscall seam.
pub struct SyscallBackend<S: Syscall> {
    sys: S,
    local: LocalBackend<'static>,
}

impl<S: Syscall> SyscallBackend<S> {
    /// Wrap a syscall implementation. Spawn/fork/wait retain the early fallback behavior.
    #[must_use]
    pub const fn new(sys: S) -> Self {
        Self {
            sys,
            local: LocalBackend::new(None),
        }
    }

    /// Wrap a syscall implementation with the P9a native spawner.
    #[must_use]
    pub const fn with_spawner(sys: S, spawner: &'static dyn EarlySpawn) -> Self {
        Self {
            sys,
            local: LocalBackend::new(Some(spawner)),
        }
    }

    fn info(&self) -> Result<SelfInfo, i32> {
        self.sys.self_info()
    }
}

fn small(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(-1)
}

impl<S: Syscall> ProcBackend for SyscallBackend<S> {
    fn getpid(&self) -> i32 {
        self.info().map_or(-1, |info| small(info.pid))
    }

    fn getppid(&self) -> i32 {
        self.info().map_or(-1, |info| small(info.ppid))
    }

    fn getuid(&self) -> u32 {
        self.info().map_or(0, |info| info.uid)
    }

    fn geteuid(&self) -> u32 {
        self.info().map_or(0, |info| info.euid)
    }

    fn getgid(&self) -> u32 {
        self.info().map_or(0, |info| info.gid)
    }

    fn getegid(&self) -> u32 {
        self.info().map_or(0, |info| info.egid)
    }

    fn getresuid(&self) -> Result<(u32, u32, u32), i32> {
        let info = self.sys.cred_info()?;
        Ok((info.uid, info.euid, info.suid))
    }

    fn getresgid(&self) -> Result<(u32, u32, u32), i32> {
        let info = self.sys.cred_info()?;
        Ok((info.gid, info.egid, info.sgid))
    }

    fn getpgid(&self, pid: i32) -> Result<i32, i32> {
        let info = self.info()?;
        if pid == 0 || small(info.pid) == pid {
            return Ok(small(info.pgid));
        }
        Ok(small(self.sys.pid_info(pid)?.pgid))
    }

    fn getsid(&self, pid: i32) -> Result<i32, i32> {
        let info = self.info()?;
        if pid == 0 || small(info.pid) == pid {
            return Ok(small(info.sid));
        }
        Ok(small(self.sys.pid_info(pid)?.sid))
    }

    fn getgroups(&self, out: &mut [u32]) -> Result<usize, i32> {
        self.sys.groups(out)
    }

    fn spawn(&self, req: &SpawnRequest) -> Result<i32, i32> {
        self.local.spawn(req)
    }

    fn fork(&self, req: &ForkRequest) -> Result<i32, i32> {
        self.local.fork(req)
    }

    fn exec(&self, req: &ExecRequest) -> i32 {
        #[cfg(target_os = "none")]
        {
            crate::exec_raw::exec(req)
        }
        #[cfg(not(target_os = "none"))]
        {
            self.local.exec(req)
        }
    }

    fn wait(&self, idtype: u32, id: i64, options: u32) -> Result<WaitResult, i32> {
        self.local.wait(idtype, id, options)
    }

    fn kill(&self, pid: i64, sig: u32, queue_value: Option<u64>) -> Result<(), i32> {
        self.local.kill(pid, sig, queue_value)
    }

    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), i32> {
        self.sys.setpgid(pid, pgid)
    }

    fn setsid(&self) -> Result<i32, i32> {
        self.sys.setsid()
    }

    fn setcred(&self, req: &SetCred) -> Result<(), i32> {
        req.validate()?;
        self.sys.setcred_ex(req)
    }

    fn rlimit(&self, pid: i32, resource: u32, new: Option<(u64, u64)>) -> Result<(u64, u64), i32> {
        self.local.rlimit(pid, resource, new)
    }

    fn rusage(&self, who: i32) -> Result<[u8; 144], i32> {
        self.local.rusage(who)
    }

    fn prio(&self, which: u32, who: i32, set: Option<i32>) -> Result<i32, i32> {
        self.local.prio(which, who, set)
    }

    fn sched(
        &self,
        op: u32,
        pid: i32,
        policy: i32,
        param: [i32; 4],
    ) -> Result<(i32, [i32; 4]), i32> {
        self.local.sched(op, pid, policy, param)
    }

    fn set_title(&self, title: &[u8]) -> Result<(), i32> {
        self.local.set_title(title)
    }

    fn prctl(&self, option: u32, a: [u64; 4]) -> Result<i64, i32> {
        self.local.prctl(option, a)
    }

    fn login(&self, set: Option<&[u8]>) -> Result<Vec<u8>, i32> {
        self.local.login(set)
    }

    fn pledge(&self, promises: Option<&[u8]>, exec_promises: Option<&[u8]>) -> Result<(), i32> {
        self.local.pledge(promises, exec_promises)
    }

    fn cap_enter(&self) -> Result<(), i32> {
        self.local.cap_enter()
    }
}

#[cfg(test)]
#[path = "syscall_backend_ut.rs"]
mod tests;
