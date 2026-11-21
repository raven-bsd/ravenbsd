// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden-trace loading for the K319 and K324 10k trace files.
//!
//! The two committed TSVs share one format:
//!
//! ```text
//! seed  events  lost  pipe  retransmits  recovery_entries/exits  event_digest
//! ```
//!
//! `lost` and `retransmits` are comma-separated `start-end` byte ranges, or `-`.

use std::path::{Path, PathBuf};

/// One parsed golden row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoldenRow {
    /// Deterministic scenario seed.
    pub seed: u64,
    /// Event count.
    pub events: usize,
    /// Final lost ranges.
    pub lost: Vec<(u32, u32)>,
    /// Final RFC 6675 pipe estimate.
    pub pipe: u32,
    /// Retransmit selections.
    pub retransmits: Vec<(u32, u32)>,
    /// Recovery entries.
    pub recovery_entries: u32,
    /// Recovery exits.
    pub recovery_exits: u32,
    /// FNV-1a digest of the canonical input events.
    pub digest: u64,
}

/// Path to the committed K319 loss/reordering trace.
#[must_use]
pub fn loss_trace_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/rack-loss-model/traces/rack-10k.tsv")
}

/// Path to the committed K324 recovery/pipe trace.
#[must_use]
pub fn recovery_trace_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/rack-recovery-model/traces/rack-recovery-10k.tsv")
}

/// Load and parse one golden trace file.
pub fn load_golden(path: &Path) -> Result<Vec<GoldenRow>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    parse_golden(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Parse golden rows, ignoring comment and empty lines.
pub fn parse_golden(text: &str) -> Result<Vec<GoldenRow>, String> {
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 7 {
            return Err(format!(
                "line {}: expected 7 tab fields, found {}",
                index + 1,
                fields.len()
            ));
        }
        let seed = parse_u64(fields[0], index + 1, "seed")?;
        let events = parse_usize(fields[1], index + 1, "events")?;
        let lost = parse_ranges(fields[2], index + 1, "lost")?;
        let pipe = parse_u32(fields[3], index + 1, "pipe")?;
        let retransmits = parse_ranges(fields[4], index + 1, "retransmits")?;
        let (recovery_entries, recovery_exits) = parse_recovery(fields[5], index + 1)?;
        let digest = u64::from_str_radix(fields[6], 16)
            .map_err(|error| format!("line {}: digest: {error}", index + 1))?;
        rows.push(GoldenRow {
            seed,
            events,
            lost,
            pipe,
            retransmits,
            recovery_entries,
            recovery_exits,
            digest,
        });
    }
    Ok(rows)
}

fn parse_u64(value: &str, line: usize, field: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|error| format!("line {line}: {field}: {error}"))
}

fn parse_u32(value: &str, line: usize, field: &str) -> Result<u32, String> {
    value
        .parse()
        .map_err(|error| format!("line {line}: {field}: {error}"))
}

fn parse_usize(value: &str, line: usize, field: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|error| format!("line {line}: {field}: {error}"))
}

fn parse_ranges(value: &str, line: usize, field: &str) -> Result<Vec<(u32, u32)>, String> {
    if value == "-" {
        return Ok(Vec::new());
    }
    value
        .split(',')
        .map(|range| {
            let (start, end) = range
                .split_once('-')
                .ok_or_else(|| format!("line {line}: {field}: {range:?} is not start-end"))?;
            let start = parse_u32(start, line, field)?;
            let end = parse_u32(end, line, field)?;
            if start >= end {
                return Err(format!(
                    "line {line}: {field}: empty or inverted range {start}-{end}"
                ));
            }
            Ok((start, end))
        })
        .collect()
}

fn parse_recovery(value: &str, line: usize) -> Result<(u32, u32), String> {
    let (entries, exits) = value
        .split_once('/')
        .ok_or_else(|| format!("line {line}: recovery: {value:?} is not entries/exits"))?;
    Ok((
        parse_u32(entries, line, "recovery_entries")?,
        parse_u32(exits, line, "recovery_exits")?,
    ))
}
