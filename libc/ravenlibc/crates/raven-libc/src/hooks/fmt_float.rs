// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Strong `__rl_fmt_float`: decimal digits from `raven-libc-dtoa` (C42).

use raven_libc_dtoa::{DigitMode, DtoaCtx, Kind, Rounding, dtoa};
use raven_libc_fmt::{DigitsOut, FloatBits, FloatClass};

/// The strong float-digit hook.
#[unsafe(no_mangle)]
pub fn __rl_fmt_float_impl(
    value: FloatBits,
    mode: u8,
    ndigits: i32,
    out: &mut [u8],
) -> Option<DigitsOut> {
    let mut context = DtoaCtx::new();
    let mode = if mode == 3 {
        DigitMode::Decimals
    } else {
        DigitMode::SigDigits
    };
    let bits = u64::try_from(value.bits & 0xffff_ffff_ffff_ffff).unwrap_or(0);
    let digits = dtoa(
        &mut context,
        f64::from_bits(bits),
        mode,
        ndigits,
        Rounding::Nearest,
        out,
    )
    .ok()?;
    let class = match digits.kind {
        Kind::Finite => FloatClass::Finite,
        Kind::Zero => FloatClass::Zero,
        Kind::Inf => FloatClass::Inf,
        Kind::Nan => FloatClass::Nan,
    };
    Some(DigitsOut {
        len: digits.len,
        decpt: digits.decpt,
        neg: digits.negative,
        class,
    })
}

/// Install the strong hook.
pub(crate) fn install() {
    crate::stdio::vprintf::__rl_fmt_float::install(Some(__rl_fmt_float_impl));
}
