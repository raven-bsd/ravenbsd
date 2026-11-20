// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `poll` over a per-thread cached EVQ.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::poll::{POLLERR, POLLHUP, POLLIN, POLLNVAL, POLLOUT, POLLPRI};
use raven_libc_abi::structs::Pollfd;
use raven_libc_sys::sys::EvqEvent;

use crate::backend::SysBackend;
use crate::fd::ofd::{OfdKind, OfdRef};
use crate::lock::Lock;
use crate::platform::Platform;
use crate::poll::{EV_HUP, EV_READABLE, EV_SIGNALED, EV_WRITABLE, MAX_POLL_FDS};

const EVQ_ADD: u32 = 1;
const EVQ_DEL: u32 = 3;

/// One event-queue registration and every `pollfd` entry that names the same source.
struct Registration<'a, B: SysBackend, L: Lock> {
    reference: OfdRef<'a, B, L>,
    events: u32,
    entries: Vec<usize>,
    rearm: u32,
}

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
    let mut registrations: Vec<Registration<'_, B, L>> = Vec::new();
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
                // An entry that asks for nothing is not ready; it used to count as ready and
                // made a caller with an idle `pollfd` spin.
                if wanted == 0 {
                    continue;
                }
                if reference.poll_signaled() {
                    // A scheme-backed socket reports through its attached notification event,
                    // not through the data handle's level.
                    wanted |= EV_SIGNALED;
                }
                let handle = reference.poll_handle();
                if let Some(registration) = registrations
                    .iter_mut()
                    .find(|known| known.reference.poll_handle() == handle)
                {
                    registration.events |= wanted | EV_HUP;
                    registration.entries.push(index);
                    let revents = delivered_revents(registration.rearm, events);
                    if revents != 0 {
                        entry.revents = revents;
                        ready = ready.saturating_add(1);
                    }
                } else {
                    // The kernel consumes the scheme event on delivery. Replay the last
                    // delivered level once so data still queued is not lost.
                    let rearm = reference.take_poll_rearm();
                    let revents = delivered_revents(rearm, events);
                    if revents != 0 {
                        entry.revents = revents;
                        ready = ready.saturating_add(1);
                    }
                    registrations.push(Registration {
                        reference,
                        events: wanted | EV_HUP,
                        entries: Vec::from([index]),
                        rearm,
                    });
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
    let backend = platform.backend();
    let mut events = [EvqEvent::default(); MAX_POLL_FDS];
    let mut attempt = 0usize;
    let count = loop {
        let queue = platform.event_queue()?;
        for (position, registration) in registrations.iter().enumerate() {
            if let Err(error) = backend.evq_ctl(
                queue,
                EVQ_ADD,
                registration.reference.poll_handle(),
                registration.events,
                u64::try_from(position).unwrap_or(0),
            ) {
                for added in registrations.iter().take(position) {
                    let _ = backend.evq_ctl(queue, EVQ_DEL, added.reference.poll_handle(), 0, 0);
                }
                return Err(error);
            }
        }
        let waited = backend.evq_wait(queue, &mut events, timeout_ns);
        for registration in &registrations {
            let _ = backend.evq_ctl(queue, EVQ_DEL, registration.reference.poll_handle(), 0, 0);
        }
        match waited {
            Err(Errno::EBUSY) if attempt == 0 => {
                // A child inherited the parent's cached queue across fork; the queue allows
                // one waiter. Drop this process's handle and create its own queue, then retry.
                platform.reset_event_queue();
                attempt = 1;
            }
            other => break other?,
        }
    };
    let mut fired = 0usize;
    for event in events.iter().take(count) {
        let Some(registration) = registrations.get(event.cookie as usize) else {
            continue;
        };
        registration.reference.set_poll_rearm(event.events);
        for &index in &registration.entries {
            let Some(entry) = fds.get_mut(index) else {
                continue;
            };
            let requested = i32::from(entry.events);
            let revents = delivered_revents(event.events, requested);
            if revents != 0 {
                entry.revents = revents;
                fired = fired.saturating_add(1);
            }
        }
    }
    Ok(fired)
}

/// Wait until `fd` is readable. Used by a blocking socket read after netd reports `EAGAIN`.
pub fn wait_readable<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    timeout_ms: i32,
) -> Result<usize, Errno> {
    let mut fds = [Pollfd {
        fd,
        events: POLLIN as i16,
        revents: 0,
    }];
    poll(platform, &mut fds, timeout_ms)
}

/// Translate a kernel readiness mask into the caller's `poll` bits.
fn delivered_revents(mask: u32, requested: i32) -> i16 {
    let mut revents = 0i16;
    if mask & EV_SIGNALED != 0 {
        // The notification event is level-only: report the caller's requested bits.
        if requested & (POLLIN | POLLPRI) != 0 {
            revents |= POLLIN as i16;
        }
        if requested & POLLOUT != 0 {
            revents |= POLLOUT as i16;
        }
    }
    if mask & EV_READABLE != 0 && requested & (POLLIN | POLLPRI) != 0 {
        revents |= POLLIN as i16;
    }
    if mask & EV_WRITABLE != 0 && requested & POLLOUT != 0 {
        revents |= POLLOUT as i16;
    }
    if mask & EV_HUP != 0 {
        revents |= POLLHUP as i16;
    }
    revents
}

/// `ppoll`.
///
/// The atomic signal-mask swap (`PG-26`) is not implemented; OpenSSH's packet path passes a
/// `NULL` mask. Poll with the caller's current mask and round the absolute timeout up to the
/// poll millisecond granularity. The mask is accepted but not swapped until PG-26 lands.
pub fn ppoll<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fds: &mut [Pollfd],
    timeout: Option<&raven_libc_abi::structs::Timespec>,
    _sigmask: Option<&[u64]>,
) -> Result<usize, Errno> {
    let timeout_ms = match timeout {
        None => -1,
        Some(value) => {
            if value.tv_sec < 0 || value.tv_nsec < 0 {
                0
            } else {
                let nanos = i128::from(value.tv_sec)
                    .saturating_mul(1_000_000_000)
                    .saturating_add(i128::from(value.tv_nsec));
                let millis = nanos.saturating_add(999_999) / 1_000_000;
                i32::try_from(millis).unwrap_or(i32::MAX)
            }
        }
    };
    poll(platform, fds, timeout_ms)
}
