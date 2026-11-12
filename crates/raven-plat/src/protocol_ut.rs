// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `plat:` path parsing.

use super::*;

#[test]
fn board_paths_parse() {
    assert_eq!(parse_resource(""), Ok(Resource::Model));
    assert_eq!(parse_resource("/model"), Ok(Resource::Model));
    assert_eq!(parse_resource("revision"), Ok(Resource::Revision));
    assert_eq!(parse_resource("serial"), Ok(Resource::Serial));
    assert_eq!(parse_resource("mac"), Ok(Resource::Mac));
    assert_eq!(parse_resource("rng"), Ok(Resource::Rng));
}

#[test]
fn clock_and_power_ids_parse() {
    assert_eq!(parse_resource("clock/12"), Ok(Resource::Clock(12)));
    assert_eq!(parse_resource("/power/0"), Ok(Resource::Power(0)));
    assert_eq!(parse_resource("clock/64"), Err(Errno::EINVAL));
    assert_eq!(parse_resource("power/16"), Err(Errno::EINVAL));
    assert_eq!(parse_resource("clock/x"), Err(Errno::EINVAL));
}

#[test]
fn gpio_pins_parse_and_are_bounded() {
    assert_eq!(parse_resource("gpio/0"), Ok(Resource::Gpio(0)));
    assert_eq!(parse_resource("/gpio/57"), Ok(Resource::Gpio(57)));
    assert_eq!(parse_resource("gpio/58"), Err(Errno::EINVAL));
    assert_eq!(parse_resource("gpio/999"), Err(Errno::EINVAL));
}

#[test]
fn unknown_paths_are_enoent() {
    assert_eq!(parse_resource("nonsense"), Err(Errno::ENOENT));
    assert_eq!(parse_resource("gpio"), Err(Errno::ENOENT));
}
