// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! SysV IPC policy (`sys/ipc.h`, `sys/msg.h`, `sys/sem.h`, `sys/shm.h`).
//!
//! `ftok` is a pure derivation over `stat(2)` data and is implemented for real. The object
//! calls (`msgget`, `semget`, `shmget`, ...) need kernel objects that do not exist, so they
//! report [`unsupported`] instead of inventing identifiers.

use raven_libc_abi::errno::Errno;

use crate::consts::ENOSYS;

/// `IPC_PRIVATE` (a private key).
pub const IPC_PRIVATE: i32 = 0;

/// The answer every SysV object operation gives while no kernel object service exists.
#[must_use]
pub const fn unsupported() -> Errno {
    ENOSYS
}

/// Whether `key` asks for a private object.
#[must_use]
pub const fn key_is_private(key: i32) -> bool {
    key == IPC_PRIVATE
}

/// Derive the `ftok` key from `stat(2)` data.
///
/// This is the established POSIX derivation: the low 16 bits of the inode number, the low 8
/// bits of the device number in bits 16-23, and the low 8 bits of `id` in bits 24-31. The
/// result is returned as the signed `key_t` bit pattern.
#[must_use]
pub fn ftok_key(dev: u64, ino: u64, id: u8) -> i32 {
    let inode = (ino & 0xffff) as u32;
    let device = ((dev & 0xff) as u32) << 16;
    let identifier = (id as u32) << 24;
    (inode | device | identifier) as i32
}

#[cfg(test)]
#[path = "sysv_ut.rs"]
mod tests;
