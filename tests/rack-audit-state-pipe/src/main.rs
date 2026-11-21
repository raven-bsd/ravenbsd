// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Run the K332/K333 audit and print every finding.

fn main() {
    let report = rack_audit_state_pipe::audit();
    println!(
        "functions: {}  tests: {}  constants parsed: {}",
        report.functions_checked, report.tests_checked, report.constants_checked
    );
    for finding in &report.findings {
        println!(
            "{:?}  {}  {}  {}",
            finding.kind, finding.id, finding.location, finding.detail
        );
    }
    println!("findings: {}", report.findings.len());
}
