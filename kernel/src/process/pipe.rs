// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel pipe endpoints and the 64 KiB ring behind them.
//!
//! A pipe is one shared ring plus two endpoint handles. Every `Object::Pipe` clone is one live
//! handle: `PROC_FORK`, `HANDLE_DUP` and `HANDLE_TRANSFER` just clone the `Arc`, so the reader and
//! writer counts follow the handle table rather than a second descriptor table. Dropping the last
//! write end wakes readers with EOF; dropping the last read end wakes writers with EPIPE.
//!
//! Lock order: the pipe state mutex is a leaf. Read/write paths release it before waking a wait
//! queue, and `WaitQueue::enqueue_if` takes it under the wait-queue lock, so the two locks never
//! nest in opposite orders. User copies happen with the state lock held but never sleep.

use alloc::sync::Arc;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_usercopy::UserSlice;
use spin::Mutex;

use crate::event::evq_core::{HUP, READABLE, WRITABLE};
use crate::process;
use crate::sched;
use crate::sched::wait::WaitQueue;
use crate::usercopy::{copy_from_user, copy_to_user};

// The ring's wrap arithmetic is bounded by the fixed capacity and cannot overflow.
#[allow(clippy::arithmetic_side_effects)]
#[path = "../../../crates/raven-ipc/src/pipe_ring.rs"]
pub(crate) mod ring;

/// Pipe ring capacity in bytes.
#[allow(dead_code)] // part of the primitive's documented contract; F_GETPIPE_SZ arrives later
pub const PIPE_CAPACITY: usize = ring::PIPE_CAPACITY;
/// `O_NONBLOCK` (Linux/libc value; the native ABI reuses the descriptor flags).
pub const O_NONBLOCK: u32 = 0o4000;
/// `O_CLOEXEC` (Linux/libc value; no exec exists yet, so it is recorded for the exec ticket).
pub const O_CLOEXEC: u32 = 0o2000000;

/// POSIX `SIGPIPE`.
const SIGPIPE: u32 = 13;
/// Wait-queue key for readers of one pipe.
const WAIT_READ: u64 = 1;
/// Wait-queue key for writers of one pipe.
const WAIT_WRITE: u64 = 2;

/// Next pipe id (the wait-queue key space).
static NEXT_PIPE_ID: AtomicU64 = AtomicU64::new(1);

/// One end of a pipe.
///
/// Cloning `Object::Pipe` clones this `Arc`; the last handle clone drops the endpoint, which
/// decrements the reader or writer count exactly once. `nonblock` and `cloexec` are captured at
/// `pipe2` time because the descriptor is immutable until `fcntl`/`exec` land.
pub struct PipeEnd {
    pipe: Arc<Pipe>,
    writer: bool,
    nonblock: bool,
    cloexec: bool,
}

impl PipeEnd {
    /// Watcher key shared by both ends of this pipe.
    pub(crate) fn event_key(&self) -> u64 {
        crate::event::pipe_key(self.pipe.id)
    }

    /// True for the read end.
    pub const fn is_reader(&self) -> bool {
        !self.writer
    }

    /// True when the end was created with `O_NONBLOCK`.
    #[allow(dead_code)] // F_GETFL will report it when fcntl lands
    pub const fn is_nonblock(&self) -> bool {
        self.nonblock
    }

    /// True when the end records `O_CLOEXEC`.
    #[allow(dead_code)] // consumed by the future exec close-on-exec sweep
    pub const fn is_cloexec(&self) -> bool {
        self.cloexec
    }

    /// Current EVQ readiness bits for this pipe end.
    ///
    /// Read end: data is `READABLE`; a closed writer is `READABLE|HUP` (a read returns EOF);
    /// otherwise it is not ready. Write end: no reader is `HUP`; any free ring space is
    /// `WRITABLE`; a full ring is not ready.
    pub fn readiness(&self) -> u32 {
        let state = self.pipe.state.lock();
        if self.writer {
            if state.read_ends == 0 {
                return HUP;
            }
            return if state.ring.free() != 0 { WRITABLE } else { 0 };
        }
        let mut events = 0;
        if !state.ring.is_empty() {
            events |= READABLE;
        }
        if state.write_ends == 0 {
            events |= READABLE | HUP;
        }
        events
    }

    /// Wake and remove every pipe waiter owned by `process_id`.
    ///
    /// A thread blocked in `read`/`write` holds this endpoint's `Arc` and a wait-queue entry;
    /// process teardown must wake it with `EIO`, otherwise the entry, the thread and its kernel
    /// stack survive the process.  The other end of the pipe is untouched.
    pub fn wake_process(&self, process_id: u64) {
        let _ = self.pipe.readers.wake_process(process_id);
        let _ = self.pipe.writers.wake_process(process_id);
    }

