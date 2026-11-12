// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers for the B47a aarch64 scheme-IPC fixture.

#![no_std]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

/// The real service manager binary packed into the fixture initfs.
pub const SVCD_PATH: &str = "/svcd";
/// The fixture scheme server.
pub const SERVER_PATH: &str = "/server";
/// The fixture client.
pub const CLIENT_PATH: &str = "/client";
/// The service name served by the real `svc:` scheme.
pub const SERVICE: &str = "b47a";

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
        raven_rt::sys::syscall2(
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
            Err(Errno::ENOENT) | Err(Errno::EIO) => {
                core::hint::spin_loop();
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}
