// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public `setjmp`/`longjmp` entry points as tail-jumps over C58's hidden asm routines.

use raven_libc_tls::tcb::Pthread;

#[path = "longjmp.rs"]
mod longjmp;
#[path = "setjmp.rs"]
mod setjmp;
#[path = "sigsetjmp.rs"]
mod sigsetjmp;

/// Return the calling thread's `sigmask` word pointer, used by `sigsetjmp`.
extern "C" fn sigmask_ptr() -> *mut u64 {
    let tp = raven_libc_sys::tp::thread_pointer() as usize;
    if tp == 0 {
        return core::ptr::null_mut();
    }
    let pthread = raven_libc_tls::tcb::tp_to_pthread(tp);
    if pthread == 0 {
        return core::ptr::null_mut();
    }
    // SAFETY: the thread pointer addresses the live TCB; the field is a live AtomicU64.
    unsafe {
        (pthread as *mut u8)
            .add(core::mem::offset_of!(Pthread, sigmask))
            .cast::<u64>()
    }
}
