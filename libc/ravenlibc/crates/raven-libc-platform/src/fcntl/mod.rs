// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The POSIX record-lock translation over [`crate::SysBackend::record_lock`].

use raven_abi::types::fcntl::{GETLK, RDLCK, RecordLock, SETLK, SETLKW, UNLCK, WRLCK};
use raven_libc_abi::fcntl::{F_GETLK, F_RDLCK, F_SETLK, F_SETLKW, F_UNLCK, F_WRLCK};
use raven_libc_abi::structs::Flock;

use crate::Errno;
use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;

/// Apply one `F_GETLK`/`F_SETLK`/`F_SETLKW` to the file behind `fd`.
///
/// The libc `struct flock` uses the POSIX `F_*` type/command values; the kernel record is the
/// `raven_abi` `RecordLock` with the same 32-byte layout. `F_GETLK` copies the first conflicting
/// lock back into `lock`; `F_SETLK` returns `EAGAIN` on a conflict and `F_SETLKW` blocks in the
/// kernel until the range is free or a signal interrupts it.
///
/// # Errors
/// [`Errno::EBADF`] for a bad descriptor, [`Errno::EINVAL`] for an unknown command or lock type,
/// and the kernel's `EAGAIN`/`EINTR`/`EINVAL` value.
pub fn record_lock<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    command: i32,
    lock: &mut Flock,
) -> Result<(), Errno> {
    let wire_command = match command {
        F_GETLK => GETLK,
        F_SETLK => SETLK,
        F_SETLKW => SETLKW,
        _ => return Err(Errno::EINVAL),
    };
    let mut wire = RecordLock {
        l_type: wire_type(lock.l_type)?,
        l_whence: lock.l_whence,
        l_start: lock.l_start,
        l_len: lock.l_len,
        l_pid: lock.l_pid,
    };
    let reference = platform.get(fd)?;
    let handle = reference.handle();
    drop(reference);
    platform
        .backend()
        .record_lock(handle, wire_command, &mut wire)?;
    if wire_command == GETLK {
        lock.l_type = libc_type(wire.l_type)?;
        lock.l_whence = wire.l_whence;
        lock.l_start = wire.l_start;
        lock.l_len = wire.l_len;
        lock.l_pid = wire.l_pid;
    }
    Ok(())
}

fn wire_type(kind: i16) -> Result<i16, Errno> {
    let libc = i32::from(kind);
    if libc == F_RDLCK {
        Ok(RDLCK)
    } else if libc == F_WRLCK {
        Ok(WRLCK)
    } else if libc == F_UNLCK {
        Ok(UNLCK)
    } else {
        Err(Errno::EINVAL)
    }
}

fn libc_type(kind: i16) -> Result<i16, Errno> {
    let wire = kind;
    if wire == RDLCK {
        Ok(i16::try_from(F_RDLCK).map_err(|_| Errno::EINVAL)?)
    } else if wire == WRLCK {
        Ok(i16::try_from(F_WRLCK).map_err(|_| Errno::EINVAL)?)
    } else if wire == UNLCK {
        Ok(i16::try_from(F_UNLCK).map_err(|_| Errno::EINVAL)?)
    } else {
        Err(Errno::EINVAL)
    }
}

#[cfg(test)]
#[path = "fcntl_ut.rs"]
mod tests;
