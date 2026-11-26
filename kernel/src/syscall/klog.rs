// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `KLOG_READ` (`0x0E`): copy complete kernel log ring records to user space.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::klog::{HEADER_LEN, KlogInfo, LOST, MORE, VERSION};
use raven_abi::types::{Handle, Rights};
use raven_usercopy::UserSlice;

use crate::log::{RECORD_HEADER_LEN, RECORD_MAX_BODY, RING};
use crate::process;
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

/// Maximum bytes in one complete record.
const MAX_RECORD: usize = RECORD_HEADER_LEN + RECORD_MAX_BODY;
/// Size of the ABI result record.
const INFO_LEN: usize = core::mem::size_of::<KlogInfo>();

/// Read kernel log records from `cursor` into the caller's buffer.
///
/// Requires `ADMIN` on the root device capability. Only complete records are copied; a record
/// that does not fit leaves `next_cursor` at its start and sets `MORE`. A cursor older than the
/// oldest retained record is clamped to `first_cursor` and sets `LOST`.
pub fn read(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let permitted = caller
        .handles
        .lock()
        .get(Handle::ROOT_DEVICE, Rights::ADMIN)
        .is_ok();
    if !permitted {
        return Err(Errno::EACCES);
    }

    let cursor = frame.rdi;
    let len = usize::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    if len < HEADER_LEN {
        return Err(Errno::ENOBUFS);
    }
    if frame.r10 == 0 {
        return Err(Errno::EINVAL);
    }
    // Validate both user ranges before touching the ring. A bad info pointer is EINVAL by
    // contract; a bad data buffer is the usual EFAULT.
    let info_slice = UserSlice::new(frame.r10, INFO_LEN as u64).map_err(|_| Errno::EINVAL)?;
    let _buffer = UserSlice::new(frame.rsi, len as u64)?;

    let (data, next_cursor, first_cursor, last_cursor, dropped, lost, more) = {
        let ring = RING.lock();
        let plan = ring.plan_read(cursor, len);
        let mut data = Vec::new();
        data.try_reserve_exact(plan.bytes)
            .map_err(|_| Errno::ENOMEM)?;
        let mut position = plan.start;
        while position < plan.next {
            let size = ring.record_size(position).ok_or(Errno::EIO)?;
            let mut record = [0u8; MAX_RECORD];
            let Some(slot) = record.get_mut(..size) else {
                return Err(Errno::EIO);
            };
            ring.copy_record(position, slot).ok_or(Errno::EIO)?;
            data.extend_from_slice(slot);
            position = position.saturating_add(size as u64);
        }
        (
            data,
            plan.next,
            ring.first_cursor(),
            ring.last_cursor(),
            ring.dropped(),
            plan.lost,
            plan.more,
        )
    };

    // Copy outside the ring lock: a user fault must never run with interrupts disabled.
    if !data.is_empty() {
        let destination = UserSlice::new(frame.rsi, data.len() as u64)?;
        copy_to_user(destination, &data)?;
    }

    let mut flags = 0;
    if lost {
        flags |= LOST;
    }
    if more {
        flags |= MORE;
    }
    let info = KlogInfo {
        version: VERSION,
        flags,
        next_cursor,
        dropped,
        first_cursor,
        last_cursor,
    };
    // SAFETY: `info` is a live stack value and the slice covers exactly its bytes.
    let bytes = unsafe {
        core::slice::from_raw_parts(
            core::ptr::addr_of!(info).cast::<u8>(),
            core::mem::size_of::<KlogInfo>(),
        )
    };
    copy_to_user(info_slice, bytes)?;
    Ok(data.len())
}
