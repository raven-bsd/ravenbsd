// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The S57 `DTRACE_*` syscall numbers and wire layouts.
//!
//! The wire records mirror `raven-dtrace-core` (the kernel includes that source); the const
//! assertions below fail the build if the userland copy drifts.

/// One trace record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// Monotonic record sequence number.
    pub sequence: u64,
    /// Kernel monotonic timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// Probe argument 0.
    pub arg0: u64,
    /// Probe argument 1.
    pub arg1: u64,
    /// Probe argument 2.
    pub arg2: u64,
    /// Probe argument 3.
    pub arg3: u64,
    /// Native syscall number for the syscall provider.
    pub syscall: u32,
    /// Thread identifier of the probe site.
    pub tid: u32,
    /// CPU index that produced the record.
    pub cpu: u16,
    /// Probe identifier.
    pub probe: u16,
}

/// One listed probe.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProbeInfo {
    /// Stable probe identifier.
    pub id: u32,
    /// Provider name, NUL-padded.
    pub provider: [u8; 16],
    /// Probe name, NUL-padded.
    pub name: [u8; 32],
}

/// Per-CPU ring statistics.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuStats {
    /// Records accepted by the ring.
    pub produced: u64,
    /// Records handed to the consumer.
    pub consumed: u64,
    /// Records refused because the ring was full.
    pub dropped: u64,
    /// Ring capacity in records.
    pub capacity: u64,
}

/// `dtrace_probe_list(out: *mut ProbeInfo, max: usize) -> count`.
pub const PROBE_LIST: usize = 0x90;
/// `dtrace_enable(probe: u32, flags: u32, event: Handle) -> 0`.
pub const ENABLE: usize = 0x91;
/// `dtrace_disable(probe: u32) -> 0`.
pub const DISABLE: usize = 0x92;
/// `dtrace_read(out: *mut Record, max: usize) -> count`.
pub const READ: usize = 0x93;
/// `dtrace_stats(out: *mut CpuStats, max: usize) -> cpus`.
pub const STATS: usize = 0x94;
/// `dtrace_clear() -> 0`.
pub const CLEAR: usize = 0x95;

const _: () = assert!(core::mem::size_of::<Record>() == 64);
const _: () = assert!(core::mem::size_of::<ProbeInfo>() == 52);
const _: () = assert!(core::mem::size_of::<CpuStats>() == 32);

/// The provider name of a listed probe, or an empty string when it is not valid UTF-8.
#[must_use]
pub fn provider(info: &ProbeInfo) -> &str {
    text(&info.provider)
}

/// The probe name of a listed probe, or an empty string when it is not valid UTF-8.
#[must_use]
pub fn name(info: &ProbeInfo) -> &str {
    text(&info.name)
}

fn text(bytes: &[u8]) -> &str {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    core::str::from_utf8(bytes.get(..end).unwrap_or(&[])).unwrap_or("")
}

#[cfg(test)]
#[path = "abi_ut.rs"]
mod tests;
