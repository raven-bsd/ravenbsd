// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `ifconfig` argument parsing.

use super::*;

#[test]
fn no_arguments_lists_interfaces() {
    assert_eq!(parse(&[]), Ok(Command::List));
}

#[test]
fn one_name_selects_an_interface() {
    assert_eq!(parse(&["lo"]), Ok(Command::Get("lo")));
    assert_eq!(parse(&["eth0"]), Ok(Command::Get("eth0")));
    assert_eq!(parse(&[""]), Ok(Command::List));
}

#[test]
fn address_and_state_commands_parse() {
    assert_eq!(
        parse(&["lo", "10.1.2.3/24"]),
        Ok(Command::SetAddress("lo", [10, 1, 2, 3], 24))
    );
    assert_eq!(
        parse(&["lo", "127.0.0.1"]),
        Ok(Command::SetAddress("lo", [127, 0, 0, 1], 24))
    );
    assert_eq!(parse(&["lo", "up"]), Ok(Command::SetState("lo", true)));
    assert_eq!(parse(&["lo", "down"]), Ok(Command::SetState("lo", false)));
}

#[test]
fn malformed_inputs_are_errors() {
    assert_eq!(parse(&["lo", "300.1.1.1/24"]), Err(ParseError::BadAddress));
    assert_eq!(parse(&["lo", "1.2.3/24"]), Err(ParseError::BadAddress));
    assert_eq!(parse(&["lo", "1.2.3.4/33"]), Err(ParseError::BadPrefix));
    assert_eq!(parse(&["bad name", "up"]), Err(ParseError::Usage));
    assert_eq!(parse(&["lo", "up", "extra"]), Err(ParseError::Usage));
}

#[test]
fn netmask_and_flags_format() {
    assert_eq!(netmask(0), [0, 0, 0, 0]);
    assert_eq!(netmask(8), [255, 0, 0, 0]);
    assert_eq!(netmask(24), [255, 255, 255, 0]);
    assert_eq!(netmask(32), [255, 255, 255, 255]);
    assert_eq!(flags_text(7), "UP,LINK_UP,LOOPBACK");
    assert_eq!(flags_text(0), "");
}
