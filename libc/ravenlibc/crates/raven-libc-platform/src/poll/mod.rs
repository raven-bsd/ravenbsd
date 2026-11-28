// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Waiting: `poll`, `select` and `epoll` over `EVQ` (doc 11 section 3.6).
//!
//! A regular file is always ready; that is what POSIX requires and it needs no kernel help. A
//! tty, pipe or socket can report only `HUP` until PG-26/`POLL*` lands, so a blocking `poll` on
//! one of those returns on hangup or timeout, not on data: an honest, documented gap. `pselect`
//! installs the caller's temporary mask, runs the existing wait and restores the mask on every
//! return; the atomic masked wait remains PG-26/S173b. `ppoll` still accepts its mask without
//! swapping it.

pub mod epoll;
#[allow(clippy::module_inception)]
pub mod poll;
pub mod select;

pub use epoll::{EpollEvent, epoll_create1, epoll_ctl, epoll_pwait, epoll_pwait2, epoll_wait};
pub use poll::{poll, ppoll, validate_user_range, wait_readable, wait_writable};
pub use select::{KernelMasks, MaskOps, pselect, select};

/// Maximum registrations in one event queue (`raven-evq-core MAX_REGISTRATIONS`).
pub const MAX_POLL_FDS: usize = 4096;

/// `FD_SETSIZE` (doc 11 section 3.2).
pub const FD_SETSIZE: usize = 1024;

// Event bits from `raven-evq-core`; this crate does not depend on that crate.
pub(crate) const EV_READABLE: u32 = 1;
pub(crate) const EV_WRITABLE: u32 = 2;
pub(crate) const EV_HUP: u32 = 4;
pub(crate) const EV_SIGNALED: u32 = 8;

#[cfg(test)]
#[path = "poll_ut.rs"]
mod tests;

#[cfg(test)]
#[path = "select_ut.rs"]
mod select_tests;

#[cfg(test)]
#[path = "epoll_ut.rs"]
mod epoll_tests;
