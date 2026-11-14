// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn every_row_of_the_ticket_table() {
    let table: &[(i32, DefaultAction)] = &[
        (1, DefaultAction::Terminate),
        (2, DefaultAction::Terminate),
        (3, DefaultAction::Core),
        (4, DefaultAction::Core),
        (5, DefaultAction::Core),
        (6, DefaultAction::Core),
        (7, DefaultAction::Core),
        (8, DefaultAction::Core),
        (9, DefaultAction::Terminate),
        (10, DefaultAction::Terminate),
        (11, DefaultAction::Core),
        (12, DefaultAction::Terminate),
        (13, DefaultAction::Terminate),
        (14, DefaultAction::Terminate),
        (15, DefaultAction::Terminate),
        (16, DefaultAction::Terminate),
        (17, DefaultAction::Ignore),
        (18, DefaultAction::Continue),
        (19, DefaultAction::Stop),
        (20, DefaultAction::Stop),
        (21, DefaultAction::Stop),
        (22, DefaultAction::Stop),
        (23, DefaultAction::Ignore),
        (24, DefaultAction::Core),
        (25, DefaultAction::Core),
        (26, DefaultAction::Terminate),
        (27, DefaultAction::Terminate),
        (28, DefaultAction::Ignore),
        (29, DefaultAction::Terminate),
        (30, DefaultAction::Terminate),
        (31, DefaultAction::Core),
    ];
    for (sig, action) in table {
        assert_eq!(default_action(*sig), Some(*action), "signal {sig}");
    }
    for sig in 35..=64 {
        assert_eq!(
            default_action(sig),
            Some(DefaultAction::Terminate),
            "rt {sig}"
        );
    }
}

#[test]
fn internal_and_invalid_signals_have_no_default() {
    for sig in [0, -1, 32, 33, 34, 65, 1000] {
        assert_eq!(default_action(sig), None, "signal {sig}");
    }
}

#[test]
fn kill_and_stop_are_not_catchable() {
    assert_eq!(default_action(9), Some(DefaultAction::Terminate));
    assert_eq!(default_action(19), Some(DefaultAction::Stop));
}
