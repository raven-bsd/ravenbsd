// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `select` and `pselect` over `poll` over `EVQ`.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::generated::signal::SIG_SETMASK;
use raven_libc_abi::poll::{POLLIN, POLLOUT, POLLPRI};
use raven_libc_abi::structs::{Pollfd, Timeval};

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;
use crate::poll::{FD_SETSIZE, poll};

fn bit(set: &[u64], fd: usize) -> bool {
    set.get(fd / 64)
        .is_some_and(|word| word & (1u64 << (fd % 64)) != 0)
}

fn set_bit(set: &mut [u64], fd: usize) {
    if let Some(word) = set.get_mut(fd / 64) {
        *word |= 1u64 << (fd % 64);
    }
}

fn clear(set: &mut [u64]) {
    for word in set.iter_mut() {
        *word = 0;
    }
}

/// `select`.
pub fn select<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    nfds: i32,
    readfds: &mut [u64],
    writefds: &mut [u64],
    exceptfds: &mut [u64],
    timeout: Option<&Timeval>,
) -> Result<i32, Errno> {
    if nfds < 0 {
        return Err(Errno::EINVAL);
    }
    let nfds = usize::try_from(nfds).map_err(|_| Errno::EINVAL)?;
    if nfds > FD_SETSIZE {
        return Err(Errno::EINVAL);
    }
    let mut fds = Vec::new();
    for fd in 0..nfds {
        let mut events = 0i16;
        if bit(readfds, fd) {
            events |= POLLIN as i16;
        }
        if bit(writefds, fd) {
            events |= POLLOUT as i16;
        }
        if bit(exceptfds, fd) {
            events |= POLLPRI as i16;
        }
        fds.push(Pollfd {
            fd: fd as i32,
            events,
            revents: 0,
        });
    }
    let timeout_ms = match timeout {
        Some(value) => {
            if value.tv_sec < 0 || value.tv_usec < 0 || value.tv_usec >= 1_000_000 {
                return Err(Errno::EINVAL);
            }
            i32::try_from(
                value
                    .tv_sec
                    .saturating_mul(1000)
                    .saturating_add(value.tv_usec.saturating_add(999) / 1000),
            )
            .map_err(|_| Errno::EINVAL)?
        }
        None => -1,
    };
    let ready = poll(platform, &mut fds, timeout_ms)?;
    if !readfds.is_empty() {
        clear(readfds);
    }
    if !writefds.is_empty() {
        clear(writefds);
    }
    if !exceptfds.is_empty() {
        clear(exceptfds);
    }
    for entry in &fds {
        if entry.revents == 0 || entry.fd < 0 {
            continue;
        }
        let fd = entry.fd as usize;
        if entry.revents as i32 & POLLIN != 0 {
            set_bit(readfds, fd);
        }
        if entry.revents as i32 & POLLOUT != 0 {
            set_bit(writefds, fd);
        }
        if entry.revents as i32 & POLLPRI != 0 {
            set_bit(exceptfds, fd);
        }
    }
    Ok(ready as i32)
}

/// The signal-mask operations `pselect` needs from the kernel.
///
/// The poll layer owns the install/wait/restore order; the kernel owns the live mask. This keeps
/// the C export free of mask state and lets a host test observe the exact swap sequence.
pub trait MaskOps {
    /// Install `set` as the calling thread's mask and return the previous mask word.
    fn install(&mut self, set: u64) -> Result<u64, Errno>;
    /// Restore the previous mask word.
    fn restore(&mut self, old: u64) -> Result<(), Errno>;
}

/// The kernel `SIGPROCMASK` implementation of [`MaskOps`].
///
/// The kernel mask is one word; the high words of the caller's 128-byte `sigset_t` are ignored
/// exactly as `sigprocmask` ignores them.
pub struct KernelMasks;

impl MaskOps for KernelMasks {
    fn install(&mut self, set: u64) -> Result<u64, Errno> {
        let mut old = 0u64;
        raven_libc_sys::signal::sigprocmask(SIG_SETMASK, Some(set), Some(&mut old))?;
        Ok(old)
    }

    fn restore(&mut self, old: u64) -> Result<(), Errno> {
        raven_libc_sys::signal::sigprocmask(SIG_SETMASK, Some(old), None)
    }
}

/// `pselect`: install the temporary mask, wait, and restore the caller mask on every return.
///
/// This is the non-atomic timeout path the S750 `t_select.c` row requires. A signal that arrives
/// between the install and the `EVQ_WAIT` still sees the temporary mask; the atomic masked wait
/// remains PG-26/S173b and is not claimed here. A restore failure after a successful wait is
/// returned to the caller; after a failed wait the wait error wins.
#[allow(clippy::too_many_arguments)] // mirrors the pselect C ABI and the select signature
pub fn pselect<B: SysBackend, L: Lock, M: MaskOps>(
    platform: &Platform<B, L>,
    nfds: i32,
    readfds: &mut [u64],
    writefds: &mut [u64],
    exceptfds: &mut [u64],
    timeout: Option<&raven_libc_abi::structs::Timespec>,
    sigmask: Option<&[u64]>,
    masks: &mut M,
) -> Result<i32, Errno> {
    let saved = match sigmask {
        Some(set) => Some(masks.install(set.first().copied().unwrap_or(0))?),
        None => None,
    };
    let result = match timeout {
        Some(value) if value.tv_sec < 0 || value.tv_nsec < 0 || value.tv_nsec >= 1_000_000_000 => {
            Err(Errno::EINVAL)
        }
        Some(value) => {
            // `select` takes microseconds; round up so the wait never ends before the caller's
            // nanosecond deadline at the platform's microsecond granularity.
            let mut seconds = value.tv_sec;
            let mut micros = (value.tv_nsec + 999) / 1000;
            if micros >= 1_000_000 {
                seconds = seconds.saturating_add(1);
                micros = 0;
            }
            let timeval = Timeval {
                tv_sec: seconds,
                tv_usec: micros,
            };
            select(platform, nfds, readfds, writefds, exceptfds, Some(&timeval))
        }
        None => select(platform, nfds, readfds, writefds, exceptfds, None),
    };
    let Some(old) = saved else {
        return result;
    };
    match result {
        Ok(ready) => {
            masks.restore(old)?;
            Ok(ready)
        }
        Err(error) => {
            let _ = masks.restore(old);
            Err(error)
        }
    }
}
