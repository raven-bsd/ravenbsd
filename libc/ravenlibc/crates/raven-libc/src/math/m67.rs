// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `signbit` and the ordering predicates.

fn signbit_value(x: f64) -> i32 {
    i32::from(raven_libc_m::signbit(x))
}

unary_i32!(signbit, signbit_value);
binary_i32!(isgreater, raven_libc_m::isgreater);
binary_i32!(isgreaterequal, raven_libc_m::isgreaterequal);
binary_i32!(isless, raven_libc_m::isless);
