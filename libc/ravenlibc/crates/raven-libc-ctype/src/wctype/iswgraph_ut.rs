// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn graph_is_print_minus_space() {
    assert!(iswgraph(u32::from(b'a')));
    assert!(!iswgraph(0x20));
    assert!(!iswgraph(0x2028));
    assert!(!iswgraph(0x0a));
    assert!(iswgraph(0x300));
}
