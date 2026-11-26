// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K356 independent audit of the merged RACK SACK/DSACK and sockopt group.
//!
//! The audit is read-only: it compares the merged `rack_sack.rs` constants and
//! ownership claims with the pinned FreeBSD `rack.c`/`tcp_rack.h`, and the K334
//! sockopt surface with the donor `rack_set_sockopt`/`rack_get_sockopt` values.
//! Findings are returned, not fixed.

use raven_tcpip::{
    Errno, IPPROTO_TCP, RackMisc, RackSockopt, SockoptDir, TCP_INFO, TCP_RACK_ABC_VAL,
    TCP_RACK_MEASURE_CNT, TCP_RACK_PACE_MIN_SEG, TCP_RACK_PACING_BETA, TCP_RACK_TIMER_SLOP,
};

/// One audit discrepancy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// Stable finding code.
    pub code: &'static str,
    /// Function or option under audit.
    pub subject: String,
    /// Exact observed versus donor value.
    pub detail: String,
}

impl Finding {
    fn new(code: &'static str, subject: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code,
            subject: subject.into(),
            detail: detail.into(),
        }
    }
}

/// Donor `tcp_rack.h` flags that the SACK group uses.
pub const DONOR_SACK_FLAGS: [(&str, u32); 5] = [
    ("RACK_ACKED", 0x000001),
    ("RACK_SACK_PASSED", 0x000010),
    ("RACK_WAS_SACKPASS", 0x000020),
    ("RACK_RWND_COLLAPSED", 0x000100),
    ("RACK_WAS_LOST", 0x400000),
];

/// Functions the K269 round-7 ownership split assigns to K336.
pub const K336_ASSIGNED: [&str; 4] = [
    "rack_proc_sack_blk",
    "rack_peer_reneges",
    "rack_handle_might_revert",
    "rack_note_dsack",
];

/// Functions the K336 report claims as its six K315 rows.
pub const K336_CLAIMED: [&str; 6] = [
    "rack_log_dsack_event",
    "rack_log_sack_passed",
    "rack_proc_sack_blk",
    "rack_peer_reneges",
    "rack_process_to_cumack",
    "rack_note_dsack",
];

fn parse_hex_or_dec(source: &str, name: &str) -> Option<u32> {
    let marker = format!("pub const {name}: u32 =");
    let start = source.find(&marker)? + marker.len();
    let tail = &source[start..];
    let end = tail.find(';')?;
    let raw = tail[..end].trim();
    if let Some(hex) = raw.strip_prefix("0x") {
        u32::from_str_radix(&hex.replace('_', ""), 16).ok()
    } else {
        raw.replace('_', "").parse::<u32>().ok()
    }
}

/// Audit the `rack_sack.rs` constants and the K336 ownership/test claims.
#[must_use]
pub fn audit_sack(sack_source: &str, misc_source: &str, sack_test_source: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (name, donor) in DONOR_SACK_FLAGS {
        let observed = parse_hex_or_dec(sack_source, name);
        match observed {
            Some(value) if value != donor => findings.push(Finding::new(
                "SACK-WRONG-CONSTANT",
                name,
                format!("observed 0x{value:06x}, donor/tcp_rack.h 0x{donor:06x}"),
            )),
            None => findings.push(Finding::new(
                "SACK-MISSING-CONSTANT",
                name,
                format!("not defined in rack_sack.rs; donor/tcp_rack.h 0x{donor:06x}"),
            )),
            _ => {}
        }
        if parse_hex_or_dec(misc_source, name).is_some() {
            findings.push(Finding::new(
                "SACK-DUPLICATE-CONSTANT",
                name,
                "defined in both rack_sack.rs and rack_misc.rs",
            ));
        }
    }
    for claimed in K336_CLAIMED {
        if !K336_ASSIGNED.contains(&claimed) {
            findings.push(Finding::new(
                "SACK-DUPLICATE-OWNER",
                claimed,
                "K336 report claims this row, but K269 assigns it to another group",
            ));
        }
        let test_prefix = format!("fn {claimed}_");
        if !sack_test_source.contains(&test_prefix) {
            findings.push(Finding::new(
                "SACK-MISSING-TEST",
                claimed,
                format!("no named `{test_prefix}...` test in rack_sack_ut.rs"),
            ));
        }
    }
    for assigned in K336_ASSIGNED {
        if !K336_CLAIMED.contains(&assigned) {
            findings.push(Finding::new(
                "SACK-MISSING-OWNER",
                assigned,
                "K269 assigns this row to K336, but the K336 report does not claim it",
            ));
        }
    }
    findings
}

