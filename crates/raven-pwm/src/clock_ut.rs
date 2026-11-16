// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the PWM clock and range plan.

use super::*;

#[test]
fn the_clock_is_100_mhz_from_plld() {
    assert_eq!(PLLD_HZ, 500_000_000);
    assert_eq!(PWM_DIVI, 5);
    assert_eq!(PWM_CLOCK_HZ, 100_000_000);
    assert_eq!(CM_CTL_START, 0x5a00_0016);
    assert_eq!(CM_DIV_START, 0x5a00_5000);
}

#[test]
fn the_48k_plan_is_the_b41_value() {
    let plan = range_plan(48_000).expect("48k");
    assert_eq!(plan.range, 2083);
    assert_eq!(plan.actual_hz, 48_007);
    assert_eq!(plan.error_ppm, 160);
}

#[test]
fn the_44k1_plan_is_the_b41_value() {
    let plan = range_plan(44_100).expect("44.1k");
    assert_eq!(plan.range, 2268);
    assert_eq!(plan.actual_hz, 44_091);
    assert_eq!(plan.error_ppm, -188);
}

#[test]
fn the_smaller_rates_have_exact_ranges() {
    assert_eq!(range_plan(22_050).map(|plan| plan.range), Some(4535));
    assert_eq!(range_plan(11_025).map(|plan| plan.range), Some(9070));
    assert_eq!(range_plan(32_000).map(|plan| plan.range), Some(3125));
    assert_eq!(range_plan(96_000).map(|plan| plan.range), Some(1041));
}

#[test]
fn an_unknown_rate_has_no_table_plan_but_has_a_nearest_range() {
    assert_eq!(range_plan(12_345), None);
    let plan = nearest_range(12_345).expect("nearest");
    assert_eq!(plan.range, 8100);
    assert_eq!(plan.actual_hz, 12_345);
    assert_eq!(plan.error_ppm, 55);
}

#[test]
fn zero_and_above_clock_rates_have_no_nearest_plan() {
    assert_eq!(nearest_range(0), None);
    assert_eq!(nearest_range(PWM_CLOCK_HZ + 1), None);
    assert_eq!(RangePlan::new(0, 2083), None);
    assert_eq!(RangePlan::new(48_000, 0), None);
}

#[test]
fn the_stop_value_preserves_the_source_and_clears_enable() {
    let current = CM_CTL_START | 0x100;
    let stop = clock_stop_value(current);
    assert_eq!(stop & CM_CTL_ENAB, 0);
    assert_eq!(stop & 0x100, 0x100);
    assert_eq!(stop & 0x0f, CM_CTL_SRC_PLLD);
}

#[test]
fn the_busy_bit_and_register_offsets_are_usable() {
    assert!(clock_busy(CM_CTL_BUSY));
    assert!(!clock_busy(0));
    assert_eq!(clock_registers(), (0xa0, 0xa4));
    assert_eq!(clock_start_value(), CM_CTL_START);
    assert_eq!(clock_divider_value(), CM_DIV_START);
}
