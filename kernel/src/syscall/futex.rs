// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Private futexes keyed by `(address-space id, user virtual address)`.
//!
//! There are 256 hash buckets, each a [`WaitQueue`]. `WAIT` checks the user word and enqueues the
//! caller under the bucket lock, so a concurrent `WAKE` either observes the waiter or the user
//! changed the word first (and the waiter returns `-EAGAIN`). The timeout is registered in the
//! sorted sleep list and cancelled by token when the wait finishes.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::futex;
use raven_usercopy::UserSlice;

use crate::process;
use crate::sched;
use crate::sched::wait::WaitQueue;
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::copy_from_user;

/// Number of futex hash buckets.
const BUCKETS: usize = 256;

static QUEUES: [WaitQueue; BUCKETS] = [const { WaitQueue::new() }; BUCKETS];
static FALLBACK: WaitQueue = WaitQueue::new();

/// `futex(addr, op, val, timeout_ns)`.
pub fn dispatch(frame: &SyscallFrame) -> Result<usize, Errno> {
    let addr = frame.rdi;
    let op = frame.rsi as u32;
    let val = frame.rdx as u32;
    let timeout_ns = frame.r10;
    let process = process::current().ok_or(Errno::ESRCH)?;

    if addr & 3 != 0 {
        return Err(Errno::EINVAL);
    }
    let slice = UserSlice::new(addr, 4)?;
    match op {
        futex::WAIT => wait(&process, slice, addr, val, timeout_ns),
        futex::WAKE => {
            let max = val.min(i32::MAX as u32) as usize;
            let count = bucket(process.id, addr).wake(process.id, addr, max);
            Ok(count)
        }
        _ => Err(Errno::EINVAL),
    }
}

fn wait(
    process: &Arc<process::Process>,
    slice: UserSlice,
    addr: u64,
    expected: u32,
    timeout_ns: u64,
) -> Result<usize, Errno> {
    let thread_id = sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let token = sched::begin_block()?;
    if timeout_ns != 0 {
        let deadline = time::now_ns().saturating_add(timeout_ns);
        sched::add_sleep(token, deadline, Errno::ETIMEDOUT.to_ret());
    }

    let queue = bucket(process.id, addr);
    let queued = queue.enqueue_if(process.id, addr, || {
        let mut bytes = [0u8; 4];
        copy_from_user(&mut bytes, slice)?;
        Ok(u32::from_le_bytes(bytes) == expected)
    });
    match queued {
        Ok(true) => {}
        Ok(false) => {
            sched::end_block(token);
            return Err(Errno::EAGAIN);
        }
        Err(error) => {
            sched::end_block(token);
            return Err(error);
        }
    }

    let status = sched::block_current();
    queue.remove(process.id, addr, thread_id);
    sched::end_block(token);
    if status == 0 {
        Ok(0)
    } else {
        Err(Errno::ETIMEDOUT)
    }
}

fn bucket(space: u64, addr: u64) -> &'static WaitQueue {
    let mixed = addr.rotate_left(17) ^ space.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let index = (mixed as usize) & (BUCKETS - 1);
    QUEUES.get(index).unwrap_or(&FALLBACK)
}

const _: () = assert!(BUCKETS == 256);
