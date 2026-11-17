// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process-table model and protocol dispatch.
//!
//! `procd` is the parent of every child it creates. The model keeps the process handle, the owner
//! connection (a uid for this slice), the process group/session and the kernel wait status. The
//! kernel remains authoritative for identity, address spaces, signal queues and exit timing.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_proc::{
    Body, FdMapEntry, GetPgidRequest, GetSidRequest, HelloRequest, KillRequest, Request, Response,
    SetPgidRequest, SpawnRequest, VERSION, WaitRequest,
};

/// `idtype_t` value for all children (libc/Linux numbering on the wire).
pub const P_ALL: u32 = 0;
/// `idtype_t` value for one pid.
pub const P_PID: u32 = 1;
/// `idtype_t` value for one process group.
pub const P_PGID: u32 = 2;
/// P2f's kernel `P_ALL` value; the wire value is translated to this.
pub const KERNEL_P_ALL: u32 = 3;
/// `WNOHANG` option bit.
pub const WNOHANG: u32 = 1;
/// `WUNTRACED` option bit.
pub const WUNTRACED: u32 = 2;
/// `WCONTINUED` wire (libc/Linux) option bit.
pub const WCONTINUED: u32 = 8;
/// P2f's kernel `WCONTINUED` option bit.
pub const KERNEL_WCONTINUED: u32 = 4;
/// POSIX SIGKILL.
pub const SIGKILL: u32 = 9;
/// POSIX SIGTERM.
pub const SIGTERM: u32 = 15;
/// Low-byte marker of a stopped wait status.
const STOPPED_STATUS_MASK: i32 = 0x7f;
/// The continued wait status.
const CONTINUED_STATUS: i32 = 0xffff;
/// POSIX `POSIX_SPAWN_SETPGROUP` from the Linux/musl header values.
pub const POSIX_SPAWN_SETPGROUP: u32 = 2;
/// POSIX `POSIX_SPAWN_SETSIGDEF`.
pub const POSIX_SPAWN_SETSIGDEF: u32 = 4;
/// POSIX `POSIX_SPAWN_SETSIGMASK`.
pub const POSIX_SPAWN_SETSIGMASK: u32 = 8;
/// POSIX `POSIX_SPAWN_SETSID`; the kernel has no setsid-by-handle operation.
pub const POSIX_SPAWN_SETSID: u32 = 128;
/// Requirement hint: the caller already resolved the path.
pub const S_USE_PATH_HINT: u32 = 1 << 16;
/// Requirement hint: every descriptor not in the fd map starts closed.
pub const S_CLOEXEC_DEFAULT: u32 = 1 << 17;
/// Largest accepted child descriptor number.
pub const MAX_CHILD_FD: i32 = 1024;
/// Highest signal number the kernel accepts.
pub const SIG_MAX: u32 = 64;
/// POSIX SIGSTOP.
pub const SIGSTOP: u32 = 19;
/// Maximum command-name bytes stored in a procd record.
pub const MAX_COMMAND: usize = 256;

const SUPPORTED_SPAWN_FLAGS: u32 = POSIX_SPAWN_SETPGROUP
    | POSIX_SPAWN_SETSIGDEF
    | POSIX_SPAWN_SETSIGMASK
    | S_USE_PATH_HINT
    | S_CLOEXEC_DEFAULT;

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

/// One result from `PROC_WAIT_EX`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitEvent {
    /// Reaped/observed pid, or zero for `WNOHANG` with nothing ready.
    pub pid: i32,
    /// Raw POSIX wait status.
    pub status: i32,
}

