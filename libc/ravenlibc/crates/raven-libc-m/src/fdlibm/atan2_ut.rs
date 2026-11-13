// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/atan2.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (donor unit tests moved out of atan2.rs per handbook rule 9)

use super::*;

#[test]
#[cfg_attr(x86_no_sse2, ignore = "FIXME(i586): possible incorrect rounding")]
fn sanity_check() {
    assert_eq!(atan2(0.0, 1.0), 0.0);
    assert_eq!(atan2(0.0, -1.0), PI);
    assert_eq!(atan2(-0.0, -1.0), -PI);
    assert_eq!(atan2(3.0, 2.0), atan(3.0 / 2.0));
    assert_eq!(atan2(2.0, -1.0), atan(2.0 / -1.0) + PI);
    assert_eq!(atan2(-2.0, -1.0), atan(-2.0 / -1.0) - PI);
}
