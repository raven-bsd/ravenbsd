// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Drives the K277 packet fixtures through `raven-pf`.
//!
//! The harness is test-only and lives outside the product workspace. It reads
//! `tests/pf-conformance/manifest.tsv`, executes every `raven-pf` scenario and
//! prints an explicit `SKIP` with the recorded reason for donor-only fixtures.
//! A missing feature is never reported as a pass.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use raven_pf::state::StateDisposition;
use raven_pf::{Direction, Engine, Ipv4Addr, Packet, Protocol, Verdict, parse};

fn main() -> ExitCode {
    let default_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let root = env::args().nth(1).map_or(default_root, PathBuf::from);
    match run(&root) {
        Ok(summary) => {
            println!(
                "fixture-harness: {} passed, {} skipped, {} failed",
                summary.passed, summary.skipped, summary.failed
            );
            if summary.failed == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("fixture-harness: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Default)]
struct Summary {
    passed: usize,
    skipped: usize,
    failed: usize,
}

fn run(root: &Path) -> Result<Summary, String> {
    let manifest_path = root.join("manifest.tsv");
    let text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let mut summary = Summary::default();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.splitn(6, '|').collect();
        if fields.len() != 6 {
            return Err(format!(
                "manifest.tsv line {}: expected 6 fields, found {}",
                index + 1,
                fields.len()
            ));
        }
        let id = fields[0];
        let kind = fields[1];
        let rules = root.join(fields[2]);
        let packets = fields[3];
        let support = fields[4];
        let expect = fields[5];
        match support {
            "raven-pf" => match kind {
                "scenario" => match run_scenario(&rules, &root.join(packets)) {
                    Ok(()) => {
                        summary.passed += 1;
                        println!("PASS {id}");
                    }
                    Err(error) => {
                        summary.failed += 1;
                        println!("FAIL {id}: {error}");
                    }
                },
                other => return Err(format!("{id}: unsupported kind `{other}`")),
            },
            "raven-pf-parse-error" => match run_parse_error(&rules, expect) {
                Ok(()) => {
                    summary.passed += 1;
                    println!("PASS {id} (rejected)");
                }
                Err(error) => {
                    summary.failed += 1;
                    println!("FAIL {id}: {error}");
                }
            },
            "donor-only" => {
                summary.skipped += 1;
                println!("SKIP {id}: {expect}");
            }
            other => return Err(format!("{id}: unknown support `{other}`")),
        }
    }
    Ok(summary)
}

fn run_parse_error(path: &Path, expect: &str) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let (line, substring) = expect
        .split_once(',')
        .ok_or_else(|| format!("parse-error expectation `{expect}` is not <line>,<substring>"))?;
    let line: usize = line
        .trim()
        .parse()
        .map_err(|error| format!("bad expected line `{line}`: {error}"))?;
    let error = parse(&text).expect_err("malformed ruleset was accepted");
    if error.line != line {
        return Err(format!(
            "expected rejection on line {line}, got line {}: {error}",
            error.line
        ));
    }
    if !error.message.contains(substring.trim()) {
        return Err(format!(
            "expected diagnostic containing `{}`, got `{}`",
            substring.trim(),
            error.message
        ));
    }
    Ok(())
}

fn run_scenario(rules: &Path, packets: &Path) -> Result<(), String> {
    let rules_text = fs::read_to_string(rules)
        .map_err(|error| format!("cannot read {}: {error}", rules.display()))?;
    let ruleset = parse(&rules_text).map_err(|error| format!("{}: {error}", rules.display()))?;
    let mut engine = Engine::new(ruleset);
    let packets_text = fs::read_to_string(packets)
        .map_err(|error| format!("cannot read {}: {error}", packets.display()))?;
    let mut expected_counters: Vec<(String, u64)> = Vec::new();
    let mut rows = 0usize;
    for (index, line) in packets_text.lines().enumerate() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("# expect-counters ") {
            for pair in rest.split_whitespace() {
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("bad counter `{pair}` in {}", packets.display()))?;
                let value: u64 = value
                    .parse()
                    .map_err(|error| format!("bad counter value `{value}`: {error}"))?;
                expected_counters.push((key.to_string(), value));
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        rows += 1;
        evaluate_row(&mut engine, line)
            .map_err(|error| format!("{} line {}: {error}", packets.display(), index + 1))?;
    }
    if rows == 0 {
        return Err(format!("{} has no packet rows", packets.display()));
    }
    let counters = engine.counters();
    for (key, value) in expected_counters {
        let actual = match key.as_str() {
            "in_pass" => counters.in_pass,
            "in_block" => counters.in_block,
            "out_pass" => counters.out_pass,
            "out_block" => counters.out_block,
            "states_created" => counters.states_created,
            "states_expired" => counters.states_expired,
            "state_hits" => counters.state_hits,
            other => return Err(format!("unknown counter `{other}`")),
        };
        if actual != value {
            return Err(format!("counter {key}: expected {value}, got {actual}"));
        }
    }
    Ok(())
}

fn evaluate_row(engine: &mut Engine, line: &str) -> Result<(), String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 12 {
        return Err(format!("expected 12 fields, found {}", fields.len()));
    }
    let now: u64 = fields[0]
        .parse()
        .map_err(|error| format!("bad now `{}`: {error}", fields[0]))?;
    let direction = match fields[1] {
        "in" => Direction::In,
        "out" => Direction::Out,
        other => return Err(format!("bad direction `{other}`")),
    };
    let interface = fields[2];
    let protocol = match fields[3] {
        "tcp" => Protocol::Tcp,
        "udp" => Protocol::Udp,
        "icmp" => Protocol::Icmp,
        other => return Err(format!("bad protocol `{other}`")),
    };
    let source = Ipv4Addr::parse(fields[4])
        .map_err(|error| format!("bad source `{}`: {error}", fields[4]))?;
    let source_port: u16 = fields[5]
        .parse()
        .map_err(|error| format!("bad source port `{}`: {error}", fields[5]))?;
    let destination = Ipv4Addr::parse(fields[6])
        .map_err(|error| format!("bad destination `{}`: {error}", fields[6]))?;
    let destination_port: u16 = fields[7]
        .parse()
        .map_err(|error| format!("bad destination port `{}`: {error}", fields[7]))?;
    let length: u32 = fields[8]
        .parse()
        .map_err(|error| format!("bad length `{}`: {error}", fields[8]))?;
    let tcp_flags = parse_flags(fields[9])?;
    let expected = match fields[10] {
        "pass" => Verdict::Pass,
        "block" => Verdict::Block,
        other => return Err(format!("bad expected verdict `{other}`")),
    };
    let expected_state = match fields[11] {
        "-" => None,
        "created" => Some(StateDisposition::Created),
        "existing" => Some(StateDisposition::Existing),
        other => return Err(format!("bad expected disposition `{other}`")),
    };
    let packet = Packet {
        direction,
        interface,
        protocol,
        source,
        destination,
        source_port,
        destination_port,
        length,
        tcp_flags,
    };
    let decision = engine.evaluate(&packet, now);
    if decision.verdict != expected {
        return Err(format!(
            "expected {:?}, got {:?} (rule {:?}, state {:?})",
            expected, decision.verdict, decision.rule, decision.state
        ));
    }
    if decision.state != expected_state {
        return Err(format!(
            "expected disposition {:?}, got {:?}",
            expected_state, decision.state
        ));
    }
    Ok(())
}

fn parse_flags(text: &str) -> Result<u8, String> {
    if text == "-" {
        return Ok(0);
    }
    let mut flags = 0u8;
    for byte in text.bytes() {
        flags |= match byte {
            b'S' => raven_pf::ast::tcp_flag::SYN,
            b'A' => raven_pf::ast::tcp_flag::ACK,
            b'F' => raven_pf::ast::tcp_flag::FIN,
            b'R' => raven_pf::ast::tcp_flag::RST,
            other => return Err(format!("bad TCP flag `{}`", char::from(other))),
        };
    }
    Ok(flags)
}
