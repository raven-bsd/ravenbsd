// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel clocks and the entropy pool.
//!
//! The monotonic clock counts from the architecture counter and the timer tick counter; the wall
//! clock (`CLOCK_REALTIME`) is the platform RTC read lazily on first use plus the monotonic
//! elapsed time, and the entropy pool feeds `GETRANDOM`/`AT_RANDOM`. The RTC and the pool are
//! wired here because the platform RTC lives in [`rtc`] and the pool needs the monotonic counter
//! for its jitter fallback.
//!
//! x86-64 uses the TSC with a calibrated scale; aarch64 uses `CNTPCT_EL0` with the frequency from
//! `CNTFRQ_EL0` (or `BootInfo.counter_freq`). Both expose the same interface to the scheduler and
//! the self-tests.

#[cfg(target_arch = "x86_64")]
use core::sync::atomic::AtomicU32;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg(target_arch = "x86_64")]
use raven_timemath::{self, Scale};

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::timer as arch_timer;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::{apic, tsc};
use crate::sync::IrqSpinLock;
use crate::{boot, kprintln};

#[cfg(target_arch = "x86_64")]
#[path = "arch/x86_64/rtc.rs"]
mod rtc;
#[cfg(target_arch = "aarch64")]
#[path = "arch/aarch64/rtc.rs"]
mod rtc;

/// The pure entropy pool, compiled from the same source as the host-tested `raven-entropy` crate.
#[path = "../../crates/raven-entropy/src/lib.rs"]
mod raven_entropy;

#[cfg(target_arch = "x86_64")]
static TSC_HZ: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "x86_64")]
static BOOT_TSC: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "x86_64")]
static TSC_MULT: AtomicU64 = AtomicU64::new(0);
#[cfg(target_arch = "x86_64")]
static TSC_SHIFT: AtomicU32 = AtomicU32::new(0);

/// Counter frequency in Hz (aarch64).
#[cfg(target_arch = "aarch64")]
static COUNTER_HZ: AtomicU64 = AtomicU64::new(0);
/// Counter value at boot (aarch64).
#[cfg(target_arch = "aarch64")]
static BOOT_COUNTER: AtomicU64 = AtomicU64::new(0);

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Wall-clock nanoseconds at boot (zero until the RTC is read).
static BOOT_REALTIME_NS: AtomicU64 = AtomicU64::new(0);
/// Whether the RTC has been read (successfully or not).
static REALTIME_READ: AtomicBool = AtomicBool::new(false);

/// Initialise the monotonic clock.
///
/// x86-64 passes the TSC frequency and the TSC value at boot; aarch64 passes the generic-timer
/// frequency and the `CNTPCT_EL0` value at boot.
pub fn init(counter_hz: u64, boot_counter: u64) {
    #[cfg(target_arch = "x86_64")]
    {
        TSC_HZ.store(counter_hz, Ordering::Release);
        BOOT_TSC.store(boot_counter, Ordering::Release);
        if let Some(scale) = raven_timemath::scale_for_tsc(counter_hz) {
            TSC_MULT.store(scale.mult, Ordering::Release);
            TSC_SHIFT.store(scale.shift, Ordering::Release);
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        COUNTER_HZ.store(counter_hz, Ordering::Release);
        BOOT_COUNTER.store(boot_counter, Ordering::Release);
    }
}

/// Nanoseconds since [`init`].
#[cfg(target_arch = "x86_64")]
pub fn now_ns() -> u64 {
    let mult = TSC_MULT.load(Ordering::Acquire);
    let shift = TSC_SHIFT.load(Ordering::Acquire);
    if mult == 0 {
        return 0;
    }
    let ticks = tsc::read().saturating_sub(BOOT_TSC.load(Ordering::Acquire));
    raven_timemath::tsc_to_ns(ticks, Scale { mult, shift })
}

/// Nanoseconds since [`init`].
#[cfg(target_arch = "aarch64")]
pub fn now_ns() -> u64 {
    let frequency = COUNTER_HZ.load(Ordering::Acquire);
    if frequency == 0 {
        return 0;
    }
    let counter = arch_timer::read_counter().saturating_sub(BOOT_COUNTER.load(Ordering::Acquire));
    // 64-bit nanoseconds overflow after a few centuries; the u128 product removes the intermediate
    // overflow at counter frequencies above ~18 GHz. Checked operations keep the kernel's
    // arithmetic policy: on overflow the clock saturates instead of wrapping.
    let nanoseconds = u128::from(counter)
        .checked_mul(1_000_000_000u128)
        .and_then(|value| value.checked_div(u128::from(frequency)))
        .unwrap_or(u128::from(u64::MAX));
    u64::try_from(nanoseconds).unwrap_or(u64::MAX)
}

/// Timer ticks since boot.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

/// Called from the timer interrupt.
pub fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    #[cfg(target_arch = "x86_64")]
    apic::eoi();
    // S27 fallback: drain COM1 from the tick when the IOAPIC line is unavailable.
    #[cfg(target_arch = "x86_64")]
    crate::console::poll();
    crate::event::notify_all();
}

