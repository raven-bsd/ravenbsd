// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers for the S39 aarch64 parity fixture.

#![no_std]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_abi::types::namespace;
use raven_initfs::Initfs;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_rt::sys::{syscall1, syscall2, syscall6};
use raven_scheme::Client;

/// The real service manager binary packed into the fixture initfs.
pub const SVCD_PATH: &str = "/svcd";
/// The fixture scheme server.
pub const SERVER_PATH: &str = "/server";
/// The fixture client.
pub const CLIENT_PATH: &str = "/client";
/// The service name registered by the real `svcd`.
pub const SERVICE: &str = "s39";
/// The scheme granted to the client through `PROC_NAMESPACE`.
pub const GRANTED_SCHEME: &str = "b47a";
/// A second registered scheme outside the granted prefix.
pub const DENIED_SCHEME: &str = "b47b";
/// `EVQ_CTL` add.
pub const EVQ_ADD: u32 = 1;
/// A user-event source is signalled.
pub const SIGNALED: usize = 8;
/// Event registration cookie used by the fixture.
pub const EVENT_COOKIE: u64 = 0x5339;

/// One event record written by `EVQ_WAIT`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EvqEvent {
    /// Opaque value supplied at registration.
    pub cookie: u64,
    /// Ready bits.
    pub events: u32,
    /// Padding.
    pub pad: u32,
}

/// Map and parse the boot initfs.
pub fn map_initfs() -> Option<Initfs<'static>> {
    let image = raven_rt::map_initfs().ok()?;
    Initfs::parse(image).ok()
}

/// Spawn a child from the initfs.
pub fn spawn_child(initfs: &Initfs<'_>, path: &str, args: &[&str]) -> Result<Handle, Errno> {
    spawn(initfs, path, &SpawnOptions { args, env: &[] }).map_err(|_| Errno::EIO)
}

/// Wait for a child and return its exit code.
pub fn wait(process: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    // SAFETY: `PROC_WAIT` writes one i32 to this live stack slot and validates the handle.
    let raw = unsafe {
        syscall2(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            &mut code as *mut i32 as usize,
        )
    };
    Errno::from_ret(raw).map(|_| code)
}

/// Open a path, retrying while the server is still registering.
pub fn open_retry(path: &str, flags: u32) -> Result<Handle, Errno> {
    let client = Client::new();
    let mut attempt = 0usize;
    while attempt < 200_000 {
        match client.open(path, flags) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) | Err(Errno::EIO) | Err(Errno::EACCES) => {
                core::hint::spin_loop();
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

/// Convert a raw handle result.
fn handle_from_ret(raw: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

/// Create an event queue.
pub fn evq_create() -> Result<Handle, Errno> {
    // SAFETY: EVQ_CREATE takes only the zero flags argument.
    handle_from_ret(unsafe { syscall1(Syscall::EVQ_CREATE as usize, 0) })
}

/// Create a user-event doorbell.
pub fn event_create() -> Result<Handle, Errno> {
    // SAFETY: EVENT_CREATE takes only the zero flags argument.
    handle_from_ret(unsafe { syscall1(Syscall::EVENT_CREATE as usize, 0) })
}

/// Add, modify or delete one event registration.
pub fn evq_ctl(
    queue: Handle,
    operation: u32,
    source: Handle,
    events: usize,
    cookie: u64,
) -> Result<(), Errno> {
    // SAFETY: the kernel validates the queue/source handles and the event mask.
    let raw = unsafe {
        syscall6(
            Syscall::EVQ_CTL as usize,
            queue.0 as usize,
            operation as usize,
            source.0 as usize,
            events,
            cookie as usize,
            0,
        )
    };
    Errno::from_ret(raw).map(|_| ())
}

/// Wait for up to `out.len()` ready records.
pub fn evq_wait(queue: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
    // SAFETY: `out` is a live writable slice and the kernel bounds the record count.
    let raw = unsafe {
        syscall6(
            Syscall::EVQ_WAIT as usize,
            queue.0 as usize,
            out.as_mut_ptr() as usize,
            out.len(),
            timeout_ns as usize,
            0,
            0,
        )
    };
    Errno::from_ret(raw)
}

/// Run one `PROC_NAMESPACE` operation.
pub fn namespace_op(target: Handle, op: u32, prefix: &str) -> Result<(), Errno> {
    // SAFETY: the kernel reads `prefix.len()` bytes from this live string and validates the
    // target handle, operation and prefix.
    let raw = unsafe {
        syscall6(
            Syscall::PROC_NAMESPACE as usize,
            target.0 as usize,
            op as usize,
            prefix.as_ptr() as usize,
            prefix.len(),
            0,
            0,
        )
    };
    Errno::from_ret(raw).map(|_| ())
}

/// Grant a scheme-name prefix to `target`.
pub fn namespace_add(target: Handle, prefix: &str) -> Result<(), Errno> {
    namespace_op(target, namespace::ADD, prefix)
}

/// Remove a scheme-name prefix from `target`.
pub fn namespace_remove(target: Handle, prefix: &str) -> Result<(), Errno> {
    namespace_op(target, namespace::REMOVE, prefix)
}

/// Clear every scheme-name prefix on `target` (exact names stay).
pub fn namespace_clear(target: Handle) -> Result<(), Errno> {
    namespace_op(target, namespace::CLEAR, "")
}
