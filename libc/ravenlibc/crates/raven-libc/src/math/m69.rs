// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `iszero`, `issubnormal` and `iscanonical`.

unary_i32!(iszero, raven_libc_m::iszero);
unary_i32!(issubnormal, raven_libc_m::issubnormal);
unary_i32!(iscanonical, raven_libc_m::iscanonical);
