// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `poll` over a per-thread cached EVQ.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_libc_abi::poll::{POLLERR, POLLHUP, POLLIN, POLLNVAL, POLLOUT, POLLPRI};
use raven_libc_abi::structs::Pollfd;
use raven_libc_abi::time::CLOCK_MONOTONIC;
use raven_libc_sys::sys::EvqEvent;
use raven_proto_net::socket::{READY_ERROR, READY_HUP, READY_READABLE, READY_WRITABLE};
use raven_proto_net::{Call, Response, SocketRequest};

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
    /// A scheme socket answers the live `Call::Poll` query; kernel-backed sources do not.
    scheme: bool,
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
            OfdKind::Tty
            | OfdKind::Pipe
            | OfdKind::Socket
            | OfdKind::EventFd
            | OfdKind::Timer
            | OfdKind::Unknown => {
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
                if reference.poll_signaled()
                    || matches!(reference.kind(), OfdKind::EventFd | OfdKind::Timer)
                {
                    // A scheme-backed socket reports through its attached notification event;
                    // an eventfd/timerfd reports through its kernel event handle, whose
                    // readiness bit is EV_SIGNALED.
                    wanted |= EV_SIGNALED;
                }
                let handle = reference.poll_handle();
                if let Some(registration) = registrations
                    .iter_mut()
                    .find(|known| known.reference.poll_handle() == handle)
                {
                    registration.events |= wanted | EV_HUP;
                    registration.entries.push(index);
                } else {
                    let scheme = reference.poll_signaled();
                    registrations.push(Registration {
                        reference,
                        events: wanted | EV_HUP,
                        entries: Vec::from([index]),
                        scheme,
                    });
                }
            }
            OfdKind::Epoll | OfdKind::ProcessFd => {
                entry.revents = POLLNVAL as i16;
                ready = ready.saturating_add(1);
            }
        }
    }
    // Scan live readiness before sleeping. A scheme socket's current level via `Call::Poll` is
    // the truth; the event registration below is only the wake hint. This also covers data that
    // arrived between the last read and this poll.
    let backend = platform.backend();
    for registration in &registrations {
        if !registration.scheme {
            continue;
        }
        // K409: the attached event is only the wake hint. The live level query must go to the
        // socket's data handle, not `poll_handle()` (which becomes the event once attached), or
        // a POLLOUT-only request never sees the socket's writable level.
        let Some(bits) = live_readiness(backend, registration.reference.handle()) else {
            continue;
        };
        ready = ready.saturating_add(apply_bits(registration, fds, bits));
    }
    if ready > 0 {
        return Ok(ready);
    }
    if registrations.is_empty() {
        return Ok(0);
    }
    let deadline = if timeout_ms < 0 {
        None
    } else {
        let timeout_ns = u64::try_from(timeout_ms)
            .unwrap_or(0)
            .saturating_mul(1_000_000);
        Some(monotonic_ns(backend).saturating_add(timeout_ns))
    };
    let mut events = [EvqEvent::default(); MAX_POLL_FDS];
    let mut attempt = 0usize;
    // C14/POSIX: a zero timeout is a non-blocking level query, not "no readiness". Allow the
    // first wait to run with a zero timeout so the current level is read; later iterations
    // honor the deadline.
    let mut first_wait = true;
    loop {
        let timeout_ns = match deadline {
            Some(deadline) => {
                let remaining = deadline.saturating_sub(monotonic_ns(backend));
                if remaining == 0 && !first_wait {
                    return Ok(0);
                }
                // EVQ_WAIT treats 0 as "wait forever". A zero-timeout poll/select is a
                // non-blocking level query: probe with 1 ns; the kernel collects ready events
                // before it sleeps, so a ready source returns immediately.
                if remaining == 0 { 1 } else { remaining }
            }
            None => 0,
        };
        first_wait = false;
        let queue = platform.event_queue()?;
        for (position, registration) in registrations.iter().enumerate() {
            if backend
                .evq_ctl(
                    queue,
                    EVQ_ADD,
                    registration.reference.poll_handle(),
                    registration.events,
                    u64::try_from(position).unwrap_or(0),
                )
                .is_err()
            {
                // POSIX: one unbacked/bad descriptor is POLLNVAL for that entry, never a
                // failure of the whole call. Remove the registrations added so far.
                for added in registrations.iter().take(position) {
                    let _ = backend.evq_ctl(queue, EVQ_DEL, added.reference.poll_handle(), 0, 0);
                }
                let mut invalid = 0usize;
                for &index in &registration.entries {
                    if let Some(entry) = fds.get_mut(index) {
                        entry.revents = POLLNVAL as i16;
                        invalid = invalid.saturating_add(1);
                    }
                }
                return Ok(invalid);
            }
        }
        let waited = backend.evq_wait(queue, &mut events, timeout_ns);
        for registration in &registrations {
            let _ = backend.evq_ctl(queue, EVQ_DEL, registration.reference.poll_handle(), 0, 0);
        }
        let count = match waited {
            Err(Errno::EBUSY) if attempt == 0 => {
                // A child inherited the parent's cached queue across fork; the queue allows
                // one waiter. Drop this process's handle and create its own queue, then retry.
                platform.reset_event_queue();
                attempt = 1;
                continue;
            }
            other => other?,
        };
        let mut fired = 0usize;
        for event in events.iter().take(count) {
            let Some(registration) = registrations.get(event.cookie as usize) else {
                continue;
            };
            if registration.scheme
                && let Some(bits) = live_readiness(backend, registration.reference.handle())
            {
                // Only the current live directions are a verdict; a delivered event is a
                // wake hint and may be stale.
                fired = fired.saturating_add(apply_bits(registration, fds, bits));
                continue;
            }
            fired = fired.saturating_add(apply_mask(registration, fds, event.events));
        }
        if fired > 0 {
            return Ok(fired);
        }
        // Spurious wake: re-scan and wait again with the remaining deadline.
        for registration in &registrations {
            if !registration.scheme {
                continue;
            }
            if let Some(bits) = live_readiness(backend, registration.reference.handle()) {
                let fired = apply_bits(registration, fds, bits);
                if fired > 0 {
                    return Ok(fired);
                }
            }
        }
        if let Some(deadline) = deadline
            && monotonic_ns(backend) >= deadline
        {
            return Ok(0);
        }
    }
}

