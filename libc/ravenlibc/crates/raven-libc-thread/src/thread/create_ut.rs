// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::kernel::KernelCaps;
use raven_libc_tls::TlsModuleIn;

#[test]
fn init_without_pg1_is_enosys() {
    let kernel = crate::test_support::MockKernel::with_caps(KernelCaps::minimal());
    let modules = [TlsModuleIn::new(0x1000, 0x10, 0x10, 16)];
    assert_eq!(
        init_main_thread(&kernel, &modules, 0x1000, 0x1000),
        Err(ENOSYS)
    );
}

#[test]
fn init_with_pg1_builds_a_tcb_and_records_the_modules() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    static IMAGE: [u8; 0x10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let modules = [TlsModuleIn::new(IMAGE.as_ptr() as usize, 0x10, 0x10, 16)];
    let tcb = init_main_thread(&kernel, &modules, 0x100_0000, 0x1_0000).unwrap();
    assert_eq!(module_count(), 1);
    assert_eq!(recorded_modules()[0].memsz, 0x10);
    let pthread = crate::thread::current_pthread(&kernel);
    assert_eq!(pthread.self_ptr, tcb);
    assert!(pthread.tid.load(Ordering::Acquire) > 0);
    assert_eq!(pthread.stack_base, 0x100_0000);
    assert_eq!(pthread.stack_size, 0x1_0000);
    assert_eq!(crate::libc_state::LIBC.threads(), 1);
    let offsets = tls_offsets().unwrap();
    assert_eq!(offsets.len, 1);
}

#[test]
fn main_and_first_child_tids_are_distinct() {
    let _serial = crate::test_support::serial();
    let caps = KernelCaps::minimal().with(crate::kernel::CapFlag::Tls, true);
    let kernel = crate::test_support::MockKernel::with_caps(caps);
    static IMAGE_TID: [u8; 0x10] = [0; 0x10];
    let modules = [TlsModuleIn::new(
        IMAGE_TID.as_ptr() as usize,
        0x10,
        0x10,
        16,
    )];
    let tcb = init_main_thread(&kernel, &modules, 0x100_0000, 0x1_0000).unwrap();
    let main_tid = crate::thread::current_pthread(&kernel)
        .tid
        .load(core::sync::atomic::Ordering::Acquire);
    let next_tid = crate::libc_state::LIBC.next_tid();
    assert_ne!(main_tid, next_tid);
    assert_eq!(main_tid, 100);
    assert_eq!(next_tid, 101);
    assert_eq!(crate::thread::current_pthread(&kernel).self_ptr, tcb);
}

#[test]
fn inserting_a_thread_sets_the_next_back_link() {
    let main = std::boxed::Box::new(Pthread::new(0));
    let first = std::boxed::Box::new(Pthread::new(0));
    let second = std::boxed::Box::new(Pthread::new(0));
    let main_address = &*main as *const Pthread as usize;
    let first_address = &*first as *const Pthread as usize;
    let second_address = &*second as *const Pthread as usize;
    raw::set_links(&main, main_address, main_address);
    insert_links(
        &main,
        &main,
        &first,
        first_address,
        main_address,
        main_address,
    );
    insert_links(
        &main,
        &first,
        &second,
        second_address,
        first_address,
        main_address,
    );
    assert_eq!(
        raw::next_thread(&main) as *const Pthread as usize,
        second_address
    );
    assert_eq!(
        raw::prev_thread(&main) as *const Pthread as usize,
        first_address
    );
    assert_eq!(
        raw::prev_thread(&second) as *const Pthread as usize,
        main_address
    );
    assert_eq!(
        raw::next_thread(&second) as *const Pthread as usize,
        first_address
    );
    assert_eq!(
        raw::prev_thread(&first) as *const Pthread as usize,
        second_address
    );
    assert_eq!(
        raw::next_thread(&first) as *const Pthread as usize,
        main_address
    );
}

#[test]
fn inserting_the_first_thread_links_the_creator_both_ways() {
    let main = std::boxed::Box::new(Pthread::new(0));
    let first = std::boxed::Box::new(Pthread::new(0));
    let main_address = &*main as *const Pthread as usize;
    let first_address = &*first as *const Pthread as usize;
    raw::set_links(&main, main_address, main_address);
    insert_links(
        &main,
        &main,
        &first,
        first_address,
        main_address,
        main_address,
    );
    assert_eq!(
        raw::next_thread(&main) as *const Pthread as usize,
        first_address
    );
    // The fresh list has no separate tail; the creator's back link must name the new node,
    // not itself (S138: a self back link is stale the moment the thread is inserted).
    assert_eq!(
        raw::prev_thread(&main) as *const Pthread as usize,
        first_address
    );
    assert_eq!(
        raw::prev_thread(&first) as *const Pthread as usize,
        main_address
    );
    assert_eq!(
        raw::next_thread(&first) as *const Pthread as usize,
        main_address
    );
}

