// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Regenerates `src/sysnums.rs` from the `linux-raw-sys` crate sources.
//!
//! ```text
//! cargo run -q -p raven-linux-abi --example gen_tables
//! cargo run -q -p raven-linux-abi --example gen_tables -- --check
//! ```
//!
//! The generator reads `src/{x86_64,aarch64,riscv64}/general.rs` from the `linux-raw-sys`
//! package selected by Cargo, extracts every `pub const __NR_name: u32 = N;` and writes the
//! three sorted tables plus the lookup functions. `--check` regenerates the tables in memory
//! and compares them with the committed file without writing anything.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const GENERATOR: &str = "cargo run -q -p raven-linux-abi --example gen_tables";
const MARKER: &str = "// --- generated tables below (do not edit) ---";
const ARCH_SOURCES: [(&str, &str, &str); 3] = [
    ("X86_64", "x86_64", "src/x86_64/general.rs"),
    ("AARCH64", "aarch64", "src/aarch64/general.rs"),
    ("RISCV64", "riscv64", "src/riscv64/general.rs"),
];

fn main() {
    if let Err(message) = run() {
        eprintln!("gen_tables: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut check = false;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--check" => check = true,
            other => {
                return Err(format!(
                    "unknown argument {other:?}; the only option is --check"
                ));
            }
        }
    }

    let package = LinuxRawSys::discover()?;
    let mut tables = Vec::new();
    for (ident, arch, relative) in ARCH_SOURCES {
        let path = package.root.join(relative);
        let entries = parse_syscalls(&path)
            .map_err(|error| format!("cannot read syscalls from {}: {error}", path.display()))?;
        tables.push((ident, arch, entries));
    }

    let body = render_body(&tables);
    let destination = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sysnums.rs");

    if check {
        let existing = fs::read_to_string(&destination)
            .map_err(|error| format!("cannot read {}: {error}", destination.display()))?;
        let committed = existing
            .split_once(MARKER)
            .map(|(_, body)| body.strip_prefix('\n').unwrap_or(body))
            .unwrap_or("");
        if committed != body {
            return Err(format!(
                "{} is out of date; run `{GENERATOR}` and commit the result",
                destination.display()
            ));
        }
        println!(
            "{} is up to date (linux-raw-sys {}, {} architectures)",
            destination.display(),
            package.version,
            tables.len()
        );
        return Ok(());
    }

    let header = render_header(&package.version, &today_utc());
    fs::write(&destination, format!("{header}{MARKER}\n{body}"))
        .map_err(|error| format!("cannot write {}: {error}", destination.display()))?;
    println!(
        "wrote {} from linux-raw-sys {} ({} syscalls in x86_64)",
        destination.display(),
        package.version,
        tables[0].2.len()
    );
    Ok(())
}

/// The `linux-raw-sys` package selected by Cargo for this workspace.
struct LinuxRawSys {
    version: String,
    root: PathBuf,
}

impl LinuxRawSys {
    fn discover() -> Result<Self, String> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--offline",
                "--manifest-path",
            ])
            .arg(&manifest)
            .output()
            .map_err(|error| format!("cannot run `cargo metadata`: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "`cargo metadata` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let metadata = String::from_utf8(output.stdout)
            .map_err(|error| format!("`cargo metadata` output is not UTF-8: {error}"))?;

        let needle = "\"name\":\"linux-raw-sys\",\"version\":\"";
        let start = metadata
            .find(needle)
            .ok_or("linux-raw-sys is not in the Cargo metadata")?
            + needle.len();
        let end = metadata[start..]
            .find('"')
            .ok_or("malformed Cargo metadata: unterminated linux-raw-sys version")?;
        let version = metadata[start..start + end].to_owned();
        if !version.starts_with("0.12.") {
            return Err(format!(
                "unsupported linux-raw-sys {version}; this generator is written for 0.12.x"
            ));
        }

        let key = "\"manifest_path\":\"";
        let after_name = &metadata[start + end..];
        let manifest_start = after_name
            .find(key)
            .ok_or("malformed Cargo metadata: linux-raw-sys has no manifest_path")?
            + key.len();
        let manifest_end = after_name[manifest_start..]
            .find('"')
            .ok_or("malformed Cargo metadata: unterminated manifest_path")?;
        let manifest_path =
            PathBuf::from(&after_name[manifest_start..manifest_start + manifest_end]);
        let root = manifest_path
            .parent()
            .ok_or("linux-raw-sys manifest_path has no parent directory")?
            .to_path_buf();

        Ok(Self { version, root })
    }
}

