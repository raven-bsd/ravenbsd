// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;

#[test]
fn uc_exhaustive_leading() {
    for value in 0u8..=255 {
        assert_eq!(
            stdc_leading_zeros_uc(value),
            value.leading_zeros(),
            "lz {value}"
        );
        assert_eq!(
            stdc_leading_ones_uc(value),
            (!value).leading_zeros(),
            "lo {value}"
        );
        assert_eq!(
            stdc_trailing_zeros_uc(value),
            value.trailing_zeros(),
            "tz {value}"
        );
        assert_eq!(
            stdc_trailing_ones_uc(value),
            (!value).trailing_zeros(),
            "to {value}"
        );
    }
}

#[test]
fn uc_exhaustive_first_and_counts() {
    for value in 0u8..=255 {
        let first_leading_zero = if value == 0xFF {
            0
        } else {
            (!value).leading_zeros() + 1
        };
        let first_leading_one = if value == 0 {
            0
        } else {
            value.leading_zeros() + 1
        };
        let first_trailing_zero = if value == 0xFF {
            0
        } else {
            (!value).trailing_zeros() + 1
        };
        let first_trailing_one = if value == 0 {
            0
        } else {
            value.trailing_zeros() + 1
        };
        assert_eq!(
            stdc_first_leading_zero_uc(value),
            first_leading_zero,
            "flz {value}"
        );
        assert_eq!(
            stdc_first_leading_one_uc(value),
            first_leading_one,
            "flo {value}"
        );
        assert_eq!(
            stdc_first_trailing_zero_uc(value),
            first_trailing_zero,
            "ftz {value}"
        );
        assert_eq!(
            stdc_first_trailing_one_uc(value),
            first_trailing_one,
            "fto {value}"
        );
        assert_eq!(
            stdc_count_zeros_uc(value),
            8 - value.count_ones(),
            "cz {value}"
        );
        assert_eq!(stdc_count_ones_uc(value), value.count_ones(), "co {value}");
        assert_eq!(
            stdc_has_single_bit_uc(value),
            value.count_ones() == 1,
            "hsb {value}"
        );
        assert_eq!(
            stdc_bit_width_uc(value),
            if value == 0 {
                0
            } else {
                8 - value.leading_zeros()
            },
            "bw {value}"
        );
        let floor = if value == 0 {
            0
        } else {
            1u8 << (7 - value.leading_zeros())
        };
        assert_eq!(stdc_bit_floor_uc(value), floor, "bf {value}");
        let ceil = if value <= 1 {
            1
        } else if value > 128 {
            0
        } else {
            let mut power = 1u8;
            while power < value {
                power <<= 1;
            }
            power
        };
        assert_eq!(stdc_bit_ceil_uc(value), ceil, "bc {value}");
    }
}

#[test]
fn us_exhaustive() {
    for value in 0u16..=0xFFFF {
        assert_eq!(
            stdc_leading_zeros_us(value),
            value.leading_zeros(),
            "lz {value}"
        );
        assert_eq!(
            stdc_leading_ones_us(value),
            (!value).leading_zeros(),
            "lo {value}"
        );
        assert_eq!(
            stdc_trailing_zeros_us(value),
            value.trailing_zeros(),
            "tz {value}"
        );
        assert_eq!(
            stdc_trailing_ones_us(value),
            (!value).trailing_zeros(),
            "to {value}"
        );
        assert_eq!(stdc_count_ones_us(value), value.count_ones(), "co {value}");
        assert_eq!(
            stdc_count_zeros_us(value),
            16 - value.count_ones(),
            "cz {value}"
        );
        assert_eq!(
            stdc_has_single_bit_us(value),
            value.count_ones() == 1,
            "hsb {value}"
        );
        assert_eq!(
            stdc_bit_width_us(value),
            if value == 0 {
                0
            } else {
                16 - value.leading_zeros()
            },
            "bw {value}"
        );
        let first_trailing_one = if value == 0 {
            0
        } else {
            value.trailing_zeros() + 1
        };
        assert_eq!(stdc_first_trailing_one_us(value), first_trailing_one);
    }
}

fn check_u64(value: u64) {
    let lz = value.leading_zeros();
    assert_eq!(stdc_leading_zeros_ull(value), lz);
    assert_eq!(stdc_leading_ones_ull(value), (!value).leading_zeros());
    assert_eq!(stdc_trailing_zeros_ull(value), value.trailing_zeros());
    assert_eq!(stdc_trailing_ones_ull(value), (!value).trailing_zeros());
    assert_eq!(stdc_count_ones_ull(value), value.count_ones());
    assert_eq!(stdc_count_zeros_ull(value), (!value).count_ones());
    assert_eq!(stdc_has_single_bit_ull(value), value.count_ones() == 1);
    assert_eq!(
        stdc_bit_width_ull(value),
        if value == 0 { 0 } else { 64 - lz }
    );
    assert_eq!(stdc_leading_zeros_ul(value), lz);
    assert_eq!(stdc_trailing_zeros_ul(value), value.trailing_zeros());
    assert_eq!(stdc_count_ones_ul(value), value.count_ones());
    assert_eq!(stdc_bit_floor_ul(value), stdc_bit_floor_ull(value));
    assert_eq!(stdc_bit_ceil_ul(value), stdc_bit_ceil_ull(value));
    let floor = if value == 0 { 0 } else { 1u64 << (63 - lz) };
    assert_eq!(stdc_bit_floor_ull(value), floor);
    let ceil = if value <= 1 {
        1
    } else if value > u64::MAX / 2 + 1 {
        0
    } else {
        value.next_power_of_two()
    };
    assert_eq!(stdc_bit_ceil_ull(value), ceil);
}

#[test]
fn u64_boundaries() {
    for value in [
        0u64,
        1,
        2,
        3,
        4,
        7,
        8,
        9,
        u64::MAX,
        u64::MAX - 1,
        u64::MAX / 2,
        u64::MAX / 2 + 1,
        u64::MAX / 2 + 2,
        1 << 63,
        (1 << 63) - 1,
    ] {
        check_u64(value);
    }
}

#[test]
fn u64_random() {
    let mut state = 0xdead_beef_1234_5678u64;
    for _ in 0..20_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        check_u64(state);
    }
}

#[test]
fn ui_boundaries() {
    for value in [0u32, 1, 2, 3, 0x7fff_ffff, 0x8000_0000, u32::MAX] {
        assert_eq!(stdc_count_ones_ui(value), value.count_ones());
        assert_eq!(stdc_leading_zeros_ui(value), value.leading_zeros());
        assert_eq!(stdc_trailing_zeros_ui(value), value.trailing_zeros());
        assert_eq!(
            stdc_bit_width_ui(value),
            if value == 0 {
                0
            } else {
                32 - value.leading_zeros()
            }
        );
        let ceil = if value <= 1 {
            1
        } else if value > u32::MAX / 2 + 1 {
            0
        } else {
            value.next_power_of_two()
        };
        assert_eq!(stdc_bit_ceil_ui(value), ceil);
    }
}