/// Uptime as `(seconds, nanoseconds)`.
#[allow(dead_code)] // Consumed by the S09 scheduler and later servers.
pub fn uptime() -> (u64, u32) {
    let now = now_ns();
    (now / 1_000_000_000, (now % 1_000_000_000) as u32)
}

/// Realtime clock at boot: the RTC epoch, read lazily on first use.
///
/// Returns zero when the platform has no usable RTC, which keeps `CLOCK_REALTIME` at zero rather
/// than blocking boot. The RTC transcript is printed once, on the first call.
pub fn boot_realtime_ns() -> u64 {
    if !REALTIME_READ.load(Ordering::Acquire) {
        read_rtc_once();
    }
    BOOT_REALTIME_NS.load(Ordering::Acquire)
}

/// Wall-clock nanoseconds since the Unix epoch (`boot_realtime_ns()` plus monotonic elapsed).
pub fn realtime_ns() -> u64 {
    boot_realtime_ns().saturating_add(now_ns())
}

/// Set the wall clock to `ns` (the `CLOCK_SET` syscall).
///
/// Re-anchors the boot offset so that [`realtime_ns`] returns `ns` at this instant and keeps
/// advancing with the monotonic clock.
pub fn set_realtime_ns(ns: u64) {
    BOOT_REALTIME_NS.store(ns.saturating_sub(now_ns()), Ordering::Release);
    REALTIME_READ.store(true, Ordering::Release);
}

/// Read the platform RTC once and print the transcript.
fn read_rtc_once() {
    match rtc::read() {
        Some(epoch_ns) => {
            BOOT_REALTIME_NS.store(epoch_ns, Ordering::Release);
            log_rtc(epoch_ns);
        }
        None => {
            kprintln!("rtc: unavailable");
        }
    }
    REALTIME_READ.store(true, Ordering::Release);
}

/// Print the RTC transcript (ISO-like date and epoch seconds).
fn log_rtc(epoch_ns: u64) {
    let seconds = epoch_ns / 1_000_000_000;
    let days = (seconds / 86_400) as i64;
    let (year, month, day) = civil_from_days(days);
    let tod = seconds % 86_400;
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    kprintln!(
        "rtc: {year}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} (epoch {seconds}s)"
    );
}

/// Days since 1970-01-01 from a civil date (Howard Hinnant's `days_from_civil`).
#[cfg(target_arch = "x86_64")]
#[allow(clippy::arithmetic_side_effects)] // Pure bounded civil-date math on i64; cannot overflow for valid dates.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Civil date `(year, month, day)` from days since 1970-01-01 (Hinnant's `civil_from_days`).
#[allow(clippy::arithmetic_side_effects)] // Pure bounded civil-date math on i64; cannot overflow for valid dates.
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Busy-wait for `ns` nanoseconds using the monotonic clock.
#[allow(dead_code)] // Used by later kernel init and the S09 scheduler.
pub fn delay_ns(ns: u64) {
    if ns == 0 {
        return;
    }
    let start = now_ns();
    if start == 0 {
        return;
    }
    let target = start.saturating_add(ns);
    while now_ns() < target {
        core::hint::spin_loop();
    }
}

