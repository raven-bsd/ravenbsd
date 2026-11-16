// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pipe(2)` and `pipe2(2)`.
//!
//! Both numbers are kernel-owned raw calls because `crates/raven-abi` is a contract crate. The
//! descriptor pair is a pair of ordinary handles in the caller's existing handle table; there is
//! no second descriptor table.

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_usercopy::UserSlice;

use crate::process::{self, handle::Object, pipe};
use crate::syscall::SyscallFrame;
use crate::usercopy::copy_to_user;

/// Raw `pipe(fds) -> 0`.
pub const PIPE: usize = 0x6e;
/// Raw `pipe2(fds, flags) -> 0`.
pub const PIPE2: usize = 0x6f;

/// Rights on the read handle: data access plus duplication and transfer.
const READ_RIGHTS: Rights = Rights(Rights::READ.0 | Rights::DUP.0 | Rights::TRANSFER.0);
/// Rights on the write handle.
const WRITE_RIGHTS: Rights = Rights(Rights::WRITE.0 | Rights::DUP.0 | Rights::TRANSFER.0);

/// `pipe(fds)`.
pub fn pipe(frame: &SyscallFrame) -> Result<usize, Errno> {
    create(frame.rdi, 0)
}

/// `pipe2(fds, flags)`.
pub fn pipe2(frame: &SyscallFrame) -> Result<usize, Errno> {
    let flags = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    create(frame.rdi, flags)
}

fn create(pointer: u64, flags: u32) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let destination = UserSlice::new(pointer, 8)?;
    let (reader, writer) = pipe::create(flags)?;
    let (read_handle, write_handle) = {
        let mut handles = caller.handles.lock();
        let read_handle = handles.insert(Object::Pipe(reader), READ_RIGHTS)?;
        match handles.insert(Object::Pipe(writer), WRITE_RIGHTS) {
            Ok(write_handle) => (read_handle, write_handle),
            Err(error) => {
                let _ = handles.close(read_handle);
                return Err(error);
            }
        }
    };
    let mut bytes = [0u8; 8];
    let (read, write) = bytes.split_at_mut(4);
    read.copy_from_slice(&read_handle.0.to_le_bytes());
    write.copy_from_slice(&write_handle.0.to_le_bytes());
    if let Err(error) = copy_to_user(destination, &bytes) {
        // The caller never saw the handles; close both so the endpoints cannot leak.
        let mut handles = caller.handles.lock();
        let _ = handles.close(write_handle);
        let _ = handles.close(read_handle);
        return Err(error);
    }
    Ok(0)
}
