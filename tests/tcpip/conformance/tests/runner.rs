// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Runner smoke tests.
use raven_tcpip_conformance::run_text;

#[test]
fn handshake_script_passes() {
    let script = "\
0.000 socket() = 3
0.000 bind(3, ..., 8080) = 0
0.000 listen(3, 1) = 0
+0.100 < S 0:0(0) win 65535 <mss 1460,sackOK,nop,wscale 7>
+0.050 > S. 0:0(0) ack 1 win 65535
+0.050 < . 1:1(0) ack 1 win 65535
+0.000 accept(3, ..., 0) = 4
";
    run_text(script).expect("handshake");
}

#[test]
fn a_wrong_expectation_fails() {
    let script = "\
0.000 listen(3, 1) = 0
+0.100 < S 0:0(0) win 65535
+0.050 > S. 5:5(0) ack 1 win 65535
";
    let error = run_text(script).expect_err("wrong seq must fail");
    assert!(error.contains("seq expected 5"), "{error}");
}
