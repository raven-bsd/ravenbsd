// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `nexttoward` and `nexttowardf` (the `long double` argument uses the x87 memory slot).

use raven_libc_m::LongDouble;

extern "C" fn nexttoward_inner(x: f64, y: LongDouble) -> f64 {
    raven_libc_m::nexttoward(x, y)
}

extern "C" fn nexttowardf_inner(x: f32, y: LongDouble) -> f32 {
    raven_libc_m::nexttowardf(x, y)
}

nexttoward!(nexttoward, nexttoward_inner);
nexttoward!(nexttowardf, nexttowardf_inner);
