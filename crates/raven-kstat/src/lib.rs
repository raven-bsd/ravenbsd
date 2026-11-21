// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Typed kstat snapshots over the existing bounded DTrace statistics syscall.
//!
//! The record model is shared with the kernel tree (`kernel/src/kstat/mod.rs`): stable
//! `module:instance:name` names, Illumos `KSTAT_DATA_*` types, units and fixed-capacity
//! snapshots. The first consumer is the existing `DTRACE_STATS` syscall (0x94), which already
//! provides a caller-bounded per-CPU `CpuStats` read. No kernel syscall is added here.

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]

#[path = "../../../kernel/src/kstat/mod.rs"]
mod model;

pub use model::{
    Error, KSTAT_STRLEN, Kstat, KstatName, KstatType, KstatUnit, Provider, Snapshot, Text, Value,
    checked_sum,
};

use core::fmt;

use raven_dtrace_user::abi::CpuStats;

/// Maximum number of CPUs the DTrace provider reads.
pub const MAX_CPUS: usize = 8;

/// Snapshot capacity used by the CLI: four counters per CPU, four global sums and the CPU count.
pub const CLI_CAPACITY: usize = MAX_CPUS * 4 + 5;

/// The fixed instance names for the DTrace per-CPU records.
const CPU_INSTANCES: [&str; MAX_CPUS] = [
    "cpu0", "cpu1", "cpu2", "cpu3", "cpu4", "cpu5", "cpu6", "cpu7",
];

/// The kernel statistics source shared by the real syscall and the host tests.
pub trait KernelStats {
    /// Fill `out` with per-CPU statistics; returns the number of CPUs represented.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Kernel`] when the kernel read fails.
    fn cpu_stats(&self, out: &mut [CpuStats]) -> Result<usize, Error>;
}

/// The DTrace per-CPU ring statistics provider.
pub struct DtraceProvider<K: KernelStats> {
    kernel: K,
}

impl<K: KernelStats> DtraceProvider<K> {
    /// Wrap one kernel statistics source.
    #[must_use]
    pub const fn new(kernel: K) -> Self {
        Self { kernel }
    }

    /// Fill a typed snapshot.
    ///
    /// # Errors
    ///
    /// Returns a model error, [`Error::Full`] when `out` is too small, or [`Error::Kernel`]
    /// when the backing read fails.
    pub fn snapshot<const N: usize>(&self, out: &mut Snapshot<N>) -> Result<(), Error> {
        <Self as Provider<N>>::snapshot(self, out)
    }
}

impl<K: KernelStats, const N: usize> Provider<N> for DtraceProvider<K> {
    fn snapshot(&self, out: &mut Snapshot<N>) -> Result<(), Error> {
        let mut stats = [CpuStats::default(); MAX_CPUS];
        let reported = self.kernel.cpu_stats(&mut stats)?;
        let cpus = reported.min(stats.len());
        for (index, entry) in stats.iter().take(cpus).enumerate() {
            let instance = CPU_INSTANCES.get(index).copied().ok_or(Error::Full)?;
            push_u64(out, "dtrace", instance, "produced", entry.produced)?;
            push_u64(out, "dtrace", instance, "consumed", entry.consumed)?;
            push_u64(out, "dtrace", instance, "dropped", entry.dropped)?;
            push_u64(out, "dtrace", instance, "capacity", entry.capacity)?;
        }
        let produced = checked_sum(stats.iter().take(cpus).map(|entry| entry.produced))?;
        let consumed = checked_sum(stats.iter().take(cpus).map(|entry| entry.consumed))?;
        let dropped = checked_sum(stats.iter().take(cpus).map(|entry| entry.dropped))?;
        let capacity = checked_sum(stats.iter().take(cpus).map(|entry| entry.capacity))?;
        push_u64(out, "dtrace", "global", "produced", produced)?;
        push_u64(out, "dtrace", "global", "consumed", consumed)?;
        push_u64(out, "dtrace", "global", "dropped", dropped)?;
        push_u64(out, "dtrace", "global", "capacity", capacity)?;
        push_u64(
            out,
            "dtrace",
            "global",
            "cpus",
            u64::try_from(cpus).map_err(|_| Error::Overflow)?,
        )?;
        Ok(())
    }
}

