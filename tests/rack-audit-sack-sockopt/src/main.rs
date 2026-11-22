// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K356 audit CLI: print the SACK/sockopt discrepancies and exit non-zero.

use std::process::ExitCode;

use rack_audit_sack_sockopt::{audit_sack, audit_sockopt};

fn main() -> ExitCode {
    let sack = include_str!("../../../crates/raven-tcpip/src/rack_sack.rs");
    let misc = include_str!("../../../crates/raven-tcpip/src/rack_misc.rs");
    let tests = include_str!("../../../crates/raven-tcpip/src/rack_sack_ut.rs");
    let mut findings = audit_sack(sack, misc, tests);
    findings.extend(audit_sockopt());
    println!("K356 RACK SACK/sockopt audit");
    println!("donor: FreeBSD rack.c/tcp_rack.h 133344405f532bd54ff3629a77d4c4e062f01fdb");
    for finding in &findings {
        println!("[{}] {}: {}", finding.code, finding.subject, finding.detail);
    }
    println!("findings: {}", findings.len());
    if findings.is_empty() {
        println!("result: PASS");
        ExitCode::SUCCESS
    } else {
        println!("result: FINDINGS");
        ExitCode::from(1)
    }
}
