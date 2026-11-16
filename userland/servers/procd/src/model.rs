// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process-table model and protocol dispatch.
//!
//! `procd` is the parent of every child it creates. The model keeps the process handle for wait
//! and terminate, the owner connection (a uid for the first server slice) and the status it has
//! already reported. The kernel still owns identity, address spaces and exit timing.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_proc::{
    Body, GetPgidRequest, GetSidRequest, HelloRequest, KillRequest, Request, Response,
    SetPgidRequest, SpawnRequest, VERSION, WaitRequest,
};

/// `idtype_t` value for all children.
pub const P_ALL: u32 = 0;
/// `idtype_t` value for one pid.
pub const P_PID: u32 = 1;
/// `WNOHANG` option bit.
pub const WNOHANG: u32 = 1;
/// POSIX SIGINT.
pub const SIGINT: u32 = 2;
/// POSIX SIGKILL.
pub const SIGKILL: u32 = 9;
/// POSIX SIGTERM.
pub const SIGTERM: u32 = 15;
/// POSIX `POSIX_SPAWN_SETPGROUP` from the Linux/musl header values.
pub const POSIX_SPAWN_SETPGROUP: u32 = 2;

/// One live process identity returned by the kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    /// Process id.
    pub pid: i32,
    /// Parent process id.
    pub ppid: i32,
    /// Process group id.
    pub pgid: i32,
    /// Session id.
    pub sid: i32,
}

/// The kernel operations the model needs. Host tests use a recording fake.
pub trait ProcSys {
    /// Spawn an initfs image and return its process handle.
    fn spawn(&mut self, path: &str, argv: &[&str], env: &[&str]) -> Result<Handle, Errno>;
    /// Map a process handle to its pid.
    fn pid(&mut self, handle: Handle) -> Result<i32, Errno>;
    /// Read a live process identity.
    fn info(&mut self, pid: i32) -> Result<ProcInfo, Errno>;
    /// Block until the child exits and return its raw exit code.
    fn wait(&mut self, handle: Handle) -> Result<i32, Errno>;
    /// Terminate the child and store `code` as its exit code.
    fn terminate(&mut self, handle: Handle, code: i32) -> Result<(), Errno>;
    /// Set a direct child's process group.
    fn setpgid(&mut self, pid: i32, pgid: i32) -> Result<(), Errno>;
    /// Close a process handle.
    fn close(&mut self, handle: Handle);
    /// The server's own pid.
    fn self_pid(&mut self) -> Result<i32, Errno>;
}

/// The status a `wait` reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitStatus {
    /// Raw exit code when the child exited.
    pub code: i32,
    /// Signal number when `procd` terminated the child.
    pub signal: Option<u32>,
}

impl WaitStatus {
    /// A normal exit.
    #[must_use]
    pub const fn exited(code: i32) -> Self {
        Self { code, signal: None }
    }

    /// A signal-style termination.
    #[must_use]
    pub const fn signaled(signal: u32) -> Self {
        Self {
            code: 128 + signal as i32,
            signal: Some(signal),
        }
    }
}

/// Encode a [`WaitStatus`] as the POSIX wait-status word.
#[must_use]
pub fn encode_wait_status(status: WaitStatus) -> i32 {
    match status.signal {
        Some(signal) => (signal & 0x7f) as i32,
        None => (status.code & 0xff) << 8,
    }
}

/// One child owned by this server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Child {
    /// Process id.
    pub pid: i32,
    /// Process handle retained for wait and terminate.
    pub handle: Handle,
    /// Owner connection uid.
    pub owner: u32,
    /// Current process group.
    pub pgid: i32,
    /// Session id.
    pub sid: i32,
    /// The status already reported by a wait, if any.
    pub status: Option<WaitStatus>,
    /// The image path, for diagnostics.
    pub image: String,
    /// The umask requested by the spawn request; the current kernel has no child umask call.
    pub umask: u32,
}

/// The `procd` process table.
pub struct Procd<S: ProcSys> {
    sys: S,
    pid: i32,
    children: BTreeMap<i32, Child>,
}

impl<S: ProcSys> Procd<S> {
    /// Create a table for a server with `pid`.
    #[must_use]
    pub fn new(sys: S, pid: i32) -> Self {
        Self {
            sys,
            pid,
            children: BTreeMap::new(),
        }
    }

    /// The server pid.
    #[must_use]
    pub const fn pid(&self) -> i32 {
        self.pid
    }

    /// The children currently owned by this server.
    #[must_use]
    pub const fn children(&self) -> &BTreeMap<i32, Child> {
        &self.children
    }

    /// The kernel seam, exposed for host tests and diagnostics.
    #[must_use]
    pub const fn sys(&self) -> &S {
        &self.sys
    }

