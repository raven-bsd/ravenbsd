// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The whole variadic table expanded once for the bare-metal build check (prefix `vstub_`, so no
//! clash with the `t_` instances of the test binaries).
//!
//! `cargo build -p raven-libc-va-test --target <arch>-unknown-none` is the build check of the
//! module on a bare target (C59 section 4.1). To make that command exercise the macro too,
//! this module expands every row of the table with the `"t_"` prefix and a dummy target: on
//! x86_64 the hosted (or soft-float) stub is assembled, on aarch64/riscv64 the link-time
//! failing stub, and with the reviewer's `raven_va_verified_*` flag the real trampoline. The
//! symbols stay inside this test crate's library and are never linked into a shipped artifact.

/// The one target every row points at: the macro only needs a `sym` path.
///
/// # Safety
///
/// The stub calls it with the ABI of the row; it is never executed, the module exists for the
/// compile-and-assemble check only.
pub unsafe extern "C" fn dummy_target() -> i32 {
    0
}

macro_rules! stub_row {
    (prefix: $prefix:literal, export: $export:literal, target: $target:ident, named: $n:tt) => {
        crate::raven_variadic!(
            prefix: $prefix,
            export: $export,
            target: crate::va_stubs::dummy_target,
            named: $n
        );
    };
}

crate::for_each_variadic!(stub_row, "vstub_");
