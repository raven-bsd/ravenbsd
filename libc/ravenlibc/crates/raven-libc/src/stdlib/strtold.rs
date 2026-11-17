// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `strtold(3)`: correctly rounded `long double` parsing through `raven-libc-dtoa`.
//!
//! x86-64 returns the 80-bit value in `st(0)`; Rust has no `f80`, so the export is a naked stub
//! that hands a 16-byte image to the helper and reloads it with `fldt`, the same contract as the
//! `long double` math entries. The other architectures defer `strtold` with the math family.

#[cfg(all(target_os = "none", target_arch = "x86_64"))]
mod x86_64 {
    use core::ffi::{c_char, c_int};

    use raven_libc_dtoa::{
        C_NUMERIC, DtoaCtx, LongDouble, RangeFlags, Rounding, strtold as dtoa_strtold,
    };

    /// The parse worker: fill `out` and the caller's end pointer.
    extern "C" fn parse(out: *mut LongDouble, s: *const c_char, endptr: *mut *mut c_char) {
        // SAFETY: the naked stub supplies a writable 16-byte `out` slot; the caller's string and
        // pointer-slot contracts are the C `strtold` contract.
        unsafe {
            let bytes = crate::ffi::cstr::slice(s);
            let mut context = DtoaCtx::new();
            match dtoa_strtold(&mut context, bytes, &C_NUMERIC, Rounding::Nearest) {
                Ok(parsed) => {
                    *out = parsed.value;
                    if !endptr.is_null() {
                        *endptr = s.add(parsed.consumed).cast_mut();
                    }
                    if parsed.range.contains(RangeFlags::OVERFLOW)
                        || parsed.range.contains(RangeFlags::UNDERFLOW)
                    {
                        crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::ERANGE));
                    }
                }
                Err(_) => {
                    *out = LongDouble([0u8; 16]);
                    if !endptr.is_null() {
                        *endptr = s.cast_mut();
                    }
                    crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::ENOMEM));
                }
            }
        }
    }

    /// `strtold`: parse `s` and return the value in `st(0)`.
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
            inner = sym parse,
        )
    }
}
