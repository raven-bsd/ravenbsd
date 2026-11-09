// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel logging, log levels and the panic handler.
//!
//! All output goes through one spin-locked COM1 writer. The macros never allocate; the panic path
//! writes directly to a fresh [`Serial`] so it cannot deadlock on a lock held by the panicking
//! thread.

use core::cell::UnsafeCell;
use core::fmt::{self, Write};
use core::ops::{Deref, DerefMut};
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::arch::serial::Serial;
use crate::cmdline::LogLevel;

/// A small spin lock used by the kernel. Interrupts are not disabled (S04 runs with them off).
pub struct SpinMutex<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: the value is only accessed through the guard returned by `lock`, which holds the lock
// until it is dropped.
unsafe impl<T: Send> Sync for SpinMutex<T> {}

impl<T> SpinMutex<T> {
    /// Create an unlocked mutex.
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Acquire the lock, spinning until it is free.
    pub fn lock(&self) -> SpinGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinGuard { mutex: self }
    }
}

/// Guard returned by [`SpinMutex::lock`].
pub struct SpinGuard<'a, T> {
    mutex: &'a SpinMutex<T>,
}

impl<T> Deref for SpinGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // SAFETY: the guard holds the lock, so this is the only access to the value.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: the guard holds the lock, so this is the only mutable access to the value.
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
    }
}

static SERIAL: SpinMutex<Serial> = SpinMutex::new(Serial);
static LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);
static TEST_EXIT: AtomicBool = AtomicBool::new(false);

/// Set the log level parsed from the command line.
pub fn init(level: LogLevel) {
    LEVEL.store(level as u8, Ordering::Release);
}

/// Remember whether the command line requested the C5 test-exit protocol.
pub fn set_test_exit(enabled: bool) {
    TEST_EXIT.store(enabled, Ordering::Release);
}

/// Whether `test-exit=1` was requested.
pub fn test_exit() -> bool {
    TEST_EXIT.load(Ordering::Acquire)
}

fn level_enabled(level: LogLevel) -> bool {
    LEVEL.load(Ordering::Acquire) >= level as u8
}

/// Write raw formatted output, without a trailing newline.
pub fn _print(args: fmt::Arguments<'_>) {
    let mut serial = SERIAL.lock();
    let _ = serial.write_fmt(args);
}

/// Write one formatted line, with a trailing newline.
pub fn _print_line(args: fmt::Arguments<'_>) {
    let mut serial = SERIAL.lock();
    let _ = serial.write_fmt(args);
    let _ = serial.write_str("\n");
}

/// Write one line when `level` is enabled.
pub fn _print_level(level: LogLevel, args: fmt::Arguments<'_>) {
    if level_enabled(level) {
        _print_line(args);
    }
}

/// Write one unconditional line.
#[macro_export]
macro_rules! kprintln {
    () => {
        $crate::log::_print_line(core::format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::log::_print_line(core::format_args!($($arg)*))
    };
}

/// Write one error line.
#[macro_export]
macro_rules! kerror {
    ($($arg:tt)*) => {
        $crate::log::_print_level(
            $crate::cmdline::LogLevel::Error,
            core::format_args!($($arg)*),
        )
    };
}

/// Write one warning line.
#[macro_export]
macro_rules! kwarn {
    ($($arg:tt)*) => {
        $crate::log::_print_level(
            $crate::cmdline::LogLevel::Warn,
            core::format_args!($($arg)*),
        )
    };
}

/// Write one informational line.
#[macro_export]
macro_rules! kinfo {
    ($($arg:tt)*) => {
        $crate::log::_print_level(
            $crate::cmdline::LogLevel::Info,
            core::format_args!($($arg)*),
        )
    };
}

/// Write one debug line.
#[macro_export]
macro_rules! kdebug {
    ($($arg:tt)*) => {
        $crate::log::_print_level(
            $crate::cmdline::LogLevel::Debug,
            core::format_args!($($arg)*),
        )
    };
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    crate::arch::serial::init();
    let mut serial = Serial;
    let location = info.location();
    let (file, line) = match location {
        Some(location) => (location.file(), location.line()),
        None => ("<unknown>", 0),
    };
    let _ = writeln!(
        serial,
        "KERNEL PANIC: {} at {}:{}",
        info.message(),
        file,
        line
    );
    if test_exit() {
        crate::boot::exit_qemu(0x11);
    }
    crate::arch::halt()
}