/// Parse `pub const __NR_name: u32 = N;` lines, sorted by name.
fn parse_syscalls(path: &Path) -> Result<Vec<(String, u32)>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut syscalls = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub const __NR_") else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(": u32 = ") else {
            continue;
        };
        let Some(value) = tail.strip_suffix(';') else {
            continue;
        };
        let number = value
            .parse::<u32>()
            .map_err(|error| format!("bad number for __NR_{name}: {error}"))?;
        if name == "syscalls" {
            // The table terminator, not a callable syscall.
            continue;
        }
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(format!("unsupported syscall name {name:?}"));
        }
        if syscalls.insert(name.to_owned(), number).is_some() {
            return Err(format!("duplicate syscall name __NR_{name}"));
        }
    }
    if syscalls.is_empty() {
        return Err("no syscall constants found".to_owned());
    }
    Ok(syscalls.into_iter().collect())
}

type SyscallTable = (&'static str, &'static str, Vec<(String, u32)>);

fn render_body(tables: &[SyscallTable]) -> String {
    let mut out = String::new();

    out.push_str("use crate::Arch;\n\n");

    for (ident, arch, entries) in tables {
        out.push_str(&format!(
            "/// Linux syscall names and numbers for `{arch}`, sorted by name.\n"
        ));
        out.push_str(&format!("pub static {ident}: &[(&str, u32)] = &[\n"));
        for (name, number) in entries {
            out.push_str(&format!("    (\"{name}\", {number}),\n"));
        }
        out.push_str("];\n\n");
    }

    for (ident, _arch, entries) in tables {
        let mut by_number = entries.clone();
        by_number.sort_by_key(|(_, number)| *number);
        out.push_str(&format!(
            "/// `{ident}` sorted by number, for [`name`] lookups.\n"
        ));
        out.push_str(&format!("static {ident}_BY_NR: &[(&str, u32)] = &[\n"));
        for (name, number) in &by_number {
            out.push_str(&format!("    (\"{name}\", {number}),\n"));
        }
        out.push_str("];\n\n");
    }

    let all_names: BTreeSet<&str> = tables
        .iter()
        .flat_map(|(_, _, entries)| entries.iter().map(|(name, _)| name.as_str()))
        .collect();
    out.push_str("/// Every syscall name known on at least one supported architecture, sorted.\n");
    out.push_str("pub static ALL_NAMES: &[&str] = &[\n");
    for name in all_names {
        out.push_str(&format!("    \"{name}\",\n"));
    }
    out.push_str("];\n\n");

    out.push_str(
        "/// Return the syscall number named `name` for `arch`, or `None` if that architecture has no such syscall.\n\
         pub fn number(arch: Arch, name: &str) -> Option<u32> {\n\
         \x20   let table = match arch {\n\
         \x20       Arch::X86_64 => X86_64,\n\
         \x20       Arch::Aarch64 => AARCH64,\n\
         \x20       Arch::Riscv64 => RISCV64,\n\
         \x20   };\n\
         \x20   table\n\
         \x20       .binary_search_by(|(candidate, _)| (*candidate).cmp(name))\n\
         \x20       .ok()\n\
         \x20       .map(|index| table[index].1)\n\
         }\n\n",
    );
    out.push_str(
        "/// Return the syscall name for `number` on `arch`, or `None` if that architecture has no such number.\n\
         pub fn name(arch: Arch, number: u32) -> Option<&'static str> {\n\
         \x20   let table = match arch {\n\
         \x20       Arch::X86_64 => X86_64_BY_NR,\n\
         \x20       Arch::Aarch64 => AARCH64_BY_NR,\n\
         \x20       Arch::Riscv64 => RISCV64_BY_NR,\n\
         \x20   };\n\
         \x20   table\n\
         \x20       .binary_search_by_key(&number, |&(_, candidate)| candidate)\n\
         \x20       .ok()\n\
         \x20       .map(|index| table[index].0)\n\
         }\n",
    );

    out
}

fn render_header(version: &str, date: &str) -> String {
    let sources = ARCH_SOURCES
        .iter()
        .map(|(_, _, path)| *path)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "// SPDX-License-Identifier: MIT\n\
         // Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n\
         //\n\
         // @generated by `{GENERATOR}`\n\
         // Source: linux-raw-sys {version} (Apache-2.0 OR MIT): {sources}\n\
         // Generated: {date}. Do not edit this file by hand.\n\
         //\n"
    )
}

/// Current UTC date as `YYYY-MM-DD`, without pulling in a date library.
fn today_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (seconds / 86_400) as i64;

    // Howard Hinnant's `civil_from_days` algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}")
}
