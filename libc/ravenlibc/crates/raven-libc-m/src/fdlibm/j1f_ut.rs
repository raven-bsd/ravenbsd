// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/j1f.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (donor unit tests moved out of j1f.rs per handbook rule 9)

use super::{j1f, y1f};
#[test]
fn test_j1f_2488() {
    // 0x401F3E49
    assert_eq!(j1f(2.4881766_f32), 0.49999475_f32);
}

#[test]
fn test_y1f_2002() {
    //allow slightly different result on x87
    let res = y1f(2.0000002_f32);
    if cfg!(x86_no_sse2) && (res == -0.10703231_f32) {
        return;
    }
    assert_eq!(res, -0.10703229_f32);
}
