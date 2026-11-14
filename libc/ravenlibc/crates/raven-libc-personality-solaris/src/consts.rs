// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The constants of the Solaris personality, in ONE file, labelled by source.
//!
//! Only two numbers are public Solaris ABI facts (`donor-map/14` 3.B.3.e):
//! `PORT_SOURCE_TIMER 1` and `PORT_SOURCE_FD 4`. Every other value below is chosen by ticket
//! C67 4.3 (PROPOSED, RD-new): source compatibility is the promise, never binary compatibility.
//! `include/port.h`, `include/thread.h` and `include/synch.h` mirror these values and
//! `tests/header_parity.rs` asserts equality.
//!
//! CLEANROOM: `06-solaris-design.md` A-sol-47 and C67 4.3.

use raven_libc_abi::structs::Timespec;
use raven_libc_platform::Errno;

/// `PORT_SOURCE_TIMER`: a `timer_create` source (the value from `donor-map/14` 3.B.3.e).
pub const PORT_SOURCE_TIMER: i32 = 1;
/// `PORT_SOURCE_USER`: a `port_send` source (**ours**, C67 4.3).
pub const PORT_SOURCE_USER: i32 = 3;
/// `PORT_SOURCE_FD`: descriptor readiness (the value from `donor-map/14` 3.B.3.e).
pub const PORT_SOURCE_FD: i32 = 4;
/// `PORT_SOURCE_ALERT`: reserved; never delivered by this ticket (**ours**, C67 4.3).
pub const PORT_SOURCE_ALERT: i32 = 5;

/// `THR_BOUND`: accepted and ignored (**ours**).
pub const THR_BOUND: i64 = 0x1;
/// `THR_NEW_LWP`: accepted and ignored (**ours**).
pub const THR_NEW_LWP: i64 = 0x2;
/// `THR_DETACHED`: create the thread detached (**ours**).
pub const THR_DETACHED: i64 = 0x4;
/// `THR_SUSPENDED`: create the thread suspended (**ours**; `ENOTSUP` until `PG-48`).
pub const THR_SUSPENDED: i64 = 0x8;
/// `THR_DAEMON`: accepted and ignored (**ours**).
pub const THR_DAEMON: i64 = 0x10;

/// `USYNC_THREAD`: private to the process (**ours**).
pub const USYNC_THREAD: i32 = 0;
/// `USYNC_PROCESS`: shared by processes (**ours**).
pub const USYNC_PROCESS: i32 = 1;

/// `PTHREAD_STACK_MIN` for `thr_min_stack()` (`donor-map/14` O-abi-13).
pub const PTHREAD_STACK_MIN: usize = 16_384;

/// Maximum associations in one port (`raven-evq-core` `MAX_REGISTRATIONS`).
pub const PORT_MAX_ASSOCIATIONS: usize = 4096;
/// Maximum queued user events in one port; the next `port_send` is `EAGAIN` (C67 4.1).
pub const PORT_MAX_QUEUED_USER_EVENTS: usize = 1024;
/// Maximum ports per process; the next `port_create` is `EMFILE` (C67 4.1).
pub const PORT_MAX_PORTS: usize = 256;

/// The cookie reserved for the doorbell registration of a port.
///
/// A real descriptor cookie is `(fd << 32) | epoch` with a non-negative fd, so `u64::MAX` can
/// never name an association (RD-new, C67 4.3 rule 1).
pub const DOORBELL_COOKIE: u64 = u64::MAX;

/// `ETIME`: the Solaris timeout error (Linux value 62, DR-13; `raven-abi` has no name for it).
pub const ETIME: Errno = Errno(62);
/// `ENOTSUP`: the Solaris "not supported" error (the Linux value of `EOPNOTSUPP`, DR-13).
pub const ENOTSUP: Errno = Errno(95);

/// `PORT_SOURCE_*` values as a table for the header parity test.
pub const PORT_SOURCES: &[(&str, i32)] = &[
    ("PORT_SOURCE_TIMER", PORT_SOURCE_TIMER),
    ("PORT_SOURCE_USER", PORT_SOURCE_USER),
    ("PORT_SOURCE_FD", PORT_SOURCE_FD),
    ("PORT_SOURCE_ALERT", PORT_SOURCE_ALERT),
];

/// `THR_*` values as a table for the header parity test.
pub const THR_FLAGS: &[(&str, i64)] = &[
    ("THR_BOUND", THR_BOUND),
    ("THR_NEW_LWP", THR_NEW_LWP),
    ("THR_DETACHED", THR_DETACHED),
    ("THR_SUSPENDED", THR_SUSPENDED),
    ("THR_DAEMON", THR_DAEMON),
];

/// `USYNC_*` values as a table for the header parity test.
pub const USYNC_TYPES: &[(&str, i32)] = &[
    ("USYNC_THREAD", USYNC_THREAD),
    ("USYNC_PROCESS", USYNC_PROCESS),
];

/// One delivered port event (`struct port_event`, 24 bytes on LP64).
///
/// `portev_user` is a pointer in the C header and a `usize` here.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortEvent {
    /// Source-specific event data (`poll(2)` bits for `PORT_SOURCE_FD`).
    pub portev_events: i32,
    /// The source (`PORT_SOURCE_*`).
    pub portev_source: u16,
    /// Reserved padding.
    pub portev_pad: u16,
    /// Source-specific object (the descriptor for `PORT_SOURCE_FD`).
    pub portev_object: usize,
    /// The caller's cookie.
    pub portev_user: usize,
}

const _: () = assert!(core::mem::size_of::<PortEvent>() == 24);
const _: () = assert!(core::mem::align_of::<PortEvent>() == 8);
const _: () = assert!(core::mem::offset_of!(PortEvent, portev_events) == 0);
const _: () = assert!(core::mem::offset_of!(PortEvent, portev_source) == 4);
const _: () = assert!(core::mem::offset_of!(PortEvent, portev_pad) == 6);
const _: () = assert!(core::mem::offset_of!(PortEvent, portev_object) == 8);
const _: () = assert!(core::mem::offset_of!(PortEvent, portev_user) == 16);

/// A timeout pointer's value: `None` means "block forever" (C67 4.3 rule 4).
///
/// A zero `timespec` is a poll. The event queue spells "forever" as `0 ns`, so a poll is sent
/// as one nanosecond (RD-new; the same choice as C66 `epoll::timeout_ns`).
///
/// # Errors
/// A negative field or a `tv_nsec` outside `0..1_000_000_000` is `EINVAL`.
pub fn timeout_ns(timeout: Option<&Timespec>) -> Result<u64, Errno> {
    let Some(value) = timeout else {
        return Ok(0);
    };
    if value.tv_sec < 0 || value.tv_nsec < 0 || value.tv_nsec >= 1_000_000_000 {
        return Err(Errno::EINVAL);
    }
    let seconds = u64::try_from(value.tv_sec).map_err(|_| Errno::EINVAL)?;
    let nanos = u64::try_from(value.tv_nsec).map_err(|_| Errno::EINVAL)?;
    let total = seconds
        .checked_mul(1_000_000_000)
        .and_then(|base| base.checked_add(nanos))
        .unwrap_or(u64::MAX);
    if total == 0 { Ok(1) } else { Ok(total) }
}

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
