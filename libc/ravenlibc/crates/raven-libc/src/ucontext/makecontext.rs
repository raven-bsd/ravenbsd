// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `makecontext(3)`: the variadic target of the C59 trampoline.
use core::ffi::c_int;

use raven_libc_abi::structs::UcontextT;
use raven_libc_fmt::VaReader;
use raven_libc_signal::context::makecontext_core;

use crate::stdio::va::{List, reader};

crate::raven_variadic!(
    prefix: "",
    export: "makecontext",
    target: crate::ucontext::makecontext::__raven_va_makecontext,
    named: 3
);

/// Prepare `context` to enter `function` with up to six integer arguments.
///
/// # Safety
/// `context` is writable; the trampoline built `list`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_makecontext(
    context: *mut UcontextT,
    function: usize,
    argc: c_int,
    list: *mut List,
) {
    if context.is_null() || argc < 0 {
        return;
    }
    let mut args = [0usize; 6];
    // SAFETY: the trampoline built the list.
    let mut source = unsafe { reader(list) };
    let count = (argc as usize).min(args.len());
    for slot in args.iter_mut().take(count) {
        *slot = source.next_u64() as usize;
    }
    let _ = makecontext_core(unsafe { &mut *context }, function, &args[..count]);
}
