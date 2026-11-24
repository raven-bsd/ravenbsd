// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `kbd:` console path contract.

use super::{KBD_CONSOLE_PATH, accepts_console_path};

#[test]
fn accepts_the_console_instance_and_root() {
    assert_eq!(KBD_CONSOLE_PATH, "console");
    assert!(accepts_console_path("console"));
    assert!(accepts_console_path("/console"));
    assert!(accepts_console_path(""));
    assert!(accepts_console_path("/"));
}

#[test]
fn rejects_other_instances() {
    assert!(!accepts_console_path("mouse"));
    assert!(!accepts_console_path("/kbd"));
    assert!(!accepts_console_path("console/extra"));
}