fn option(dir: SockoptDir, name: i32, value: u32) -> RackSockopt {
    RackSockopt {
        dir,
        level: IPPROTO_TCP,
        name,
        value,
    }
}

/// Audit the K334 sockopt values against the donor `rack.c` cases.
#[must_use]
pub fn audit_sockopt() -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut rack = RackMisc::new();

    // Donor rack.c:22985-22991: 0 and >255 are EINVAL, not a clamp to 1.
    let set_zero = rack.rack_set_sockopt(&option(SockoptDir::Set, TCP_RACK_MEASURE_CNT, 0));
    if set_zero.is_ok() {
        let read_back = rack.rack_get_sockopt(&option(SockoptDir::Get, TCP_RACK_MEASURE_CNT, 0));
        findings.push(Finding::new(
            "SOCKOPT-DONOR-DIVERGENCE",
            "TCP_RACK_MEASURE_CNT=0",
            format!(
                "observed {set_zero:?}, read back {read_back:?}; donor rack.c:22985 returns EINVAL"
            ),
        ));
    }

    // Donor rack.c:22999-23005: 0 and >=255 are EINVAL, not a clamp to 255.
    let set_abc = rack.rack_set_sockopt(&option(SockoptDir::Set, TCP_RACK_ABC_VAL, 300));
    if set_abc.is_ok() {
        let read_back = rack.rack_get_sockopt(&option(SockoptDir::Get, TCP_RACK_ABC_VAL, 0));
        findings.push(Finding::new(
            "SOCKOPT-DONOR-DIVERGENCE",
            "TCP_RACK_ABC_VAL=300",
            format!(
                "observed {set_abc:?}, read back {read_back:?}; donor rack.c:22999 returns EINVAL"
            ),
        ));
    }

    // Donor rack.c:23239-23243 masks with 0xffff: 70000 -> 4464, not 65535.
    let set_min = rack.rack_set_sockopt(&option(SockoptDir::Set, TCP_RACK_PACE_MIN_SEG, 70_000));
    let got_min = rack.rack_get_sockopt(&option(SockoptDir::Get, TCP_RACK_PACE_MIN_SEG, 0));
    if set_min.is_ok() && got_min == Ok(Some(65_535)) {
        findings.push(Finding::new(
            "SOCKOPT-DONOR-DIVERGENCE",
            "TCP_RACK_PACE_MIN_SEG=70000",
            "observed 65535; donor rack.c:23241 masks 0xffff -> 4464",
        ));
    }

    // Donor rack.c:22929: set returns EINVAL for this option.
    let set_beta = rack.rack_set_sockopt(&option(SockoptDir::Set, TCP_RACK_PACING_BETA, 50));
    if set_beta == Err(Errno::EOPNOTSUPP) {
        findings.push(Finding::new(
            "SOCKOPT-DONOR-DIVERGENCE",
            "TCP_RACK_PACING_BETA set",
            "observed EOPNOTSUPP; donor rack.c:22929 returns EINVAL",
        ));
    }

    // Donor rack.c:24373 fills and returns tcp_info for TCP_INFO.
    let info = rack.rack_get_sockopt(&option(SockoptDir::Get, TCP_INFO, 0));
    if info == Err(Errno::EOPNOTSUPP) {
        findings.push(Finding::new(
            "SOCKOPT-DONOR-DIVERGENCE",
            "TCP_INFO get",
            "observed EOPNOTSUPP; donor rack.c:24373 calls rack_fill_info and copies tcp_info",
        ));
    }

    // Donor tcp.h:322 defines TCP_RACK_PACING_BETA_ECN 1139 and rack.c:22945 handles set/get.
    let ecn = rack.rack_get_sockopt(&option(SockoptDir::Get, 1139, 0));
    if ecn == Err(Errno::EOPNOTSUPP) {
        findings.push(Finding::new(
            "SOCKOPT-MISSING-OPTION",
            "TCP_RACK_PACING_BETA_ECN",
            "observed EOPNOTSUPP; donor tcp.h:322 / rack.c:22945 handles 1139",
        ));
    }

    // A few values the K340 matrix records and the donor confirms.
    let timer = rack.rack_set_sockopt(&option(SockoptDir::Set, TCP_RACK_TIMER_SLOP, 123));
    if timer.is_err() {
        findings.push(Finding::new(
            "SOCKOPT-WRONG-IMPLEMENTATION",
            "TCP_RACK_TIMER_SLOP",
            format!("observed {timer:?}; donor rack.c:22932 stores optval"),
        ));
    }
    findings
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
