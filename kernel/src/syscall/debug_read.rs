// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `DEBUG_READ`: raw console input.
//!
//! Bytes are delivered exactly as received (no CR translation, no line buffering).  A reader
//! blocks on the console wait queue until at least one byte is available, then copies up to
//! `len` bytes in one chunk and returns the count.  Several readers may block; the receive path
//! wakes one at a time and every waiter re-checks the ring, so a wakeup cannot be lost and no
//! byte is handed to two readers.

use raven_abi::errno::Errno;
use raven_usercopy::UserSlice;

use crate::console;
use crate::sched;
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::copy_to_user;

/// Largest amount copied per call; a bigger `len` yields a short read, never a longer one.
const CHUNK: usize = 256;

/// `debug_read(buf: *mut u8, len: usize, timeout_ns: u64) -> isize`.
pub fn handle(frame: &SyscallFrame) -> Result<usize, Errno> {
    let length = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if length == 0 {
        return Ok(0);
    }
    let destination = UserSlice::new(frame.rdi, u64::try_from(length).map_err(|_| Errno::EINVAL)?)?;
    let timeout_ns = frame.rdx;
    let thread_id = sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let deadline = (timeout_ns != 0).then(|| time::now_ns().saturating_add(timeout_ns));
    let chunk = length.min(CHUNK);
    let mut buffer = [0u8; CHUNK];

    loop {
        // The overrun report is emitted here, in process context; the ISR never logs.
        console::log_dropped();
        let count = console::read_into(buffer.get_mut(..chunk).ok_or(Errno::EINVAL)?);
        if count > 0 {
            let bytes = buffer.get(..count).ok_or(Errno::EFAULT)?;
            copy_to_user(destination, bytes)?;
            console::log_dropped();
            return Ok(count);
        }
        if let Some(deadline) = deadline
            && time::now_ns() >= deadline
        {
            return Ok(0);
        }
        let token = sched::begin_block_interruptible()?;
        if sched::take_kick() {
            sched::end_block(token);
            return Err(Errno::EINTR);
        }
        if let Some(deadline) = deadline {
            sched::add_sleep(token, deadline, Errno::ETIMEDOUT.to_ret());
        }
        let queued = console::waiters().enqueue_if(0, 0, || Ok(!console::available()));
        match queued {
            Ok(true) => {
                let status = sched::block_current();
                let kicked = sched::take_kick();
                console::waiters().remove(0, 0, thread_id);
                sched::end_block(token);
                if status == Errno::EINTR.to_ret() || kicked {
                    return Err(Errno::EINTR);
                }
                if status == Errno::ETIMEDOUT.to_ret() {
                    return Ok(0);
                }
            }
            Ok(false) => {
                sched::end_block(token);
            }
            Err(error) => {
                sched::end_block(token);
                return Err(error);
            }
        }
    }
}
