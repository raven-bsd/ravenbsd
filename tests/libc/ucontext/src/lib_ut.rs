// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_abi::structs::{StackT, UcontextT};
use raven_libc_signal::context::makecontext_core;
use raven_libc_signal::frame;

use crate::context_abi;

const ITERATIONS: usize = 1000;
const STACK_SIZE: usize = 64 * 1024;

static ENTRIES: AtomicUsize = AtomicUsize::new(0);

extern "C" fn ping_pong(main: usize, worker: usize) {
    loop {
        ENTRIES.fetch_add(1, Ordering::Relaxed);
        // SAFETY: both addresses are the live contexts owned by the test;
        // the C ucontext contract keeps them alive while the loop runs.
        let result =
            unsafe { context_abi::swapcontext(worker as *mut UcontextT, main as *const UcontextT) };
        if result != 0 {
            return;
        }
    }
}

/// The failure the target C test hit: the mcontext is at `ucontext + 40`.
#[test]
fn the_mcontext_starts_at_ucontext_plus_forty_bytes() {
    let context = UcontextT::ZERO;
    let base = context.bytes.as_ptr() as usize;
    let machine = frame::uc_mcontext_ptr(&context) as usize;
    assert_eq!(machine - base, frame::UC_MCONTEXT_WORD * 8);
    #[cfg(target_arch = "x86_64")]
    assert_eq!(machine - base, 40);
}

/// Exactly the C73 `check_ucontext` loop, through the C ABI wrappers.
#[test]
fn a_thousand_makecontext_swapcontext_round_trips() {
    ENTRIES.store(0, Ordering::Relaxed);
    let mut main = UcontextT::ZERO;
    let mut worker = UcontextT::ZERO;
    let mut stack = vec![0u8; STACK_SIZE];
    let main_ptr = &mut main as *mut UcontextT;
    let worker_ptr = &mut worker as *mut UcontextT;

    frame::set_uc_stack(
        &mut worker,
        &StackT {
            ss_sp: stack.as_mut_ptr().cast(),
            ss_flags: 0,
            ss_size: STACK_SIZE as u64,
        },
    );
    frame::set_uc_link(&mut worker, main_ptr as usize);
    assert!(
        makecontext_core(
            &mut worker,
            ping_pong as *const () as usize,
            &[main_ptr as usize, worker_ptr as usize],
        )
        .is_ok()
    );

    for _ in 0..ITERATIONS {
        // SAFETY: `main_ptr` and `worker_ptr` are the live contexts above.
        let result = unsafe { context_abi::swapcontext(main_ptr, worker_ptr as *const UcontextT) };
        assert_eq!(result, 0);
    }
    assert_eq!(ENTRIES.load(Ordering::Relaxed), ITERATIONS);
}

#[test]
fn null_contexts_report_efault() {
    let null_mut: *mut UcontextT = core::ptr::null_mut();
    let null_const: *const UcontextT = core::ptr::null();
    // SAFETY: the wrappers reject NULL before dereferencing it.
    assert_eq!(unsafe { context_abi::getcontext(null_mut) }, -1);
    // SAFETY: the wrapper rejects NULL before dereferencing it.
    assert_eq!(unsafe { context_abi::setcontext(null_const) }, -1);
    // SAFETY: the wrapper rejects either NULL before dereferencing.
    let result = unsafe { context_abi::swapcontext(null_mut, null_const) };
    assert_eq!(result, -1);
}
