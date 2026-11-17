// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `lrint` and `llrint` with their f32 forms.

unary_i64!(lrint, raven_libc_m::lrint);
unaryf_i64!(lrintf, raven_libc_m::lrintf);
unary_i64!(llrint, raven_libc_m::llrint);
unaryf_i64!(llrintf, raven_libc_m::llrintf);
