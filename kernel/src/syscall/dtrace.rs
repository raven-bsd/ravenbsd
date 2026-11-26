// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `DTRACE_*` syscalls.
//!
//! The probe state and per-CPU rings live in `kernel/src/dtrace/`, included here as `provider` so
//! the syscall module owns the only wiring point in this file. Numbers are raw constants until the
//! reviewer assigns them in `raven-abi` (the ticket does not own the contract crate).

#[path = "../dtrace/mod.rs"]
mod provider;

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::process;
use crate::process::handle::Object;
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

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

/// Largest number of records one `DTRACE_READ` returns.
const MAX_READ_RECORDS: usize = 1024;

/// True for a DTrace control syscall. The syscall probe skips these.
#[must_use]
pub fn is_control(number: usize) -> bool {
    matches!(number, PROBE_LIST | ENABLE | DISABLE | READ | STATS | CLEAR)
}

/// Fire the syscall-entry probe. Called by the native dispatcher before decoding the call.
pub fn syscall_probe(frame: &SyscallFrame) {
    provider::probe_syscall(frame);
}

/// Dispatch one `DTRACE_*` control syscall.
///
/// Every state-changing control (`ENABLE`, `DISABLE`, `READ`, `STATS`, `CLEAR`) requires effective
/// uid 0 before the handler runs, so a denied caller cannot touch probe state or a ring. The
/// read-only `PROBE_LIST` stays available: it returns static probe ids/providers/names and never
/// a cross-process record. This mirrors the Illumos `dtrace_ioctl` `PRIV_DTRACE_*` gate.
pub fn dispatch(number: usize, frame: &SyscallFrame) -> Result<usize, Errno> {
    if number != PROBE_LIST {
        super::basic::require_root()?;
    }
    match number {
        PROBE_LIST => probe_list(frame),
        ENABLE => enable(frame),
        DISABLE => disable(frame),
        READ => read(frame),
        STATS => stats(frame),
        CLEAR => clear(),
        _ => Err(Errno::ENOSYS),
    }
}

fn probe_list(frame: &SyscallFrame) -> Result<usize, Errno> {
    let out = frame.rdi;
    let max = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let probes = provider::probes();
    if max == 0 {
        return Ok(probes.len());
    }
    let count = max.min(probes.len());
    let bytes = count
        .checked_mul(core::mem::size_of::<provider::model::ProbeInfo>())
        .ok_or(Errno::EINVAL)?;
    if out == 0 {
        return Err(Errno::EFAULT);
    }
    let destination = UserSlice::new(out, bytes as u64)?;
    let mut infos = Vec::new();
    infos.try_reserve_exact(count).map_err(|_| Errno::ENOMEM)?;
    for probe in probes.iter().take(count) {
        infos.push(provider::probe_info(probe));
    }
    // SAFETY: `infos` holds `count` initialised records and the byte view covers exactly them.
    let source = unsafe { core::slice::from_raw_parts(infos.as_ptr().cast::<u8>(), bytes) };
    copy_to_user(destination, source)?;
    Ok(probes.len())
}

fn enable(frame: &SyscallFrame) -> Result<usize, Errno> {
    let probe = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    let flags = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if flags != 0 {
        return Err(Errno::EINVAL);
    }
    let handle = Handle(u32::try_from(frame.rdx).map_err(|_| Errno::EBADF)?);
    let event = if handle == Handle::NONE {
        None
    } else {
        let caller = process::current().ok_or(Errno::ESRCH)?;
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::SIGNAL) {
            Ok(Object::UserEvent(event)) => Some(Arc::clone(event)),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    provider::enable(probe, event)?;
    Ok(0)
}

fn disable(frame: &SyscallFrame) -> Result<usize, Errno> {
    let probe = u32::try_from(frame.rdi).map_err(|_| Errno::EINVAL)?;
    provider::disable(probe)?;
    Ok(0)
}

fn read(frame: &SyscallFrame) -> Result<usize, Errno> {
    let out = frame.rdi;
    let max = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if max == 0 {
        return Ok(0);
    }
    if max > MAX_READ_RECORDS {
        return Err(Errno::EINVAL);
    }
    let bytes = max
        .checked_mul(core::mem::size_of::<provider::model::Record>())
        .ok_or(Errno::EINVAL)?;
    let destination = UserSlice::new(out, bytes as u64)?;
    let mut records = Vec::new();
    records.try_reserve_exact(max).map_err(|_| Errno::ENOMEM)?;
    let count = provider::drain(max, &mut records);
    if count == 0 {
        return Ok(0);
    }
    let length = count
        .checked_mul(core::mem::size_of::<provider::model::Record>())
        .ok_or(Errno::EINVAL)?;
    // SAFETY: `records` holds `count` initialised records and the byte view covers exactly them.
    let source = unsafe { core::slice::from_raw_parts(records.as_ptr().cast::<u8>(), length) };
    copy_to_user(destination, source)?;
    Ok(count)
}

fn stats(frame: &SyscallFrame) -> Result<usize, Errno> {
    let out = frame.rdi;
    let max = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let cpus = crate::sched::smp::online_cpus().min(provider::MAX_DTRACE_CPUS);
    if max == 0 {
        return Ok(cpus);
    }
    let count = max.min(cpus);
    let bytes = count
        .checked_mul(core::mem::size_of::<provider::model::CpuStats>())
        .ok_or(Errno::EINVAL)?;
    if out == 0 {
        return Err(Errno::EFAULT);
    }
    let destination = UserSlice::new(out, bytes as u64)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(|_| Errno::ENOMEM)?;
    entries.resize(count, provider::model::CpuStats::default());
    let filled = provider::stats(&mut entries);
    let length = filled
        .checked_mul(core::mem::size_of::<provider::model::CpuStats>())
        .ok_or(Errno::EINVAL)?;
    // SAFETY: `entries[..filled]` is initialised and the byte view covers exactly it.
    let source = unsafe { core::slice::from_raw_parts(entries.as_ptr().cast::<u8>(), length) };
    copy_to_user(destination, source)?;
    Ok(cpus)
}

fn clear() -> Result<usize, Errno> {
    provider::clear()?;
    Ok(0)
}
