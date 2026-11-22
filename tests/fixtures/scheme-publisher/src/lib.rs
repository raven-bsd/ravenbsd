// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared K532 fixture helpers: initfs mapping, spawns and credential syscalls.

#![no_std]
#![allow(unsafe_code)] // Raw process syscalls; every block names the live argument's contract.

extern crate alloc;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_proto_fs::OpenFlags;
use raven_rt::spawn::{SpawnError, SpawnOptions};
use raven_scheme::Client;

/// Read-only initfs server started by init.
pub const INITFSD: &str = "/sbin/initfsd";
/// Unprivileged hostile scheme server.
pub const FAKE_SERVER: &str = "/sbin/fake";
/// Setuid root publisher used for the protected-name reclaim row.
pub const TRUSTED_PUBLISHER: &str = "/sbin/trusted";
/// Short-lived unprivileged probe for the before-boot file/vfs row.
pub const PROBE: &str = "/sbin/probe";
/// Plain child image served by the hostile scheme.
pub const CHILD: &str = "/bin/child";
/// Setuid child image in the trusted initfs.
pub const SETUID_CHILD: &str = "/bin/setuid-child";
/// Unprivileged hostile scheme name.
pub const FAKE_SCHEME: &str = "evil";
/// Fixture caller uid.
pub const UID: u32 = 1000;
/// Fixture caller gid.
pub const GID: u32 = 1000;
/// Raw `proc_exec_cred(proc, file, flags)`.
pub const PROC_EXEC_CRED: usize = 0x3F;
/// Raw `proc_pid_info(pid, out, count)`.
pub const PROC_PID_INFO: usize = 0x3A;

/// Map and parse the boot initfs.
pub fn initfs() -> Result<Initfs<'static>, &'static str> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map")?;
    Initfs::parse(image).map_err(|_| "initfs parse")
}

/// Spawn `path` directly from the mapped initfs.
pub fn spawn_initfs(
    initfs: &Initfs<'static>,
    path: &str,
    args: &[&str],
) -> Result<Handle, SpawnError> {
    raven_rt::spawn::spawn(initfs, path, &SpawnOptions { args, env: &[] })
}

/// Open `initfs:path` and spawn it through the native setuid/setgid path.
pub fn spawn_file(path: &str, args: &[&str], nosuid: bool) -> Result<Handle, SpawnError> {
    let client = Client::new();
    let open_path = alloc::format!("initfs:{path}");
    let file = client
        .open(&open_path, u32::from(OpenFlags::READ.to_wire()))
        .map_err(SpawnError::Sys)?;
    let result = raven_rt::spawn::spawn_file(file, path, &SpawnOptions { args, env: &[] }, nosuid);
    let _ = client.close(file);
    result
}

/// Retry a scheme open until the server is ready.
pub fn open_retry(path: &str, attempts: usize) -> Result<Handle, Errno> {
    let client = Client::new();
    let flags = u32::from(OpenFlags::READ.to_wire());
    let mut last = Err(Errno::ENOENT);
    let mut attempt = 0usize;
    while attempt < attempts {
        match client.open(path, flags) {
            Ok(handle) => return Ok(handle),
            Err(error) => last = Err(error),
        }
        let _ = raven_rt::nanosleep(10_000_000);
        attempt = attempt.saturating_add(1);
    }
    last
}

/// Wait for a child process.
pub fn wait(handle: Handle) -> Result<i32, Errno> {
    raven_rt::spawn::wait_process(handle.0)
}

/// Register a scheme name for the caller.
pub fn scheme_register(name: &str) -> Result<(), Errno> {
    raven_rt::sys::scheme_register(name)
}

/// Create an empty process with no threads.
pub fn create_empty() -> Result<Handle, Errno> {
    // SAFETY: PROC_CREATE takes only the scalar flags argument.
    let raw = unsafe { raven_rt::sys::syscall1(Syscall::PROC_CREATE as usize, 0) };
    let value = Errno::from_ret(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Apply the exec credential transition to an empty child.
pub fn exec_cred(child: Handle, file: Handle, nosuid: bool) -> Result<(), Errno> {
    // SAFETY: PROC_EXEC_CRED takes two scalar handles and the scalar flags word.
    let raw = unsafe {
        raven_rt::sys::syscall3(
            PROC_EXEC_CRED,
            child.0 as usize,
            file.0 as usize,
            usize::from(nosuid),
        )
    };
    Errno::from_ret(raw).map(|_| ())
}

/// Read `(euid, egid)` for a process handle.
pub fn child_ids(handle: Handle) -> Result<(u32, u32), Errno> {
    let pid = raven_rt::spawn::pid_for_handle(handle.0)?;
    let pid = usize::try_from(pid).map_err(|_| Errno::EINVAL)?;
    let mut words = [0u64; 8];
    // SAFETY: PROC_PID_INFO writes at most eight u64 identity words into the live array.
    let raw = unsafe {
        raven_rt::sys::syscall3(PROC_PID_INFO, pid, words.as_mut_ptr() as usize, words.len())
    };
    Errno::from_ret(raw)?;
    let euid = u32::try_from(words[5]).unwrap_or(0);
    let egid = u32::try_from(words[7]).unwrap_or(0);
    Ok((euid, egid))
}

/// Close a process or scheme handle.
pub fn close(handle: Handle) {
    let _ = raven_rt::spawn::close_handle(handle.0);
}
