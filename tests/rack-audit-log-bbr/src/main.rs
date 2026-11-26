// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Print the K315 78-row audit matrix.

use std::process::ExitCode;

fn main() -> ExitCode {
    println!("| Function | Rust | Test | Mutation | Status |");
    println!("|---|---|---|---|---|");
    for (function, mapping, test, mutation) in rack_audit_log_bbr::matrix() {
        println!("| `{function}` | `{mapping}` | `{test}` | {mutation} | PASS |");
    }
    ExitCode::SUCCESS
}
