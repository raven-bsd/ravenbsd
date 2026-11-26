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
use crate::cmdline::{CmdLine, LogLevel};
use crate::sync::IrqSpinLock;

/// Fixed kernel log ring size in bytes.
pub const RING_SIZE: usize = 64 * 1024;
/// Fixed record header size, mirrored from the ABI.
pub const RECORD_HEADER_LEN: usize = raven_abi::types::klog::HEADER_LEN;
/// Maximum record body, mirrored from the ABI.
pub const RECORD_MAX_BODY: usize = raven_abi::types::klog::MAX_BODY;
/// Panic-path record flag, mirrored from the ABI.
pub const RECORD_FLAG_PANIC: u8 = raven_abi::types::klog::RECORD_PANIC;

/// One read plan over the retained records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadPlan {
    /// First record the caller may read (clamped cursor).
    pub start: u64,
    /// Cursor after the last record that fits.
    pub next: u64,
    /// Total bytes of the records between `start` and `next`.
    pub bytes: usize,
    /// The supplied cursor was older than the first retained record.
    pub lost: bool,
    /// The next record does not fit in the caller's buffer.
    pub more: bool,
}

/// A fixed circular byte ring of complete kernel log records.
///
/// `write_cursor` and `first_cursor` are absolute byte positions; the physical index is the
/// position modulo the ring size, so a record may wrap across the end. Records are never split:
/// a write that does not fit (oversized body, or no longer representable) increments `dropped`.
/// A write that reuses physical space drops whole oldest records first, so `first_cursor` always
/// names a complete retained record.
pub struct Ring<const N: usize> {
    buffer: [u8; N],
    write_cursor: u64,
    first_cursor: u64,
    dropped: u64,
}

impl<const N: usize> Ring<N> {
    /// Create an empty ring.
    pub const fn new() -> Self {
        Self {
            buffer: [0; N],
            write_cursor: 0,
            first_cursor: 0,
            dropped: 0,
        }
    }

    /// Absolute cursor of the oldest retained record.
    pub const fn first_cursor(&self) -> u64 {
        self.first_cursor
    }

    /// Absolute cursor of the next write (also the caught-up read position).
    pub const fn last_cursor(&self) -> u64 {
        self.write_cursor
    }

    /// Cumulative records lost to overwrite or oversized bodies.
    pub const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Note one record that was not stored because its body was too large.
    pub fn note_dropped(&mut self) {
        self.dropped = self.dropped.saturating_add(1);
    }

    /// Append one record at `mono_ns`.
    ///
    /// Returns false and increments `dropped` when the body exceeds the record maximum or the
    /// record cannot be represented in this ring.
    pub fn write_at(&mut self, mono_ns: u64, level: u8, flags: u8, body: &[u8]) -> bool {
        let Some(record_len) = RECORD_HEADER_LEN.checked_add(body.len()) else {
            self.note_dropped();
            return false;
        };
        if body.len() > RECORD_MAX_BODY || record_len > N {
            self.note_dropped();
            return false;
        }
        let capacity = N as u64;
        let record_len = record_len as u64;
        while self
            .write_cursor
            .saturating_sub(self.first_cursor)
            .saturating_add(record_len)
            > capacity
        {
            let Some(size) = self.record_size(self.first_cursor) else {
                break;
            };
            self.first_cursor = self.first_cursor.saturating_add(size as u64);
            self.dropped = self.dropped.saturating_add(1);
        }
        if self
            .write_cursor
            .saturating_sub(self.first_cursor)
            .saturating_add(record_len)
            > capacity
        {
            self.note_dropped();
            return false;
        }
        self.store(mono_ns, level, flags, body)
    }

