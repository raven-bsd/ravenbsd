// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn counters_clamp_to_i32_max() {
    assert_eq!(clamp_i32(0), 0);
    assert_eq!(clamp_i32(i32::MAX as usize), i32::MAX);
    assert_eq!(clamp_i32(i32::MAX as usize + 1), i32::MAX);
    assert_eq!(clamp_i32(usize::MAX), i32::MAX);
}

#[test]
fn narrow_preserves_every_field() {
    let wide = Mallinfo2 {
        arena: 1,
        ordblks: 2,
        smblks: 3,
        hblks: 4,
        hblkhd: 5,
        usmblks: 6,
        fsmblks: 7,
        uordblks: 8,
        fordblks: 9,
        keepcost: 10,
    };
    let narrow_info = narrow(wide);
    assert_eq!(narrow_info.arena, 1);
    assert_eq!(narrow_info.ordblks, 2);
    assert_eq!(narrow_info.smblks, 3);
    assert_eq!(narrow_info.hblks, 4);
    assert_eq!(narrow_info.hblkhd, 5);
    assert_eq!(narrow_info.usmblks, 6);
    assert_eq!(narrow_info.fsmblks, 7);
    assert_eq!(narrow_info.uordblks, 8);
    assert_eq!(narrow_info.fordblks, 9);
    assert_eq!(narrow_info.keepcost, 10);
}
