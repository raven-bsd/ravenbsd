// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Private and shared futexes.
//!
//! Private waits (flag [`futex::PRIVATE`]) use `(pid, user virtual address)`. Shared waits resolve
//! the VMA and, for a shared memory-object mapping, use `(object id, byte offset)` so two mappings
//! of the same object at different VAs share a queue; anonymous/private VMAs keep the private key.
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
/// High bit tagging shared `(object id, offset)` keys apart from private `(pid, vaddr)` keys.
const SHARED_KEY_TAG: u64 = 1 << 63;

static QUEUES: [WaitQueue; BUCKETS] = [const { WaitQueue::new() }; BUCKETS];
static FALLBACK: WaitQueue = WaitQueue::new();

/// `futex(addr, op, val, arg4, addr2, val3)`.
pub fn dispatch(frame: &SyscallFrame) -> Result<usize, Errno> {
    let addr = frame.rdi;
    let op = frame.rsi as u32;
    let val = frame.rdx as u32;
    let timeout_ns = frame.r10;
    let process = process::current().ok_or(Errno::ESRCH)?;

    if addr & 3 != 0 {
        return Err(Errno::EINVAL);
    }
    let private = op & futex::PRIVATE != 0;
    let command = op & !futex::PRIVATE;
    let slice = UserSlice::new(addr, 4)?;
    let (space, key) = futex_key(&process, addr, private)?;
    match command {
        futex::WAIT => wait(slice, space, key, val, timeout_ns),
        futex::WAKE => {
            let max = val.min(i32::MAX as u32) as usize;
            let count = bucket(space, key).wake(space, key, max);
            Ok(count)
        }
        futex::REQUEUE => requeue(frame, &process, private, space, key, val),
        _ => Err(Errno::EINVAL),
    }
}

/// `FUTEX_REQUEUE`: wake `val` waiters at the source, move up to `arg4` of the rest to `addr2`.
fn requeue(
    frame: &SyscallFrame,
    process: &Arc<process::Process>,
    private: bool,
    source_space: u64,
    source_key: u64,
    val: u32,
) -> Result<usize, Errno> {
    let destination = frame.r8;
    if destination & 3 != 0 {
        return Err(Errno::EINVAL);
    }
    // NetBSD `futex_func_requeue` rejects negative wake and requeue counts.
    let wake = val as i32;
    let max = (frame.r10 as u32) as i32;
    if wake < 0 || max < 0 {
        return Err(Errno::EINVAL);
    }
    let _destination_slice = UserSlice::new(destination, 4)?;
    let (destination_space, destination_key) = futex_key(process, destination, private)?;
    Ok(bucket(source_space, source_key).requeue(
        (source_space, source_key),
        bucket(destination_space, destination_key),
        (destination_space, destination_key),
        wake as usize,
        max as usize,
    ))
}

/// Resolve the wait-queue key for `address`.
fn futex_key(
    process: &Arc<process::Process>,
    address: u64,
    private: bool,
) -> Result<(u64, u64), Errno> {
    if private {
        return Ok((process.id, address));
    }
    let space = process.address_space.lock();
    let space = space.as_ref().ok_or(Errno::EINVAL)?;
    match space.shared_futex_key(address)? {
        Some((object, offset)) => Ok((SHARED_KEY_TAG | object, offset)),
        None => Ok((process.id, address)),
    }
}

