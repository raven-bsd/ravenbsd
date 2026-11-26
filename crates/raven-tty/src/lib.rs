// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1989, 1993 The Regents of the University of California.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: the `make_raw` flag set is ported from
// netbsd:lib/libc/termios/cfmakeraw.c (BSD-3-Clause); the `tcsetattr` action mapping is
// netbsd:lib/libc/termios/tcsetattr.c. The Rust guard, signal restore and API are first-party.

//! Raw terminal mode for the `raven config` TUI.
//!
//! [`RawMode`] saves the current `termios`, clears the canonical/echo/signal flags on the same
//! descriptor and restores the saved settings on drop or on HUP/INT/QUIT/TERM. The wrapper is
//! `no_std` and calls the C `tcgetattr`/`tcsetattr` symbols that ravenlibc exports; no unsafe
//! termios code lives in `raven` or `raven-tui`.

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::cell::UnsafeCell;
use core::ffi::c_int;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// `TCSANOW`: apply the settings immediately.
pub const TCSANOW: c_int = 0;
/// `SIGHUP`.
const SIGHUP: c_int = 1;
/// `SIGINT`.
const SIGINT: c_int = 2;
/// `SIGQUIT`.
const SIGQUIT: c_int = 3;
/// `SIGTERM`.
const SIGTERM: c_int = 15;

const IGNBRK: u32 = 0x0000_0001;
const BRKINT: u32 = 0x0000_0002;
const PARMRK: u32 = 0x0000_0008;
const ISTRIP: u32 = 0x0000_0020;
const INLCR: u32 = 0x0000_0040;
const IGNCR: u32 = 0x0000_0080;
const ICRNL: u32 = 0x0000_0100;
const IXON: u32 = 0x0000_0200;
const IMAXBEL: u32 = 0x0000_2000;
const OPOST: u32 = 0x0000_0001;
const ISIG: u32 = 0x0000_0080;
const ICANON: u32 = 0x0000_0100;
const ECHO: u32 = 0x0000_0008;
const ECHONL: u32 = 0x0000_0010;
const IEXTEN: u32 = 0x0000_0400;
const CSIZE: u32 = 0x0000_0300;
const CS8: u32 = 0x0000_0300;
const PARENB: u32 = 0x0000_1000;

/// The pinned FreeBSD public `struct termios` (DR-71/C20).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Termios {
    pub(crate) c_iflag: u32,
    pub(crate) c_oflag: u32,
    pub(crate) c_cflag: u32,
    pub(crate) c_lflag: u32,
    pub(crate) c_cc: [u8; 20],
    pub(crate) c_ispeed: u32,
    pub(crate) c_ospeed: u32,
}

const _: () = assert!(core::mem::size_of::<Termios>() == 44);
const _: () = assert!(core::mem::align_of::<Termios>() == 4);
const _: () = assert!(core::mem::offset_of!(Termios, c_iflag) == 0);
const _: () = assert!(core::mem::offset_of!(Termios, c_oflag) == 4);
const _: () = assert!(core::mem::offset_of!(Termios, c_cflag) == 8);
const _: () = assert!(core::mem::offset_of!(Termios, c_lflag) == 12);
const _: () = assert!(core::mem::offset_of!(Termios, c_cc) == 16);
const _: () = assert!(core::mem::offset_of!(Termios, c_ispeed) == 36);
const _: () = assert!(core::mem::offset_of!(Termios, c_ospeed) == 40);

impl Termios {
    pub(crate) const ZERO: Self = Self {
        c_iflag: 0,
        c_oflag: 0,
        c_cflag: 0,
        c_lflag: 0,
        c_cc: [0; 20],
        c_ispeed: 0,
        c_ospeed: 0,
    };

