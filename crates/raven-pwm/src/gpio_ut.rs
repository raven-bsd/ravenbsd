// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the GPIO40/41 pinmux arithmetic.

use super::*;

#[test]
fn the_default_plan_sets_both_pins_to_alt0_and_pull_off() {
    let plan = PinPlan::default();
    assert!(gpfsel4_is_alt0(plan.gpfsel4));
    assert!(gppuppdn2_is_pull_off(plan.gppuppdn2));
    assert_eq!(plan.gpfsel4 & 0x3f, 4 | (4 << 3));
}

#[test]
fn the_plan_preserves_unrelated_gpio_fields() {
    let plan = PinPlan::new(0xffff_ffff, 0xffff_ffff);
    assert_eq!(plan.gpfsel4 & !0x3f, !0x3f);
    assert_eq!(plan.gppuppdn2 & !0xf_0000, !0xf_0000);
    assert!(gpfsel4_is_alt0(plan.gpfsel4));
    assert!(gppuppdn2_is_pull_off(plan.gppuppdn2));
}

#[test]
fn the_plan_clears_a_previous_function_and_pull() {
    let plan = configure(0x0000_0000, 0x000f_0000);
    assert_eq!(plan.gpfsel4 & 0x3f, 0x24);
    assert_eq!(plan.gppuppdn2 & 0xf_0000, 0);
}

#[test]
fn a_wrong_function_is_detected() {
    let plan = PinPlan::default();
    assert!(!gpfsel4_is_alt0(plan.gpfsel4 & !0x7));
    assert!(!gppuppdn2_is_pull_off(plan.gppuppdn2 | (1 << 16)));
}

#[test]
fn the_pin_list_is_gpio40_and_gpio41() {
    assert_eq!(PWM_PINS, [40, 41]);
}
