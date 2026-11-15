// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the clock divider table and the typed bus/timing transitions.

use super::*;

#[test]
fn specification_version_selects_the_divider_encoding() {
    assert_eq!(SpecVersion::from_register(0), SpecVersion::V1);
    assert_eq!(SpecVersion::from_register(1), SpecVersion::V2);
    assert_eq!(SpecVersion::from_register(2), SpecVersion::V3);
    assert_eq!(SpecVersion::from_register(0xff), SpecVersion::V3);
    assert!(!SpecVersion::V2.uses_ten_bit_divider());
    assert!(SpecVersion::V3.uses_ten_bit_divider());
}

#[test]
fn version_3_plans_the_pi4_speeds_from_a_50_mhz_base() {
    let identification =
        ClockPlan::for_frequency(50_000_000, 400_000, SpecVersion::V3).expect("identification");
    assert_eq!(identification.divider, 63);
    assert_eq!(identification.actual_hz, 396_825);
    assert_eq!(identification.control, 0x3f00);
    assert_eq!(identification.target_hz, 400_000);

    let default =
        ClockPlan::for_frequency(50_000_000, 25_000_000, SpecVersion::V3).expect("default speed");
    assert_eq!(default.divider, 1);
    assert_eq!(default.actual_hz, 25_000_000);
    assert_eq!(default.control, 0x0100);

    let high =
        ClockPlan::for_frequency(50_000_000, 50_000_000, SpecVersion::V3).expect("high speed");
    assert_eq!(high.divider, 0);
    assert_eq!(high.actual_hz, 50_000_000);
    assert_eq!(high.control, 0);
}

#[test]
fn version_3_splits_a_large_divider_across_the_two_fields() {
    let plan =
        ClockPlan::for_frequency(200_000_000, 400_000, SpecVersion::V3).expect("identification");
    assert_eq!(plan.divider, 250);
    assert_eq!(plan.control, 0xfa00);
    assert_eq!(plan.actual_hz, 400_000);
}

#[test]
fn version_2_plans_powers_of_two_from_a_50_mhz_base() {
    let identification =
        ClockPlan::for_frequency(50_000_000, 400_000, SpecVersion::V2).expect("identification");
    assert_eq!(identification.divider, 128);
    assert_eq!(identification.actual_hz, 390_625);
    assert_eq!(identification.control, 0x4000);

    let default =
        ClockPlan::for_frequency(50_000_000, 25_000_000, SpecVersion::V2).expect("default speed");
    assert_eq!(default.divider, 2);
    assert_eq!(default.actual_hz, 25_000_000);
    assert_eq!(default.control, 0x0100);

    let high =
        ClockPlan::for_frequency(50_000_000, 50_000_000, SpecVersion::V2).expect("high speed");
    assert_eq!(high.divider, 1);
    assert_eq!(high.actual_hz, 50_000_000);
    assert_eq!(high.control, 0);
}

#[test]
fn impossible_clock_requests_are_typed_errors() {
    assert_eq!(
        ClockPlan::for_frequency(0, 400_000, SpecVersion::V3),
        Err(ClockError::BaseClockZero)
    );
    assert_eq!(
        ClockPlan::for_frequency(50_000_000, 0, SpecVersion::V3),
        Err(ClockError::TargetZero)
    );
    assert_eq!(
        ClockPlan::for_frequency(50_000_000, 1_000, SpecVersion::V3),
        Err(ClockError::TooSlow)
    );
    assert_eq!(
        ClockPlan::for_frequency(50_000_000, 1_000, SpecVersion::V2),
        Err(ClockError::TooSlow)
    );
}