    /// Clear the canonical, echo and signal flags and select the 8-bit data path.
    ///
    /// This is the NetBSD `cfmakeraw` flag set, including `IMAXBEL`; the `MIN`/`TIME` control
    /// characters are left as the caller set them, exactly like the donor.
    pub(crate) fn make_raw(&mut self) {
        self.c_iflag &=
            !(IMAXBEL | IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON);
        self.c_oflag &= !OPOST;
        self.c_lflag &= !(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
        self.c_cflag &= !(CSIZE | PARENB);
        self.c_cflag |= CS8;
    }
}

unsafe extern "C" {
    fn tcgetattr(fd: c_int, termios: *mut Termios) -> c_int;
    fn tcsetattr(fd: c_int, action: c_int, termios: *const Termios) -> c_int;
    fn signal(signum: c_int, handler: usize) -> usize;
    fn _exit(code: c_int) -> !;
}

/// Raw-mode failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `tcgetattr` rejected the descriptor (not a terminal).
    GetAttr,
    /// `tcsetattr` rejected the new settings.
    SetAttr,
    /// The console handles carry no `RSH_FD0` scheme descriptor.
    NoConsole,
    /// The console scheme rejected `GET_MODE`.
    GetMode,
    /// The console scheme rejected `SET_MODE`.
    SetMode,
    /// Reading the console handle failed.
    Read,
    /// Another raw-mode guard is already active.
    Busy,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::GetAttr => "tcgetattr failed",
            Self::SetAttr => "tcsetattr failed",
            Self::NoConsole => "cannot open tty:console",
            Self::GetMode => "console GET_MODE failed",
            Self::SetMode => "console SET_MODE failed",
            Self::Read => "console read failed",
            Self::Busy => "raw mode is already active",
        })
    }
}

/// A saved termios record for the signal handler.
struct Saved(UnsafeCell<Termios>);

// SAFETY: the signal handler and `enable`/`drop` are the only users; the handler runs with the
// process stopped and `ACTIVE`/`SIGNAL_FD` are atomics. Only one terminal is put in raw mode.
unsafe impl Sync for Saved {}

static SAVED: Saved = Saved(UnsafeCell::new(Termios::ZERO));
static ACTIVE: AtomicBool = AtomicBool::new(false);
static SIGNAL_FD: AtomicI32 = AtomicI32::new(0);

/// One active raw-mode session. Dropping it restores the saved settings.
#[derive(Debug)]
pub struct RawMode {
    original: Termios,
    fd: c_int,
    handlers: [(c_int, usize); 4],
}

impl RawMode {
    /// Put `fd` in raw mode and install the restore handlers.
    ///
    /// # Errors
    /// [`Error::GetAttr`] when the descriptor is not a terminal, [`Error::SetAttr`] when the
    /// terminal rejects the settings, and [`Error::Busy`] when another guard is active.
    pub fn enable(fd: c_int) -> Result<Self, Error> {
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(Error::Busy);
        }
        let mut original = Termios::ZERO;
        // SAFETY: `original` is writable and `fd` is the caller's open terminal descriptor.
        if unsafe { tcgetattr(fd, &mut original) } != 0 {
            ACTIVE.store(false, Ordering::SeqCst);
            return Err(Error::GetAttr);
        }
        let mut raw = original;
        raw.make_raw();
        // SAFETY: `raw` is readable and was derived from this descriptor.
        if unsafe { tcsetattr(fd, TCSANOW, &raw) } != 0 {
            ACTIVE.store(false, Ordering::SeqCst);
            return Err(Error::SetAttr);
        }
        // SAFETY: `SAVED` is only written while ACTIVE is being claimed, before handlers run.
        unsafe {
            *SAVED.0.get() = original;
        }
        SIGNAL_FD.store(fd, Ordering::SeqCst);
        let mut handlers = [(0, 0); 4];
        for (slot, number) in handlers.iter_mut().zip([SIGHUP, SIGINT, SIGQUIT, SIGTERM]) {
            // SAFETY: the handler has the C signal ABI and only restores termios then calls
            // `_exit`, which is async-signal-safe.
            let previous = unsafe { signal(number, restore_on_signal as *const () as usize) };
            *slot = (number, previous);
        }
        Ok(Self {
            original,
            fd,
            handlers,
        })
    }

    /// Restore the terminal now and consume the guard.
    pub fn restore(self) {
        drop(self);
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        for (number, previous) in self.handlers {
            if previous != usize::MAX {
                // SAFETY: restoring the handler value returned by `signal`.
                let _ = unsafe { signal(number, previous) };
            }
        }
        // SAFETY: `self.original` was read from this descriptor and remains readable.
        let _ = unsafe { tcsetattr(self.fd, TCSANOW, &self.original) };
        ACTIVE.store(false, Ordering::SeqCst);
    }
}

extern "C" fn restore_on_signal(signum: c_int) {
    // SAFETY: the handler restores the saved termios (the signal interrupted the TUI) and exits
    // without touching non-async-signal-safe Rust state.
    unsafe {
        if ACTIVE.load(Ordering::SeqCst) {
            let fd = SIGNAL_FD.load(Ordering::SeqCst);
            let _ = tcsetattr(fd, TCSANOW, SAVED.0.get());
        }
        _exit(128 + signum);
    }
}

#[cfg(target_os = "ravenbsd")]
mod scheme;

#[cfg(target_os = "ravenbsd")]
pub use scheme::{Console, SchemeRawMode};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
