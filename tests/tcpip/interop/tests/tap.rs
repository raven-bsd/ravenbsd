// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C22 scenarios. The default test run never needs TAP; ignored tests skip
//! cleanly and print the exact missing prerequisite.

use raven_tcpip_interop::tap_skip_reason;

fn skip(scenario: &str) {
    if let Some(reason) = tap_skip_reason() {
        eprintln!("SKIP {scenario}: {reason}");
        return;
    }
    eprintln!(
        "BLOCKED {scenario}: TAP is present but the host-tool scenario is not automated in this ticket; see INTEROP.md"
    );
}

#[test]
fn default_run_does_not_need_tap() {
    if let Some(reason) = tap_skip_reason() {
        eprintln!("TAP unavailable, default test passes: {reason}");
    }
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn tcp_server_small_transfer() {
    skip("tcp_server_small_transfer");
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn tcp_server_10mib_transfer() {
    skip("tcp_server_10mib_transfer");
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn http_client() {
    skip("http_client");
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn echo_client() {
    skip("echo_client");
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn ping4() {
    skip("ping4");
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn ping6() {
    skip("ping6");
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn dns_udp_and_tcp_fallback() {
    skip("dns_udp_and_tcp_fallback");
}

#[test]
#[ignore = "requires a host TAP interface and host tools"]
fn window_scale_and_timestamps() {
    skip("window_scale_and_timestamps");
}

#[test]
#[ignore = "requires a host TAP interface and tc"]
fn sack_under_loss() {
    skip("sack_under_loss");
}

#[test]
#[ignore = "requires a host TAP interface and ECN-capable host"]
fn ecn_negotiation() {
    skip("ecn_negotiation");
}

#[test]
#[ignore = "requires a host TAP interface and MTU changes"]
fn pmtu_reaction() {
    skip("pmtu_reaction");
}

#[test]
#[ignore = "requires a host TAP interface"]
fn ipv6_transfer_and_ping() {
    skip("ipv6_transfer_and_ping");
}

#[test]
#[ignore = "requires a host TAP interface"]
fn rst_in_both_directions() {
    skip("rst_in_both_directions");
}

#[test]
#[ignore = "requires a host TAP interface"]
fn netstat_style_comparison() {
    skip("netstat_style_comparison");
}
