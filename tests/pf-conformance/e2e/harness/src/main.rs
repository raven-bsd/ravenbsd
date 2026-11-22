// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K297 end-to-end conformance: pf.conf -> pfctl -> raven-pf -> tcpdump.
//!
//! The harness is test-only and outside the product workspace. For every K277
//! scenario it loads the rules with `raven-pfctl` (the pfctl parser/printer),
//! evaluates the K277 packet rows with the `raven-pf` engine, builds a pcap
//! from the same packets and decodes it with `raven-tcpdump` (unfiltered and
//! with a protocol filter). A known product divergence (K277's fragment
//! port-rule case) is recorded as KNOWN; donor-only rows are SKIP. The
//! `--mutate-verdict`/`--mutate-pcap` modes prove the harness catches a wrong
//! expectation and a corrupted capture.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use raven_pf::{
    Direction, Engine, IpAddr, Packet, Protocol, Translation, Verdict,
    parse as parse_pf,
};
use raven_pfctl::{parse_config, print_config};
use raven_tcpdump::cli::Options as TcpdumpOptions;
use raven_tcpdump::pcap::{self, Packet as PcapPacket, TimestampPrecision};
use raven_tcpdump::{Timestamps, execute};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    None,
    Verdict,
    Pcap,
}

#[derive(Default)]
struct Summary {
    passed: usize,
    skipped: usize,
    known: usize,
    failed: usize,
    scenarios: Vec<String>,
}

