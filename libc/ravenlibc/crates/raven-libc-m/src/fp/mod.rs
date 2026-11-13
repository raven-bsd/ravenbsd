// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Floating-point classification and NaN construction.

pub use classify::{
    fpclassify, fpclassifyf, isfinite, isfinitef, isinf, isinff, isnan, isnanf, isnormal,
    isnormalf, signbit, signbitf,
};
pub use nan::{nan, nanf};

mod classify;
mod nan;
