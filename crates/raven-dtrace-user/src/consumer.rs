// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Consumer state machine and record formatting.

use core::fmt;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::abi::{CpuStats, ProbeInfo, Record};

/// The syscall backend the consumer drives.
///
/// The target implementation is [`crate::sys::SyscallKernel`]; the host tests use a fake, so the
/// consumer logic runs without a kernel.
pub trait TraceKernel {
    /// List probes; returns the total number and fills `out` up to its length.
    fn probe_list(&self, out: &mut [ProbeInfo]) -> Result<usize, Errno>;
    /// Enable one probe, optionally with a wakeup event handle.
    fn enable(&self, probe: u32, event: Handle) -> Result<(), Errno>;
    /// Disable one probe.
    fn disable(&self, probe: u32) -> Result<(), Errno>;
    /// Read at most `out.len()` records; returns the count.
    fn read(&self, out: &mut [Record]) -> Result<usize, Errno>;
    /// Read per-CPU statistics; returns the number of CPUs represented.
    fn stats(&self, out: &mut [CpuStats]) -> Result<usize, Errno>;
    /// Reset the rings; the caller disables every probe first.
    fn clear(&self) -> Result<(), Errno>;
}

/// A DTrace consumer over one kernel backend.
pub struct Consumer<K: TraceKernel> {
    kernel: K,
}

impl<K: TraceKernel> Consumer<K> {
    /// Wrap a kernel backend.
    pub const fn new(kernel: K) -> Self {
        Self { kernel }
    }

    /// List the registered probes.
    pub fn list_probes(&self, out: &mut [ProbeInfo]) -> Result<usize, Errno> {
        self.kernel.probe_list(out)
    }

    /// Enable a probe without a wakeup event.
    pub fn enable(&self, probe: u32) -> Result<(), Errno> {
        self.kernel.enable(probe, Handle::NONE)
    }

    /// Enable a probe and signal `event` when a ring becomes non-empty.
    pub fn enable_with_event(&self, probe: u32, event: Handle) -> Result<(), Errno> {
        self.kernel.enable(probe, event)
    }

    /// Disable a probe.
    pub fn disable(&self, probe: u32) -> Result<(), Errno> {
        self.kernel.disable(probe)
    }

    /// Read one batch of records.
    pub fn read(&self, out: &mut [Record]) -> Result<usize, Errno> {
        self.kernel.read(out)
    }

    /// Read one batch of per-CPU statistics.
    pub fn stats(&self, out: &mut [CpuStats]) -> Result<usize, Errno> {
        self.kernel.stats(out)
    }

    /// Reset the rings.
    pub fn clear(&self) -> Result<(), Errno> {
        self.kernel.clear()
    }

    /// Read up to `batches` batches and visit every record.
    pub fn drain(
        &self,
        buffer: &mut [Record],
        batches: usize,
        mut visit: impl FnMut(&Record),
    ) -> Result<usize, Errno> {
        let mut total = 0usize;
        for _ in 0..batches {
            let count = self.read(buffer)?;
            if count == 0 {
                break;
            }
            for record in buffer.iter().take(count) {
                visit(record);
                total = total.saturating_add(1);
            }
        }
        Ok(total)
    }
}

/// Format one record as a single line.
pub fn format_record<W: fmt::Write>(record: &Record, writer: &mut W) -> fmt::Result {
    write!(
        writer,
        "cpu={} probe={} tid={} syscall={} ts={} args={:#x},{:#x},{:#x},{:#x}",
        record.cpu,
        record.probe,
        record.tid,
        record.syscall,
        record.timestamp_ns,
        record.arg0,
        record.arg1,
        record.arg2,
        record.arg3
    )
}

#[cfg(test)]
#[path = "consumer_ut.rs"]
mod tests;
