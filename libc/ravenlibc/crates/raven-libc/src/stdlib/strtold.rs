// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strtold(3)`: correctly rounded `long double` parsing.

use core::ffi::{c_char, c_int};

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, RangeFlags, Rounding, strtold as dtoa_strtold};

extern "C" fn strtold_inner(out: *mut u128, s: *const c_char, endptr: *mut *mut c_char) {
    // SAFETY: the caller's string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(s) };
    let mut context = DtoaCtx::new();
    let value = match dtoa_strtold(&mut context, bytes, &C_NUMERIC, Rounding::Nearest) {
        Ok(parsed) => {
            if !endptr.is_null() {
                // SAFETY: the caller grants a writable pointer slot.
                unsafe { *endptr = s.add(parsed.consumed).cast_mut() };
            }
            if parsed.range.contains(RangeFlags::OVERFLOW)
                || parsed.range.contains(RangeFlags::UNDERFLOW)
            {
                crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::ERANGE));
            }
            raven_libc_m::LongDouble::from_dtoa_bytes(parsed.value.0).to_bits()
        }
        Err(_) => {
            crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::ENOMEM));
            if !endptr.is_null() {
                // SAFETY: as above; no conversion consumed input.
                unsafe { *endptr = s.cast_mut() };
            }
            0
        }
    };
    // SAFETY: the naked stub supplies a writable 16-byte result slot.
    unsafe { *out = value };
}

/// `strtold`: the x87 memory-slot/st(0) C ABI wrapper.
///
/// # Safety
/// The compiler-generated C ABI contract applies.
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtold() -> ! {
    core::arch::naked_asm!(
        "sub rsp, 24",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "lea rdi, [rsp]",
        "call {inner}",
        "fld tbyte ptr [rsp]",
        "add rsp, 24",
        "ret",
        inner = sym strtold_inner,
    )
}
