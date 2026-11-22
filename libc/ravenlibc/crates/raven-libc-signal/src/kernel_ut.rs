// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn production_capability_matrix_matches_pg7() {
    let caps = SigKernel::caps(&SysKernel);
    assert!(caps.entry);
    assert!(caps.kick);
    assert!(caps.sigreturn);
    assert!(caps.resend);
    assert!(caps.eintr);
    assert!(caps.proc_kill);
    assert!(!caps.stop);
    assert!(!caps.fp_in_frame);
    assert_eq!(caps, SigCaps::PRODUCTION);
}

#[test]
fn host_capability_matrix_is_all_true_but_fp() {
    let caps = SigCaps::HOST;
    assert!(caps.entry && caps.kick && caps.sigreturn && caps.resend);
    assert!(caps.eintr && caps.proc_kill && caps.stop);
    assert!(!caps.fp_in_frame);
}

#[test]
fn host_fake_control_primitives_fail_without_a_real_kernel() {
    let kernel = SysKernel::new();
    assert!(kernel.kick(1, KICK_CHECK).is_err());
    assert!(kernel.sig_setup(0x1000, 0, 0).is_err());
    assert!(kernel.resend(1, &SiginfoT::ZERO, 0).is_err());
    assert!(kernel.proc_kill(1, 15, None).is_err());
    assert_eq!(kernel.take_process_pending(!0), None);
}

#[test]
fn kick_bits_are_the_four_proposed_ones() {
    assert_eq!(KICK_CHECK, 1);
    assert_eq!(KICK_CANCEL, 2);
    assert_eq!(KICK_SYNCCALL, 4);
    assert_eq!(KICK_TIMER, 8);
    assert_eq!(KICK_KNOWN, 15);
    assert!(valid_kick_bits(0));
    assert!(valid_kick_bits(KICK_KNOWN));
    assert!(!valid_kick_bits(16));
    assert!(!valid_kick_bits(0xffff_fffe));
}

#[test]
fn fault_kinds_decode_and_reject() {
    for (value, kind) in [
        (1, FaultKind::PageNomap),
        (2, FaultKind::PagePerm),
        (3, FaultKind::Divide),
        (4, FaultKind::Illegal),
        (5, FaultKind::Misalign),
        (6, FaultKind::Breakpoint),
    ] {
        assert_eq!(FaultKind::from_u32(value), Some(kind));
    }
    assert_eq!(FaultKind::from_u32(0), None);
    assert_eq!(FaultKind::from_u32(7), None);
}

#[test]
fn monotonic_nanoseconds_convert_without_overflow() {
    assert_eq!(nanosecond_value(1, 2), 1_000_000_002);
    assert_eq!(nanosecond_value(-1, -1), 0);
    assert_eq!(nanosecond_value(i64::MAX, i64::MAX), u64::MAX);
}

#[test]
fn fault_record_is_repr_c() {
    assert_eq!(core::mem::size_of::<FaultRecord>(), 24);
    assert_eq!(core::mem::align_of::<FaultRecord>(), 8);
}

#[test]
fn bad_signal_is_einval() {
    assert_eq!(bad_signal(), EINVAL_ERR);
}
