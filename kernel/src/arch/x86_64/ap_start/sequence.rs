// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The x86_64 per-AP INIT-SIPI-SIPI protocol.
//!
//! The sequence is pure data so the host test under `tests/fixtures/smp-ap` can prove the exact
//! ICR writes and settle delays; [`super::start_ap`] performs them.

/// INIT IPI with level assert and level trigger set.
pub(crate) const INIT_ASSERT: u32 = (0b101 << 8) | (1 << 14) | (1 << 15);
/// INIT IPI with level deassert and level trigger set.
pub(crate) const INIT_DEASSERT: u32 = (0b101 << 8) | (1 << 15);

/// One ICR write and the delay that follows it in a per-AP sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StartStep {
    /// ICR low value delivered to the target APIC.
    pub icr: u32,
    /// Delay after the write, in nanoseconds.
    pub delay_ns: u64,
}

/// How long the boot processor waits for one AP to publish `online`.
///
/// NetBSD `sys/arch/x86/x86/cpu.c:cpu_boot_secondary` waits 100000 * 10 us = 1 s for
/// `CPUF_RUNNING` and does not send another INIT, which would reset an AP that is still
/// executing the trampoline.
pub(crate) const AP_START_WAIT_NS: u64 = 200_000_000;

/// SIPI ICR value for a real-mode startup vector.
pub(crate) fn sipi(vector: u8) -> u32 {
    (0b110 << 8) | (vector as u32) | (1 << 14) | (1 << 15)
}

/// The INIT-SIPI-SIPI sequence sent to one AP.
///
/// NetBSD `sys/arch/x86/x86/cpu.c:mp_cpu_start` deasserts INIT, waits 10 ms, then sends two
/// SIPIs 200 us apart. The settle delay gives the AP time to enter wait-for-SIPI; the second
/// SIPI covers a first SIPI that arrived while the AP was still resetting.
pub(crate) fn start_sequence(vector: u8) -> [StartStep; 3] {
    [
        StartStep {
            icr: INIT_ASSERT,
            delay_ns: 10_000_000,
        },
        StartStep {
            icr: INIT_DEASSERT,
            delay_ns: 200_000,
        },
        StartStep {
            icr: sipi(vector),
            delay_ns: 0,
        },
    ]
}

/// Park an AP that did not publish `online`.
///
/// INIT resets the AP into wait-for-SIPI and no SIPI follows, so it cannot start executing the
/// trampoline after the boot processor drops the low mapping.
pub(crate) fn reset_sequence() -> [StartStep; 2] {
    [
        StartStep {
            icr: INIT_ASSERT,
            delay_ns: 0,
        },
        StartStep {
            icr: INIT_DEASSERT,
            delay_ns: 10_000_000,
        },
    ]
}
