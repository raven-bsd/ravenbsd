// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `xtask` binary entry point.

#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    xtask::run_cli(std::env::args().skip(1).collect())
}
