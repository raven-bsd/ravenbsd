// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generate the K324 golden RACK recovery/pipe trace.

use rack_recovery_model::{TRACE_COUNT, generate, header};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(flag) = args.next() else {
        eprintln!("usage: rack-recovery-model --generate <path>");
        std::process::exit(2);
    };
    let Some(path) = args.next() else {
        eprintln!("usage: rack-recovery-model --generate <path>");
        std::process::exit(2);
    };
    if flag != "--generate" {
        eprintln!("usage: rack-recovery-model --generate <path>");
        std::process::exit(2);
    }
    let mut text = header();
    for row in generate(TRACE_COUNT) {
        text.push_str(&row);
        text.push('\n');
    }
    std::fs::write(&path, text).unwrap_or_else(|error| {
        eprintln!("cannot write {path}: {error}");
        std::process::exit(1);
    });
}
