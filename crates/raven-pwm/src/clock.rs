// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CPRMAN PWM clock and range plan for the Pi 4 analog jack.
//!
//! The audio path uses PLLD (nominal 500 MHz) divided by 5 for a 100 MHz PWM clock. The
//! PWM range register is the number of clock ticks per sample frame; the BCM2711 datasheet and
//! the B41 spike select the integer values in [`range_plan`].

use crate::registers::{
    CM_CTL_BUSY, CM_CTL_ENAB, CM_CTL_SRC_PLLD, CM_DIV_DIVI_SHIFT, CM_PASSWD, CM_PWMCTL, CM_PWMDIV,
};

/// Nominal PLLD rate in Hz.
pub const PLLD_HZ: u32 = 500_000_000;
/// PWM clock divider (`DIVI=5`).
pub const PWM_DIVI: u32 = 5;
/// PWM clock rate presented to the range counter.
pub const PWM_CLOCK_HZ: u32 = PLLD_HZ / PWM_DIVI;
/// Firmware clock id for the PWM block.
pub const PWM_CLOCK_ID: u32 = 10;
/// `CM_PWMCTL` value that enables PLLD with MASH=0.
pub const CM_CTL_START: u32 = CM_PASSWD | CM_CTL_ENAB | CM_CTL_SRC_PLLD;
/// `CM_PWMDIV` value for `DIVI=5`, `DIVF=0`.
pub const CM_DIV_START: u32 = CM_PASSWD | (PWM_DIVI << CM_DIV_DIVI_SHIFT);
/// Maximum busy-poll attempts while starting or stopping the clock.
pub const CM_BUSY_POLLS: u32 = 1000;

/// One sample-rate/range choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangePlan {
    /// Requested sample rate in Hz.
    pub rate_hz: u32,
    /// Value written to `RNG1`/`RNG2`.
    pub range: u32,
    /// Actual rate `PWM_CLOCK_HZ / range`.
    pub actual_hz: u32,
    /// Signed error in parts per million.
    pub error_ppm: i32,
}

impl RangePlan {
    /// Build a plan from a range value, or `None` for a zero rate/range.
    #[must_use]
    pub const fn new(rate_hz: u32, range: u32) -> Option<Self> {
        if rate_hz == 0 || range == 0 {
            return None;
        }
        let actual_hz = PWM_CLOCK_HZ / range;
        let divisor = rate_hz as i128 * range as i128;
        let scaled = (PWM_CLOCK_HZ as i128 - divisor) * 1_000_000;
        let error = if scaled >= 0 {
            (scaled + divisor / 2) / divisor
        } else {
            (scaled - divisor / 2) / divisor
        };
        let error_ppm = if error < i32::MIN as i128 {
            i32::MIN
        } else if error > i32::MAX as i128 {
            i32::MAX
        } else {
            error as i32
        };
        Some(Self {
            rate_hz,
            range,
            actual_hz,
            error_ppm,
        })
    }
}

/// The documented range for a common rate, or `None` for an unsupported rate.
#[must_use]
pub const fn range_for_rate(rate_hz: u32) -> Option<u32> {
    match rate_hz {
        48_000 => Some(2083),
        44_100 => Some(2268),
        22_050 => Some(4535),
        11_025 => Some(9070),
        32_000 => Some(3125),
        96_000 => Some(1041),
        _ => None,
    }
}

/// The plan for a common rate, or `None` for an unsupported rate.
#[must_use]
pub const fn range_plan(rate_hz: u32) -> Option<RangePlan> {
    match range_for_rate(rate_hz) {
        Some(range) => RangePlan::new(rate_hz, range),
        None => None,
    }
}

/// The nearest integer range for a rate, with no table restriction.
///
/// This is useful for diagnostics; the driver should use [`range_plan`] so a board log records a
/// known-good value.
#[must_use]
pub const fn nearest_range(rate_hz: u32) -> Option<RangePlan> {
    if rate_hz == 0 || rate_hz > PWM_CLOCK_HZ {
        return None;
    }
    let range = PWM_CLOCK_HZ / rate_hz;
    if range == 0 || range > 0xffff {
        return None;
    }
    RangePlan::new(rate_hz, range)
}

/// The stop value preserving the non-enable bits of `current`.
#[must_use]
pub const fn clock_stop_value(current: u32) -> u32 {
    CM_PASSWD | (current & !CM_CTL_ENAB)
}

/// The start value used by the Pi 4 bring-up.
#[must_use]
pub const fn clock_start_value() -> u32 {
    CM_CTL_START
}

/// The divider value used by the Pi 4 bring-up.
#[must_use]
pub const fn clock_divider_value() -> u32 {
    CM_DIV_START
}

/// True when the busy bit is set.
#[must_use]
pub const fn clock_busy(value: u32) -> bool {
    value & CM_CTL_BUSY != 0
}

/// The register offsets used by the clock sequence.
#[must_use]
pub const fn clock_registers() -> (usize, usize) {
    (CM_PWMCTL, CM_PWMDIV)
}

#[cfg(test)]
#[path = "clock_ut.rs"]
mod tests;
