// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only conformance matrix for the canonical RACK flags and sendmap (K360).
//!
//! `fixtures/tcp_rack_flags.txt` is the pinned donor name/value/type manifest (FreeBSD
//! `tcp_rack.h`, `tcp_var.h`, `rack.c` and `rack_bbr_common.h` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`). The matrix compares every row to the exported
//! `raven_tcpip` constants and exercises the sendmap/flag transitions in the merged RACK modules.
//! No product source is edited.

#![forbid(unsafe_code)]

use core::str::FromStr;
use std::collections::BTreeSet;

use raven_tcpip::{
    PACE_MAX_IP_BYTES, RACK_ACKED, RACK_APP_LIMITED, RACK_DEFERRED, RACK_DELAYED_ACK_TIME,
    RACK_HAD_PUSH, RACK_HAS_FIN, RACK_HAS_SYN, RACK_IN_GP_WIN, RACK_INITIAL_RTO, RACK_IS_PCM,
    RACK_MERGED, RACK_MUST_RXT, RACK_NUM_OF_RETRANS, RACK_OVERMAX, RACK_PMTU_CHG,
    RACK_PRR_ADDBACKMAX, RACK_REORDER_FADE_DEFAULT, RACK_RTO_MAX, RACK_RTO_MIN,
    RACK_RWND_COLLAPSED, RACK_SACK_PASSED, RACK_SENT_FP, RACK_SENT_SP, RACK_SENT_W_DSACK,
    RACK_SHUFFLED, RACK_STRADDLE, RACK_TLP, RACK_TLP_MIN, RACK_TO_REXT, RACK_WAS_ACKED,
    RACK_WAS_LOST, RACK_WAS_SACKPASS, TF_CONGRECOVERY, TF_FASTRECOVERY, TF_WASCRECOVERY,
    TF_WASFRECOVERY,
};

/// The pinned donor constant manifest.
pub const FIXTURE: &str = include_str!("../fixtures/tcp_rack_flags.txt");

/// Every canonical name the matrix must cover.
pub const CANONICAL_NAMES: &[&str] = &[
    "RACK_ACKED",
    "RACK_TO_REXT",
    "RACK_DEFERRED",
    "RACK_OVERMAX",
    "RACK_SACK_PASSED",
    "RACK_WAS_SACKPASS",
    "RACK_HAS_FIN",
    "RACK_TLP",
    "RACK_RWND_COLLAPSED",
    "RACK_APP_LIMITED",
    "RACK_WAS_ACKED",
    "RACK_HAS_SYN",
    "RACK_SENT_W_DSACK",
    "RACK_SENT_SP",
    "RACK_SENT_FP",
    "RACK_HAD_PUSH",
    "RACK_MUST_RXT",
    "RACK_IN_GP_WIN",
    "RACK_SHUFFLED",
    "RACK_MERGED",
    "RACK_PMTU_CHG",
    "RACK_STRADDLE",
    "RACK_WAS_LOST",
    "RACK_IS_PCM",
    "RACK_NUM_OF_RETRANS",
    "RACK_INITIAL_RTO",
    "RACK_RTO_MIN",
    "RACK_RTO_MAX",
    "RACK_TLP_MIN",
    "RACK_DELAYED_ACK_TIME",
    "RACK_PRR_ADDBACKMAX",
    "RACK_REORDER_FADE_DEFAULT",
    "PACE_MAX_IP_BYTES",
    "TF_FASTRECOVERY",
    "TF_WASFRECOVERY",
    "TF_CONGRECOVERY",
    "TF_WASCRECOVERY",
];

/// One manifest row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlagRow {
    /// Constant name.
    pub name: String,
    /// Donor value.
    pub value: u64,
    /// Donor type (`u32` or `usize`).
    pub kind: &'static str,
}

/// Parse the `name value type` manifest, skipping `#` comments and blank lines.
pub fn parse_fixture(text: &str) -> Result<Vec<FlagRow>, String> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut fields = line.split_whitespace();
        let name = fields
            .next()
            .ok_or_else(|| format!("line {}: missing name", index + 1))?;
        let value = fields
            .next()
            .ok_or_else(|| format!("line {}: missing value", index + 1))?;
        let kind = fields
            .next()
            .ok_or_else(|| format!("line {}: missing type", index + 1))?;
        if fields.next().is_some() {
            return Err(format!("line {}: trailing fields", index + 1));
        }
        let value = if let Some(hex) = value.strip_prefix("0x") {
            u64::from_str_radix(hex, 16)
        } else {
            u64::from_str(value)
        }
        .map_err(|_| format!("line {}: bad value {value}", index + 1))?;
        let kind = match kind {
            "u32" => "u32",
            "usize" => "usize",
            other => return Err(format!("line {}: unknown type {other}", index + 1)),
        };
        rows.push(FlagRow {
            name: name.to_owned(),
            value,
            kind,
        });
    }
    Ok(rows)
}

