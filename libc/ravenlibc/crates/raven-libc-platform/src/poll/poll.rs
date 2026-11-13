// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `poll` over a per-thread cached EVQ.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::poll::{POLLERR, POLLHUP, POLLIN, POLLNVAL, POLLOUT, POLLPRI};
use raven_libc_abi::structs::Pollfd;
use raven_libc_sys::sys::EvqEvent;

use crate::backend::SysBackend;
use crate::fd::ofd::OfdKind;
use crate::lock::Lock;
use crate::platform::Platform;
use crate::poll::{EV_HUP, EV_READABLE, EV_WRITABLE, MAX_POLL_FDS};

const EVQ_ADD: u32 = 1;
const EVQ_DEL: u32 = 3;

/// `poll`.
pub fn poll<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fds: &mut [Pollfd],
    timeout_ms: i32,
) -> Result<usize, Errno> {
    if fds.len() > MAX_POLL_FDS {
        return Err(Errno::EINVAL);
    }
    let mut ready = 0usize;
    let mut registrations: Vec<(usize, raven_abi::types::Handle, u32)> = Vec::new();
    for (index, entry) in fds.iter_mut().enumerate() {
        entry.revents = 0;
        if entry.fd < 0 {
            continue;
        }
        let reference = match platform.get(entry.fd) {
            Ok(reference) => reference,
            Err(_) => {
                entry.revents = POLLNVAL as i16;
                ready = ready.saturating_add(1);
                continue;
            }
        };
        let events = i32::from(entry.events);
        match reference.kind() {
            OfdKind::File | OfdKind::Dir => {
                let mask = events & (POLLIN | POLLOUT | POLLPRI | POLLERR);
                if mask != 0 {
                    entry.revents = mask as i16;
                    ready = ready.saturating_add(1);
                }
            }
            OfdKind::Tty | OfdKind::Pipe | OfdKind::Socket | OfdKind::Unknown => {
                let mut wanted = 0u32;
                if events & (POLLIN | POLLPRI) != 0 {
                    wanted |= EV_READABLE;
                }
                if events & POLLOUT != 0 {
                    wanted |= EV_WRITABLE;
                }
                if wanted == 0 {
                    ready = ready.saturating_add(1);
                } else {
                    registrations.push((index, reference.handle(), wanted | EV_HUP));
                }
            }
            OfdKind::Epoll | OfdKind::Timer | OfdKind::EventFd | OfdKind::ProcessFd => {
                entry.revents = POLLNVAL as i16;
                ready = ready.saturating_add(1);
            }
        }
    }
    if ready > 0 {
        return Ok(ready);
    }
    if registrations.is_empty() || timeout_ms == 0 {
        return Ok(0);
    }
    let timeout_ns = if timeout_ms < 0 {
        0
    } else {
        u64::try_from(timeout_ms)
            .unwrap_or(0)
            .saturating_mul(1_000_000)
    };
    let queue = platform.event_queue()?;
    let backend = platform.backend();
    for (index, handle, events) in &registrations {
        backend.evq_ctl(
            queue,
            EVQ_ADD,
            *handle,
            *events,
            u64::try_from(*index).unwrap_or(0),
        )?;
    }
    let mut events = [EvqEvent::default(); MAX_POLL_FDS];
    let waited = backend.evq_wait(queue, &mut events, timeout_ns);
    for (_, handle, _) in &registrations {
        let _ = backend.evq_ctl(queue, EVQ_DEL, *handle, 0, 0);
    }
    let count = waited?;
    let mut fired = 0usize;
    for event in events.iter().take(count) {
        let index = event.cookie as usize;
        if let Some(entry) = fds.get_mut(index) {
            let mut revents = 0i16;
            if event.events & EV_READABLE != 0 {
                revents |= POLLIN as i16;
            }
            if event.events & EV_WRITABLE != 0 {
                revents |= POLLOUT as i16;
            }
            if event.events & EV_HUP != 0 {
                revents |= POLLHUP as i16;
            }
            if revents != 0 {
                entry.revents = revents;
                fired = fired.saturating_add(1);
            }
        }
    }
    Ok(fired)
}

/// `ppoll`: the signal mask swap must be atomic with the wait (PG-26); until then `ENOSYS`.
pub fn ppoll<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _fds: &mut [Pollfd],
    _timeout: Option<&raven_libc_abi::structs::Timespec>,
    _sigmask: Option<&[u64]>,
) -> Result<usize, Errno> {
    Err(Errno::ENOSYS)
}