#[test]
fn timing_walks_identification_to_default_to_high_speed() {
    let mut timing = Timing::new(50_000_000, SpecVersion::V3).expect("timing");
    assert_eq!(timing.speed(), SpeedMode::Identification);
    assert_eq!(timing.width(), BusWidth::Bit1);
    assert_eq!(
        timing.clock_plan().expect("identification plan").actual_hz,
        396_825
    );

    let default = timing.switch_to_default().expect("default speed");
    assert_eq!(timing.speed(), SpeedMode::Default);
    assert_eq!(default.actual_hz, 25_000_000);

    let high = timing.switch_to_high_speed().expect("high speed");
    assert_eq!(timing.speed(), SpeedMode::HighSpeed);
    assert_eq!(high.actual_hz, 50_000_000);

    assert_eq!(
        timing.switch_to_high_speed(),
        Err(TimingError::InvalidSpeedTransition {
            from: SpeedMode::HighSpeed,
            to: SpeedMode::HighSpeed,
        })
    );
}

#[test]
fn high_speed_cannot_skip_the_default_mode() {
    let mut timing = Timing::new(50_000_000, SpecVersion::V3).expect("timing");
    assert_eq!(
        timing.switch_to_high_speed(),
        Err(TimingError::InvalidSpeedTransition {
            from: SpeedMode::Identification,
            to: SpeedMode::HighSpeed,
        })
    );
}

#[test]
fn four_bit_mode_requires_the_card_to_leave_identification() {
    let mut timing = Timing::new(50_000_000, SpecVersion::V3).expect("timing");
    assert_eq!(
        timing.set_bus_width(BusWidth::Bit4),
        Err(TimingError::InvalidBusWidth {
            speed: SpeedMode::Identification,
            width: BusWidth::Bit4,
        })
    );
    timing.switch_to_default().expect("default speed");
    timing.set_bus_width(BusWidth::Bit4).expect("four bit");
    assert_eq!(timing.width(), BusWidth::Bit4);
    timing.set_bus_width(BusWidth::Bit1).expect("one bit");
    assert_eq!(timing.width(), BusWidth::Bit1);
}

#[test]
fn host_control_respects_the_emmc2_high_speed_quirk() {
    let mut timing = Timing::new(50_000_000, SpecVersion::V3).expect("timing");
    timing.switch_to_default().expect("default speed");
    timing.set_bus_width(BusWidth::Bit4).expect("four bit");
    assert_eq!(timing.host_control(Quirks::PI4_EMMC2, DmaMode::Pio), 0x02);
    timing.switch_to_high_speed().expect("high speed");
    assert_eq!(timing.host_control(Quirks::PI4_EMMC2, DmaMode::Pio), 0x02);
    assert_eq!(timing.host_control(Quirks::PI4_EMMC2, DmaMode::Adma2), 0x12);

    let mut generic = Quirks::PI4_EMMC2;
    generic.no_high_speed_bit = false;
    assert_eq!(timing.host_control(generic, DmaMode::Pio), 0x06);
}

#[test]
fn bus_width_arguments_match_the_sd_specification() {
    assert_eq!(BusWidth::Bit1.acmd6_argument(), 0);
    assert_eq!(BusWidth::Bit4.acmd6_argument(), 2);
    assert_eq!(BusWidth::Bit1.host_control_bits(), 0);
    assert_eq!(BusWidth::Bit4.host_control_bits(), 0x02);
}

#[test]
fn speed_steps_are_ordered_and_match_the_pi4_contract() {
    assert_eq!(PI4_SPEED_STEPS.len(), 3);
    assert_eq!(PI4_SPEED_STEPS[0].mode, SpeedMode::Identification);
    assert_eq!(PI4_SPEED_STEPS[0].frequency_hz, 400_000);
    assert_eq!(PI4_SPEED_STEPS[0].width, BusWidth::Bit1);
    assert_eq!(PI4_SPEED_STEPS[1].mode, SpeedMode::Default);
    assert_eq!(PI4_SPEED_STEPS[1].frequency_hz, 25_000_000);
    assert_eq!(PI4_SPEED_STEPS[1].width, BusWidth::Bit4);
    assert_eq!(PI4_SPEED_STEPS[2].mode, SpeedMode::HighSpeed);
    assert_eq!(PI4_SPEED_STEPS[2].frequency_hz, 50_000_000);
    assert_eq!(PI4_SPEED_STEPS[2].width, BusWidth::Bit4);
    assert_eq!(SpeedMode::Identification.name(), "identification");
    assert_eq!(SpeedMode::HighSpeed.name(), "high-speed");
}