    /// True when both handles refer to the same pipe ring.
    pub fn same_pipe(&self, other: &PipeEnd) -> bool {
        Arc::ptr_eq(&self.pipe, &other.pipe)
    }

    /// Release the 64 KiB ring when no live process holds either end.
    ///
    /// A blocked reader/writer leaves an endpoint `Arc` on an abandoned kernel stack.  The
    /// endpoint structure itself is small, but the ring must not stay allocated after the last
    /// live process reference is gone.
    pub fn retire(&self) {
        let mut state = self.pipe.state.lock();
        state.read_ends = 0;
        state.write_ends = 0;
        if let Some(empty) = ring::Ring::new(1) {
            state.ring = empty;
        }
        drop(state);
        crate::event::notify_source(self.event_key());
    }
}

impl Drop for PipeEnd {
    fn drop(&mut self) {
        let (wake_writers, wake_readers) = {
            let mut state = self.pipe.state.lock();
            if self.writer {
                state.write_ends = state.write_ends.saturating_sub(1);
            } else {
                state.read_ends = state.read_ends.saturating_sub(1);
            }
            (state.read_ends == 0, state.write_ends == 0)
        };
        if wake_writers {
            self.pipe.writers.wake_all();
        }
        if wake_readers {
            self.pipe.readers.wake_all();
        }
        crate::event::notify_source(self.event_key());
    }
}

/// The shared pipe state.
struct Pipe {
    id: u64,
    state: Mutex<PipeState>,
    readers: WaitQueue,
    writers: WaitQueue,
}

struct PipeState {
    ring: ring::Ring,
    read_ends: u32,
    write_ends: u32,
}

/// Create one pipe and its two endpoints.
///
/// `flags` accepts only [`O_CLOEXEC`] and [`O_NONBLOCK`]. `O_NONBLOCK` applies to both ends, as
/// on Linux; `O_CLOEXEC` is recorded per endpoint for a future exec.
pub fn create(flags: u32) -> Result<(Arc<PipeEnd>, Arc<PipeEnd>), Errno> {
    if flags & !(O_CLOEXEC | O_NONBLOCK) != 0 {
        return Err(Errno::EINVAL);
    }
    let ring = ring::Ring::pipe().ok_or(Errno::ENOMEM)?;
    let id = NEXT_PIPE_ID.fetch_add(1, Ordering::Relaxed);
    let pipe = Arc::new(Pipe {
        id,
        state: Mutex::new(PipeState {
            ring,
            read_ends: 1,
            write_ends: 1,
        }),
        readers: WaitQueue::new(),
        writers: WaitQueue::new(),
    });
    let nonblock = flags & O_NONBLOCK != 0;
    let cloexec = flags & O_CLOEXEC != 0;
    let reader = Arc::new(PipeEnd {
        pipe: Arc::clone(&pipe),
        writer: false,
        nonblock,
        cloexec,
    });
    let writer = Arc::new(PipeEnd {
        pipe,
        writer: true,
        nonblock,
        cloexec,
    });
    Ok((reader, writer))
}

/// Read one chunk from the read end.
///
/// Returns at most `destination.length()` bytes and never blocks when data is already buffered.
/// An empty ring with live writers blocks (or `EAGAIN` with `O_NONBLOCK`); an empty ring with no
/// writers returns 0 (EOF).
pub fn read(end: &Arc<PipeEnd>, destination: UserSlice) -> Result<usize, Errno> {
    if end.writer {
        return Err(Errno::EBADF);
    }
    if destination.is_empty() {
        return Ok(0);
    }
    let limit = usize::try_from(destination.length()).map_err(|_| Errno::EINVAL)?;
    loop {
        let mut state = end.pipe.state.lock();
        if state.ring.is_empty() {
            if state.write_ends == 0 {
                return Ok(0);
            }
            if end.nonblock {
                return Err(Errno::EAGAIN);
            }
            drop(state);
            // `enqueue_if` runs its condition under the wait-queue lock and enqueues when it is
            // true: "still empty and a writer may arrive" is exactly the wait predicate.
            wait(&end.pipe.readers, end.pipe.id, WAIT_READ, || {
                let state = end.pipe.state.lock();
                Ok(state.ring.is_empty() && state.write_ends != 0)
            })?;
            continue;
        }
        let want = state.ring.len().min(limit);
        let (first, second) = state.ring.read_parts();
        let first_take = want.min(first.len());
        if let Some(bytes) = first.get(..first_take) {
            copy_out(destination.address(), bytes)?;
        }
        let second_take = want.saturating_sub(first_take).min(second.len());
        if second_take != 0
            && let Some(bytes) = second.get(..second_take)
        {
            copy_out(
                destination.address().saturating_add(first_take as u64),
                bytes,
            )?;
        }
        let copied = first_take.saturating_add(second_take);
        state.ring.consume(copied);
        drop(state);
        end.pipe.writers.wake_all();
        crate::event::notify_source(end.event_key());
        return Ok(copied);
    }
}