/// The kernel operations the model needs. Host tests use a recording fake.
pub trait ProcSys {
    /// Spawn an initfs image and return its process handle.
    fn spawn(&mut self, path: &str, argv: &[&str], env: &[&str]) -> Result<Handle, Errno>;
    /// Spawn with an fd map and initial signal sets.
    ///
    /// The default fails closed: an implementation that only supports plain spawn must reject a
    /// request carrying attributes rather than silently dropping them.
    #[allow(clippy::too_many_arguments)] // mirrors the wire request fields
    fn spawn_with_attributes(
        &mut self,
        path: &str,
        argv: &[&str],
        env: &[&str],
        fdmap: &[(i32, u32)],
        sigmask: &[u8; 128],
        sigdefault: &[u8; 128],
        ignored: &[u8; 128],
    ) -> Result<Handle, Errno> {
        let _ = (path, argv, env, fdmap, sigmask, sigdefault, ignored);
        Err(Errno::ENOSYS)
    }
    /// Check that a transferred handle is usable and carries `TRANSFER`.
    ///
    /// The default fails closed; an implementation without the handle-transfer seam must reject
    /// a non-empty fd map.
    fn validate_handle(&mut self, handle: Handle) -> Result<(), Errno> {
        let _ = handle;
        Err(Errno::ENOSYS)
    }
    /// Map a process handle to its pid.
    fn pid(&mut self, handle: Handle) -> Result<i32, Errno>;
    /// Read a live process identity.
    fn info(&mut self, pid: i32) -> Result<ProcInfo, Errno>;
    /// Wait for a child with the P2f idtype/options/status contract.
    fn wait_ex(&mut self, idtype: u32, id: i32, options: u32) -> Result<WaitEvent, Errno>;
    /// Send a signal to a pid or a negative process group.
    fn signal(&mut self, target: i32, signo: u32) -> Result<(), Errno>;
    /// Read a process group.
    fn getpgid(&mut self, pid: i32) -> Result<i32, Errno>;
    /// Set a direct child's process group.
    fn setpgid(&mut self, pid: i32, pgid: i32) -> Result<(), Errno>;
    /// Close a process handle.
    fn close(&mut self, handle: Handle);
    /// The server's own pid.
    fn self_pid(&mut self) -> Result<i32, Errno>;
}

/// One child owned by this server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Child {
    /// Process id.
    pub pid: i32,
    /// Process handle retained for cleanup and close.
    pub handle: Handle,
    /// Owner connection uid.
    pub owner: u32,
    /// Current process group.
    pub pgid: i32,
    /// Session id.
    pub sid: i32,
    /// The image path, for diagnostics.
    pub image: String,
    /// The command name (`argv[0]`) shown to `ps`.
    pub command: String,
    /// The umask requested by the spawn request; the current kernel has no child umask call.
    pub umask: u32,
    /// The descriptor map installed in the child.
    pub fdmap: Vec<(i32, u32)>,
    /// Initial blocked-signal set.
    pub sigmask: [u8; 128],
    /// Signals reset to `SIG_DFL` at spawn.
    pub sigdefault: [u8; 128],
    /// Signals ignored by the parent.
    pub ignored: [u8; 128],
}

/// One child entry in the read-only `proc:registry` view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryEntry {
    /// Process id.
    pub pid: i32,
    /// Parent process id (`procd`).
    pub ppid: i32,
    /// Process group id.
    pub pgid: i32,
    /// Session id.
    pub sid: i32,
    /// Spawning connection's effective uid.
    pub uid: u32,
    /// Command name (`argv[0]`).
    pub command: String,
}

impl core::fmt::Display for RegistryEntry {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "{} {} {} {} {} {}",
            self.pid, self.ppid, self.pgid, self.sid, self.uid, self.command
        )
    }
}

