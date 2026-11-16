// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The target syscall backend.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::sys::syscall6;

use crate::abi::{CLEAR, CpuStats, DISABLE, ENABLE, PROBE_LIST, ProbeInfo, READ, Record, STATS};
use crate::consumer::TraceKernel;

/// The native `DTRACE_*` syscall backend.
pub struct SyscallKernel;

impl TraceKernel for SyscallKernel {
    fn probe_list(&self, out: &mut [ProbeInfo]) -> Result<usize, Errno> {
        // SAFETY: the kernel validates the user range and copies at most `out.len()` records.
        Errno::from_ret(unsafe {
            syscall6(PROBE_LIST, out.as_mut_ptr() as usize, out.len(), 0, 0, 0, 0)
        })
    }

    fn enable(&self, probe: u32, event: Handle) -> Result<(), Errno> {
        // SAFETY: scalar arguments; the kernel validates the probe id and the event handle.
        Errno::from_ret(unsafe { syscall6(ENABLE, probe as usize, 0, event.0 as usize, 0, 0, 0) })
            .map(|_| ())
    }

    fn disable(&self, probe: u32) -> Result<(), Errno> {
        // SAFETY: scalar argument; the kernel validates the probe id.
        Errno::from_ret(unsafe { syscall6(DISABLE, probe as usize, 0, 0, 0, 0, 0) }).map(|_| ())
    }

    fn read(&self, out: &mut [Record]) -> Result<usize, Errno> {
        // SAFETY: the kernel validates the user range and copies at most `out.len()` records.
        Errno::from_ret(unsafe { syscall6(READ, out.as_mut_ptr() as usize, out.len(), 0, 0, 0, 0) })
    }

    fn stats(&self, out: &mut [CpuStats]) -> Result<usize, Errno> {
        // SAFETY: the kernel validates the user range and copies at most `out.len()` entries.
        Errno::from_ret(unsafe {
            syscall6(STATS, out.as_mut_ptr() as usize, out.len(), 0, 0, 0, 0)
        })
    }

    fn clear(&self) -> Result<(), Errno> {
        // SAFETY: DTRACE_CLEAR takes no arguments.
        Errno::from_ret(unsafe { syscall6(CLEAR, 0, 0, 0, 0, 0, 0) }).map(|_| ())
    }
}
