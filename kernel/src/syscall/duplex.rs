// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Raw duplex endpoint syscalls (`0x110`/`0x111`).
//!
//! The numbers stay kernel-owned until the reviewer promotes the contract into `raven-abi`.
//! Both endpoints live in the caller's existing handle table; there is no userspace server.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_usercopy::UserSlice;

use crate::process::{self, duplex, handle::Object};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

/// Raw `duplex_pair(out: *mut [u32; 2], flags: u32) -> 0`.
pub const DUPLEX_PAIR: usize = 0x110;
/// Raw `duplex_shutdown(handle: u32, how: u32) -> 0`.
pub const DUPLEX_SHUTDOWN: usize = 0x111;

/// Both endpoints carry data access plus duplication and transfer.
const END_RIGHTS: Rights =
    Rights(Rights::READ.0 | Rights::WRITE.0 | Rights::DUP.0 | Rights::TRANSFER.0);

/// `duplex_pair(out, flags)`.
pub fn pair(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let destination = UserSlice::new(frame.rdi, 8)?;
    let flags = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let (left, right) = duplex::create(flags)?;
    let (left_handle, right_handle) = {
        let mut handles = caller.handles.lock();
        let left_handle = handles.insert(Object::Duplex(left), END_RIGHTS)?;
        match handles.insert(Object::Duplex(right), END_RIGHTS) {
            Ok(right_handle) => (left_handle, right_handle),
            Err(error) => {
                let _ = handles.close(left_handle);
                return Err(error);
            }
        }
    };
    let mut bytes = [0u8; 8];
    let (left, right) = bytes.split_at_mut(4);
    left.copy_from_slice(&left_handle.0.to_le_bytes());
    right.copy_from_slice(&right_handle.0.to_le_bytes());
    if let Err(error) = copy_to_user(destination, &bytes) {
        // The caller never saw the handles; close both so the endpoints cannot leak.
        let mut handles = caller.handles.lock();
        let _ = handles.close(right_handle);
        let _ = handles.close(left_handle);
        return Err(error);
    }
    Ok(0)
}

/// `duplex_shutdown(handle, how)`.
pub fn shutdown(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = raven_abi::types::Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let how = u8::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let needed = match how {
        duplex::SHUT_RD => Rights::READ,
        duplex::SHUT_WR => Rights::WRITE,
        duplex::SHUT_RDWR => Rights::READ.union(Rights::WRITE),
        _ => return Err(Errno::EINVAL),
    };
    let end = match caller.handles.lock().get(handle, needed)? {
        Object::Duplex(end) => Arc::clone(end),
        _ => return Err(Errno::EBADF),
    };
    duplex::shutdown(&end, how)?;
    Ok(0)
}
