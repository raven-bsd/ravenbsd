// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `posix_openpt`: allocate a `pty:` master pair.
//!
//! Each call takes the next process-local pair id and opens `pty:master/<id>` on the real pty
//! server. The id is remembered per descriptor so `ptsname_r` can name the matching
//! `pty:slave/<id>`; the pty server creates the pair on first open and keeps its own line
//! discipline.
use core::ffi::c_int;
use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use raven_libc_abi::errno::{EBADF, EMFILE, ENOSYS};
use raven_libc_platform::xlat::open_flags;
use raven_libc_platform::{OfdKind, RealSys, SysBackend, scheme::open_scheme_path};

/// How many descriptor -> pair bindings one process can track.
const SLOTS: usize = 64;

/// `(fd, pair id)` slots; an fd of `-1` is free.
static PAIRS: [(AtomicI32, AtomicU64); SLOTS] =
    [const { (AtomicI32::new(-1), AtomicU64::new(0)) }; SLOTS];
/// The next pair id. Pair 0 is the historical `pty:master` used by the tty fixture.
static NEXT_PAIR: AtomicU64 = AtomicU64::new(1);

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
    for slot in &PAIRS {
        if slot.0.load(Ordering::Relaxed) == fd {
            return Some(slot.1.load(Ordering::Relaxed));
        }
    }
    None
}

/// Format `pty:master/<id>` without allocating.
fn master_path(pair: u64, out: &mut [u8; 32]) -> Option<usize> {
    const PREFIX: &[u8] = b"pty:master/";
    let mut digits = [0u8; 20];
    let mut value = pair;
    let mut count = 0usize;
    loop {
        digits[count] = b'0' + u8::try_from(value % 10).ok()?;
        count = count.saturating_add(1);
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let total = PREFIX.len().checked_add(count)?;
    if total > out.len() {
        return None;
    }
    out.get_mut(..PREFIX.len())?.copy_from_slice(PREFIX);
    for index in 0..count {
        out[PREFIX.len() + index] = digits[count - 1 - index];
    }
    Some(total)
}

fn open_master(flags: c_int) -> Result<c_int, u16> {
    let pair = NEXT_PAIR.fetch_add(1, Ordering::Relaxed);
    let mut path = [0u8; 32];
    let length = master_path(pair, &mut path).ok_or(ENOSYS as u16)?;
    let spec = open_flags(flags as u32).map_err(|error| error.0)?;
    let handle = open_scheme_path(&RealSys, path.get(..length).ok_or(EBADF as u16)?, spec.wire)
        .map_err(|error| error.0)?;
    let fd = crate::ffi::platform::global()
        .install(handle, OfdKind::Tty, spec.status, None, spec.cloexec)
        .map_err(|error| error.0);
    let fd = match fd {
        Ok(fd) => fd,
        Err(error) => {
            let _ = RealSys.close(handle);
            return Err(error);
        }
    };
    if !remember(fd, pair) {
        // SAFETY: `fd` is the descriptor just installed by this call.
        unsafe {
            super::close(fd);
        }
        return Err(EMFILE as u16);
    }
    Ok(fd)
}

/// `posix_openpt`.
///
/// # Safety
/// Scalar arguments only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn posix_openpt(flags: c_int) -> c_int {
    match open_master(flags) {
        Ok(fd) => fd,
        Err(error) => {
            // SAFETY: the errno cell is this thread's.
            unsafe { super::set_errno(c_int::from(error)) };
            -1
        }
    }
}
