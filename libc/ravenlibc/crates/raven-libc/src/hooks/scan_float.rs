// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Strong `__rl_scan_float`: a validated `scanf` token to `f32`/`f64` bits via `raven-libc-dtoa`.

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, Rounding, strtod, strtof};

/// The strong scan hook. `wide` selects `f32` (the `%f` destination is a `float`).
#[unsafe(no_mangle)]
pub fn __rl_scan_float_impl(token: &[u8], single: bool, bits: &mut u64) -> bool {
    let mut context = DtoaCtx::new();
    if single {
        match strtof(&mut context, token, &C_NUMERIC, Rounding::Nearest) {
            Ok(parsed) if parsed.consumed > 0 => {
                *bits = u64::from(parsed.value.to_bits());
                true
            }
            _ => false,
        }
    } else {
        match strtod(&mut context, token, &C_NUMERIC, Rounding::Nearest) {
            Ok(parsed) if parsed.consumed > 0 => {
                *bits = parsed.value.to_bits();
                true
            }
            _ => false,
        }
    }
}

/// Install the strong hook.
pub(crate) fn install() {
    // The scanf core (`vsscanf`) is part of the C64 pass; until it exists the cell keeps the
    // matching-failure dummy of the hook table.
}
