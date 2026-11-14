// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C70 finding: a conversion never consumes past its slice.

use raven_libc_dtoa::{DtoaCtx, NumericLocale, Rounding, strtod, strtof};

#[test]
fn trailing_point_does_not_consume_the_end() {
    let locale = NumericLocale {
        decimal_point: b".",
    };
    let mut context = DtoaCtx::new();
    let result = strtod(&mut context, b"65.", &locale, Rounding::Nearest).expect("value");
    assert_eq!(result.consumed, 3);
    assert_eq!(result.value, 65.0);

    let mut context = DtoaCtx::new();
    let result = strtof(&mut context, b"65.", &locale, Rounding::Nearest).expect("value");
    assert_eq!(result.consumed, 3);
    assert_eq!(result.value, 65.0);
}
