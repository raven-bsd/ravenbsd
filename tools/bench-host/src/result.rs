// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Result schema, host description and result-file I/O.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::stats::{Stats, summarize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct HostInfo {
    pub(crate) cpu: String,
    pub(crate) kernel: String,
    pub(crate) qemu: String,
    pub(crate) kvm: bool,
    pub(crate) governor: String,
    pub(crate) arch: String,
    pub(crate) cpus: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct BenchResult {
    pub(crate) name: String,
    pub(crate) unit: String,
    pub(crate) n: usize,
    pub(crate) median: f64,
    pub(crate) p99: f64,
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) host: HostInfo,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) note: Option<String>,
}

impl BenchResult {
    pub(crate) fn from_samples(
        name: &str,
        unit: &str,
        samples: &[f64],
        note: Option<String>,
    ) -> Self {
        let Stats {
            n,
            median,
            p99,
            min,
            max,
        } = summarize(samples);
        Self {
            name: name.to_string(),
            unit: unit.to_string(),
            n,
            median,
            p99,
            min,
            max,
            host: host_info(),
            note,
        }
    }

    pub(crate) fn write(&self) -> Result<PathBuf> {
        let directory = results_dir();
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create {}", directory.display()))?;
        let path = directory.join(format!("{}.json", self.name));
        let text = serde_json::to_string_pretty(self)?;
        fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
        Ok(path)
    }

    pub(crate) fn print(&self) {
        if self.n == 0 {
            println!(
                "{}: not measurable{}",
                self.name,
                self.note
                    .as_deref()
                    .map(|note| format!(" ({note})"))
                    .unwrap_or_default()
            );
        } else {
            println!(
                "{}: median {:.1} {}, p99 {:.1} {}, n={}",
                self.name, self.median, self.unit, self.p99, self.unit, self.n
            );
        }
    }
}

pub(crate) fn results_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("results")
}

pub(crate) fn read_result(name: &str) -> Result<Option<BenchResult>> {
    let path = results_dir().join(format!("{name}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let result = serde_json::from_str(&text)
        .with_context(|| format!("invalid result JSON {}", path.display()))?;
    Ok(Some(result))
}

pub(crate) fn read_all_results() -> Result<Vec<BenchResult>> {
    let directory = results_dir();
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut results: Vec<BenchResult> = Vec::new();
    for entry in
        fs::read_dir(&directory).with_context(|| format!("cannot read {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        results.push(
            serde_json::from_str(&text)
                .with_context(|| format!("invalid result JSON {}", path.display()))?,
        );
    }
    results.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(results)
}

pub(crate) fn host_info() -> HostInfo {
    HostInfo {
        cpu: first_cpu_model(),
        kernel: read_trim("/proc/sys/kernel/osrelease"),
        qemu: qemu_version(),
        kvm: kvm_accessible(),
        governor: read_trim("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"),
        arch: std::env::consts::ARCH.to_string(),
        cpus: cpu_count(),
    }
}

fn cpu_count() -> usize {
    fs::read_to_string("/proc/cpuinfo")
        .map(|text| {
            text.lines()
                .filter(|line| line.starts_with("processor"))
                .count()
                .max(1)
        })
        .unwrap_or(1)
}

fn first_cpu_model() -> String {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix("model name"))
                .and_then(|value| value.split_once(':'))
                .map(|(_, value)| value.trim().to_string())
        })
        .unwrap_or_else(|| String::from("unknown"))
}

fn qemu_version() -> String {
    Command::new("qemu-system-x86_64")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from("not installed"))
}

fn kvm_accessible() -> bool {
    fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .is_ok()
}

fn read_trim(path: &str) -> String {
    fs::read_to_string(Path::new(path))
        .map(|text| text.trim().to_string())
        .unwrap_or_else(|_| String::from("unknown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_json_has_the_contract_fields() {
        let result = BenchResult::from_samples("x", "ns", &[1.0, 2.0, 3.0], Some("note".into()));
        let value = serde_json::to_value(&result).unwrap();
        for key in [
            "name", "unit", "n", "median", "p99", "min", "max", "host", "note",
        ] {
            assert!(value.get(key).is_some(), "missing {key}");
        }
        assert_eq!(value["name"], "x");
        assert_eq!(value["n"], 3);
        assert_eq!(value["unit"], "ns");
        assert_eq!(value["note"], "note");
    }

    #[test]
    fn result_without_note_omits_it() {
        let result = BenchResult::from_samples("x", "ns", &[1.0], None);
        let value = serde_json::to_value(&result).unwrap();
        assert!(value.get("note").is_none());
    }
}