/// Write one buffer to the write end.
///
/// A blocking write fills the ring and waits for readers until every byte is written. A
/// non-blocking write returns the bytes that fitted (or `EAGAIN` when none did). A pipe with no
/// read end queues `SIGPIPE` on the caller and returns `EPIPE`.
pub fn write(end: &Arc<PipeEnd>, source: UserSlice) -> Result<usize, Errno> {
    if !end.writer {
        return Err(Errno::EBADF);
    }
    if source.is_empty() {
        return Ok(0);
    }
    let total = usize::try_from(source.length()).map_err(|_| Errno::EINVAL)?;
    let mut done = 0usize;
    loop {
        let mut state = end.pipe.state.lock();
        if state.read_ends == 0 {
            drop(state);
            raise_sigpipe();
            return Err(Errno::EPIPE);
        }
        let free = state.ring.free();
        if free == 0 {
            if end.nonblock {
                return if done == 0 {
                    Err(Errno::EAGAIN)
                } else {
                    Ok(done)
                };
            }
            drop(state);
            // Same predicate direction as the read side: wait while the ring is full and a
            // reader may still drain it. A kick after a partial write reports the bytes already
            // accepted rather than discarding them.
            if let Err(error) = wait(&end.pipe.writers, end.pipe.id, WAIT_WRITE, || {
                let state = end.pipe.state.lock();
                Ok(state.ring.free() == 0 && state.read_ends != 0)
            }) {
                return if error == Errno::EINTR && done != 0 {
                    Ok(done)
                } else {
                    Err(error)
                };
            }
            continue;
        }
        let chunk = total.saturating_sub(done).min(free);
        let (first, second) = state.ring.write_parts();
        let first_take = chunk.min(first.len());
        if let Some(bytes) = first.get_mut(..first_take) {
            copy_in(bytes, source.address().saturating_add(done as u64))?;
        }
        let second_take = chunk.saturating_sub(first_take).min(second.len());
        if second_take != 0
            && let Some(bytes) = second.get_mut(..second_take)
        {
            copy_in(
                bytes,
                source
                    .address()
                    .saturating_add(done.saturating_add(first_take) as u64),
            )?;
        }
        let copied = first_take.saturating_add(second_take);
        state.ring.produce(copied);
        drop(state);
        end.pipe.readers.wake_all();
        crate::event::notify_source(end.event_key());
        done = done.saturating_add(copied);
        if done >= total {
            return Ok(done);
        }
        if end.nonblock {
            return Ok(done);
        }
    }
}

fn copy_out(address: u64, bytes: &[u8]) -> Result<(), Errno> {
    if bytes.is_empty() {
        return Ok(());
    }
    let length = u64::try_from(bytes.len()).map_err(|_| Errno::EINVAL)?;
    copy_to_user(UserSlice::new(address, length)?, bytes)
}

fn copy_in(destination: &mut [u8], address: u64) -> Result<(), Errno> {
    let length = u64::try_from(destination.len()).map_err(|_| Errno::EINVAL)?;
    copy_from_user(destination, UserSlice::new(address, length)?)
}

/// Queue `SIGPIPE` on the calling process. The kernel keeps dispositions in userspace, so the
/// signal is queued for `PROC_SIGRECV` rather than terminating the process here.
fn raise_sigpipe() {
    if let Some(caller) = process::current() {
        caller.queue_signal(SIGPIPE, 0);
    }
}

/// Block the current thread while `condition` is true.
///
/// Mirrors the futex/wait pattern: `enqueue_if` runs the condition under the wait-queue lock and
/// enqueues only when it is true; the caller is removed after the wake, and an exit kick
/// surfaces as `EINTR`.
fn wait(
    queue: &WaitQueue,
    space: u64,
    addr: u64,
    condition: impl FnOnce() -> Result<bool, Errno>,
) -> Result<(), Errno> {
    let thread_id = sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let token = sched::begin_block_interruptible()?;
    if sched::take_kick() {
        sched::end_block(token);
        return Err(Errno::EINTR);
    }
    match queue.enqueue_if(space, addr, condition) {
        Ok(true) => {}
        Ok(false) => {
            sched::end_block(token);
            return Ok(());
        }
        Err(error) => {
            sched::end_block(token);
            return Err(error);
        }
    }
    let status = sched::block_current();
    let kicked = sched::take_kick();
    queue.remove(space, addr, thread_id);
    sched::end_block(token);
    if status == Errno::EINTR.to_ret() || kicked {
        Err(Errno::EINTR)
    } else if status == Errno::EFAULT.to_ret() {
        Err(Errno::EFAULT)
    } else {
        Ok(())
    }
}
