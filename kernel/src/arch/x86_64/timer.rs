// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! LAPIC timer calibration and the timer interrupt stubs.

use core::fmt;

use crate::arch::x86_64::{apic, idt, pic, tsc, tsc::TscSource};
use crate::kprintln;
use crate::time;

/// Timer interrupt frequency.
pub const HZ: u32 = 1000;
/// Timer vector.
pub const TIMER_VECTOR: u8 = 0x20;
/// Spurious-interrupt vector.
pub const SPURIOUS_VECTOR: u8 = 0xFF;
/// LAPIC timer input divisor used during calibration and at run time.
const LAPIC_DIVISOR: u32 = 16;

/// Why timer initialisation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerError {
    /// The TSC scale could not be built.
    TscScale,
    /// The LAPIC divisor could not be encoded.
    Divisor,
    /// The calibration window timed out.
    CalibrationTimeout,
    /// The PIT reference window timed out.
    PitTimeout,
    /// The measured LAPIC rate was zero or could not be represented.
    CalibrationRate,
    /// The timer IDT gate could not be installed.
    Idt,
}

impl fmt::Display for TimerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimerError::TscScale => f.write_str("cannot build TSC scale"),
            TimerError::Divisor => f.write_str("invalid LAPIC divisor"),
            TimerError::CalibrationTimeout => f.write_str("LAPIC calibration timed out"),
            TimerError::PitTimeout => f.write_str("PIT calibration timed out"),
            TimerError::CalibrationRate => f.write_str("LAPIC timer rate is unusable"),
            TimerError::Idt => f.write_str("cannot install timer IDT gate"),
        }
    }
}

struct Calibration {
    tsc_hz: u64,
    source: TscSource,
    lapic_counts_per_sec: u64,
}

/// Calibrate the LAPIC timer, install the timer gates and start the tick.
pub fn init() -> Result<(), TimerError> {
    let detected = tsc::detect();
    let calibration = match calibrate_with_pit(detected.source) {
        Ok(measured) => {
            if detected.hz != 0 && rates_agree(detected.hz, measured.tsc_hz) {
                Calibration {
                    tsc_hz: detected.hz,
                    source: detected.source,
                    lapic_counts_per_sec: measured.lapic_counts_per_sec,
                }
            } else {
                measured
            }
        }
        Err(_) if detected.hz != 0 => {
            let counts = calibrate_with_tsc(detected.hz)?;
            Calibration {
                tsc_hz: detected.hz,
                source: detected.source,
                lapic_counts_per_sec: counts,
            }
        }
        Err(error) => return Err(error),
    };

    let initial = raven_timemath::lapic_initial_count(calibration.lapic_counts_per_sec, HZ)
        .ok_or(TimerError::CalibrationRate)?;
    install_handlers()?;
    let divisor = raven_timemath::lapic_divisor_bits(LAPIC_DIVISOR).ok_or(TimerError::Divisor)?;
    apic::timer_set_divisor(divisor);
    apic::timer_start_periodic(TIMER_VECTOR, initial);
    time::init(calibration.tsc_hz, tsc::read());

    if calibration.tsc_hz == 0 {
        kprintln!("time: tsc unavailable");
    } else {
        let source = match calibration.source {
            TscSource::Cpuid15 | TscSource::Cpuid16 => "cpuid",
            TscSource::Pit | TscSource::None => "pit",
        };
        kprintln!(
            "time: tsc {} MHz ({})",
            calibration.tsc_hz / 1_000_000,
            source
        );
        if !detected.invariant {
            kprintln!("time: tsc not invariant");
        }
    }
    kprintln!("timer: {HZ} Hz");
    Ok(())
}

