// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_adapter_adds_no_violation_without_a_frozen_file() {
    let mut violations = Vec::new();
    run(Path::new("does-not-exist"), &mut violations).expect("lint");
    assert!(violations.is_empty());
}
