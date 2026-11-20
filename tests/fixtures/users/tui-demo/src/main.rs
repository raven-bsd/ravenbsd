// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K229 fixture: a stub TUI shell. The `raven config` Users form installs this binary as a
//! user's passwd shell; login/getty start it after authentication.

#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use raven_rt::entry;

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    raven_rt::println!("tui-demo: ready");
    raven_rt::println!("tui-demo: exit 0");
    0
}
