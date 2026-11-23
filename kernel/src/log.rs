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

    /// Try to acquire the lock without spinning.
    ///
    /// Only the x86-64 emergency exception writer has a non-blocking caller; aarch64 does not
    /// compile that path, so keep the helper out of the aarch64 binary instead of warning.
    #[cfg(target_arch = "x86_64")]
    pub fn try_lock(&self) -> Option<SpinGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinGuard { mutex: self })
        } else {
            None
        }
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

/// Deliberately hold the log lock for the exception-fallback injection.
///
/// Only reached when the `log-lock-fault` boot flag selects the injection in
/// `arch/x86_64/exception.rs`; the guard is intentionally leaked so the fault path sees a held
/// lock. The machine halts after the exception transcript.
#[cfg(target_arch = "x86_64")]
pub fn hold_lock_for_test() {
    core::mem::forget(SERIAL.lock());
}

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

/// Write one emergency line without waiting for the log lock.
///
/// The exception path uses this path so a held/corrupted log lock cannot hang the fault handler.
/// Normal logging remains serialized; this fallback may interleave with another CPU's output but
/// always produces the diagnostic.
#[cfg(target_arch = "x86_64")]
pub fn _print_line_emergency(args: fmt::Arguments<'_>) {
    if let Some(mut serial) = SERIAL.try_lock() {
        let _ = serial.write_fmt(args);
        let _ = serial.write_str("\n");
        return;
    }
    crate::arch::serial::init();
    let mut serial = Serial;
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

/// Write one line from exception context without waiting for the log lock.
#[macro_export]
macro_rules! kprintln_emergency {
    () => {
        $crate::log::_print_line_emergency(core::format_args!(""))
    };
    ($($arg:tt)*) => {
        $crate::log::_print_line_emergency(core::format_args!($($arg)*))
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
    let (fail_size, fail_align, ra0, ra1) = crate::memory::heap::fail_info();
    if fail_size != 0 {
        let _ = writeln!(
            serial,
            "K434 alloc-fail size={fail_size} align={fail_align} ra0=0x{ra0:x} ra1=0x{ra1:x}"
        );
        let reap = crate::sched::reap_len();
        let live = crate::sched::thread::live_threads();
        let dead_live = crate::process::dead_live_threads();
        let dropped = crate::sched::thread::slots_dropped();
        let census = crate::sched::thread::all_thread_census();
        let _ = writeln!(
            serial,
            "K434 holders reap={reap} live_threads={live} slots_dropped={dropped} dead_live_weak={dead_live}"
        );
        let _ = writeln!(
            serial,
            "K434 thread-census alive={} refs1={} refs2={} dead={}",
            census[0], census[1], census[2], census[3]
        );
        if let Some((kmem_fail, large_live, in_use)) = crate::memory::heap::kmem_counters() {
            let _ = writeln!(
                serial,
                "K434 kmem alloc_fail={kmem_fail} large_live={large_live} in_use={in_use}"
            );
        }
        let (free_fails, free_size, free_align, free_ptr) = crate::memory::heap::free_fail_info();
        let _ = writeln!(
            serial,
            "K434 free-fail count={free_fails} size={free_size} align={free_align} ptr=0x{free_ptr:x}"
        );
        if let Some((counts, bytes)) = crate::memory::heap::segment_histogram() {
            let _ = writeln!(
                serial,
                "K434 segments count 1K={} 2K={} 4K={} 8K={} 16K={} 64K={} 1M={} big={}",
                counts[0],
                counts[1],
                counts[2],
                counts[3],
                counts[4],
                counts[5],
                counts[6],
                counts[7]
            );
            let _ = writeln!(
                serial,
                "K434 segments bytes 1K={} 2K={} 4K={} 8K={} 16K={} 64K={} 1M={} big={}",
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]
            );
        }
        let bt = crate::memory::heap::fail_backtrace();
        let _ = writeln!(
            serial,
            "K434 alloc-bt 0x{:x} 0x{:x} 0x{:x} 0x{:x} 0x{:x} 0x{:x}",
            bt[0], bt[1], bt[2], bt[3], bt[4], bt[5]
        );
        if let Some(index) = raven_kmem::class_for(fail_size) {
            if let Some(stats) = crate::memory::heap::class_stats(index) {
                let class = raven_kmem::SIZE_CLASSES.get(index).copied().unwrap_or(0);
                let _ = writeln!(
                    serial,
                    "K434 class size={fail_size} index={index} class={class} slabs={} used={} free={} waste={} fail={}",
                    stats.slabs, stats.in_use, stats.free, stats.waste_bytes, stats.alloc_fail
                );
            }
        }
        let grow = crate::memory::heap::grow_fail();
        if let Some((spans, free, largest, fail, used)) = crate::memory::heap::arena_stats() {
            let _ = writeln!(
                serial,
                "K434 arena grow_fail={grow} spans={spans} free={free} largest={largest} fail={fail} used={used}"
            );
        }
    }
    if test_exit() {
        crate::boot::exit_qemu(0x11);
    }
    crate::arch::halt()
}