/// Choose and truncate the command name for a spawn request.
#[must_use]
pub fn command_name(argv: &[&str], path: &str) -> String {
    let text = argv.first().copied().unwrap_or(path);
    let mut end = 0usize;
    for (index, character) in text.char_indices() {
        let next = index.saturating_add(character.len_utf8());
        if next > MAX_COMMAND {
            break;
        }
        end = next;
    }
    String::from(&text[..end])
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

    /// A `ps`-consumable snapshot of the children, sorted by pid.
    #[must_use]
    pub fn registry(&self) -> Vec<RegistryEntry> {
        self.children
            .values()
            .map(|child| RegistryEntry {
                pid: child.pid,
                ppid: self.pid,
                pgid: child.pgid,
                sid: child.sid,
                uid: child.owner,
                command: child.command.clone(),
            })
            .collect()
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
            Request::Other(_) => Response::error(Errno::ENOSYS),
        }
    }

    fn hello(&self, _hello: HelloRequest) -> Response {
        Response::body(
            0,
            Body::Hello {
                server_version: u32::from(VERSION),
                pid: self.pid,
            },
        )
    }

    fn spawn(&mut self, uid: u32, spawn: SpawnRequest) -> Response {
        if spawn.spawn_flags & !SUPPORTED_SPAWN_FLAGS != 0
            || spawn.sched_policy != 0
            || spawn.sched_prio != 0
            || spawn.tcpgrp_fd != -1
        {
            self.close_fdmap(&spawn.fdmap);
            return Response::error(Errno::ENOSYS);
        }
        if spawn.cwd != b"/" && !spawn.cwd.is_empty() {
            self.close_fdmap(&spawn.fdmap);
            return Response::error(Errno::ENOSYS);
        }
        if let Err(error) = self.validate_spawn(&spawn) {
            self.close_fdmap(&spawn.fdmap);
            return Response::error(error);
        }
        let path = match text(&spawn.path) {
            Ok(path) if path.starts_with('/') => path.to_string(),
            Ok(_) => {
                self.close_fdmap(&spawn.fdmap);
                return Response::error(Errno::EINVAL);
            }
            Err(error) => {
                self.close_fdmap(&spawn.fdmap);
                return Response::error(error);
            }
        };
        let argv = match texts(&spawn.argv) {
            Ok(argv) if !argv.is_empty() => argv,
            Ok(_) => {
                self.close_fdmap(&spawn.fdmap);
                return Response::error(Errno::EINVAL);
            }
            Err(error) => {
                self.close_fdmap(&spawn.fdmap);
                return Response::error(error);
            }
        };
        let env = match texts(&spawn.envp) {
            Ok(env) => env,
            Err(error) => {
                self.close_fdmap(&spawn.fdmap);
                return Response::error(error);
            }
        };

        let fdmap: Vec<(i32, u32)> = spawn
            .fdmap
            .iter()
            .map(|entry| (entry.fd, entry.handle))
            .collect();
        let command = command_name(&argv, &path);
        let attributes = !fdmap.is_empty()
            || spawn.sigmask.iter().any(|byte| *byte != 0)
            || spawn.sigdefault.iter().any(|byte| *byte != 0)
            || spawn.ignored.iter().any(|byte| *byte != 0);
        let spawned = if attributes {
            self.sys.spawn_with_attributes(
                &path,
                &argv,
                &env,
                &fdmap,
                &spawn.sigmask,
                &spawn.sigdefault,
                &spawn.ignored,
            )
        } else {
            self.sys.spawn(&path, &argv, &env)
        };
        let handle = match spawned {
            Ok(handle) => handle,
            Err(error) => {
                self.close_fdmap(&spawn.fdmap);
                return Response::error(error);
            }
        };
        // The client transferred each source handle into this server for this spawn. Once the
        // child has its own copies the server-side originals are no longer needed.
        self.close_fdmap(&spawn.fdmap);
        let pid = match self.sys.pid(handle) {
            Ok(pid) if pid > 0 => pid,
            Ok(_) => {
                self.sys.close(handle);
                return Response::error(Errno::EINVAL);
            }
            Err(error) => {
                self.sys.close(handle);
                return Response::error(error);
            }
        };
        let mut pgid = None;
        if spawn.spawn_flags & POSIX_SPAWN_SETPGROUP != 0 {
            let target = if spawn.pgroup == 0 { pid } else { spawn.pgroup };
            if target < 0 {
                self.cleanup(pid, handle);
                return Response::error(Errno::EINVAL);
            }
            if let Err(error) = self.sys.setpgid(pid, target) {
                self.cleanup(pid, handle);
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
                    self.cleanup(pid, handle);
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
                image: path,
                command,
                umask: spawn.umask,
                fdmap,
                sigmask: spawn.sigmask,
                sigdefault: spawn.sigdefault,
                ignored: spawn.ignored,
            },
        );
        Response::ok(i64::from(pid))
    }

    fn validate_spawn(&mut self, spawn: &SpawnRequest) -> Result<(), Errno> {
        if spawn.spawn_flags & POSIX_SPAWN_SETPGROUP != 0 && spawn.pgroup < 0 {
            return Err(Errno::EINVAL);
        }
        if spawn.spawn_flags & POSIX_SPAWN_SETSIGDEF == 0
            && spawn.sigdefault.iter().any(|byte| *byte != 0)
        {
            return Err(Errno::EINVAL);
        }
        validate_signal_set(&spawn.sigmask, true)?;
        validate_signal_set(&spawn.sigdefault, false)?;
        validate_signal_set(&spawn.ignored, true)?;
        if spawn.fdmap.len() > raven_proto_proc::MAX_FDS {
            return Err(Errno::EINVAL);
        }
        let mut descriptors = BTreeSet::new();
        for entry in &spawn.fdmap {
            if entry.fd < 0 || entry.fd >= MAX_CHILD_FD || entry.handle == Handle::NONE.0 {
                return Err(Errno::EINVAL);
            }
            if !descriptors.insert(entry.fd) {
                return Err(Errno::EINVAL);
            }
            self.sys.validate_handle(Handle(entry.handle))?;
        }
        Ok(())
    }

    fn close_fdmap(&mut self, map: &[FdMapEntry]) {
        let mut handles = BTreeSet::new();
        for entry in map {
            if handles.insert(entry.handle) {
                self.sys.close(Handle(entry.handle));
            }
        }
    }

    fn wait(&mut self, uid: u32, wait: WaitRequest) -> Response {
        let idtype = match wait.idtype {
            P_ALL | KERNEL_P_ALL => P_ALL,
            P_PID => P_PID,
            P_PGID => P_PGID,
            _ => return Response::error(Errno::EINVAL),
        };
        if wait.options & !(WNOHANG | WUNTRACED | WCONTINUED) != 0 {
            return Response::error(Errno::EINVAL);
        }
        let id = i32::try_from(wait.id).unwrap_or(-1);
        if idtype != P_ALL && id <= 0 {
            return Response::error(Errno::ENOSYS);
        }
        if let Err(error) = self.check_wait_owner(uid, idtype, id) {
            return Response::error(error);
        }
        let kernel_type = if idtype == P_ALL {
            KERNEL_P_ALL
        } else {
            idtype
        };
        let kernel_id = if idtype == P_ALL { 0 } else { id };
        let options = (wait.options & (WNOHANG | WUNTRACED))
            | if wait.options & WCONTINUED != 0 {
                KERNEL_WCONTINUED
            } else {
                0
            };
        let event = match self.sys.wait_ex(kernel_type, kernel_id, options) {
            Ok(event) => event,
            Err(error) => return Response::error(error),
        };
        if event.pid == 0 {
            return Response::body(
                0,
                Body::Wait {
                    status: 0,
                    siginfo: [0u8; 128],
                    rusage: [0u8; 144],
                },
            );
        }
        let pid = event.pid;
        let Some(child) = self.children.get(&pid) else {
            return Response::error(Errno::ECHILD);
        };
        if child.owner != uid && uid != 0 {
            return Response::error(Errno::EPERM);
        }
        let handle = child.handle;
        if is_terminal(event.status) {
            self.children.remove(&pid);
            self.sys.close(handle);
        }
        Response::body(
            i64::from(pid),
            Body::Wait {
                status: event.status,
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
            if kill.pid > 0 {
                return match self.child_owned(uid, kill.pid as i32) {
                    true => Response::ok(0),
                    false => Response::error(Errno::ESRCH),
                };
            }
            let group = -kill.pid as i32;
            return match self.group_owned(uid, group) {
                true => Response::ok(0),
                false => Response::error(Errno::ESRCH),
            };
        }
        if kill.pid > 0 {
            let pid = i32::try_from(kill.pid).unwrap_or(-1);
            let Some(child) = self.children.get(&pid) else {
                return Response::error(Errno::ESRCH);
            };
            if child.owner != uid && uid != 0 {
                return Response::error(Errno::EPERM);
            }
            return match self.sys.signal(pid, kill.sig) {
                Ok(()) => Response::ok(0),
                Err(error) => Response::error(error),
            };
        }
        let group = -kill.pid as i32;
        if !self.group_owned(uid, group) {
            return Response::error(Errno::ESRCH);
        }
        match self.sys.signal(-group, kill.sig) {
            Ok(()) => Response::ok(0),
            Err(error) => Response::error(error),
        }
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
        match self.sys.getpgid(getpgid.pid) {
            Ok(pgid) => Response::ok(i64::from(pgid)),
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

    fn check_wait_owner(&self, uid: u32, idtype: u32, id: i32) -> Result<(), Errno> {
        if uid == 0 {
            return Ok(());
        }
        match idtype {
            P_PID => match self.children.get(&id) {
                Some(child) if child.owner == uid => Ok(()),
                Some(_) => Err(Errno::EPERM),
                None => Err(Errno::ECHILD),
            },
            P_PGID => {
                let mut found = false;
                for child in self.children.values().filter(|child| child.pgid == id) {
                    found = true;
                    if child.owner != uid {
                        return Err(Errno::EPERM);
                    }
                }
                if found { Ok(()) } else { Err(Errno::ECHILD) }
            }
            _ => {
                if self.children.is_empty() {
                    return Err(Errno::ECHILD);
                }
                if self.children.values().all(|child| child.owner == uid) {
                    Ok(())
                } else {
                    Err(Errno::EPERM)
                }
            }
        }
    }

    fn child_owned(&self, uid: u32, pid: i32) -> bool {
        self.children
            .get(&pid)
            .is_some_and(|child| child.owner == uid || uid == 0)
    }

    fn group_owned(&self, uid: u32, pgid: i32) -> bool {
        let mut found = false;
        for child in self.children.values().filter(|child| child.pgid == pgid) {
            found = true;
            if child.owner != uid && uid != 0 {
                return false;
            }
        }
        found
    }

    fn cleanup(&mut self, pid: i32, handle: Handle) {
        let _ = self.sys.signal(pid, SIGKILL);
        let _ = self.sys.wait_ex(P_PID, pid, 0);
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

/// Validate one 1024-bit `sigset_t` against the kernel's signal range.
///
/// Blocked and ignored sets may not name `SIGKILL` or `SIGSTOP`; the default-disposition set has
/// no such restriction (resetting those two is a no-op).
fn validate_signal_set(set: &[u8; 128], may_block: bool) -> Result<(), Errno> {
    for (index, byte) in set.iter().enumerate() {
        for bit in 0..8u32 {
            if byte & (1u8 << bit) == 0 {
                continue;
            }
            let signo = u32::try_from(index)
                .unwrap_or(u32::MAX)
                .saturating_mul(8)
                .saturating_add(bit)
                .saturating_add(1);
            if signo > SIG_MAX {
                return Err(Errno::EINVAL);
            }
            if may_block && (signo == SIGKILL || signo == SIGSTOP) {
                return Err(Errno::EINVAL);
            }
        }
    }
    Ok(())
}

fn is_terminal(status: i32) -> bool {
    status != CONTINUED_STATUS && (status & STOPPED_STATUS_MASK) != STOPPED_STATUS_MASK
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
