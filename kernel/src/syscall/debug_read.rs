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
use raven_abi::types::prot;
use raven_usercopy::UserSlice;

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::console;
#[cfg(target_arch = "x86_64")]
use crate::console;
use crate::process;
use crate::sched;
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::copy_to_user;

/// Largest amount copied per call; a bigger `len` yields a short read, never a longer one.
const CHUNK: usize = 256;

/// User page granularity used by the writable-range walk.
const PAGE: u64 = 4096;

/// True when every page of `address..address + length` is covered by a writable user VMA.
///
/// `UserSlice` only proves that the range lies inside the user window. An unmapped or
/// `PROT_NONE` address still faults in `copy_to_user` after the console ring has been drained,
/// which would lose the bytes. The K516 contract requires `EFAULT` to leave queued input
/// untouched, so the writable mapping is checked before the ring is consumed. This is
/// deliberately stricter than NetBSD `ttread`, which takes the byte with `getc` before
/// `ureadc` reports the fault.
fn destination_writable(process: &process::Process, address: u64, length: u64) -> bool {
    if length == 0 {
        return true;
    }
    let Some(end) = address.checked_add(length) else {
        return false;
    };
    let space = process.address_space.lock();
    let Some(space) = space.as_ref() else {
        return false;
    };
    let mut page = address & !(PAGE - 1);
    while page < end {
        let Some(vma) = space.vmas().find(page) else {
            return false;
        };
        if vma.prot() & (prot::WRITE as u8) == 0 {
            return false;
        }
        page = page.saturating_add(PAGE);
    }
    true
}

/// `debug_read(buf: *mut u8, len: usize, timeout_ns: u64) -> isize`.
pub fn handle(frame: &SyscallFrame) -> Result<usize, Errno> {
    let length = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    if length == 0 {
        return Ok(0);
    }
    let destination = UserSlice::new(frame.rdi, u64::try_from(length).map_err(|_| Errno::EINVAL)?)?;
    let current = process::current().ok_or(Errno::ESRCH)?;
    let chunk = length.min(CHUNK);
    if !destination_writable(
        &current,
        destination.address(),
        u64::try_from(chunk).map_err(|_| Errno::EINVAL)?,
    ) {
        return Err(Errno::EFAULT);
    }
    let timeout_ns = frame.rdx;
    let thread_id = sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let deadline = (timeout_ns != 0).then(|| time::now_ns().saturating_add(timeout_ns));
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
        if current.has_pending_signal() {
            sched::end_block(token);
            return Err(Errno::EINTR);
        }
        if let Some(deadline) = deadline {
            sched::add_sleep(token, deadline, Errno::ETIMEDOUT.to_ret());
        }
        let queued = console::waiters().enqueue_if(0, 0, || Ok(!console::available()));
        match queued {
            Ok(true) => {
                // A queued process signal wakes this wait; the signal itself stays queued for
                // `PROC_SIGRECV`, and the console bytes stay in the ring because nothing has
                // been copied yet.
                let signal_queued = current
                    .signal_waiters
                    .enqueue_if(current.id, 0, || Ok(!current.has_pending_signal()));
                match signal_queued {
                    Ok(true) => {}
                    Ok(false) => {
                        console::waiters().remove(0, 0, thread_id);
                        sched::end_block(token);
                        return Err(Errno::EINTR);
                    }
                    Err(error) => {
                        console::waiters().remove(0, 0, thread_id);
                        sched::end_block(token);
                        return Err(error);
                    }
                }
                let status = sched::block_current();
                let kicked = sched::take_kick();
                console::waiters().remove(0, 0, thread_id);
                current.signal_waiters.remove(current.id, 0, thread_id);
                sched::end_block(token);
                if status == Errno::EINTR.to_ret() || kicked {
                    return Err(Errno::EINTR);
                }
                if current.has_pending_signal() {
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
