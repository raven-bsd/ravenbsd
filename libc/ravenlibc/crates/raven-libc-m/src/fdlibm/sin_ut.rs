// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/sin.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (donor unit tests moved out of sin.rs per handbook rule 9)

use super::*;

#[test]
#[cfg_attr(x86_no_sse2, ignore = "FIXME(i586): possible incorrect rounding")]
fn test_near_pi() {
    let x = f64::from_bits(0x400921fb000FD5DD); // 3.141592026217707
    let sx = f64::from_bits(0x3ea50d15ced1a4a2); // 6.273720864039205e-7
    assert_eq!(sin(x), sx);
}
