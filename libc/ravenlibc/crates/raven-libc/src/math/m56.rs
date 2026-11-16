// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Continued `long double` C exports.

ld_ternary!(fmal, super::m43::fmal_inner);
ld_scalbn_i32!(scalbnl, super::m43::scalbnl_inner);
ld_scalbn_i32!(ldexpl, super::m43::ldexpl_inner);
ld_scalbn_i64!(scalblnl, super::m43::scalblnl_inner);
