// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integer calibration math for the TSC and the LAPIC timer.
//!
//! The crate is deliberately dependency-free and `forbid(unsafe_code)`. All divisions use
//! `u128` intermediates and the public functions return `Option` instead of panicking on rates
//! that cannot be represented.

#![no_std]
#![forbid(unsafe_code)]

/// Nanoseconds in one second.
pub const NS_PER_SEC: u128 = 1_000_000_000;
/// Fixed-point shift used by the TSC-to-nanoseconds scale.
pub const TSC_SHIFT: u32 = 32;

/// A fixed-point `ns = (ticks * mult) >> shift` conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale {
    /// Multiplier.
    pub mult: u64,
    /// Right shift, always [`TSC_SHIFT`] in this implementation.
    pub shift: u32,
}

/// Derive the TSC frequency from CPUID leaf 0x15.
///
/// `eax` is the denominator, `ebx` the numerator and `ecx` the crystal frequency in Hz. The leaf
/// is only useful when all three fields are non-zero.
pub fn tsc_hz_from_cpuid15(eax: u32, ebx: u32, ecx: u32) -> Option<u64> {
    if eax == 0 || ebx == 0 || ecx == 0 {
        return None;
    }
    let hz = (u128::from(ecx).checked_mul(u128::from(ebx))?) / u128::from(eax);
    let hz = u64::try_from(hz).ok()?;
    (hz != 0).then_some(hz)
}

/// Derive the TSC frequency from CPUID leaf 0x16's base-frequency field.
pub fn tsc_hz_from_cpuid16(base_mhz: u32) -> Option<u64> {
    if base_mhz == 0 {
        return None;
    }
    u64::from(base_mhz).checked_mul(1_000_000)
}

/// Counts per second from a count and the elapsed time.
pub fn counts_per_second(counts: u64, elapsed_ns: u64) -> Option<u64> {
    if elapsed_ns == 0 {
        return None;
    }
    let rate = u128::from(counts).checked_mul(NS_PER_SEC)? / u128::from(elapsed_ns);
    u64::try_from(rate).ok()
}

/// Initial LAPIC count that produces `hz` interrupts per second.
pub fn lapic_initial_count(counts_per_sec: u64, hz: u32) -> Option<u32> {
    if counts_per_sec == 0 || hz == 0 {
        return None;
    }
    let hz = u128::from(hz);
    let count = (u128::from(counts_per_sec) + hz / 2) / hz;
    let count = u32::try_from(count).ok()?;
    (count != 0).then_some(count)
}

/// Encode a LAPIC timer divisor (1, 2, 4, ..., 128) as the 4-bit DIV_CONF value.
pub fn lapic_divisor_bits(divisor: u32) -> Option<u8> {
    match divisor {
        1 => Some(0b0000),
        2 => Some(0b0001),
        4 => Some(0b0010),
        8 => Some(0b0011),
        16 => Some(0b1000),
        32 => Some(0b1001),
        64 => Some(0b1010),
        128 => Some(0b1011),
        _ => None,
    }
}

/// Build the `ticks -> nanoseconds` scale for a TSC frequency.
///
/// `1e9 * 2^32 / hz` fits `u64` for every frequency from 1 MHz to 10 GHz; the rounding error is
/// far below 1 ppm in that range.
pub fn scale_for_tsc(tsc_hz: u64) -> Option<Scale> {
    if tsc_hz == 0 {
        return None;
    }
    let numerator = NS_PER_SEC.checked_shl(TSC_SHIFT)?;
    let mult = numerator
        .checked_add(u128::from(tsc_hz) / 2)?
        .checked_div(u128::from(tsc_hz))?;
    let mult = u64::try_from(mult).ok()?;
    (mult != 0).then_some(Scale {
        mult,
        shift: TSC_SHIFT,
    })
}

/// Convert TSC ticks to nanoseconds, saturating at `u64::MAX`.
pub fn tsc_to_ns(ticks: u64, scale: Scale) -> u64 {
    let ns = (u128::from(ticks) * u128::from(scale.mult)) >> scale.shift;
    if ns > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ns as u64
    }
}

