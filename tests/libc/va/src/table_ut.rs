// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The one expansion of `for_each_variadic!` with the test prefix `"t_"`.
//!
//! Included by the table test and by every differential test that calls a trampoline. The
//! `snprintf` and `sscanf` rows get the REAL engine targets (`targets::vsnprintf`,
//! `targets::vsscanf`); every other row gets a recorder with the row's `named` count.

macro_rules! raven_variadic_test {
    (prefix: $prefix:literal, export: "snprintf", target: $target:ident, named: $n:tt) => {
        raven_libc_va_test::raven_variadic!(
            prefix: $prefix,
            export: "snprintf",
            target: crate::targets::vsnprintf,
            named: 3
        );
    };
    (prefix: $prefix:literal, export: "sscanf", target: $target:ident, named: $n:tt) => {
        raven_libc_va_test::raven_variadic!(
            prefix: $prefix,
            export: "sscanf",
            target: crate::targets::vsscanf,
            named: 2
        );
    };
    (prefix: $prefix:literal, export: $export:literal, target: $target:ident, named: $n:tt) => {
        crate::recorder_named!(name: $target, named: $n);
        raven_libc_va_test::raven_variadic!(
            prefix: $prefix,
            export: $export,
            target: $target,
            named: $n
        );
    };
}

raven_libc_va_test::for_each_variadic!(raven_variadic_test, "t_");
