// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `lround` and `llround` with their f32 forms.

unary_i64!(lround, raven_libc_m::lround);
unaryf_i64!(lroundf, raven_libc_m::lroundf);
unary_i64!(llround, raven_libc_m::llround);
unaryf_i64!(llroundf, raven_libc_m::llroundf);