    /// Size of the record at `cursor`, or None when `cursor` is not a retained record start.
    pub fn record_size(&self, cursor: u64) -> Option<usize> {
        if cursor < self.first_cursor || cursor >= self.write_cursor {
            return None;
        }
        let mut header = [0u8; RECORD_HEADER_LEN];
        for (index, slot) in header.iter_mut().enumerate() {
            *slot = self.read_byte(cursor.checked_add(index as u64)?)?;
        }
        let len = u16::from_le_bytes([*header.get(10)?, *header.get(11)?]) as usize;
        if len > RECORD_MAX_BODY {
            return None;
        }
        RECORD_HEADER_LEN.checked_add(len)
    }

    /// Copy one complete record to `out`, whose length must equal [`Self::record_size`].
    pub fn copy_record(&self, cursor: u64, out: &mut [u8]) -> Option<()> {
        let size = self.record_size(cursor)?;
        if out.len() != size {
            return None;
        }
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = self.read_byte(cursor.checked_add(index as u64)?)?;
        }
        Some(())
    }

    /// Plan one read. `capacity` is the caller's byte budget.
    pub fn plan_read(&self, cursor: u64, capacity: usize) -> ReadPlan {
        let mut start = cursor;
        let mut lost = false;
        if start < self.first_cursor {
            start = self.first_cursor;
            lost = true;
        }
        if start > self.write_cursor {
            start = self.write_cursor;
        }
        let mut next = start;
        let mut bytes = 0usize;
        let mut more = false;
        while next < self.write_cursor {
            let Some(size) = self.record_size(next) else {
                break;
            };
            let Some(total) = bytes.checked_add(size) else {
                more = true;
                break;
            };
            if total > capacity {
                more = true;
                break;
            }
            bytes = total;
            next = next.saturating_add(size as u64);
        }
        ReadPlan {
            start,
            next,
            bytes,
            lost,
            more,
        }
    }

    fn store(&mut self, mono_ns: u64, level: u8, flags: u8, body: &[u8]) -> bool {
        let mut header = [0u8; RECORD_HEADER_LEN];
        if let Some(slice) = header.get_mut(..8) {
            slice.copy_from_slice(&mono_ns.to_le_bytes());
        }
        if let Some(slot) = header.get_mut(8) {
            *slot = level;
        }
        if let Some(slot) = header.get_mut(9) {
            *slot = flags;
        }
        let Ok(len) = u16::try_from(body.len()) else {
            return false;
        };
        if let Some(slice) = header.get_mut(10..12) {
            slice.copy_from_slice(&len.to_le_bytes());
        }
        let mut cursor = self.write_cursor;
        for byte in header {
            if !self.write_byte(cursor, byte) {
                return false;
            }
            cursor = cursor.saturating_add(1);
        }
        for byte in body.iter().copied() {
            if !self.write_byte(cursor, byte) {
                return false;
            }
            cursor = cursor.saturating_add(1);
        }
        self.write_cursor = cursor;
        true
    }

    fn read_byte(&self, cursor: u64) -> Option<u8> {
        let index = usize::try_from(cursor.checked_rem(N as u64)?).ok()?;
        self.buffer.get(index).copied()
    }

    fn write_byte(&mut self, cursor: u64, value: u8) -> bool {
        let Some(index) = cursor
            .checked_rem(N as u64)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        let Some(slot) = self.buffer.get_mut(index) else {
            return false;
        };
        *slot = value;
        true
    }
}

/// A bounded UTF-8 formatter for one ring record body.
struct LineBuffer {
    bytes: [u8; RECORD_MAX_BODY],
    len: usize,
    overflow: bool,
}

impl LineBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; RECORD_MAX_BODY],
            len: 0,
            overflow: false,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or(&[])
    }
}

