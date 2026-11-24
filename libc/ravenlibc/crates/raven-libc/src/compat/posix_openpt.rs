// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `posix_openpt`: allocate a `pty:` master pair.
//!
//! The clone device is `pty:ptmx`; the pty server owns the unit pool and allocates the lowest free
//! pair, exactly as the NetBSD/FreeBSD clone drivers do. The pair id is read back from the
//! endpoint inode (`2N+1` master) and remembered per descriptor so `ptsname_r` can name the
//! matching `pty:slave/<id>`.
use core::ffi::c_int;
use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use raven_libc_abi::errno::{EIO, EMFILE};
use raven_libc_platform::xlat::open_flags;
use raven_libc_platform::{OfdKind, RealSys, SysBackend, scheme::open_scheme_path};

/// How many descriptor -> pair bindings one process can track.
const SLOTS: usize = 64;

/// `(fd, pair id)` slots; an fd of `-1` is free.
static PAIRS: [(AtomicI32, AtomicU64); SLOTS] =
    [const { (AtomicI32::new(-1), AtomicU64::new(0)) }; SLOTS];

/// Remember `fd` -> `pair`; a reused descriptor overwrites its old binding.
fn remember(fd: c_int, pair: u64) -> bool {
    let mut free = None;
    for slot in &PAIRS {
        if slot.0.load(Ordering::Relaxed) == fd {
            slot.1.store(pair, Ordering::Relaxed);
            return true;
        }
        if free.is_none() && slot.0.load(Ordering::Relaxed) < 0 {
            free = Some(slot);
        }
    }
    if let Some(slot) = free {
        slot.1.store(pair, Ordering::Relaxed);
        slot.0.store(fd, Ordering::Relaxed);
        return true;
    }
    false
}

/// The pair id bound to `fd`.
pub(crate) fn pair_for(fd: c_int) -> Option<u64> {
    // A negative descriptor has no pty pair. `-1` is also the empty-slot sentinel in `PAIRS`;
    // without this guard `pair_for(-1)` matches the first empty slot and reports pair 0.
    if fd < 0 {
        return None;
    }
    for slot in &PAIRS {
        if slot.0.load(Ordering::Relaxed) == fd {
            return Some(slot.1.load(Ordering::Relaxed));
        }
    }
    None
}

/// `posix_openpt`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_openpt(flags: c_int) -> c_int {
    let spec = match open_flags(flags as u32) {
        Ok(spec) => spec,
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            return -1;
        }
    };
    let handle = match open_scheme_path(&RealSys, b"pty:ptmx", spec.wire) {
        Ok(handle) => handle,
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            return -1;
        }
    };
    let platform = crate::ffi::platform::global();
    let fd = match platform.install(handle, OfdKind::Tty, spec.status, None, spec.cloexec) {
        Ok(fd) => fd,
        Err(error) => {
            let _ = RealSys.close(handle);
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error.0)) };
            return -1;
        }
    };
    // The clone endpoint is the pty master; record the role so the poll/EOF path does not have to
    // guess it from a path (`posix_openpt` installs this master with no public name).
    if let Ok(reference) = platform.get(fd) {
        reference.set_pty_master();
    }
    let pair = match platform.fstat(fd) {
        Ok(stat) if stat.st_ino >= 3 => (stat.st_ino - 1) / 2,
        _ => {
            let _ = platform.close(fd);
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(EIO as c_int) };
            return -1;
        }
    };
    if !remember(fd, pair) {
        let _ = platform.close(fd);
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(EMFILE as c_int) };
        return -1;
    }
    fd
}
