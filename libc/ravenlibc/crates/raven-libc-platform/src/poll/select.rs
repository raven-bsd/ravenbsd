// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `select` and `pselect` over `poll` over `EVQ`.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
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

/// `pselect`: the atomic mask swap needs PG-26; until then `ENOSYS`.
pub fn pselect<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _nfds: i32,
    _readfds: &mut [u64],
    _writefds: &mut [u64],
    _exceptfds: &mut [u64],
    _timeout: Option<&raven_libc_abi::structs::Timespec>,
    _sigmask: Option<&[u64]>,
) -> Result<i32, Errno> {
    Err(Errno::ENOSYS)
}
