// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::sync::atomic::Ordering;

#[test]
fn size_is_512() {
    assert_eq!(core::mem::size_of::<Pthread>(), 512);
    assert_eq!(Pthread::size(), 512);
    assert_eq!(core::mem::align_of::<Pthread>(), 16);
}

#[test]
fn part_one_offsets_are_the_abi() {
    #[cfg(target_arch = "x86_64")]
    {
        assert_eq!(core::mem::offset_of!(Pthread, self_ptr), 0);
        assert_eq!(core::mem::offset_of!(Pthread, dtv), 8);
        assert_eq!(core::mem::offset_of!(Pthread, prev), 16);
        assert_eq!(core::mem::offset_of!(Pthread, next), 24);
        assert_eq!(core::mem::offset_of!(Pthread, sysinfo), 32);
        assert_eq!(core::mem::offset_of!(Pthread, canary), 40);
        assert_eq!(core::mem::offset_of!(Pthread, tid), 48);
        assert_eq!(core::mem::offset_of!(Pthread, errno_val), 52);
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        assert_eq!(core::mem::offset_of!(Pthread, self_ptr), 0);
        assert_eq!(core::mem::offset_of!(Pthread, prev), 8);
        assert_eq!(core::mem::offset_of!(Pthread, next), 16);
        assert_eq!(core::mem::offset_of!(Pthread, sysinfo), 24);
        assert_eq!(
            core::mem::offset_of!(Pthread, dtv) + core::mem::size_of::<usize>(),
            512
        );
        assert_eq!(
            core::mem::offset_of!(Pthread, canary) + core::mem::size_of::<usize>(),
            core::mem::offset_of!(Pthread, dtv)
        );
    }
}

#[test]
fn errno_offset_matches_the_constant() {
    #[cfg(target_arch = "x86_64")]
    assert_eq!(errno_offset_from_tp(), 52);
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    assert!(errno_offset_from_tp() < 0);
}

#[test]
fn tp_round_trip() {
    for address in [0x1000usize, 0x7fff_ffff_f000, 0x1_0000_0000] {
        assert_eq!(tp_to_pthread(pthread_to_tp(address)), address);
    }
}

#[test]
fn variant_two_tp_is_the_tcb() {
    #[cfg(target_arch = "x86_64")]
    assert_eq!(pthread_to_tp(0x1234), 0x1234);
}

#[test]
fn variant_one_tp_is_above_the_tcb() {
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    assert_eq!(
        pthread_to_tp(0x1000),
        0x1000 + core::mem::size_of::<Pthread>()
    );
}

#[test]
fn new_sets_the_self_pointer_and_joinable_state() {
    let tcb = Pthread::new(0x4000);
    assert_eq!(tcb.self_ptr, 0x4000);
    assert_eq!(tcb.detach_state(), DT_JOINABLE);
    assert_eq!(tcb.tid.load(Ordering::Relaxed), 0);
    assert_eq!(tcb.errno_val, 0);
    assert!(!tcb.is_alive());
}

#[test]
fn detach_state_swap_returns_the_old_value() {
    let tcb = Pthread::new(0);
    assert_eq!(
        tcb.swap_detach_state(DT_EXITING, Ordering::AcqRel),
        DT_JOINABLE
    );
    assert_eq!(tcb.detach_state(), DT_EXITING);
    assert_eq!(
        tcb.swap_detach_state(DT_EXITED, Ordering::AcqRel),
        DT_EXITING
    );
}

#[test]
fn a_live_tid_means_alive() {
    let tcb = Pthread::new(0);
    tcb.tid.store(7, Ordering::Release);
    assert!(tcb.is_alive());
    tcb.tid.store(-1, Ordering::Release);
    assert!(!tcb.is_alive());
}

#[test]
fn errno_location_points_at_the_cell() {
    let mut tcb = Pthread::new(0);
    let address = &mut tcb as *mut Pthread as usize;
    let tp = pthread_to_tp(address);
    let location = errno_location(tp);
    assert_eq!(
        location as usize,
        address + core::mem::offset_of!(Pthread, errno_val)
    );
    tcb.errno_val = 42;
    assert_eq!(tcb.errno_val, 42);
    assert_eq!(location, (&mut tcb.errno_val) as *mut i32);
}

#[test]
fn detach_state_values_are_the_donor_enum() {
    assert_eq!(DT_EXITED, 0);
    assert_eq!(DT_EXITING, 1);
    assert_eq!(DT_JOINABLE, 2);
    assert_eq!(DT_DETACHED, 3);
}

#[test]
fn name_and_signal_private_sizes() {
    assert_eq!(NAME_LEN, 16);
    assert_eq!(SIG_PRIVATE_WORDS, 32);
    assert_eq!(SIG_PRIVATE_BYTES, 256);
    assert!(core::hint::black_box(SIG_PRIVATE_BYTES) >= 176);
}

#[test]
fn tsd_words_follow_the_abi_header() {
    assert_eq!(TSD_WORDS, 128);
}