#[test]
fn sequential_insertions_track_the_live_tail() {
    let main = std::boxed::Box::new(Pthread::new(0));
    let first = std::boxed::Box::new(Pthread::new(0));
    let second = std::boxed::Box::new(Pthread::new(0));
    let main_address = &*main as *const Pthread as usize;
    let first_address = &*first as *const Pthread as usize;
    let second_address = &*second as *const Pthread as usize;
    raw::set_links(&main, main_address, main_address);
    // The snapshots are the live links exactly as `create` reads them under the list lock.
    let previous_prev = raw::prev_thread(&main) as *const Pthread as usize;
    let next = raw::next_thread(&main) as *const Pthread as usize;
    let next_next = raw::next_thread(raw::tcb(next)) as *const Pthread as usize;
    insert_links(
        &main,
        raw::tcb(next),
        &first,
        first_address,
        previous_prev,
        next_next,
    );
    let previous_prev = raw::prev_thread(&main) as *const Pthread as usize;
    let next = raw::next_thread(&main) as *const Pthread as usize;
    let next_next = raw::next_thread(raw::tcb(next)) as *const Pthread as usize;
    insert_links(
        &main,
        raw::tcb(next),
        &second,
        second_address,
        previous_prev,
        next_next,
    );
    assert_eq!(
        raw::next_thread(&main) as *const Pthread as usize,
        second_address
    );
    // `first` is the tail after both insertions, so `main.prev` must name it.
    assert_eq!(
        raw::prev_thread(&main) as *const Pthread as usize,
        first_address
    );
    assert_eq!(
        raw::next_thread(&second) as *const Pthread as usize,
        first_address
    );
    assert_eq!(
        raw::prev_thread(&first) as *const Pthread as usize,
        second_address
    );
    assert_eq!(
        raw::next_thread(&first) as *const Pthread as usize,
        main_address
    );
}

#[test]
fn create_before_init_is_enosys() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    crate::libc_state::LIBC.reset(0);
    let start: StartFn = |_| 0;
    let result = create(&kernel, None, start, 0);
    crate::libc_state::LIBC.reset(kernel.tcb());
    assert_eq!(result, Err(ENOSYS));
}

#[test]
fn create_after_init_with_no_modules_reaches_the_kernel() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let tcb = init_main_thread(&kernel, &[], 0x100_0000, 0x1_0000).unwrap();
    assert_ne!(tcb, 0);
    assert_eq!(module_count(), 0);
    let mut attr = crate::thread::attr::init();
    attr.set_stacksize_raw(crate::limits::PTHREAD_STACK_MIN);
    let start: StartFn = |_| 0;
    // The mock kernel cannot map a stack, so the call now reaches ENOMEM. The point is that a
    // zero-module main TCB no longer returns ENOSYS before the thread machinery runs.
    assert_eq!(
        create(&kernel, Some(&attr), start, 0),
        Err(crate::limits::ENOMEM)
    );
}

#[test]
fn create_with_a_bad_attribute_is_einval() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    static IMAGE2: [u8; 0x10] = [0; 0x10];
    let modules = [TlsModuleIn::new(IMAGE2.as_ptr() as usize, 0x10, 0x10, 16)];
    let _ = init_main_thread(&kernel, &modules, 0x100_0000, 0x1_0000);
    let mut attr = crate::thread::attr::init();
    attr.set_stacksize_raw(1);
    let start: StartFn = |_| 0;
    assert_eq!(create(&kernel, Some(&attr), start, 0), Err(EINVAL));
}

#[test]
fn create_reports_enomem_when_the_fake_cannot_map() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    static IMAGE3: [u8; 0x10] = [0; 0x10];
    let modules = [TlsModuleIn::new(IMAGE3.as_ptr() as usize, 0x10, 0x10, 16)];
    let _ = init_main_thread(&kernel, &modules, 0x100_0000, 0x1_0000);
    let mut attr = crate::thread::attr::init();
    attr.set_stacksize_raw(crate::limits::PTHREAD_STACK_MIN);
    let start: StartFn = |_| 0;
    assert_eq!(
        create(&kernel, Some(&attr), start, 0),
        Err(crate::limits::ENOMEM)
    );
}

#[test]
fn the_start_args_trampoline_pointer_is_first() {
    assert_eq!(core::mem::offset_of!(StartArgs, trampoline), 0);
    assert_eq!(
        core::mem::size_of::<StartArgs>(),
        3 * core::mem::size_of::<usize>()
    );
}

#[test]
fn tp_and_tcb_helpers_round_trip() {
    let tcb = 0x1000;
    assert_eq!(tcb_of(tp_of(tcb)), tcb);
}
