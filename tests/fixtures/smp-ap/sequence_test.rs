// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host test for the x86_64 per-AP start protocol in
//! `kernel/src/arch/x86_64/ap_start/sequence.rs`.
//!
//! The kernel test harness does not build on the host (K511/S182), so this fixture compiles the
//! production sequence module directly and proves the exact ICR writes and settle delays.

#[path = "../../../kernel/src/arch/x86_64/ap_start/sequence.rs"]
mod sequence;

use sequence::{AP_START_WAIT_NS, INIT_ASSERT, INIT_DEASSERT, sipi, start_sequence};

/// NetBSD `sys/arch/x86/x86/cpu.c:mp_cpu_start` sends INIT, waits 10 ms after deassert, then two
/// SIPIs 200 us apart. The second SIPI covers a first SIPI that reaches the AP before it has
/// entered wait-for-SIPI.
#[test]
fn start_sequence_is_init_settle_then_two_sipis() {
    let steps: &[sequence::StartStep] = &start_sequence(0x08);
    assert_eq!(steps.len(), 4, "one INIT pair plus two SIPIs");
    assert_eq!(steps[0].icr, INIT_ASSERT);
    assert_eq!(steps[0].delay_ns, 0);
    assert_eq!(steps[1].icr, INIT_DEASSERT);
    assert_eq!(steps[1].delay_ns, 10_000_000, "settle after INIT deassert");
    assert_eq!(steps[2].icr, sipi(0x08));
    assert_eq!(steps[2].delay_ns, 200_000, "delay between SIPIs");
    assert_eq!(steps[3].icr, sipi(0x08));
    assert_eq!(steps[3].delay_ns, 200_000, "delay after the last SIPI");
}

/// NetBSD `sys/arch/x86/x86/cpu.c:cpu_boot_secondary` waits 100000 * 10 us = 1 s for
/// `CPUF_RUNNING` and never re-sends INIT.
#[test]
fn wait_budget_is_one_second() {
    assert_eq!(AP_START_WAIT_NS, 1_000_000_000);
}
