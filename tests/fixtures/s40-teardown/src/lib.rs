// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared S40 fixture helpers: scheme paths, blocking waits and child spawn.

#![no_std]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::spawn::{RtSys, SpawnError, SpawnOptions, spawn};
use raven_rt::sys;

/// Read access bit for `OPEN`.
pub const O_READ: u32 = 1;
/// Write access bit for `OPEN`.
pub const O_WRITE: u32 = 2;
/// Read/write access for `OPEN`.
pub const O_RDWR: u32 = O_READ | O_WRITE;
/// Scheme path whose `WRITE` the server holds past the client's death.
pub const HOLD_PATH: &str = "s40:hold";
/// Scheme path used as a FIFO barrier after the churn.
pub const SYNC_PATH: &str = "s40:sync";
/// Scheme name registered by every faulting client (and re-registered by the name check).
pub const FAULT_SCHEME: &str = "s40-fault";
/// A user address deliberately left unmapped in the faulting client.
pub const FAULT_ADDRESS: usize = 0x3000_0000;

/// Wait for `process` and return its exit code.
pub fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = 0i32;
    RtSys.proc_wait(process, &mut code)?;
    Ok(code)
}

/// Open `path`, retrying while the server is still registering.
pub fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < 2000 {
        match sys::open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = sys::nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

/// Spawn `/path` from `initfs` with no arguments.
pub fn spawn_path(initfs: &Initfs<'_>, path: &str) -> Result<Handle, Errno> {
    spawn(
        initfs,
        path,
        &SpawnOptions {
            args: &[],
            env: &[],
        },
    )
    .map_err(|error| match error {
        SpawnError::Sys(error) => error,
        _ => Errno::EIO,
    })
}
