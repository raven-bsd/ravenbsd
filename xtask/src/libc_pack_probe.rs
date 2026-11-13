// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Rust size probes P0-P3 (`donor-map/16` 3.A.7.1). The probes have their own `_start` until
//! C61 ships crt1; the numbers are interim and C69 re-measures with the real crt.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::libc_pack::Arch;
use crate::libc_pack_tools::Tools;

const MEMBER_FORBIDDEN: &[(&str, &[&str])] = &[
    (
        "p0",
        &[
            "stdio",
            "printf",
            "fwrite",
            "dtoa",
            "fmt_float",
            "locale",
            "tz",
            "resolv",
            "regex",
            "iconv",
        ],
    ),
    ("p1", &["stdio", "vfprintf", "fwrite"]),
    (
        "p2",
        &[
            "dtoa",
            "fmt_float",
            "locale",
            "tz",
            "resolv",
            "regex",
            "iconv",
        ],
    ),
    ("p3", &[]),
];

/// Build, link and measure the requested probes.
pub(crate) fn run(
    root: &Path,
    arch: &Arch,
    out: &Path,
    archive: &Path,
    which: &str,
    json: bool,
) -> Result<()> {
    let tools = Tools::discover(root, *arch)?;
    let dir = out.join("probes");
    std::fs::create_dir_all(&dir)?;
    let selected: Vec<&str> = if which == "all" {
        vec!["p0", "p1", "p2", "p3"]
    } else {
        vec![which]
    };
    let mut results = Vec::new();
    for id in selected {
        let source = root
            .join("libc/ravenlibc/crates/raven-libc/probes")
            .join(format!("{id}.rs"));
        if !source.is_file() {
            println!("libc-pack: probe {id}: skipped(no source)");
            continue;
        }
        let object = dir.join(format!("{id}.o"));
        let binary = dir.join(id);
        let map = dir.join(format!("{id}.map"));
        let compile = Command::new("rustc")
            .args(["--edition", "2024", "--crate-type", "lib", "--emit=obj"])
            .args(["--target", arch.triple()])
            .args([
                "-C",
                "panic=abort",
                "-C",
                "opt-level=z",
                "-C",
                "relocation-model=pic",
            ])
            .arg("-o")
            .arg(&object)
            .arg(&source)
            .output()
            .context("cannot run rustc for a probe")?;
        if !compile.status.success() {
            println!(
                "libc-pack: probe {id}: FAILED ({})",
                String::from_utf8_lossy(&compile.stderr).trim()
            );
            continue;
        }
        let link = Command::new(tools.lld())
            .args(["-static", "--gc-sections", "-e", "_start"])
            .arg(&object)
            .arg(archive)
            .arg("-Map")
            .arg(&map)
            .arg("-o")
            .arg(&binary)
            .output()
            .context("cannot run ld.lld for a probe")?;
        if !link.status.success() {
            println!(
                "libc-pack: probe {id}: FAILED ({})",
                String::from_utf8_lossy(&link.stderr).trim()
            );
            continue;
        }
        let size_output = Command::new(tools.size())
            .arg(&binary)
            .output()
            .context("cannot run size")?;
        let text = String::from_utf8_lossy(&size_output.stdout);
        let (text_bytes, data_bytes, bss) = parse_size(&text);
        let members = pulled_members(&map);
        let forbidden = MEMBER_FORBIDDEN
            .iter()
            .find(|(probe, _)| *probe == id)
            .map(|(_, names)| *names)
            .unwrap_or(&[]);
        let bad = members
            .iter()
            .filter(|member| forbidden.iter().any(|name| member.contains(name)))
            .count();
        let status = if bad == 0 { "ok" } else { "FORBIDDEN-MEMBER" };
        if json {
            results.push(format!(
                "{{\"id\":\"{id}\",\"arch\":\"{}\",\"text\":{text_bytes},\"data\":{data_bytes},\"bss\":{bss},\"members\":{},\"status\":\"{status}\"}}",
                arch.short(),
                members.len()
            ));
        } else {
            println!(
                "libc-pack: probe {id}: {status} S={} B={bss} members={}",
                text_bytes.saturating_add(data_bytes),
                members.len()
            );
        }
    }
    if json {
        println!("{{\"probes\":[{}]}}", results.join(","));
    }
    Ok(())
}

fn parse_size(text: &str) -> (u64, u64, u64) {
    for line in text.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 3 {
            continue;
        }
        let Ok(text_bytes) = fields.first().unwrap_or(&"0").parse::<u64>() else {
            continue;
        };
        let data = fields
            .get(1)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let bss = fields
            .get(2)
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        return (text_bytes, data, bss);
    }
    (0, 0, 0)
}

fn pulled_members(map: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(map) else {
        return Vec::new();
    };
    let mut members = Vec::new();
    for line in text.lines() {
        let Some(start) = line.find("libc.a(") else {
            continue;
        };
        let rest = line.get(start + 7..).unwrap_or("");
        if let Some(end) = rest.find(')') {
            let name = rest.get(..end).unwrap_or("").to_string();
            if !members.contains(&name) {
                members.push(name);
            }
        }
    }
    members.sort();
    members
}
