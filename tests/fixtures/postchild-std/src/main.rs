// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// K495 rev167 test-only static std fixture. It starts, runs one external child through the Raven
// exec path, then inserts into a HashSet and HashMap and reads the environment. If it faults at the
// same near-null read as brush's TraceEvent set, the defect is in Raven's std/exec/environ contract;
// if it completes, the fault is brush-internal. It is test tooling, not product code.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::process::Command;

fn marker(text: &str) {
    println!("{text}");
    let _ = std::io::stdout().flush();
}

fn main() {
    marker(&format!("POSTCHILD start pid={}", std::process::id()));

    let status = Command::new("/bin/ls").arg("/").status();
    marker(&format!("POSTCHILD child status={status:?}"));

    let mut set = HashSet::new();
    for value in 0..64u32 {
        set.insert(value);
    }
    marker(&format!("POSTCHILD set len={}", set.len()));

    let mut map = HashMap::new();
    for value in 0..64u32 {
        map.insert(value, value.wrapping_mul(3));
    }
    marker(&format!(
        "POSTCHILD map len={} value={:?}",
        map.len(),
        map.get(&7)
    ));

    let count = std::env::vars().count();
    marker(&format!(
        "POSTCHILD env count={count} home={:?}",
        std::env::var("HOME")
    ));

    marker("POSTCHILD done");
}