/// Translate the live `Call::Poll` bits into `revents` for every entry that named the source.
fn apply_bits<B: SysBackend, L: Lock>(
    registration: &Registration<'_, B, L>,
    fds: &mut [Pollfd],
    bits: i32,
) -> usize {
    let mut fired = 0usize;
    for &index in &registration.entries {
        let Some(entry) = fds.get_mut(index) else {
            continue;
        };
        let requested = i32::from(entry.events);
        let mut revents = 0i16;
        if bits & READY_READABLE != 0 && requested & (POLLIN | POLLPRI) != 0 {
            revents |= POLLIN as i16;
        }
        if bits & READY_WRITABLE != 0 && requested & POLLOUT != 0 {
            revents |= POLLOUT as i16;
        }
        if bits & READY_HUP != 0 || registration.reference.is_eof() {
            revents |= POLLHUP as i16;
        }
        if bits & READY_ERROR != 0 {
            revents |= POLLERR as i16;
        }
        if revents != 0 {
            entry.revents = revents;
            fired = fired.saturating_add(1);
        }
    }
    fired
}

/// Translate a kernel event mask (socketpair/pipe/tty) into `revents`.
fn apply_mask<B: SysBackend, L: Lock>(
    registration: &Registration<'_, B, L>,
    fds: &mut [Pollfd],
    mask: u32,
) -> usize {
    let mut fired = 0usize;
    for &index in &registration.entries {
        let Some(entry) = fds.get_mut(index) else {
            continue;
        };
        let requested = i32::from(entry.events);
        let revents = delivered_revents(mask, requested, registration.reference.is_eof());
        if revents != 0 {
            entry.revents = revents;
            fired = fired.saturating_add(1);
        }
    }
    fired
}

/// Non-consuming readiness query for a scheme socket (`Call::Poll`, K244).
fn live_readiness<B: SysBackend>(backend: &B, handle: Handle) -> Option<i32> {
    let mut request = [0u8; 8];
    let used = SocketRequest::Call(Call::Poll).encode(&mut request).ok()?;
    let mut response = [0u8; 16];
    let count = backend
        .call(handle, request.get(..used)?, &mut response)
        .ok()?;
    match Response::decode(response.get(..count)?).ok()? {
        Response::Value(bits) => Some(bits),
        _ => None,
    }
}

/// Monotonic nanoseconds, used for the remaining poll deadline.
fn monotonic_ns<B: SysBackend>(backend: &B) -> u64 {
    let Ok(time) = backend.clock_get(CLOCK_MONOTONIC as u32) else {
        return 0;
    };
    let sec = u64::try_from(time.tv_sec).unwrap_or(0);
    let nsec = u64::try_from(time.tv_nsec).unwrap_or(0);
    sec.saturating_mul(1_000_000_000).saturating_add(nsec)
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

/// Wait until `fd` is writable. Used to emulate a blocking pipe write over the non-blocking
/// kernel pipe endpoint created by `Platform::pipe2`.
pub fn wait_writable<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    fd: i32,
    timeout_ms: i32,
) -> Result<usize, Errno> {
    let mut fds = [Pollfd {
        fd,
        events: POLLOUT as i16,
        revents: 0,
    }];
    poll(platform, &mut fds, timeout_ms)
}

/// Translate a kernel readiness mask into the caller's `poll` bits.
///
/// `eof` is the sticky tty-master hangup observed by `read`; it is the pty server's `POLLHUP`
/// until the scheme-file readiness path (PG-25) reports the level directly.
fn delivered_revents(mask: u32, requested: i32, eof: bool) -> i16 {
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
    if mask & EV_HUP != 0 || eof {
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
            if value.tv_sec < 0 || value.tv_nsec < 0 || value.tv_nsec >= 1_000_000_000 {
                return Err(Errno::EINVAL);
            }
            let nanos = i128::from(value.tv_sec)
                .saturating_mul(1_000_000_000)
                .saturating_add(i128::from(value.tv_nsec));
            let millis = nanos.saturating_add(999_999) / 1_000_000;
            i32::try_from(millis).unwrap_or(i32::MAX)
        }
    };
    poll(platform, fds, timeout_ms)
}
