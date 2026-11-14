// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The C `getopt` data symbols and argv/diagnostic seams over C41's `GetoptState` (C60c).
//!
//! The engine is BSD non-permuting `getopt` and permuting `getopt_long`; the shell owns the
//! `argv` pointer array (permutation is written back), `optarg`/`optind`/`opterr`/`optopt`/
//! `optreset` and the error line on descriptor 2. `POSIXLY_CORRECT` is read through the one
//! environment table, never the kernel.

use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

use raven_libc_stdlib::getopt::{ArgSource, Diag, GetoptState, LongOpt, OptArg, Step};

/// The C `struct option`.
#[repr(C)]
pub(crate) struct OptionC {
    /// The long name.
    pub name: *const c_char,
    /// `no_argument`, `required_argument` or `optional_argument`.
    pub has_arg: c_int,
    /// The flag slot, or NULL.
    pub flag: *mut c_int,
    /// The value returned or stored.
    pub val: c_int,
}

/// The `argv` seam: pointer words and their C strings.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ArgvSource;

impl ArgSource for ArgvSource {
    type Arg = usize;

    fn text(&self, argument: Self::Arg) -> &[u8] {
        if argument == 0 {
            return &[];
        }
        // SAFETY: the caller's argv slots are NUL-terminated strings.
        unsafe { crate::ffi::cstr::slice(argument as *const c_char) }
    }
}

/// The diagnostic seam: one `warnx` line on descriptor 2.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StderrDiag;

impl Diag for StderrDiag {
    fn warnx(&mut self, parts: &[&[u8]]) {
        let platform = crate::ffi::platform::global();
        for part in parts {
            let _ = platform.write(2, part);
        }
        let _ = platform.write(2, b"\n");
    }
}

/// Read the shell data symbols and seed the engine state.
pub(crate) fn seed() -> GetoptState {
    let mut state = GetoptState::new();
    // SAFETY: the data symbols are process globals; single-threaded until C57.
    unsafe {
        state.set_optind(crate::stdlib::optind::optind);
        state.set_opterr(crate::stdlib::opterr::opterr);
        state.set_optreset(crate::stdlib::optreset::optreset != 0);
        crate::stdlib::optreset::optreset = 0;
    }
    state
}

/// Write the engine state and result back to the C data symbols and `optarg`.
pub(crate) fn commit(state: &GetoptState, step: &Step, pointers: &[usize]) {
    let argument = match step.optarg {
        OptArg::None => core::ptr::null_mut(),
        OptArg::At { arg_index, offset } => match pointers.get(arg_index) {
            Some(base) => base.wrapping_add(offset) as *mut c_char,
            None => core::ptr::null_mut(),
        },
    };
    // SAFETY: the data symbols are process globals; single-threaded until C57.
    unsafe {
        crate::stdlib::optind::optind = state.optind();
        crate::stdlib::optopt::optopt = state.optopt();
        crate::stdlib::optarg::optarg = argument;
    }
}

/// Build the engine's long-option table from the C array.
///
/// # Safety
/// `options` is NULL or a NULL-name-terminated `struct option` array.
pub(crate) unsafe fn long_options<'a>(options: *const OptionC) -> Vec<LongOpt<'a>> {
    let mut table: Vec<LongOpt<'a>> = Vec::new();
    if options.is_null() {
        return table;
    }
    let mut index = 0usize;
    while index < 4096 {
        // SAFETY: the caller's NULL-terminated option array.
        let option = unsafe { &*options.add(index) };
        if option.name.is_null() {
            break;
        }
        // SAFETY: the name is a NUL-terminated string.
        let name = unsafe { crate::ffi::cstr::slice(option.name) };
        table.push(LongOpt {
            name,
            has_arg: option.has_arg,
            flag_is_null: option.flag.is_null(),
            val: option.val,
        });
        index = index.wrapping_add(1);
    }
    table
}