impl fmt::Write for LineBuffer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.overflow {
            return Ok(());
        }
        let Some(end) = self.len.checked_add(value.len()) else {
            self.overflow = true;
            return Ok(());
        };
        if end > RECORD_MAX_BODY {
            self.overflow = true;
            return Ok(());
        }
        let Some(slice) = self.bytes.get_mut(self.len..end) else {
            self.overflow = true;
            return Ok(());
        };
        slice.copy_from_slice(value.as_bytes());
        self.len = end;
        Ok(())
    }
}

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
static TEST_WRITE: AtomicU8 = AtomicU8::new(TEST_WRITE_UNKNOWN);
/// The fixed kernel log ring. It has a const initialiser, so it exists before the first
/// `kprintln!` and retains early boot lines.
pub(crate) static RING: IrqSpinLock<Ring<RING_SIZE>> = IrqSpinLock::new(Ring::new());

const TEST_WRITE_UNKNOWN: u8 = 0;
const TEST_WRITE_OFF: u8 = 1;
const TEST_WRITE_ON: u8 = 2;

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

/// Write raw bytes under the console lock, so a user `DEBUG_WRITE` cannot interleave with a kernel
/// log line. Donor: FreeBSD `sys/kern/kern_cons.c:566 cnputsn` holds `cnputs_mtx` across the whole
/// string (the same console-lock shape as [`_print_line`]).
pub fn write_bytes(bytes: &[u8]) {
    {
        let _guard = SERIAL.lock();
        crate::arch::serial::write_bytes(bytes);
    }
    // S51b qtest injection: `klog-test=1` lets the fixture write ring records through the
    // existing DEBUG_WRITE path without adding a second user-facing logging syscall.
    if test_write_enabled() {
        record_bytes(LogLevel::Info, 0, bytes);
    }
}

/// Write raw formatted output, without a trailing newline.
pub fn _print(args: fmt::Arguments<'_>) {
    let mut serial = SERIAL.lock();
    let _ = serial.write_fmt(args);
}

/// Write one formatted line, with a trailing newline.
pub fn _print_line(args: fmt::Arguments<'_>) {
    _print_line_at(LogLevel::Info, args);
}

fn _print_line_at(level: LogLevel, args: fmt::Arguments<'_>) {
    let mut line = LineBuffer::new();
    let _ = line.write_fmt(args);
    {
        let mut serial = SERIAL.lock();
        let _ = serial.write_fmt(args);
        let _ = serial.write_str("\n");
    }
    record_line(level, 0, &line);
}

fn record_line(level: LogLevel, flags: u8, line: &LineBuffer) {
    if line.overflow {
        RING.lock().note_dropped();
        return;
    }
    record_bytes(level, flags, line.as_bytes());
}

fn record_bytes(level: LogLevel, flags: u8, body: &[u8]) {
    let now = crate::time::now_ns();
    RING.lock().write_at(now, level as u8, flags, body);
}

fn record_panic(line: &LineBuffer) -> bool {
    let now = crate::time::now_ns();
    let Some(mut ring) = RING.try_lock() else {
        return false;
    };
    if line.overflow {
        ring.note_dropped();
        return false;
    }
    ring.write_at(
        now,
        LogLevel::Error as u8,
        RECORD_FLAG_PANIC,
        line.as_bytes(),
    )
}

fn test_write_enabled() -> bool {
    match TEST_WRITE.load(Ordering::Acquire) {
        TEST_WRITE_ON => true,
        TEST_WRITE_OFF => false,
        _ => {
            let enabled = CmdLine::parse(crate::memory::paging::cmdline())
                .get("klog-test")
                .is_some();
            TEST_WRITE.store(
                if enabled {
                    TEST_WRITE_ON
                } else {
                    TEST_WRITE_OFF
                },
                Ordering::Release,
            );
            enabled
        }
    }
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
        _print_line_at(level, args);
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
    let mut panic_line = LineBuffer::new();
    let _ = write!(
        &mut panic_line,
        "KERNEL PANIC: {} at {}:{}",
        info.message(),
        file,
        line
    );
    // Try-lock only: a panic on the CPU that already holds the ring must not deadlock. The serial
    // transcript above is unconditional.
    let _ = record_panic(&panic_line);
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

#[cfg(test)]
#[path = "log_ut.rs"]
mod tests;
