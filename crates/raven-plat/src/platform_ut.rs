// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the fake platform backend.

use super::*;
use crate::gpio::{Function, Pull};

#[test]
fn the_fake_board_is_stable() {
    let mut platform = FakePlatform::new();
    let board = platform.board_info().expect("board");
    assert_eq!(board, BoardInfo::fake());
    assert_eq!(platform.clock_rate(12), Ok(50_000_000));
    assert_eq!(platform.power_state(0), Ok(1));
    assert_eq!(platform.clock_rate(99), Err(Errno::ENODEV));
}

#[test]
fn the_fake_rng_and_gpio_are_bounded() {
    let mut platform = FakePlatform::new();
    let mut bytes = [0u8; 16];
    assert_eq!(platform.rng_fill(&mut bytes), Ok(16));
    assert!(bytes.iter().any(|byte| *byte != 0));
    assert_eq!(
        platform.gpio_apply(
            17,
            PinConfig {
                function: Function::Output,
                pull: Pull::Up,
                level: true,
            },
        ),
        Ok(())
    );
    assert_eq!(platform.gpio_level(16), Ok(true));
    assert_eq!(platform.gpio_level(17), Ok(false));
    assert_eq!(
        platform.gpio_apply(58, PinConfig::default()),
        Err(Errno::EINVAL)
    );
    assert_eq!(platform.gpio_level(58), Err(Errno::EINVAL));
}