    /// Dispatch one decoded request for the owner `uid`.
    pub fn dispatch(&mut self, uid: u32, request: Request) -> Response {
        match request {
            Request::Hello(hello) => self.hello(hello),
            Request::Spawn(spawn) => self.spawn(uid, spawn),
            Request::Wait(wait) => self.wait(uid, wait),
            Request::Kill(kill) => self.kill(uid, kill),
            Request::SetPgid(setpgid) => self.set_pgid(uid, setpgid),
            Request::GetPgid(getpgid) => self.get_pgid(getpgid),
            Request::GetSid(getsid) => self.get_sid(getsid),
            Request::Other(op) => {
                let _ = op;
                Response::error(Errno::ENOSYS)
            }
        }
    }

    fn hello(&self, hello: HelloRequest) -> Response {
        let _ = hello;
        Response::body(
            0,
            Body::Hello {
                server_version: u32::from(VERSION),
                pid: self.pid,
            },
        )
    }

    fn spawn(&mut self, uid: u32, spawn: SpawnRequest) -> Response {
        if spawn.spawn_flags & !POSIX_SPAWN_SETPGROUP != 0
            || spawn.sigdefault.iter().any(|byte| *byte != 0)
            || spawn.sigmask.iter().any(|byte| *byte != 0)
            || spawn.ignored.iter().any(|byte| *byte != 0)
            || spawn.sched_policy != 0
            || spawn.sched_prio != 0
            || spawn.tcpgrp_fd != -1
            || !spawn.fdmap.is_empty()
        {
            return Response::error(Errno::ENOSYS);
        }
        if spawn.cwd != b"/" && !spawn.cwd.is_empty() {
            return Response::error(Errno::ENOSYS);
        }
        let path = match text(&spawn.path) {
            Ok(path) if path.starts_with('/') => path.to_string(),
            Ok(_) => return Response::error(Errno::EINVAL),
            Err(error) => return Response::error(error),
        };
        let argv = match texts(&spawn.argv) {
            Ok(argv) if !argv.is_empty() => argv,
            Ok(_) => return Response::error(Errno::EINVAL),
            Err(error) => return Response::error(error),
        };
        let env = match texts(&spawn.envp) {
            Ok(env) => env,
            Err(error) => return Response::error(error),
        };

        let handle = match self.sys.spawn(&path, &argv, &env) {
            Ok(handle) => handle,
            Err(error) => return Response::error(error),
        };
        let pid = match self.sys.pid(handle) {
            Ok(pid) if pid > 0 => pid,
            Ok(_) => {
                self.cleanup(handle);
                return Response::error(Errno::EINVAL);
            }
            Err(error) => {
                self.cleanup(handle);
                return Response::error(error);
            }
        };
        let mut pgid = None;
        if spawn.spawn_flags & POSIX_SPAWN_SETPGROUP != 0 {
            let target = if spawn.pgroup == 0 { pid } else { spawn.pgroup };
            if target < 0 {
                self.cleanup(handle);
                return Response::error(Errno::EINVAL);
            }
            if let Err(error) = self.sys.setpgid(pid, target) {
                self.cleanup(handle);
                return Response::error(error);
            }
            pgid = Some(target);
        }
        // A fast child may already have exited by the time the pid query returns. The child
        // inherited the server's group and session unless SETPGROUP changed the group; use the
        // server identity as the fallback rather than rejecting a successful spawn.
        let info = match self.sys.info(pid) {
            Ok(info) => info,
            Err(_) => match self.sys.info(self.pid) {
                Ok(info) => info,
                Err(error) => {
                    self.cleanup(handle);
                    return Response::error(error);
                }
            },
        };
        self.children.insert(
            pid,
            Child {
                pid,
                handle,
                owner: uid,
                pgid: pgid.unwrap_or(info.pgid),
                sid: info.sid,
                status: None,
                image: path,
                umask: spawn.umask,
            },
        );
        Response::ok(i64::from(pid))
    }

    fn wait(&mut self, uid: u32, wait: WaitRequest) -> Response {
        if wait.idtype != P_PID || wait.id <= 0 {
            return Response::error(Errno::ENOSYS);
        }
        if wait.options & WNOHANG != 0 {
            return Response::error(Errno::ENOSYS);
        }
        if wait.options != 0 {
            return Response::error(Errno::EINVAL);
        }
        let pid = i32::try_from(wait.id).unwrap_or(-1);
        let Some(child) = self.children.get(&pid) else {
            return Response::error(Errno::ECHILD);
        };
        if child.owner != uid && uid != 0 {
            return Response::error(Errno::EPERM);
        }
        let handle = child.handle;
        let status = match self.sys.wait(handle) {
            Ok(code) => {
                if let Some(child) = self.children.get(&pid) {
                    child.status.unwrap_or_else(|| WaitStatus::exited(code))
                } else {
                    WaitStatus::exited(code)
                }
            }
            Err(error) => return Response::error(error),
        };
        self.children.remove(&pid);
        self.sys.close(handle);
        Response::body(
            i64::from(pid),
            Body::Wait {
                status: encode_wait_status(status),
                siginfo: [0u8; 128],
                rusage: [0u8; 144],
            },
        )
    }

