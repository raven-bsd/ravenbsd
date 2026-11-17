// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wcstold`: wide `strtold`.

use core::ffi::{c_char, c_int};

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, RangeFlags, Rounding, strtold as dtoa_strtold};

use super::wcsto_support::{encode, units_for_bytes};

extern "C" fn wcstold_inner(out: *mut u128, input: *const u32, endptr: *mut *mut u32) {
    let bytes = encode(input);
    let mut converted: *mut c_char = core::ptr::null_mut();
    let mut context = DtoaCtx::new();
    let value = match dtoa_strtold(&mut context, &bytes, &C_NUMERIC, Rounding::Nearest) {
        Ok(parsed) => {
            if !endptr.is_null() {
                // SAFETY: the caller grants a writable end pointer.
                unsafe {
                    *endptr = input
                        .add(units_for_bytes(input, parsed.consumed))
                        .cast_mut();
                }
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
                // SAFETY: as above.
                unsafe { *endptr = input.cast_mut() };
            }
            0
        }
    };
    let _ = converted;
    // SAFETY: the naked stub supplies a writable 16-byte result slot.
    unsafe { *out = value };
}

/// `wcstold`: the x87 memory-slot/st(0) C ABI wrapper.
///
/// # Safety
/// The compiler-generated C ABI contract applies.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstold() -> ! {
    core::arch::naked_asm!(
        "sub rsp, 24",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "lea rdi, [rsp]",
        "call {inner}",
        "fld tbyte ptr [rsp]",
        "add rsp, 24",
        "ret",
        inner = sym wcstold_inner,
    )
}

/// `wcstold`: the aarch64 AAPCS64 binary128-in-`v0` C ABI wrapper.
///
/// # Safety
/// The compiler-generated C ABI contract applies.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstold() -> ! {
    core::arch::naked_asm!(
        ".arch armv8-a",
        "sub sp, sp, #32",
        "str x30, [sp, #16]",
        "mov x2, x1",
        "mov x1, x0",
        "mov x0, sp",
        "bl {inner}",
        "ldr q0, [sp]",
        "ldr x30, [sp, #16]",
        "add sp, sp, #32",
        "ret",
        inner = sym wcstold_inner,
    )
}