/// Wait for the next interrupt.
#[inline]
fn wait_for_interrupt() {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: interrupts are enabled before the self-test; `hlt` wakes on the timer tick.
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: interrupts are enabled before the self-test; `wfi` wakes on the timer IRQ.
        unsafe {
            core::arch::asm!("wfi", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Run the `selftest=timer` checks.
pub fn selftest() {
    let start_ticks = ticks();
    let start_ns = now_ns();
    let target = start_ticks.saturating_add(100);
    while ticks() < target {
        wait_for_interrupt();
    }
    let elapsed = now_ns().saturating_sub(start_ns);
    let elapsed_ms = elapsed / 1_000_000;
    kprintln!("selftest: timer 100 ticks in {elapsed_ms} ms");
    #[cfg(target_arch = "aarch64")]
    kprintln!("selftest: timer interrupts={}", crate::arch::irq::count());

    let mut previous = now_ns();
    let mut index = 0u32;
    while index < 1000 {
        let current = now_ns();
        if current < previous {
            kprintln!("selftest: timer monotonicity failed");
            boot::exit_qemu(0x11);
        }
        previous = current;
        index = index.saturating_add(1);
    }

    if !(70..=130).contains(&elapsed_ms) {
        kprintln!("selftest: timer interval out of range");
        boot::exit_qemu(0x11);
    }
}

/// The detected counter frequency, for diagnostics.
#[allow(dead_code)] // Diagnostic helper for later tickets.
pub fn tsc_hz() -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        TSC_HZ.load(Ordering::Acquire)
    }
    #[cfg(target_arch = "aarch64")]
    {
        COUNTER_HZ.load(Ordering::Acquire)
    }
}

// ---------------------------------------------------------------------------
// Entropy
// ---------------------------------------------------------------------------

/// The global pool, seeded lazily on the first `GETRANDOM`.
static ENTROPY_POOL: IrqSpinLock<Option<raven_entropy::Pool>> = IrqSpinLock::new(None);
/// Whether the pool has been seeded (a benign fast path; double-init is harmless).
static ENTROPY_READY: AtomicBool = AtomicBool::new(false);

/// The entropy source the pool was seeded from.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Variants are arch-gated, so one is always unused on a given architecture.
enum EntropySource {
    Rdrand,
    Rndr,
    Jitter,
}

impl EntropySource {
    fn name(self) -> &'static str {
        match self {
            EntropySource::Rdrand => "rdrand",
            EntropySource::Rndr => "rndr",
            EntropySource::Jitter => "jitter",
        }
    }
}

/// Fill `out` with pool bytes, seeding the pool on first use.
///
/// Never blocks: the pool is seeded from a hardware source when present (RDRAND/RNDR) and from
/// timer jitter otherwise, and a reseed from jitter covers a pool that has exhausted its budget.
/// Returns the pool error mapped by the caller to a defined errno when no source can be sampled.
pub fn entropy_fill(out: &mut [u8]) -> Result<usize, raven_entropy::Error> {
    ensure_entropy();
    let mut guard = ENTROPY_POOL.lock();
    let pool = match guard.as_mut() {
        Some(pool) => pool,
        None => return Err(raven_entropy::Error::Unseeded),
    };
    match pool.fill(out) {
        Ok(written) => Ok(written),
        Err(raven_entropy::Error::NeedsReseed) => {
            jitter_entropy(pool);
            pool.fill(out)
        }
        Err(error) => Err(error),
    }
}

/// Seed the pool once; the race between two first-time callers is benign (both produce a valid
/// seeded pool and one wins).
fn ensure_entropy() {
    if ENTROPY_READY.load(Ordering::Acquire) {
        return;
    }
    let mut pool = raven_entropy::Pool::new();
    let source = sample_entropy(&mut pool);
    *ENTROPY_POOL.lock() = Some(pool);
    ENTROPY_READY.store(true, Ordering::Release);
    kprintln!("entropy: source={}", source.name());
}

