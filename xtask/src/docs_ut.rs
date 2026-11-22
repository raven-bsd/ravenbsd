// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the frozen documentation step.

use std::path::Path;

use super::*;

#[test]
fn frozen_docs_step_is_a_no_op() {
    assert_eq!(run(Path::new(".")).expect("docs step"), 0);
}