    fn kill(&mut self, uid: u32, kill: KillRequest) -> Response {
        if kill.flags & 1 != 0 {
            return Response::error(Errno::ENOSYS);
        }
        if kill.pid == 0 || kill.pid == -1 {
            return Response::error(Errno::ENOSYS);
        }
        if kill.sig == 0 {
            let exists = (kill.pid > 0 && self.child_owned(uid, kill.pid as i32))
                || (kill.pid < 0 && self.group_owned(uid, -kill.pid as i32));
            return if exists {
                Response::ok(0)
            } else {
                Response::error(Errno::ESRCH)
            };
        }
        let Some(signal) = terminate_signal(kill.sig) else {
            return Response::error(Errno::ENOSYS);
        };
        if kill.pid > 0 {
            let pid = i32::try_from(kill.pid).unwrap_or(-1);
            let Some(child) = self.children.get(&pid) else {
                return Response::error(Errno::ESRCH);
            };
            if child.owner != uid && uid != 0 {
                return Response::error(Errno::EPERM);
            }
            let handle = child.handle;
            if let Err(error) = self.sys.terminate(handle, 128 + signal as i32) {
                return Response::error(error);
            }
            if let Some(child) = self.children.get_mut(&pid) {
                child.status = Some(WaitStatus::signaled(signal));
            }
            return Response::ok(0);
        }

        let group = -kill.pid as i32;
        let pids: Vec<i32> = self
            .children
            .iter()
            .filter(|(_, child)| child.pgid == group && (child.owner == uid || uid == 0))
            .map(|(pid, _)| *pid)
            .collect();
        if pids.is_empty() {
            return Response::error(Errno::ESRCH);
        }
        for pid in pids {
            if let Some(child) = self.children.get(&pid) {
                let handle = child.handle;
                if let Err(error) = self.sys.terminate(handle, 128 + signal as i32) {
                    return Response::error(error);
                }
                if let Some(child) = self.children.get_mut(&pid) {
                    child.status = Some(WaitStatus::signaled(signal));
                }
            }
        }
        Response::ok(0)
    }

    fn set_pgid(&mut self, uid: u32, setpgid: SetPgidRequest) -> Response {
        if setpgid.pid <= 0 {
            return Response::error(Errno::ENOSYS);
        }
        let Some(child) = self.children.get(&setpgid.pid) else {
            return Response::error(Errno::ESRCH);
        };
        if child.owner != uid && uid != 0 {
            return Response::error(Errno::EPERM);
        }
        let pgid = if setpgid.pgid == 0 {
            setpgid.pid
        } else {
            setpgid.pgid
        };
        if let Err(error) = self.sys.setpgid(setpgid.pid, pgid) {
            return Response::error(error);
        }
        if let Some(child) = self.children.get_mut(&setpgid.pid) {
            child.pgid = pgid;
        }
        Response::ok(0)
    }

    fn get_pgid(&mut self, getpgid: GetPgidRequest) -> Response {
        if getpgid.pid <= 0 {
            return Response::error(Errno::ENOSYS);
        }
        match self.sys.info(getpgid.pid) {
            Ok(info) => Response::ok(i64::from(info.pgid)),
            Err(error) => Response::error(error),
        }
    }

    fn get_sid(&mut self, getsid: GetSidRequest) -> Response {
        if getsid.pid <= 0 {
            return Response::error(Errno::ENOSYS);
        }
        match self.sys.info(getsid.pid) {
            Ok(info) => Response::ok(i64::from(info.sid)),
            Err(error) => Response::error(error),
        }
    }

    fn child_owned(&self, uid: u32, pid: i32) -> bool {
        self.children
            .get(&pid)
            .is_some_and(|child| child.owner == uid || uid == 0)
    }

    fn group_owned(&self, uid: u32, pgid: i32) -> bool {
        self.children
            .values()
            .any(|child| child.pgid == pgid && (child.owner == uid || uid == 0))
    }

    fn cleanup(&mut self, handle: Handle) {
        let _ = self.sys.terminate(handle, 125);
        let _ = self.sys.wait(handle);
        self.sys.close(handle);
    }
}

fn text(bytes: &[u8]) -> Result<&str, Errno> {
    if bytes.contains(&0) {
        return Err(Errno::EINVAL);
    }
    core::str::from_utf8(bytes).map_err(|_| Errno::EILSEQ)
}

fn texts(values: &[Vec<u8>]) -> Result<Vec<&str>, Errno> {
    let mut result = Vec::with_capacity(values.len());
    for value in values {
        result.push(text(value)?);
    }
    Ok(result)
}

fn terminate_signal(signal: u32) -> Option<u32> {
    match signal {
        SIGINT | SIGKILL | SIGTERM => Some(signal),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
