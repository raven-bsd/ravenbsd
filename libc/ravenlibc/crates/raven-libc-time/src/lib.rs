// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Pure-logic half of `<time.h>`, `<sys/time.h>` and the POSIX timers.
//!
//! This is the `raven-libc-time` crate of `architecture/raven-libc/donor-map/06-time-tz.md`.
//! It contains the tzcode port (TZif reader, POSIX `TZ` parser, `localtime`, `mktime`, `timegm`),
//! `strftime`, `strptime`, `asctime`, `difftime`, the `TimeLocale` seam, and a host-testable model
//! of the clock, sleep and POSIX-timer calls over `TimeSys`. The C-ABI shell and the real syscall
//! implementation live in `raven-libc` and `raven-libc-platform`.
//!
//! The crate is `no_std` plus `alloc`, class P (`#![forbid(unsafe_code)]`), and never reads the
//! clock, the environment or a file except through the explicit `TimeSys` parameter.
#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

pub mod asctime;
pub mod clock;
pub mod difftime;
pub mod itimer;
pub mod locale;
pub mod sleep;
pub mod strftime;
pub mod strptime;
pub mod sys;
pub mod timer;
pub mod tz;
pub mod utime;
pub mod wcsftime;

pub use raven_libc_abi::errno::Errno;
pub use raven_libc_abi::structs::Timespec;
pub use sys::{ClockId, TimeSys};
pub use tz::consts::{TZ_MAX_CHARS, TZ_MAX_LEAPS, TZ_MAX_TIMES, TZ_MAX_TYPES, TZ_STRLEN_MAX};
pub use tz::{Tm, TzCache, TzError, TzState, ZoneIdx};
