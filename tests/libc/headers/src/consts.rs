// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `spec-constants.toml`, `header-only-constants.toml`, `probes.toml` and the
//! errno parity check.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::clang::Arch;
use crate::repo_root;
use crate::toml_lite::{self, Value};

/// A constant asserted by T4.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecConstant {
    /// Macro name.
    pub name: String,
    /// Expression compared with `==`, as text.
    pub value: String,
    /// Headers that define it.
    pub headers: Vec<String>,
    /// Architectures the row applies to.
    pub arches: Vec<String>,
    /// Provenance note, usually `doc 14 3.A.x`.
    pub source: String,
}

/// A name that appears in the headers but not in the generator output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderOnly {
    /// Macro name.
    pub name: String,
    /// Why it has no C36/generator row.
    pub reason: String,
    /// Owning ticket.
    pub owner: String,
}

/// A visibility probe row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Probe {
    /// Header that declares the symbol.
    pub header: String,
    /// Function or data symbol.
    pub symbol: String,
    /// Mode names where it must compile.
    pub visible_in: Vec<String>,
    /// Mode names where it must be absent.
    pub hidden_in: Vec<String>,
    /// Owning ticket.
    pub owner: String,
    /// True when the row is waiting for a later ticket.
    pub pending: bool,
}

/// A `-Wno-*` exception row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exception {
    /// Header the exception applies to.
    pub header: String,
    /// The flag, for example `-Wno-deprecated-declarations`.
    pub flag: String,
    /// Why it is unavoidable.
    pub reason: String,
}

fn data_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name)
}