fn calibrate_with_tsc(tsc_hz: u64) -> Result<u64, TimerError> {
    let scale = raven_timemath::scale_for_tsc(tsc_hz).ok_or(TimerError::TscScale)?;
    let divisor = raven_timemath::lapic_divisor_bits(LAPIC_DIVISOR).ok_or(TimerError::Divisor)?;
    apic::timer_set_divisor(divisor);
    apic::timer_set_lvt(apic::LVT_MASK);
    apic::timer_set_initial(u32::MAX);
    let start = tsc::read();
    let window_tsc = (tsc_hz / 100).max(1);
    let timeout = window_tsc.saturating_mul(5);
    loop {
        let elapsed = tsc::read().wrapping_sub(start);
        if elapsed >= window_tsc {
            break;
        }
        if elapsed > timeout {
            apic::timer_stop();
            return Err(TimerError::CalibrationTimeout);
        }
        core::hint::spin_loop();
    }
    let end = tsc::read();
    let current = apic::timer_current();
    apic::timer_stop();

    let elapsed_ns = raven_timemath::tsc_to_ns(end.wrapping_sub(start), scale);
    let counts = u64::from(u32::MAX.saturating_sub(current));
    raven_timemath::counts_per_second(counts, elapsed_ns)
        .filter(|rate| *rate != 0)
        .ok_or(TimerError::CalibrationRate)
}

fn calibrate_with_pit(source: TscSource) -> Result<Calibration, TimerError> {
    let divisor = raven_timemath::lapic_divisor_bits(LAPIC_DIVISOR).ok_or(TimerError::Divisor)?;
    apic::timer_set_divisor(divisor);
    apic::timer_set_lvt(apic::LVT_MASK);
    apic::timer_set_initial(u32::MAX);
    let tsc_start = tsc::read();
    if !pic::pit_wait_10ms() {
        apic::timer_stop();
        return Err(TimerError::PitTimeout);
    }
    let tsc_end = tsc::read();
    let current = apic::timer_current();
    apic::timer_stop();

    let window_ns = pic::pit_window_ns();
    let counts = u64::from(u32::MAX.saturating_sub(current));
    let lapic_counts_per_sec = raven_timemath::counts_per_second(counts, window_ns)
        .filter(|rate| *rate != 0)
        .ok_or(TimerError::CalibrationRate)?;
    let tsc_hz = if source == TscSource::None {
        0
    } else {
        raven_timemath::counts_per_second(tsc_end.wrapping_sub(tsc_start), window_ns).unwrap_or(0)
    };
    Ok(Calibration {
        tsc_hz,
        source: TscSource::Pit,
        lapic_counts_per_sec,
    })
}

fn rates_agree(cpuid_hz: u64, measured_hz: u64) -> bool {
    if measured_hz == 0 {
        return false;
    }
    let difference = u128::from(cpuid_hz.abs_diff(measured_hz));
    difference * 20 <= u128::from(measured_hz)
}

fn install_handlers() -> Result<(), TimerError> {
    if !idt::set_handler(u16::from(TIMER_VECTOR), timer_isr_address()) {
        return Err(TimerError::Idt);
    }
    if !idt::set_handler(u16::from(SPURIOUS_VECTOR), spurious_isr_address()) {
        return Err(TimerError::Idt);
    }
    Ok(())
}

core::arch::global_asm!(
    r#"
    .section .text
    .macro isr_stub name, target
    .global \name
    .type \name, @function
\name:
    test byte ptr [rsp+8], 3
    jz 1f
    swapgs
1:
    cld
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push rbp
    mov rbp, rsp
    and rsp, -16
    call \target
    mov rsp, rbp
    pop rbp
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    test byte ptr [rsp+8], 3
    jz 2f
    swapgs
2:
    iretq
    .size \name, . - \name
    .endm

    isr_stub timer_isr, timer_interrupt
    isr_stub spurious_isr, spurious_interrupt
"#
);

unsafe extern "C" {
    fn timer_isr();
    fn spurious_isr();
}

fn timer_isr_address() -> u64 {
    timer_isr as *const () as usize as u64
}

fn spurious_isr_address() -> u64 {
    spurious_isr as *const () as usize as u64
}

#[unsafe(no_mangle)]
extern "C" fn timer_interrupt() {
    time::on_tick();
}

#[unsafe(no_mangle)]
extern "C" fn spurious_interrupt() {
    apic::on_spurious();
}
