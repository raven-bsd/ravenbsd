// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn builtin_tls_is_aligned_and_large() {
    assert_eq!(builtin_tls() % 16, 0);
    assert_eq!(
        raven_libc_tls::raw::BUILTIN_TLS.capacity(),
        raven_libc_tls::consts::BUILTIN_TLS_SIZE
    );
}

#[test]
fn word_views_share_the_same_storage() {
    let words = [0u32; 4];
    at_u32(&words, 1).store(7, core::sync::atomic::Ordering::Release);
    assert_eq!(
        at_u32(&words, 1).load(core::sync::atomic::Ordering::Acquire),
        7
    );
    at_i32(&words, 2).store(-3, core::sync::atomic::Ordering::Release);
    assert_eq!(
        at_u32(&words, 2).load(core::sync::atomic::Ordering::Acquire),
        (-3i32) as u32
    );
}

#[test]
fn usize_pairs_round_trip() {
    let mut words = [0u32; 10];
    store_usize(&mut words, 6, 0x1234_5678_9abc_def0);
    assert_eq!(load_usize(&words, 6), 0x1234_5678_9abc_def0);
}

#[test]
fn the_i32_view_is_bit_compatible() {
    let word = core::sync::atomic::AtomicU32::new(0x8000_0001);
    let signed = as_i32(&word);
    assert_eq!(
        signed.load(core::sync::atomic::Ordering::Acquire),
        -0x7fff_ffff
    );
    assert_eq!(
        as_u32(signed).load(core::sync::atomic::Ordering::Acquire),
        0x8000_0001
    );
}

#[test]
fn tcb_round_trips_through_tp() {
    let tcb = crate::test_support::MockKernel::new().tcb();
    let tp = raven_libc_tls::tcb::pthread_to_tp(tcb);
    assert_eq!(raven_libc_tls::tcb::tp_to_pthread(tp), tcb);
}

#[test]
fn next_and_prev_reach_a_mock_tcb() {
    let kernel = crate::test_support::MockKernel::new();
    let tcb = kernel.tcb();
    let pthread = tcb_ref(tcb);
    set_links(pthread, tcb, tcb);
    assert_eq!(next_thread(pthread) as *const Pthread as usize, tcb);
    assert_eq!(prev_thread(pthread) as *const Pthread as usize, tcb);
}

fn tcb_ref(address: usize) -> &'static Pthread {
    tcb(address)
}

#[test]
fn robust_head_helpers_round_trip() {
    let kernel = crate::test_support::MockKernel::new();
    let pthread = tcb(kernel.tcb());
    let head = robust_head(pthread);
    set_robust_head(pthread, 0);
    assert_eq!(robust_head(pthread), 0);
    set_robust_head(pthread, head);
    assert_eq!(robust_head(pthread), head);
}

#[test]
fn small_word_read_write_round_trips() {
    let mut value = 0u32;
    let address = &mut value as *mut u32 as usize;
    write_u32(address, 0xdead_beef);
    assert_eq!(read_u32(address), 0xdead_beef);
}
