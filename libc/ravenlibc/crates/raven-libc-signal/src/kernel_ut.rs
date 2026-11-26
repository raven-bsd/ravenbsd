// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn production_capability_matrix_is_all_false() {
    let caps = SigKernel::caps(&SysKernel);
    assert!(!caps.kick);
    assert!(!caps.resend);
    assert!(!caps.eintr);
    assert!(!caps.proc_kill);
    assert!(!caps.stop);
    assert!(!caps.fp_in_frame);
    assert_eq!(caps, SigCaps::PRODUCTION);
}

#[test]
fn host_capability_matrix_is_all_true_but_fp() {
    let caps = SigCaps::HOST;
    assert!(caps.kick && caps.resend);
    assert!(caps.eintr && caps.proc_kill && caps.stop);
    assert!(!caps.fp_in_frame);
}

#[test]
fn missing_kernel_primitives_return_enosys() {
    let kernel = SysKernel::new();
    assert_eq!(kernel.kick(1, KICK_CHECK), Err(ENOSYS_ERR));
    assert_eq!(kernel.resend(1, &SiginfoT::ZERO, 0), Err(ENOSYS_ERR));
    assert_eq!(kernel.proc_kill(1, 15, None), Err(ENOSYS_ERR));
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

// --- K443: the six-word process record carries the kernel-owned siginfo fields ---

#[test]
fn process_record_carries_the_sigqueue_payload() {
    // SI_QUEUE is the C value -1 (stored as 0xffff_ffff).
    let words = [10, 42, u32::MAX as u64, 1000, 0x5a5a, 0];
    let (signo, info) = process_siginfo(&words);
    assert_eq!(signo, 10);
    assert_eq!(crate::frame::si_code(&info), -1);
    assert_eq!(crate::frame::si_pid(&info), 42);
    assert_eq!(crate::frame::si_uid(&info), 1000);
    assert_eq!(crate::frame::si_value(&info).bytes[0], 0x5a5a);
}

#[test]
fn process_record_preserves_the_child_exit_code() {
    // CLD_EXITED = 1; the kernel, not libc, stamps the SIGCHLD record.
    let words = [17, 7, 1, 1000, 0, 0];
    let (signo, info) = process_siginfo(&words);
    assert_eq!(signo, 17);
    assert_eq!(crate::frame::si_code(&info), 1);
    assert_eq!(crate::frame::si_pid(&info), 7);
}

#[test]
fn plain_user_signal_is_si_user() {
    let words = [15, 9, 0, 0, 0, 0];
    let (signo, info) = process_siginfo(&words);
    assert_eq!(signo, 15);
    assert_eq!(crate::frame::si_code(&info), 0);
}
