// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `prctl(2)`: the C59 variadic trampoline and its four-slot reader.
use core::ffi::{c_char, c_int};

use raven_libc_fmt::VaReader;

use raven_libc_personality_linux::prctl as model;
use raven_libc_personality_linux::seams::Errno;

use crate::personality_common::{RealProc, ret_i64};
use crate::stdio::va::{List, reader};

crate::raven_variadic!(
    prefix: "",
    export: "prctl",
    target: crate::personality_linux::prctl::__raven_va_prctl,
    named: 1
);

/// The trampoline target: one option and four `unsigned long` slots.
///
/// # Safety
/// The trampoline built `list`; `a2` is a pointer for the name getter/setter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_va_prctl(option: c_int, list: *mut List) -> c_int {
    // SAFETY: the trampoline's list is valid for the call.
    let mut values = unsafe { reader(list) };
    let a2 = values.next_u64();
    let a3 = values.next_u64();
    let a4 = values.next_u64();
    let a5 = values.next_u64();
    match option {
        model::PR_SET_NAME => {
            if a2 == 0 {
                return ret_i64(Err(Errno::EFAULT));
            }
            // SAFETY: the caller passes a readable C string for PR_SET_NAME.
            let name = unsafe { crate::ffi::cstr::slice_n(a2 as *const c_char, 15) };
            ret_i64(model::set_name(&mut RealProc, name))
        }
        model::PR_GET_NAME => {
            if a2 == 0 {
                return ret_i64(Err(Errno::EFAULT));
            }
            match model::get_name(&mut RealProc) {
                Ok(name) => {
                    // SAFETY: the caller passes a writable 16-byte buffer.
                    unsafe { core::ptr::copy_nonoverlapping(name.as_ptr(), a2 as *mut u8, 16) };
                    0
                }
                Err(error) => ret_i64(Err(error)),
            }
        }
        model::PR_GET_PDEATHSIG => {
            if a2 == 0 {
                return ret_i64(Err(Errno::EFAULT));
            }
            match model::prctl(&mut RealProc, option, a2, a3, a4, a5) {
                Ok(value) => {
                    let signal = c_int::try_from(value).unwrap_or(-1);
                    // SAFETY: the caller passes a writable int for PR_GET_PDEATHSIG.
                    unsafe { core::ptr::write_unaligned(a2 as *mut c_int, signal) };
                    0
                }
                Err(error) => ret_i64(Err(error)),
            }
        }
        _ => ret_i64(model::prctl(&mut RealProc, option, a2, a3, a4, a5)),
    }
}