fn read_data(name: &str) -> Result<String, String> {
    let path = data_file(name);
    std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn value_text(value: &Value) -> Result<String, String> {
    match value {
        Value::Str(text) => Ok(text.clone()),
        Value::Int(number) => Ok(number.to_string()),
        _ => Err("expected a string or integer value".to_string()),
    }
}

/// Load `spec-constants.toml`.
pub fn load_spec() -> Result<Vec<SpecConstant>, String> {
    let text = read_data("spec-constants.toml")?;
    let doc = toml_lite::parse(&text).map_err(|error| format!("spec-constants.toml: {error}"))?;
    let mut rows = Vec::new();
    for table in doc.get("constant") {
        let value = table.fields.get("value").ok_or_else(|| {
            let _ = &table;
            "spec constant has no value".to_string()
        })?;
        rows.push(SpecConstant {
            name: table.get_str("name")?.to_string(),
            value: value_text(value)?,
            headers: table.get_str_array("headers")?,
            arches: table.get_str_array("arches")?,
            source: table.get_str("source")?.to_string(),
        });
    }
    Ok(rows)
}

/// Load `header-only-constants.toml`.
pub fn load_header_only() -> Result<Vec<HeaderOnly>, String> {
    let text = read_data("header-only-constants.toml")?;
    let doc =
        toml_lite::parse(&text).map_err(|error| format!("header-only-constants.toml: {error}"))?;
    let mut rows = Vec::new();
    for table in doc.get("constant") {
        let owner = table
            .fields
            .get("owner")
            .map_or(Ok("C37".to_string()), value_text)?;
        rows.push(HeaderOnly {
            name: table.get_str("name")?.to_string(),
            reason: table.get_str("reason")?.to_string(),
            owner,
        });
    }
    Ok(rows)
}

/// Load `probes.toml`.
pub fn load_probes() -> Result<Vec<Probe>, String> {
    let text = read_data("probes.toml")?;
    let doc = toml_lite::parse(&text).map_err(|error| format!("probes.toml: {error}"))?;
    let mut rows = Vec::new();
    for table in doc.get("probe") {
        rows.push(Probe {
            header: table.get_str("header")?.to_string(),
            symbol: table.get_str("symbol")?.to_string(),
            visible_in: table.get_str_array("visible_in")?,
            hidden_in: table.get_str_array("hidden_in")?,
            owner: table.get_str("owner")?.to_string(),
            pending: table.fields.contains_key("pending")
                && table.get_bool("pending").unwrap_or(false),
        });
    }
    Ok(rows)
}

/// Load `exceptions.toml` (an empty file is valid).
pub fn load_exceptions() -> Result<Vec<Exception>, String> {
    let text = read_data("exceptions.toml")?;
    let doc = toml_lite::parse(&text).map_err(|error| format!("exceptions.toml: {error}"))?;
    let mut rows = Vec::new();
    for table in doc.get("exception") {
        rows.push(Exception {
            header: table.get_str("header")?.to_string(),
            flag: table.get_str("flag")?.to_string(),
            reason: table.get_str("reason")?.to_string(),
        });
    }
    Ok(rows)
}

/// Generate the `_Static_assert` TU for one architecture.
#[must_use]
pub fn assert_source(rows: &[SpecConstant], arch: Arch) -> String {
    let mut source = String::from("#include <sys/cdefs.h>\n");
    let mut headers = BTreeSet::new();
    for row in rows {
        if row.arches.iter().any(|name| name == arch.name()) {
            headers.extend(row.headers.iter().cloned());
        }
    }
    for header in headers {
        source.push_str(&format!("#include <{header}>\n"));
    }
    for row in rows {
        if !row.arches.iter().any(|name| name == arch.name()) {
            continue;
        }
        source.push_str(&format!(
            "_Static_assert(({}) == ({}), \"{} constant\");\n",
            row.name, row.value, row.name
        ));
    }
    source
}

/// All macro definitions produced by the C36 files copied into the tree.
pub fn generated_defines() -> Result<BTreeMap<String, String>, String> {
    let directory = repo_root().join("libc/ravenlibc/include/bits");
    let sources = [
        "errno.h",
        "signum.h",
        "sysconf.h",
        "abi_fcntl.h",
        "abi_limits.h",
        "abi_misc.h",
        "abi_mman.h",
        "abi_netinet.h",
        "abi_poll.h",
        "abi_pthread.h",
        "abi_resource.h",
        "abi_socket.h",
        "abi_stat.h",
        "abi_time.h",
    ];
    let mut defines = BTreeMap::new();
    for name in sources {
        let path = directory.join(name);
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        for line in text.lines() {
            let Some(rest) = line.strip_prefix("#define ") else {
                continue;
            };
            if let Some((key, value)) = split_define(rest) {
                if let Some(previous) = defines.get(key) {
                    if previous != value {
                        return Err(format!(
                            "generated macro {key} has two values: {previous} and {value}"
                        ));
                    }
                } else {
                    defines.insert(key.to_string(), value.to_string());
                }
            }
        }
    }
    Ok(defines)
}

fn split_define(text: &str) -> Option<(&str, &str)> {
    let mut parts = text.splitn(2, char::is_whitespace);
    let key = parts.next()?;
    if key.is_empty() || key.contains('(') {
        return None;
    }
    Some((key, parts.next().unwrap_or("").trim()))
}

/// The family prefixes of T4(b).
pub const PREFIXES: &[&str] = &[
    "E", "SIG", "SA_", "SS_", "O_", "F_", "AT_", "PROT_", "MAP_", "S_", "SO_", "AF_", "PF_",
    "IPPROTO_", "TCP_", "CLOCK_", "RLIMIT_", "_SC_", "_PC_",
];

/// True when `name` starts with one of the T4(b) family prefixes.
#[must_use]
pub fn in_family(name: &str) -> bool {
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// Compare one architecture's `-dM` output with the generated tables and the
/// header-only list. Returns `(mismatches, unlisted)`.
#[must_use]
pub fn scan_families(
    dump: &[(String, String)],
    generated: &BTreeMap<String, String>,
    header_only: &BTreeMap<String, String>,
) -> (Vec<(String, String, String)>, Vec<String>) {
    let mut mismatches = Vec::new();
    let mut unlisted = Vec::new();
    for (name, value) in dump {
        if !in_family(name) {
            continue;
        }
        if let Some(expected) = generated.get(name) {
            if expected != value {
                mismatches.push((name.clone(), value.clone(), expected.clone()));
            }
        } else if !header_only.contains_key(name) {
            unlisted.push(name.clone());
        }
    }
    (mismatches, unlisted)
}

/// Parse the C36 errno source table and compare it with the installed header.
///
/// Returns `(numeric_count, alias_count)` on success.
pub fn errno_parity() -> Result<(usize, usize), String> {
    let toml_path = repo_root().join("libc/ravenlibc/abi/errno.toml");
    let toml_text = std::fs::read_to_string(&toml_path)
        .map_err(|error| format!("cannot read {}: {error}", toml_path.display()))?;
    let mut expected = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    for line in toml_text.lines() {
        let Some(start) = line.find("{ name = \"") else {
            continue;
        };
        let rest = &line[start + 10..];
        let Some(end) = rest.find('"') else {
            continue;
        };
        let name = &rest[..end];
        if let Some(value_start) = line.find("value = ") {
            let tail = &line[value_start + 8..];
            let digits: String = tail
                .chars()
                .take_while(|character| character.is_ascii_digit())
                .collect();
            if digits.is_empty() {
                return Err(format!("cannot parse the value of {name} in errno.toml"));
            }
            expected.insert(name.to_string(), digits);
        } else if let Some(alias_start) = line.find("alias = \"") {
            let tail = &line[alias_start + 9..];
            let Some(end) = tail.find('"') else {
                continue;
            };
            aliases.insert(name.to_string(), tail[..end].to_string());
        }
    }
    let header_path = repo_root().join("libc/ravenlibc/include/bits/errno.h");
    let header_text = std::fs::read_to_string(&header_path)
        .map_err(|error| format!("cannot read {}: {error}", header_path.display()))?;
    let mut actual = BTreeMap::new();
    let mut actual_aliases = BTreeMap::new();
    for line in header_text.lines() {
        let Some(rest) = line.strip_prefix("#define ") else {
            continue;
        };
        let Some((name, value)) = split_define(rest) else {
            continue;
        };
        if !name.starts_with('E') {
            continue;
        }
        if value.chars().all(|character| character.is_ascii_digit()) {
            actual.insert(name.to_string(), value.to_string());
        } else {
            actual_aliases.insert(name.to_string(), value.to_string());
        }
    }
    if expected != actual {
        let mut message = String::from("errno numbers differ from abi/errno.toml:");
        for (name, value) in &expected {
            match actual.get(name) {
                Some(other) if other == value => {}
                Some(other) => message.push_str(&format!(" {name}: {other} != {value};")),
                None => message.push_str(&format!(" {name}: missing;")),
            }
        }
        for name in actual.keys() {
            if !expected.contains_key(name) {
                message.push_str(&format!(" {name}: unexpected;"));
            }
        }
        return Err(message);
    }
    if aliases != actual_aliases {
        return Err(format!(
            "errno aliases differ: {:?} != {:?}",
            aliases, actual_aliases
        ));
    }
    if expected.contains_key("EWOULDBLOCK") || expected.contains_key("EDEADLOCK") {
        return Err("the alias slots 41/58 must not be numeric rows".to_string());
    }
    Ok((expected.len(), aliases.len()))
}

/// Count of the Linux errno numbers 1..133 after removing the 41/58 gaps.
#[must_use]
pub fn linux_errno_count() -> usize {
    (1_u32..=133)
        .filter(|value| *value != 41 && *value != 58)
        .count()
}

#[cfg(test)]
#[path = "consts_ut.rs"]
mod tests;
