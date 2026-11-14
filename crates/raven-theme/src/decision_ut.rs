// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn auto_needs_a_tty_and_no_no_color() {
    assert!(Decision::Auto.enabled(true, false, false, false));
    assert!(!Decision::Auto.enabled(false, false, false, false));
    assert!(!Decision::Auto.enabled(true, true, false, false));
    assert!(!Decision::Auto.enabled(true, false, true, false));
    assert!(!Decision::Auto.enabled(true, false, false, true));
}

#[test]
fn always_overrides_no_color_but_never_json() {
    assert!(Decision::Always.enabled(false, true, true, false));
    assert!(!Decision::Always.enabled(true, false, false, true));
}

#[test]
fn never_is_plain() {
    assert!(!Decision::Never.enabled(true, false, false, false));
}