/// Convert nanoseconds to TSC ticks, saturating at `u64::MAX`.
pub fn ns_to_tsc(ns: u64, scale: Scale) -> u64 {
    if scale.mult == 0 {
        return 0;
    }
    let ticks = (u128::from(ns) << scale.shift) / u128::from(scale.mult);
    if ticks > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ticks as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_scale_accuracy(hz: u64) {
        let scale = scale_for_tsc(hz).expect("scale");
        for ns in [1_000u64, 1_000_000, 1_000_000_000, 86_400_000_000_000] {
            let ticks = ((u128::from(ns) * u128::from(hz)) / NS_PER_SEC) as u64;
            let measured = tsc_to_ns(ticks, scale);
            let error = measured.abs_diff(ns);
            let relative = error as f64 / ns as f64;
            assert!(
                relative <= 1e-6,
                "hz={hz} ns={ns} measured={measured} relative={relative}"
            );
        }
    }

    #[test]
    fn cpuid15_normal() {
        assert_eq!(tsc_hz_from_cpuid15(1, 24, 100_000_000), Some(2_400_000_000));
    }

    #[test]
    fn cpuid15_zero_fields_mean_unknown() {
        assert_eq!(tsc_hz_from_cpuid15(0, 24, 100_000_000), None);
        assert_eq!(tsc_hz_from_cpuid15(1, 0, 100_000_000), None);
        assert_eq!(tsc_hz_from_cpuid15(1, 24, 0), None);
    }

    #[test]
    fn cpuid15_max_values_do_not_overflow() {
        assert_eq!(
            tsc_hz_from_cpuid15(1, u32::MAX, u32::MAX),
            Some(u64::from(u32::MAX) * u64::from(u32::MAX))
        );
    }

    #[test]
    fn cpuid16_normal() {
        assert_eq!(tsc_hz_from_cpuid16(2400), Some(2_400_000_000));
        assert_eq!(tsc_hz_from_cpuid16(0), None);
    }

    #[test]
    fn counts_per_second_normal() {
        assert_eq!(counts_per_second(1_000, 10_000_000), Some(100_000));
        assert_eq!(counts_per_second(0, 10_000_000), Some(0));
    }

    #[test]
    fn counts_per_second_needs_time() {
        assert_eq!(counts_per_second(1, 0), None);
    }

    #[test]
    fn lapic_initial_count_rounds() {
        assert_eq!(lapic_initial_count(6_250_000, 1_000), Some(6_250));
        assert_eq!(lapic_initial_count(1, 1_000), None);
        assert_eq!(lapic_initial_count(0, 1_000), None);
        assert_eq!(lapic_initial_count(1_000, 0), None);
    }

    #[test]
    fn divisor_encoding_is_architectural() {
        assert_eq!(lapic_divisor_bits(1), Some(0b0000));
        assert_eq!(lapic_divisor_bits(2), Some(0b0001));
        assert_eq!(lapic_divisor_bits(8), Some(0b0011));
        assert_eq!(lapic_divisor_bits(16), Some(0b1000));
        assert_eq!(lapic_divisor_bits(128), Some(0b1011));
        assert_eq!(lapic_divisor_bits(3), None);
        assert_eq!(lapic_divisor_bits(256), None);
    }

    #[test]
    fn scale_at_one_mhz() {
        assert_scale_accuracy(1_000_000);
    }

    #[test]
    fn scale_at_ten_mhz() {
        assert_scale_accuracy(10_000_000);
    }

    #[test]
    fn scale_at_one_ghz() {
        assert_scale_accuracy(1_000_000_000);
    }

    #[test]
    fn scale_at_ten_ghz() {
        assert_scale_accuracy(10_000_000_000);
    }

    #[test]
    fn scale_rejects_zero() {
        assert_eq!(scale_for_tsc(0), None);
    }

    #[test]
    fn tsc_to_ns_saturates() {
        let scale = scale_for_tsc(1_000_000_000).unwrap();
        assert_eq!(tsc_to_ns(u64::MAX, scale), u64::MAX);
    }

    #[test]
    fn ns_to_tsc_saturates() {
        let scale = scale_for_tsc(1_000_000_000).unwrap();
        assert_eq!(ns_to_tsc(u64::MAX, scale), u64::MAX);
    }

    #[test]
    fn ns_to_tsc_round_trip_is_close() {
        let scale = scale_for_tsc(2_400_000_000).unwrap();
        let ns = 1_234_567_890u64;
        let ticks = ns_to_tsc(ns, scale);
        let back = tsc_to_ns(ticks, scale);
        assert!(back.abs_diff(ns) <= 1);
    }

    #[test]
    fn monotonic_ticks_give_monotonic_ns() {
        let scale = scale_for_tsc(3_000_000_000).unwrap();
        let mut previous = 0;
        for ticks in [0u64, 1, 1000, 1_000_000, u64::MAX / 2] {
            let ns = tsc_to_ns(ticks, scale);
            assert!(ns >= previous);
            previous = ns;
        }
    }
}