/// Store the `flag`-form option value or the `longindex` result.
///
/// # Safety
/// `options`/`longindex` follow the C `getopt_long` contract.
pub(crate) unsafe fn long_result(
    step: &Step,
    options: *const OptionC,
    longindex: *mut c_int,
) -> c_int {
    if let Some(index) = step.flag_to_set {
        // SAFETY: the caller's option array; `index` came from the same table.
        if let Some(option) = unsafe { options.add(index).as_ref() }
            && !option.flag.is_null()
        {
            // SAFETY: the caller's writable flag slot.
            unsafe { *option.flag = step.code };
        }
        return 0;
    }
    if let Some(index) = step.long_index
        && !longindex.is_null()
        && let Ok(value) = c_int::try_from(index)
    {
        // SAFETY: the caller's writable longindex slot.
        unsafe { *longindex = value };
    }
    step.code
}

/// Build the argv pointer vector from the C array.
///
/// # Safety
/// `argv` is NULL or points to `argc` readable pointer slots.
pub(crate) unsafe fn argv_pointers(argv: *const *mut c_char, argc: c_int) -> Vec<usize> {
    let mut pointers = Vec::new();
    if argv.is_null() || argc <= 0 {
        return pointers;
    }
    let count = usize::try_from(argc).unwrap_or(0);
    let mut index = 0usize;
    while index < count {
        // SAFETY: the caller's argv array.
        pointers.push(unsafe { *argv.add(index) } as usize);
        index = index.wrapping_add(1);
    }
    pointers
}

/// Write a permuted order back to the caller's `argv`.
///
/// # Safety
/// `argv` points to `pointers.len()` writable pointer slots.
pub(crate) unsafe fn write_argv(argv: *mut *mut c_char, pointers: &[usize]) {
    if argv.is_null() {
        return;
    }
    let mut index = 0usize;
    while index < pointers.len() {
        if let Some(value) = pointers.get(index) {
            // SAFETY: the caller's argv array.
            unsafe { *argv.add(index) = *value as *mut c_char };
        }
        index = index.wrapping_add(1);
    }
    let _ = core::mem::size_of::<c_void>();
}

/// The shared `getopt` body.
///
/// # Safety
/// The C `getopt` argument contract.
pub(crate) unsafe fn short(argv: *mut *mut c_char, argc: c_int, optstring: *const c_char) -> c_int {
    // SAFETY: the caller's argv and optstring contracts.
    let mut pointers = unsafe { argv_pointers(argv, argc) };
    // SAFETY: the caller's optstring contract.
    let options = unsafe { crate::ffi::cstr::slice(optstring) };
    let mut state = seed();
    let mut diag = StderrDiag;
    let step = state.getopt(
        &mut pointers,
        &ArgvSource,
        options,
        crate::stdlib::env_support::get(b"POSIXLY_CORRECT").is_some(),
        &mut diag,
    );
    commit(&state, &step, &pointers);
    // SAFETY: the caller's argv array.
    unsafe { write_argv(argv, &pointers) };
    step.code
}

/// The shared `getopt_long`/`getopt_long_only` body.
///
/// # Safety
/// The C `getopt_long` argument contract.
pub(crate) unsafe fn long(
    argv: *mut *mut c_char,
    argc: c_int,
    optstring: *const c_char,
    options: *const OptionC,
    longindex: *mut c_int,
    only: bool,
) -> c_int {
    // SAFETY: the caller's arrays.
    let mut pointers = unsafe { argv_pointers(argv, argc) };
    // SAFETY: the caller's optstring and option array.
    let optstring = unsafe { crate::ffi::cstr::slice(optstring) };
    // SAFETY: the caller's option array.
    let table = unsafe { long_options(options) };
    let mut state = seed();
    let mut diag = StderrDiag;
    let step = state.getopt_long(
        &mut pointers,
        &ArgvSource,
        optstring,
        &table,
        only,
        crate::stdlib::env_support::get(b"POSIXLY_CORRECT").is_some(),
        &mut diag,
    );
    commit(&state, &step, &pointers);
    // SAFETY: the caller's argv array.
    unsafe { write_argv(argv, &pointers) };
    // SAFETY: the caller's option array.
    unsafe { long_result(&step, options, longindex) }
}
