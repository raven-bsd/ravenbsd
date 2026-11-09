// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TSC detection and reads.
//!
//! Frequency detection prefers CPUID leaf 0x15 (crystal/numerator/denominator), then leaf 0x16
//! (base MHz). If neither is usable the caller calibrates the TSC against the PIT.

use core::arch::x86_64::__cpuid;

/// Where the detected TSC frequency came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TscSource {
    /// CPUID leaf 0x15.
    Cpuid15,
    /// CPUID leaf 0x16.
    Cpuid16,
    /// The PIT calibration window.
    Pit,
    /// TSC is unavailable.
    None,
}

/// TSC properties detected at boot.
#[derive(Clone, Copy, Debug)]
pub struct TscInfo {
    /// Frequency in Hz, or zero when detection and calibration have not run yet.
    pub hz: u64,
    /// Whether CPUID advertises an invariant TSC.
    pub invariant: bool,
    /// Detection source.
    pub source: TscSource,
}

/// Detect the TSC frequency from CPUID.
pub fn detect() -> TscInfo {
    let max_extended = __cpuid(0x8000_0000).eax;
    let invariant = max_extended >= 0x8000_0007 && __cpuid(0x8000_0007).edx & (1 << 8) != 0;

    let features = __cpuid(1);
    if features.edx & (1 << 4) == 0 {
        return TscInfo {
            hz: 0,
            invariant: false,
            source: TscSource::None,
        };
    }

    let leaf15 = __cpuid(0x15);
    if let Some(hz) = raven_timemath::tsc_hz_from_cpuid15(leaf15.eax, leaf15.ebx, leaf15.ecx) {
        return TscInfo {
            hz,
            invariant,
            source: TscSource::Cpuid15,
        };
    }

    let leaf16 = __cpuid(0x16);
    if let Some(hz) = raven_timemath::tsc_hz_from_cpuid16(leaf16.eax) {
        return TscInfo {
            hz,
            invariant,
            source: TscSource::Cpuid16,
        };
    }

    TscInfo {
        hz: 0,
        invariant,
        source: TscSource::Pit,
    }
}

/// Read the time-stamp counter.
#[inline]
pub fn read() -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: `rdtsc` is an unprivileged read-only instruction; it has no memory side effects.
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags),
        );
    }
    (u64::from(high) << 32) | u64::from(low)
}
