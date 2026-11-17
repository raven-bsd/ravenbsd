// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `logb`, `logbf`, `llogb` and `scalbln`.

unary!(logb, raven_libc_m::logb);
unaryf!(logbf, raven_libc_m::logbf);
unary_i64!(llogb, raven_libc_m::llogb);
scalbln_i64!(scalbln, raven_libc_m::scalbln, core::ffi::c_double);