fn main() -> ExitCode {
    let mut root = None;
    let mut mutation = Mutation::None;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--mutate-verdict" => mutation = Mutation::Verdict,
            "--mutate-pcap" => mutation = Mutation::Pcap,
            other => root = Some(PathBuf::from(other)),
        }
    }
    let default_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let root = root.unwrap_or(default_root);
    match run(&root, mutation) {
        Ok(summary) => {
            println!(
                "pf-e2e: {} passed, {} skipped, {} known, {} failed",
                summary.passed, summary.skipped, summary.known, summary.failed
            );
            let caught = summary.failed > 0;
            match mutation {
                Mutation::None => {
                    if summary.failed == 0 {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Mutation::Verdict | Mutation::Pcap => {
                    if caught {
                        println!("mutation caught: {:?}", mutation);
                        ExitCode::SUCCESS
                    } else {
                        println!("mutation NOT caught: {:?}", mutation);
                        ExitCode::FAILURE
                    }
                }
            }
        }
        Err(error) => {
            eprintln!("pf-e2e: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path, mutation: Mutation) -> Result<Summary, String> {
    let fixtures = root.join("tests/pf-conformance");
    let out_dir = root.join("build/pf-e2e");
    fs::create_dir_all(&out_dir).map_err(|error| format!("create {}: {error}", out_dir.display()))?;
    let manifest_path = fixtures.join("manifest.tsv");
    let manifest = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("{}: {error}", manifest_path.display()))?;
    let mut summary = Summary::default();
    let mut first_scenario = true;
    for (index, line) in manifest.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.splitn(6, '|').collect();
        if fields.len() != 6 {
            return Err(format!(
                "manifest.tsv line {}: expected 6 fields",
                index + 1
            ));
        }
        let id = fields[0];
        let kind = fields[1];
        let rules = fixtures.join(fields[2]);
        let packets = fields[3];
        let support = fields[4];
        match (kind, support) {
            ("parse-error", "raven-pf-parse-error") => {
                match run_parse_error(&rules, fields[5]) {
                    Ok(()) => {
                        summary.passed += 1;
                        summary.scenarios.push(format!("{id}\tpass\tpfctl+engine rejected"));
                        println!("PASS {id} (pfctl and raven-pf rejected)");
                    }
                    Err(error) => {
                        summary.failed += 1;
                        summary.scenarios.push(format!("{id}\tfail\t{error}"));
                        println!("FAIL {id}: {error}");
                    }
                }
            }
            ("scenario", "raven-pf") | ("scenario", "raven-pf-known-divergence") => {
                let applied = if first_scenario { mutation } else { Mutation::None };
                first_scenario = false;
                match run_scenario(
                    root,
                    &out_dir,
                    id,
                    &rules,
                    &fixtures.join(packets),
                    applied,
                ) {
                    Ok(()) => {
                        summary.passed += 1;
                        summary
                            .scenarios
                            .push(format!("{id}\tpass\tpfctl+engine+tcpdump"));
                        println!("PASS {id} (pfctl + {packets} + tcpdump)");
                    }
                    Err(error) if support == "raven-pf-known-divergence" => {
                        summary.known += 1;
                        summary
                            .scenarios
                            .push(format!("{id}\tknown\t{error}"));
                        println!("KNOWN {id}: {error}");
                    }
                    Err(error) => {
                        summary.failed += 1;
                        summary.scenarios.push(format!("{id}\tfail\t{error}"));
                        println!("FAIL {id}: {error}");
                    }
                }
            }
            ("scenario", "donor-only") => {
                summary.skipped += 1;
                summary
                    .scenarios
                    .push(format!("{id}\tskip\tdonor-only: {}", fields[5]));
                println!("SKIP {id}: {}", fields[5]);
            }
            (kind, support) => {
                return Err(format!("{id}: unsupported kind/support {kind}/{support}"));
            }
        }
    }

    // K297-specific table case: the K277 matrix has no table scenario.
    match run_table_case(root, &out_dir) {
        Ok(()) => {
            summary.passed += 1;
            summary
                .scenarios
                .push("table\tpass\tpfctl+engine+tcpdump".to_string());
            println!("PASS table (table membership through pfctl/engine/tcpdump)");
        }
        Err(error) => {
            summary.failed += 1;
            summary.scenarios.push(format!("table\tfail\t{error}"));
            println!("FAIL table: {error}");
        }
    }

    let generated = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let report = format!(
        "{{\n  \"schema\": 1,\n  \"generated_unix\": {generated},\n  \"root\": \"{}\",\n  \"summary\": {{\"passed\": {}, \"skipped\": {}, \"known\": {}, \"failed\": {}}},\n  \"scenarios\": [{}]\n}}\n",
        root.display(),
        summary.passed,
        summary.skipped,
        summary.known,
        summary.failed,
        summary
            .scenarios
            .iter()
            .map(|row| {
                let fields: Vec<&str> = row.splitn(3, '\t').collect();
                format!(
                    "{{\"id\": \"{}\", \"status\": \"{}\", \"detail\": \"{}\"}}",
                    escape(fields.first().copied().unwrap_or("")),
                    escape(fields.get(1).copied().unwrap_or("")),
                    escape(fields.get(2).copied().unwrap_or(""))
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    );
    let report_name = match mutation {
        Mutation::None => "report.json",
        Mutation::Verdict => "report-mutate-verdict.json",
        Mutation::Pcap => "report-mutate-pcap.json",
    };
    fs::write(out_dir.join(report_name), report)
        .map_err(|error| format!("write report: {error}"))?;
    Ok(summary)
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// RavenBSD pf rules use `set default` and the `quick` position that the K274
/// pfctl grammar does not model. Strip/relocate only those Raven extensions;
/// every rule itself is still parsed and printed by pfctl.
fn adapt_for_pfctl(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("set default ") {
            continue;
        }
        let mut line = line.to_string();
        for (from, to) in [
            ("pass quick in", "pass in quick"),
            ("pass quick out", "pass out quick"),
            ("block quick in", "block in quick"),
            ("block quick out", "block out quick"),
        ] {
            if let Some(rest) = line.trim_start().strip_prefix(from) {
                let indent = &line[..line.len() - line.trim_start().len()];
                line = format!("{indent}{to}{rest}");
            }
        }
        line = line.replace(" static-port", "");
        line = line.replace(" proto any", "");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn run_parse_error(path: &Path, expect: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let (line, substring) = expect
        .split_once(',')
        .ok_or_else(|| format!("bad parse-error expectation `{expect}`"))?;
    let line: usize = line
        .trim()
        .parse()
        .map_err(|error| format!("bad expected line: {error}"))?;
    let adapted = adapt_for_pfctl(&text);
    if parse_config(&adapted).is_ok() {
        return Err("pfctl accepted a malformed ruleset".to_string());
    }
    let error = parse_pf(&text).expect_err("raven-pf accepted a malformed ruleset");
    if error.line != line {
        return Err(format!(
            "raven-pf rejected on line {}, expected {line}",
            error.line
        ));
    }
    if !error.message.contains(substring.trim()) {
        return Err(format!(
            "raven-pf diagnostic `{}` lacks `{}`",
            error.message,
            substring.trim()
        ));
    }
    Ok(())
}

fn run_scenario(
    root: &Path,
    out_dir: &Path,
    id: &str,
    rules: &Path,
    packets: &Path,
    mutation: Mutation,
) -> Result<(), String> {
    let rules_text =
        fs::read_to_string(rules).map_err(|error| format!("{}: {error}", rules.display()))?;
    let adapted = adapt_for_pfctl(&rules_text);
    let config = parse_config(&adapted).map_err(|error| format!("pfctl: {error:?}"))?;
    let printed = print_config(&config);
    let reparsed = parse_config(&printed).map_err(|error| format!("pfctl reprint: {error:?}"))?;
    if print_config(&reparsed) != printed {
        return Err("pfctl print/parse round-trip differs".to_string());
    }

    let rows = read_rows(packets)?;
    if rows.is_empty() {
        return Err(format!("{} has no packet rows", packets.display()));
    }
    let frames = build_frames(&rows)?;
    let mut frames = frames;
    if mutation == Mutation::Pcap {
        mutate_frame(&mut frames)?;
    }
    let pcap_bytes = write_pcap(&frames);
    let pcap_dir = if mutation == Mutation::None {
        out_dir.to_path_buf()
    } else {
        let dir = out_dir.join("mutation");
        fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
        dir
    };
    let pcap_path = pcap_dir.join(format!("{id}.pcap"));
    fs::write(&pcap_path, &pcap_bytes)
        .map_err(|error| format!("write {}: {error}", pcap_path.display()))?;

    let decoded = execute(
        &pcap_bytes,
        &TcpdumpOptions {
            read: Some(pcap_path.display().to_string()),
            numeric: true,
            timestamps: Timestamps::Unix,
            ..TcpdumpOptions::default()
        },
    )
    .map_err(|error| format!("tcpdump decode: {error}"))?;
    if decoded.selected != rows.len() {
        return Err(format!(
            "tcpdump selected {} of {} packets",
            decoded.selected,
            rows.len()
        ));
    }
    let first_line = expected_tcpdump_line(&rows[0]);
    let first_decoded = decoded.text.lines().next().unwrap_or("");
    if !first_decoded.contains(&first_line) {
        return Err(format!(
            "tcpdump first line lacks `{first_line}`; got `{first_decoded}`"
        ));
    }
    let (filter, expected_selected) = filter_expectation(&rows);
    let filtered = execute(
        &pcap_bytes,
        &TcpdumpOptions {
            read: Some(pcap_path.display().to_string()),
            numeric: true,
            timestamps: Timestamps::Unix,
            filter: Some(filter.to_string()),
            ..TcpdumpOptions::default()
        },
    )
    .map_err(|error| format!("tcpdump filter: {error}"))?;
    if filtered.selected != expected_selected {
        return Err(format!(
            "tcpdump filter `{filter}` selected {}, expected {expected_selected}",
            filtered.selected
        ));
    }

    let ruleset = parse_pf(&rules_text).map_err(|error| format!("raven-pf: {error}"))?;
    let mut engine = Engine::new(ruleset);
    for (index, row) in rows.iter().enumerate() {
        evaluate_row(
            &mut engine,
            row,
            mutation == Mutation::Verdict && index == 0,
        )
        .map_err(|error| format!("engine row {}: {error}", index + 1))?;
    }
    let _ = root;
    Ok(())
}

fn run_table_case(root: &Path, out_dir: &Path) -> Result<(), String> {
    let rules = "set default block\n\
                 table <web> { 192.0.2.10, 2001:db8::10 }\n\
                 pass in on eth0 proto tcp from any to <web> port 80 keep state\n";
    let adapted = adapt_for_pfctl(rules);
    let config = parse_config(&adapted).map_err(|error| format!("pfctl: {error:?}"))?;
    let _ = print_config(&config);
    let ruleset = parse_pf(rules).map_err(|error| format!("raven-pf: {error}"))?;
    let mut engine = Engine::new(ruleset);
    let rows = vec![
        Row {
            now: 0,
            direction: Direction::In,
            interface: "eth0",
            protocol: Protocol::Tcp,
            source: IpAddr::parse("10.0.0.5").map_err(|error| error.to_string())?,
            source_port: 40000,
            destination: IpAddr::parse("192.0.2.10").map_err(|error| error.to_string())?,
            destination_port: 80,
            length: 60,
            tcp_flags: 0x02,
            expected: Verdict::Pass,
            disposition: Some(raven_pf::StateDisposition::Created),
            fragment: false,
            translation: None,
        },
        Row {
            now: 1,
            direction: Direction::In,
            interface: "eth0",
            protocol: Protocol::Tcp,
            source: IpAddr::parse("10.0.0.5").map_err(|error| error.to_string())?,
            source_port: 40001,
            destination: IpAddr::parse("192.0.2.11").map_err(|error| error.to_string())?,
            destination_port: 80,
            length: 60,
            tcp_flags: 0x02,
            expected: Verdict::Block,
            disposition: None,
            fragment: false,
            translation: None,
        },
    ];
    let frames = build_frames(&rows)?;
    let pcap_bytes = write_pcap(&frames);
    let pcap_path = out_dir.join("table.pcap");
    fs::write(&pcap_path, &pcap_bytes)
        .map_err(|error| format!("write {}: {error}", pcap_path.display()))?;
    let decoded = execute(
        &pcap_bytes,
        &TcpdumpOptions {
            read: Some(pcap_path.display().to_string()),
            numeric: true,
            timestamps: Timestamps::Unix,
            ..TcpdumpOptions::default()
        },
    )
    .map_err(|error| format!("tcpdump: {error}"))?;
    if decoded.selected != 2 {
        return Err(format!("tcpdump selected {}", decoded.selected));
    }
    for (index, row) in rows.iter().enumerate() {
        evaluate_row(&mut engine, row, false)
            .map_err(|error| format!("engine row {}: {error}", index + 1))?;
    }
    let _ = root;
    Ok(())
}

#[derive(Clone, Debug)]
struct Row {
    now: u64,
    direction: Direction,
    interface: &'static str,
    protocol: Protocol,
    source: IpAddr,
    source_port: u16,
    destination: IpAddr,
    destination_port: u16,
    length: u32,
    tcp_flags: u8,
    expected: Verdict,
    disposition: Option<raven_pf::StateDisposition>,
    fragment: bool,
    translation: Option<Translation>,
}

fn read_rows(path: &Path) -> Result<Vec<Row>, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if !(12..=14).contains(&fields.len()) {
            return Err(format!(
                "{}:{}: expected 12..14 fields, found {}",
                path.display(),
                index + 1,
                fields.len()
            ));
        }
        let interface = match fields[2] {
            "lo" => "lo",
            "eth0" => "eth0",
            other => return Err(format!("{}:{}: bad interface {other}", path.display(), index + 1)),
        };
        rows.push(Row {
            now: fields[0].parse().map_err(|error| format!("now: {error}"))?,
            direction: match fields[1] {
                "in" => Direction::In,
                "out" => Direction::Out,
                other => return Err(format!("direction {other}")),
            },
            interface,
            protocol: match fields[3] {
                "tcp" => Protocol::Tcp,
                "udp" => Protocol::Udp,
                "icmp" => Protocol::Icmp,
                "icmp6" => Protocol::Icmpv6,
                other => return Err(format!("protocol {other}")),
            },
            source: IpAddr::parse(fields[4]).map_err(|error| error.to_string())?,
            source_port: fields[5].parse().map_err(|error| format!("sport: {error}"))?,
            destination: IpAddr::parse(fields[6]).map_err(|error| error.to_string())?,
            destination_port: fields[7].parse().map_err(|error| format!("dport: {error}"))?,
            length: fields[8].parse().map_err(|error| format!("length: {error}"))?,
            tcp_flags: parse_flags(fields[9])?,
            expected: match fields[10] {
                "pass" => Verdict::Pass,
                "block" => Verdict::Block,
                other => return Err(format!("verdict {other}")),
            },
            disposition: match fields[11] {
                "-" => None,
                "created" => Some(raven_pf::StateDisposition::Created),
                "existing" => Some(raven_pf::StateDisposition::Existing),
                other => return Err(format!("disposition {other}")),
            },
            fragment: fields.get(12).is_some_and(|field| *field == "frag"),
            translation: match fields.get(13).copied().unwrap_or("-") {
                "-" => None,
                text => Some(parse_translation(text)?),
            },
        });
    }
    Ok(rows)
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
            other => return Err(format!("flag {}", char::from(other))),
        };
    }
    Ok(flags)
}

fn parse_translation(text: &str) -> Result<Translation, String> {
    let (source, destination) = text
        .split_once('>')
        .ok_or_else(|| format!("bad translation {text}"))?;
    let (source, source_port) = source.rsplit_once(':').ok_or("bad translation source")?;
    let (destination, destination_port) =
        destination.rsplit_once(':').ok_or("bad translation destination")?;
    Ok(Translation {
        rule: 0,
        source: IpAddr::parse(source).map_err(|error| error.to_string())?,
        source_port: source_port.parse().map_err(|error| format!("port: {error}"))?,
        destination: IpAddr::parse(destination).map_err(|error| error.to_string())?,
        destination_port: destination_port
            .parse()
            .map_err(|error| format!("port: {error}"))?,
    })
}

fn evaluate_row(engine: &mut Engine, row: &Row, mutate: bool) -> Result<(), String> {
    let packet = Packet {
        direction: row.direction,
        interface: row.interface,
        protocol: row.protocol,
        source: row.source,
        source_port: row.source_port,
        destination: row.destination,
        destination_port: row.destination_port,
        length: row.length,
        tcp_flags: row.tcp_flags,
        fragment: row.fragment,
    };
    let decision = engine.evaluate(&packet, row.now);
    let mut expected = row.expected;
    if mutate {
        expected = match expected {
            Verdict::Pass => Verdict::Block,
            Verdict::Block => Verdict::Pass,
        };
    }
    if decision.verdict != expected {
        return Err(format!(
            "expected {:?}, got {:?} (rule {:?})",
            expected, decision.verdict, decision.rule
        ));
    }
    if !mutate && decision.state != row.disposition {
        return Err(format!(
            "expected state {:?}, got {:?}",
            row.disposition, decision.state
        ));
    }
    if let Some(expected) = row.translation {
        let actual = decision
            .translation
            .ok_or_else(|| "expected a translation, got none".to_string())?;
        if actual.source != expected.source
            || actual.source_port != expected.source_port
            || actual.destination != expected.destination
            || actual.destination_port != expected.destination_port
        {
            return Err(format!("expected translation {expected:?}, got {actual:?}"));
        }
    }
    Ok(())
}

fn expected_tcpdump_line(row: &Row) -> String {
    match row.protocol {
        Protocol::Icmp | Protocol::Icmpv6 => "echo request".to_string(),
        _ => format!(
            "{}.{} > {}.{}:",
            row.source, row.source_port, row.destination, row.destination_port
        ),
    }
}

fn filter_expectation(rows: &[Row]) -> (&'static str, usize) {
    let tcp = rows.iter().filter(|row| row.protocol == Protocol::Tcp).count();
    if tcp > 0 {
        return ("tcp", tcp);
    }
    let udp = rows.iter().filter(|row| row.protocol == Protocol::Udp).count();
    if udp > 0 {
        return ("udp", udp);
    }
    let icmp6 = rows
        .iter()
        .filter(|row| row.protocol == Protocol::Icmpv6)
        .count();
    if icmp6 > 0 {
        return ("icmp6", icmp6);
    }
    ("icmp", rows.len())
}

fn build_frames(rows: &[Row]) -> Result<Vec<Vec<u8>>, String> {
    rows.iter().map(build_frame).collect()
}

fn build_frame(row: &Row) -> Result<Vec<u8>, String> {
    let ipv6 = row.source.is_v6();
    if ipv6 != row.destination.is_v6() {
        return Err("mixed address families in one row".to_string());
    }
    let mut frame = Vec::with_capacity(64);
    frame.extend_from_slice(&[0x02, 0, 0, 0, 0, 0x0b]);
    frame.extend_from_slice(&[0x02, 0, 0, 0, 0, 0x0a]);
    let l4_len = match row.protocol {
        Protocol::Tcp => 20,
        Protocol::Udp | Protocol::Icmp | Protocol::Icmpv6 => 8,
        Protocol::Any => return Err("cannot build an Any frame".to_string()),
    };
    if ipv6 {
        frame.extend_from_slice(&0x86ddu16.to_be_bytes());
    } else {
        frame.extend_from_slice(&0x0800u16.to_be_bytes());
    }
    let ip_offset = frame.len();
    if ipv6 {
        frame.resize(ip_offset + 40, 0);
        frame[ip_offset] = 0x60;
        frame[ip_offset + 4..ip_offset + 6]
            .copy_from_slice(&u16::try_from(l4_len).unwrap_or(0).to_be_bytes());
        frame[ip_offset + 6] = match row.protocol {
            Protocol::Tcp => 6,
            Protocol::Udp => 17,
            Protocol::Icmpv6 => 58,
            _ => 0,
        };
        frame[ip_offset + 7] = 64;
        let IpAddr::V6(source) = row.source else { unreachable!() };
        let IpAddr::V6(destination) = row.destination else { unreachable!() };
        frame[ip_offset + 8..ip_offset + 24].copy_from_slice(&source.octets());
        frame[ip_offset + 24..ip_offset + 40].copy_from_slice(&destination.octets());
    } else {
        frame.resize(ip_offset + 20, 0);
        frame[ip_offset] = 0x45;
        frame[ip_offset + 2..ip_offset + 4]
            .copy_from_slice(&u16::try_from(20 + l4_len).unwrap_or(0).to_be_bytes());
        frame[ip_offset + 4..ip_offset + 6].copy_from_slice(&1u16.to_be_bytes());
        if row.fragment {
            frame[ip_offset + 6..ip_offset + 8].copy_from_slice(&0x2000u16.to_be_bytes());
        }
        frame[ip_offset + 8] = 64;
        frame[ip_offset + 9] = match row.protocol {
            Protocol::Tcp => 6,
            Protocol::Udp => 17,
            Protocol::Icmp => 1,
            _ => 0,
        };
        let IpAddr::V4(source) = row.source else { unreachable!() };
        let IpAddr::V4(destination) = row.destination else { unreachable!() };
        frame[ip_offset + 12..ip_offset + 16].copy_from_slice(&source.octets());
        frame[ip_offset + 16..ip_offset + 20].copy_from_slice(&destination.octets());
        let checksum = checksum(&frame[ip_offset..ip_offset + 20]);
        frame[ip_offset + 10..ip_offset + 12].copy_from_slice(&checksum.to_be_bytes());
    }
    let l4 = frame.len();
    frame.resize(l4 + l4_len, 0);
    match row.protocol {
        Protocol::Tcp => {
            frame[l4..l4 + 2].copy_from_slice(&row.source_port.to_be_bytes());
            frame[l4 + 2..l4 + 4].copy_from_slice(&row.destination_port.to_be_bytes());
            frame[l4 + 4..l4 + 8].copy_from_slice(&1000u32.to_be_bytes());
            frame[l4 + 12] = 5 << 4;
            frame[l4 + 13] = row.tcp_flags;
            frame[l4 + 14..l4 + 16].copy_from_slice(&65535u16.to_be_bytes());
            let sum = l4_checksum(&frame, ip_offset, l4, l4_len, if ipv6 { 6 } else { 6 });
            frame[l4 + 16..l4 + 18].copy_from_slice(&sum.to_be_bytes());
        }
        Protocol::Udp => {
            frame[l4..l4 + 2].copy_from_slice(&row.source_port.to_be_bytes());
            frame[l4 + 2..l4 + 4].copy_from_slice(&row.destination_port.to_be_bytes());
            frame[l4 + 4..l4 + 6].copy_from_slice(&8u16.to_be_bytes());
            if ipv6 {
                let sum = l4_checksum(&frame, ip_offset, l4, l4_len, 17);
                frame[l4 + 6..l4 + 8].copy_from_slice(&sum.to_be_bytes());
            }
        }
        Protocol::Icmp => {
            frame[l4] = 8;
            frame[l4 + 4..l4 + 6].copy_from_slice(&1u16.to_be_bytes());
            frame[l4 + 6..l4 + 8].copy_from_slice(&1u16.to_be_bytes());
            let sum = checksum(&frame[l4..l4 + l4_len]);
            frame[l4 + 2..l4 + 4].copy_from_slice(&sum.to_be_bytes());
        }
        Protocol::Icmpv6 => {
            frame[l4] = 128;
            frame[l4 + 4..l4 + 6].copy_from_slice(&1u16.to_be_bytes());
            frame[l4 + 6..l4 + 8].copy_from_slice(&1u16.to_be_bytes());
            let sum = l4_checksum(&frame, ip_offset, l4, l4_len, 58);
            frame[l4 + 2..l4 + 4].copy_from_slice(&sum.to_be_bytes());
        }
        Protocol::Any => unreachable!(),
    }
    Ok(frame)
}

fn mutate_frame(frames: &mut [Vec<u8>]) -> Result<(), String> {
    let frame = frames.first_mut().ok_or("no frame to mutate")?;
    // Ethernet + IPv4 + TCP destination port. Flipping it makes the expected
    // tcpdump tuple in the first row disappear.
    let at = 14 + 20 + 2;
    let byte = frame.get_mut(at).ok_or("frame too short to mutate")?;
    *byte ^= 0x01;
    Ok(())
}

fn write_pcap(frames: &[Vec<u8>]) -> Vec<u8> {
    let packets: Vec<PcapPacket<'_>> = frames
        .iter()
        .map(|frame| PcapPacket {
            ts_sec: 1_700_000_000,
            ts_frac: 0,
            precision: TimestampPrecision::Microseconds,
            caplen: u32::try_from(frame.len()).unwrap_or(0),
            orig_len: u32::try_from(frame.len()).unwrap_or(0),
            data: frame,
        })
        .collect();
    pcap::write(&packets, 1, 65535)
}

fn l4_checksum(frame: &[u8], ip_offset: usize, l4: usize, l4_len: usize, protocol: u8) -> u16 {
    let mut sum = 0u32;
    if frame[ip_offset] >> 4 == 6 {
        sum = add_bytes(sum, &frame[ip_offset + 8..ip_offset + 24]);
        sum = add_bytes(sum, &frame[ip_offset + 24..ip_offset + 40]);
        sum = add_bytes(sum, &(u32::try_from(l4_len).unwrap_or(0)).to_be_bytes());
        sum = add_bytes(sum, &[0, 0, 0, protocol]);
    } else {
        sum = add_bytes(sum, &frame[ip_offset + 12..ip_offset + 16]);
        sum = add_bytes(sum, &frame[ip_offset + 16..ip_offset + 20]);
        sum = add_bytes(
            sum,
            &[
                0,
                protocol,
                u8::try_from(l4_len >> 8).unwrap_or(0),
                u8::try_from(l4_len & 0xff).unwrap_or(0),
            ],
        );
    }
    sum = add_bytes(sum, &frame[l4..l4 + l4_len]);
    let folded = fold(sum);
    if folded == 0 { 0xffff } else { folded }
}

fn checksum(data: &[u8]) -> u16 {
    fold(add_bytes(0, data))
}

fn add_bytes(mut sum: u32, data: &[u8]) -> u32 {
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        sum += u32::from(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    if let [last] = chunks.remainder() {
        sum += u32::from(*last) << 8;
    }
    sum
}

fn fold(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}
