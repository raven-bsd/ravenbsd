// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn control_capabilities_emit_ansi() {
    assert_eq!(
        capability("xterm", "clear", &[]),
        Ok(Value::Bytes("\x1b[H\x1b[2J".to_string()))
    );
    assert_eq!(
        capability("vt100", "bold", &[]),
        Ok(Value::Bytes("\x1b[1m".to_string()))
    );
    assert_eq!(
        capability("xterm", "bel", &[]),
        Ok(Value::Bytes("\x07".to_string()))
    );
}

#[test]
fn numeric_capabilities_are_reported() {
    assert_eq!(capability("xterm", "cols", &[]), Ok(Value::Number(80)));
    assert_eq!(capability("xterm", "lines", &[]), Ok(Value::Number(24)));
}

#[test]
fn cup_formats_row_and_column() {
    assert_eq!(
        capability("xterm", "cup", &[3, 12]),
        Ok(Value::Bytes("\x1b[3;12H".to_string()))
    );
}

#[test]
fn cup_rejects_bad_parameters() {
    assert_eq!(capability("xterm", "cup", &[]), Err(Error::BadParameter));
    assert_eq!(
        capability("xterm", "cup", &[0, 4]),
        Err(Error::BadParameter)
    );
}

#[test]
fn unknown_capability_and_term_fail() {
    assert_eq!(
        capability("xterm", "nope", &[]),
        Err(Error::UnknownCapability)
    );
    assert_eq!(capability("", "clear", &[]), Err(Error::UnknownTerm));
    assert_eq!(capability("xterm", "", &[]), Err(Error::MissingCapability));
}

#[test]
fn extra_parameters_are_rejected() {
    assert_eq!(
        capability("xterm", "bold", &[1]),
        Err(Error::TooManyParameters)
    );
}
