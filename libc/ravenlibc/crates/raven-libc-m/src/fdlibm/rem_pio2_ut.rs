// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/rem_pio2.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (donor unit tests moved out of rem_pio2.rs per handbook rule 9)

use super::rem_pio2;

#[test]
// FIXME(correctness): inaccurate results on i586
#[cfg_attr(x86_no_sse2, ignore)]
fn test_near_pi() {
    let arg = 3.141592025756836;
    let arg = force_eval!(arg);
    assert_eq!(
        rem_pio2(arg),
        (2, -6.278329573009626e-7, -2.1125998133974653e-23)
    );
    let arg = 3.141592033207416;
    let arg = force_eval!(arg);
    assert_eq!(
        rem_pio2(arg),
        (2, -6.20382377148128e-7, -2.1125998133974653e-23)
    );
    let arg = 3.141592144966125;
    let arg = force_eval!(arg);
    assert_eq!(
        rem_pio2(arg),
        (2, -5.086236681942706e-7, -2.1125998133974653e-23)
    );
    let arg = 3.141592979431152;
    let arg = force_eval!(arg);
    assert_eq!(
        rem_pio2(arg),
        (2, 3.2584135866119817e-7, -2.1125998133974653e-23)
    );
}

#[test]
fn test_overflow_b9b847() {
    let _ = rem_pio2(-3054214.5490637687);
}

#[test]
fn test_overflow_4747b9() {
    let _ = rem_pio2(917340800458.2274);
}