fn wait(
    slice: UserSlice,
    space: u64,
    key: u64,
    expected: u32,
    timeout_ns: u64,
) -> Result<usize, Errno> {
    let thread_id = sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let token = sched::begin_block_interruptible()?;
    if sched::take_kick() {
        sched::end_block(token);
        return Err(Errno::EINTR);
    }
    if timeout_ns != 0 {
        let deadline = time::now_ns().saturating_add(timeout_ns);
        sched::add_sleep(token, deadline, Errno::ETIMEDOUT.to_ret());
    }

    let queue = bucket(space, key);
    let queued = queue.enqueue_if(space, key, || {
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
    let kicked = sched::take_kick();
    // A requeue can move this waiter to a different bucket, so the queue and key captured before
    // the block no longer identify its entry. Remove it by thread id from every futex bucket.
    remove_thread(thread_id);
    sched::end_block(token);
    if status == Errno::EINTR.to_ret() || kicked {
        Err(Errno::EINTR)
    } else if status == Errno::EFAULT.to_ret() {
        Err(Errno::EFAULT)
    } else if status == 0 {
        Ok(0)
    } else {
        Err(Errno::ETIMEDOUT)
    }
}

/// Wake up to `max` waiters on `address` in `process` (robust-list walk).
pub fn wake_process_address(process: &Arc<process::Process>, address: u64, max: usize) -> usize {
    let Ok((space, key)) = futex_key(process, address, false) else {
        return 0;
    };
    bucket(space, key).wake(space, key, max)
}

/// Drop every futex waiter owned by `thread_id`.
pub fn remove_thread(thread_id: u64) {
    for queue in QUEUES.iter() {
        queue.remove_thread(thread_id);
    }
    FALLBACK.remove_thread(thread_id);
}

/// True when `thread_id` still has a futex entry (S36 holder diagnostic).
pub fn contains_thread(thread_id: u64) -> bool {
    QUEUES.iter().any(|queue| queue.contains_thread(thread_id))
        || FALLBACK.contains_thread(thread_id)
}

/// Non-blocking [`contains_thread`] for the metrics-thread census.
pub fn try_contains_thread(thread_id: u64) -> Option<bool> {
    let mut busy = false;
    for queue in QUEUES.iter() {
        match queue.try_contains_thread(thread_id) {
            Some(true) => return Some(true),
            Some(false) => {}
            None => busy = true,
        }
    }
    match FALLBACK.try_contains_thread(thread_id) {
        Some(true) => Some(true),
        Some(false) if !busy => Some(false),
        _ => None,
    }
}

/// Wake and remove every futex waiter owned by `process_id` (process teardown).
pub fn remove_process(process_id: u64) {
    for queue in QUEUES.iter() {
        let _ = queue.wake_process(process_id);
    }
    let _ = FALLBACK.wake_process(process_id);
}

/// Live futex waiters across all buckets (S36 timeout-retention diagnostic).
pub fn waiters_len() -> usize {
    let mut total = 0usize;
    for queue in QUEUES.iter() {
        total = total.saturating_add(queue.len());
    }
    total.saturating_add(FALLBACK.len())
}

/// Wake private-key waiters in `[start, end)` after an unmap.
pub fn remove_private_range(process_id: u64, start: u64, end: u64) {
    for queue in QUEUES.iter() {
        let _ = queue.wake_private_range(process_id, start, end);
    }
    let _ = FALLBACK.wake_private_range(process_id, start, end);
}

/// Wake shared-key waiters of `object_id` in `[start, end)` after an unmap.
pub fn remove_shared_range(process_id: u64, object_id: u64, start: u64, end: u64) {
    let object_space = SHARED_KEY_TAG | object_id;
    for queue in QUEUES.iter() {
        let _ = queue.wake_shared_range(process_id, object_space, start, end);
    }
    let _ = FALLBACK.wake_shared_range(process_id, object_space, start, end);
}

/// Wake every futex waiter on `(space, addr)`.
///
/// Used by `THREAD_EXIT`/`THREAD_EXIT_EX` after the clear-on-exit word is
/// stored; the address key matches the private-futex bucket used by `WAIT`.
pub fn wake_address(space: u64, addr: u64) {
    let _ = bucket(space, addr).wake(space, addr, usize::MAX);
}

fn bucket(space: u64, addr: u64) -> &'static WaitQueue {
    let mixed = addr.rotate_left(17) ^ space.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let index = (mixed as usize) & (BUCKETS - 1);
    QUEUES.get(index).unwrap_or(&FALLBACK)
}

const _: () = assert!(BUCKETS == 256);