fn push_u64<const N: usize>(
    out: &mut Snapshot<N>,
    module: &str,
    instance: &str,
    name: &str,
    value: u64,
) -> Result<(), Error> {
    let name = KstatName::new(
        Text::parse_name(module)?,
        Text::parse_name(instance)?,
        Text::parse_name(name)?,
    )?;
    out.push(Kstat::new(name, Value::Uint64(value), KstatUnit::Count))
}

/// Parsed `kstat` command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// `-n`/`--ndjson`: emit one JSON object per line.
    pub ndjson: bool,
    /// Optional exact `module:instance:name` filter.
    pub filter: Option<KstatName>,
    /// `-h`/`--help`.
    pub help: bool,
}

/// Why the command line was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgError {
    /// An unknown option.
    Unknown,
    /// More than one name was supplied.
    DuplicateName,
    /// A malformed name.
    InvalidName,
}

/// Parse the arguments after the program name.
///
/// # Errors
///
/// Returns [`ArgError::Unknown`] for an unknown option, [`ArgError::DuplicateName`] for a second
/// name and [`ArgError::InvalidName`] for a malformed name.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, ArgError> {
    let mut options = Options {
        ndjson: false,
        filter: None,
        help: false,
    };
    for argument in args {
        match argument {
            "-n" | "--ndjson" => options.ndjson = true,
            "-h" | "--help" => options.help = true,
            _ if argument.starts_with('-') => return Err(ArgError::Unknown),
            _ => {
                if options.filter.is_some() {
                    return Err(ArgError::DuplicateName);
                }
                options.filter =
                    Some(KstatName::parse(argument).map_err(|_| ArgError::InvalidName)?);
            }
        }
    }
    Ok(options)
}

/// Format one human-readable line per selected record.
///
/// # Errors
///
/// Returns the writer's [`fmt::Error`].
pub fn format_human<W: fmt::Write, const N: usize>(
    snapshot: &Snapshot<N>,
    filter: Option<&KstatName>,
    out: &mut W,
) -> fmt::Result {
    for record in snapshot.iter() {
        if !selected(record, filter) {
            continue;
        }
        writeln!(
            out,
            "kstat: name={} type={} unit={} value={}",
            record.name(),
            record.kind().as_str(),
            record.unit().as_str(),
            record.value()
        )?;
    }
    Ok(())
}

/// Format one NDJSON object per selected record.
///
/// # Errors
///
/// Returns the writer's [`fmt::Error`].
pub fn format_ndjson<W: fmt::Write, const N: usize>(
    snapshot: &Snapshot<N>,
    filter: Option<&KstatName>,
    out: &mut W,
) -> fmt::Result {
    for record in snapshot.iter() {
        if !selected(record, filter) {
            continue;
        }
        write!(
            out,
            "{{\"name\":\"{}\",\"type\":\"{}\",\"unit\":\"{}\",\"value\":",
            record.name(),
            record.kind().as_str(),
            record.unit().as_str()
        )?;
        match record.value() {
            Value::Text(text) => write_json_string(out, text.as_str())?,
            value => write!(out, "{value}")?,
        }
        writeln!(out, "}}")?;
    }
    Ok(())
}

fn selected(record: &Kstat, filter: Option<&KstatName>) -> bool {
    filter.is_none_or(|name| record.name() == name)
}

fn write_json_string<W: fmt::Write>(out: &mut W, text: &str) -> fmt::Result {
    out.write_char('"')?;
    for character in text.chars() {
        match character {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            '\n' => out.write_str("\\n")?,
            '\r' => out.write_str("\\r")?,
            '\t' => out.write_str("\\t")?,
            character if (character as u32) < 0x20 => {
                write!(out, "\\u{:04x}", character as u32)?;
            }
            character => out.write_char(character)?,
        }
    }
    out.write_char('"')
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