/// Sample a hardware entropy source, falling back to timer jitter.
fn sample_entropy(pool: &mut raven_entropy::Pool) -> EntropySource {
    #[cfg(target_arch = "x86_64")]
    if let Some(bytes) = rdrand_entropy() {
        pool.add_entropy(&bytes);
        return EntropySource::Rdrand;
    }
    #[cfg(target_arch = "aarch64")]
    if let Some(bytes) = rndr_entropy() {
        pool.add_entropy(&bytes);
        return EntropySource::Rndr;
    }
    jitter_entropy(pool);
    EntropySource::Jitter
}

/// Read 64 bytes from RDRAND when the CPUID feature bit is present.
#[cfg(target_arch = "x86_64")]
fn rdrand_entropy() -> Option<[u8; 64]> {
    // RDRAND is present when CPUID.1:ECX[30] is set.
    let features = core::arch::x86_64::__cpuid(1);
    if features.ecx & (1 << 30) == 0 {
        return None;
    }
    let mut bytes = [0u8; 64];
    for chunk in bytes.chunks_exact_mut(8) {
        let value = rdrand64()?;
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    Some(bytes)
}

/// One RDRAND word with a short retry loop (RDRAND may set CF=0 under contention).
#[cfg(target_arch = "x86_64")]
fn rdrand64() -> Option<u64> {
    for _ in 0..10 {
        let mut value: u64;
        let mut ok: u8;
        // SAFETY: `rdrand` is available per the CPUID.1:ECX[30] check and has no memory side
        // effects; `setc` records the success flag. `value` is only used when `ok` is set.
        unsafe {
            core::arch::asm!(
                "rdrand {value}",
                "setc {ok}",
                value = out(reg) value,
                ok = out(reg_byte) ok,
                options(nomem, nostack),
            );
        }
        if ok != 0 {
            return Some(value);
        }
    }
    None
}

/// Read 64 bytes from RNDR when the architecture feature is present.
#[cfg(target_arch = "aarch64")]
fn rndr_entropy() -> Option<[u8; 64]> {
    if !rndr_available() {
        return None;
    }
    let mut bytes = [0u8; 64];
    for chunk in bytes.chunks_exact_mut(8) {
        let mut value: u64;
        // SAFETY: RNDR is a read-only system register enabled by the `ID_AA64ISAR0_EL1` check in
        // `rndr_available`; it has no memory side effects.
        unsafe {
            core::arch::asm!(
                "mrs {}, s3_3_c2_c4_0",
                out(reg) value,
                options(nomem, nostack, preserves_flags),
            );
        }
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    Some(bytes)
}

/// Whether the CPU implements RNDR (`ID_AA64ISAR0_EL1[63:60] == 1`).
#[cfg(target_arch = "aarch64")]
fn rndr_available() -> bool {
    let isar: u64;
    // SAFETY: ID_AA64ISAR0_EL1 is a read-only system register readable at EL1.
    unsafe {
        core::arch::asm!(
            "mrs {}, id_aa64isar0_el1",
            out(reg) isar,
            options(nomem, nostack, preserves_flags),
        );
    }
    (isar >> 60) & 0xf == 1
}

/// Fallback entropy from timer jitter.
///
/// Samples the low bits of the monotonic clock delta between tightly spaced reads. This is a
/// weak, possibly observable source: it keeps boot and early userspace from blocking but is
/// **not** cryptographically strong, exactly as documented in `raven-entropy`.
fn jitter_entropy(pool: &mut raven_entropy::Pool) {
    let mut bytes = [0u8; 64];
    let mut previous = now_ns();
    for chunk in bytes.chunks_exact_mut(2) {
        for _ in 0..16 {
            core::hint::spin_loop();
        }
        let current = now_ns();
        let delta = current.saturating_sub(previous);
        previous = current;
        chunk.copy_from_slice(&(delta as u16).to_le_bytes());
    }
    pool.add_entropy(&bytes);
}
