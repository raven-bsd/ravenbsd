// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C floating-point environment: exception flags, rounding mode, saved environments.
//!
//! The x86_64 backend keeps the x87 control/status words and `mxcsr` consistent (the donor
//! `amd64/fenv.c` rule); the soft-float backend keeps the flags, masks and rounding mode in one
//! software word (4.8). aarch64 and riscv64 use the software backend until their register
//! backends land (reported as a phase-1 gap).

#[cfg(all(
    target_arch = "x86_64",
    not(feature = "soft-float"),
    target_feature = "sse2"
))]
#[allow(unsafe_code)]
#[path = "x86_64.rs"]
mod imp;

#[cfg(not(all(
    target_arch = "x86_64",
    not(feature = "soft-float"),
    target_feature = "sse2"
)))]
#[path = "softfloat.rs"]
mod imp;

pub use imp::{
    Femode, FenvT, FexceptT, feclearexcept, fedisableexcept, feenableexcept, fegetenv, fegetexcept,
    fegetexceptflag, fegetmode, fegetround, feholdexcept, feraiseexcept, fesetenv, fesetexceptflag,
    fesetmode, fesetround, fetestexcept, feupdateenv,
};

#[cfg(all(
    target_arch = "x86_64",
    not(feature = "soft-float"),
    target_feature = "sse2"
))]
pub(crate) fn raise_software(_flags: u32) {}

#[cfg(not(all(
    target_arch = "x86_64",
    not(feature = "soft-float"),
    target_feature = "sse2"
)))]
pub(crate) fn raise_software(flags: u32) {
    imp::raise(flags);
}

/// `FE_DFL_ENV`: the default environment for [`fesetenv`] on the soft-float backend.
pub fn fesetenv_default() -> i32 {
    imp::fesetenv_default()
}
