// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setproctitle(3)`: the C59 variadic trampoline.
use core::ffi::c_char;

use crate::personality_common::{RealProcBsd, TITLE_CACHE};
use crate::stdio::va::{List, reader};

crate::raven_variadic!(
    prefix: "",
    export: "setproctitle",
    target: crate::personality_bsd::setproctitle::__raven_va_setproctitle,
    named: 1
);

/// Render `format` with the printf engine and send the title to `procd`.
///
/// # Safety
/// The trampoline built `list`; `format` is NULL or NUL terminated.
pub(crate) unsafe fn run(fast: bool, format: *const c_char, list: *mut List) {
    let rendered = if format.is_null() {
        None
    } else {
        // SAFETY: the caller's format contract.
        let format = unsafe { crate::ffi::cstr::slice(format) };
        // SAFETY: the trampoline built the list.
        let mut env = crate::va::CEnv::new(
            unsafe { reader(list) },
            crate::stdio::vprintf::errno_text,
            crate::stdio::vprintf::alloc,
        );
        raven_libc_stdio::vasprintf_core(
            format,
            &mut env,
            &crate::stdio::vprintf::options(),
            &crate::stdio::vprintf::FloatHook,
        )
        .ok()
    };
    let program = crate::personality_linux::error_globals::program_name();
    let _ = TITLE_CACHE.with(|cache| {
        raven_libc_personality_bsd::setproctitle::setproctitle(
            &mut RealProcBsd,
            cache,
            &program,
            rendered.as_deref(),
            fast,
        )
    });
}

/// The trampoline target.
///
/// # Safety
/// The trampoline built `list`; `format` is NULL or NUL terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_setproctitle(format: *const c_char, list: *mut List) {
    // SAFETY: forwarded from this function's contract.
    unsafe { run(false, format, list) };
}