/// The exported product value and type for a canonical name.
#[must_use]
pub fn actual(name: &str) -> Option<(u64, &'static str)> {
    Some(match name {
        "RACK_ACKED" => (u64::from(RACK_ACKED), "u32"),
        "RACK_TO_REXT" => (u64::from(RACK_TO_REXT), "u32"),
        "RACK_DEFERRED" => (u64::from(RACK_DEFERRED), "u32"),
        "RACK_OVERMAX" => (u64::from(RACK_OVERMAX), "u32"),
        "RACK_SACK_PASSED" => (u64::from(RACK_SACK_PASSED), "u32"),
        "RACK_WAS_SACKPASS" => (u64::from(RACK_WAS_SACKPASS), "u32"),
        "RACK_HAS_FIN" => (u64::from(RACK_HAS_FIN), "u32"),
        "RACK_TLP" => (u64::from(RACK_TLP), "u32"),
        "RACK_RWND_COLLAPSED" => (u64::from(RACK_RWND_COLLAPSED), "u32"),
        "RACK_APP_LIMITED" => (u64::from(RACK_APP_LIMITED), "u32"),
        "RACK_WAS_ACKED" => (u64::from(RACK_WAS_ACKED), "u32"),
        "RACK_HAS_SYN" => (u64::from(RACK_HAS_SYN), "u32"),
        "RACK_SENT_W_DSACK" => (u64::from(RACK_SENT_W_DSACK), "u32"),
        "RACK_SENT_SP" => (u64::from(RACK_SENT_SP), "u32"),
        "RACK_SENT_FP" => (u64::from(RACK_SENT_FP), "u32"),
        "RACK_HAD_PUSH" => (u64::from(RACK_HAD_PUSH), "u32"),
        "RACK_MUST_RXT" => (u64::from(RACK_MUST_RXT), "u32"),
        "RACK_IN_GP_WIN" => (u64::from(RACK_IN_GP_WIN), "u32"),
        "RACK_SHUFFLED" => (u64::from(RACK_SHUFFLED), "u32"),
        "RACK_MERGED" => (u64::from(RACK_MERGED), "u32"),
        "RACK_PMTU_CHG" => (u64::from(RACK_PMTU_CHG), "u32"),
        "RACK_STRADDLE" => (u64::from(RACK_STRADDLE), "u32"),
        "RACK_WAS_LOST" => (u64::from(RACK_WAS_LOST), "u32"),
        "RACK_IS_PCM" => (u64::from(RACK_IS_PCM), "u32"),
        "RACK_NUM_OF_RETRANS" => (RACK_NUM_OF_RETRANS as u64, "usize"),
        "RACK_INITIAL_RTO" => (u64::from(RACK_INITIAL_RTO), "u32"),
        "RACK_RTO_MIN" => (u64::from(RACK_RTO_MIN), "u32"),
        "RACK_RTO_MAX" => (u64::from(RACK_RTO_MAX), "u32"),
        "RACK_TLP_MIN" => (u64::from(RACK_TLP_MIN), "u32"),
        "RACK_DELAYED_ACK_TIME" => (u64::from(RACK_DELAYED_ACK_TIME), "u32"),
        "RACK_PRR_ADDBACKMAX" => (u64::from(RACK_PRR_ADDBACKMAX), "u32"),
        "RACK_REORDER_FADE_DEFAULT" => (u64::from(RACK_REORDER_FADE_DEFAULT), "u32"),
        "PACE_MAX_IP_BYTES" => (u64::from(PACE_MAX_IP_BYTES), "u32"),
        "TF_FASTRECOVERY" => (u64::from(TF_FASTRECOVERY), "u32"),
        "TF_WASFRECOVERY" => (u64::from(TF_WASFRECOVERY), "u32"),
        "TF_CONGRECOVERY" => (u64::from(TF_CONGRECOVERY), "u32"),
        "TF_WASCRECOVERY" => (u64::from(TF_WASCRECOVERY), "u32"),
        _ => return None,
    })
}

/// Compare every manifest row to the product constant.
///
/// Returns a list of human-readable failures; an empty list is a pass. A manifest name unknown to
/// the product, a duplicate row, a wrong value or a wrong type is a failure.
pub fn check_matrix(text: &str) -> Result<Vec<String>, String> {
    let rows = parse_fixture(text)?;
    let mut seen = BTreeSet::new();
    let mut failures = Vec::new();
    for row in rows {
        if !seen.insert(row.name.clone()) {
            failures.push(format!("duplicate {}", row.name));
            continue;
        }
        match actual(&row.name) {
            None => failures.push(format!("unknown {}", row.name)),
            Some((value, kind)) => {
                if value != row.value {
                    failures.push(format!(
                        "value {}: product {value} donor {}",
                        row.name, row.value
                    ));
                }
                if kind != row.kind {
                    failures.push(format!(
                        "type {}: product {kind} donor {}",
                        row.name, row.kind
                    ));
                }
            }
        }
    }
    Ok(failures)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
