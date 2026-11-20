// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel duplex endpoint pairs.
//!
//! A duplex pair is two endpoint handles over one shared object. Each endpoint can read and
//! write; the object keeps one FIFO in each direction, so the two directions never share a queue.
//! Every `Object::Duplex` clone is one live handle. The last handle for a side closes that side:
//! the peer reads EOF and writes fail with `EPIPE`.
//!
//! Byte-stream mode uses the same 64 KiB ring as the pipe primitive. Datagram mode keeps a bounded
//! queue of messages so reads preserve boundaries. Both modes use explicit shutdown flags; read
//! and write paths release the state lock before waking a wait queue.
//!
//! Lock order: the state mutex is a leaf. Read/write paths release it before waking a wait queue,
//! and the wait-queue condition acquires it under the wait-queue lock. User copies happen with the
//! state lock held but never sleep.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_usercopy::UserSlice;
use spin::Mutex;

use crate::event;
use crate::event::evq_core::{HUP, READABLE, WRITABLE};
use crate::process;
use crate::process::pipe::ring;
use crate::sched;
use crate::sched::wait::WaitQueue;
use crate::usercopy::{copy_from_user, copy_to_user};

/// Byte-stream capacity in bytes.
#[allow(dead_code)] // part of the documented contract; a future F_GETPIPE_SZ-style query uses it
pub const DUPLEX_CAPACITY: usize = ring::PIPE_CAPACITY;
/// Datagram queue capacity in bytes.
pub const DUPLEX_DATAGRAM_CAPACITY: usize = 64 * 1024;
/// Largest single datagram.
pub const DUPLEX_DATAGRAM_MAX: usize = 64 * 1024;
/// Maximum number of queued datagrams.
pub const DUPLEX_DATAGRAM_MAX_MESSAGES: usize = 64;
/// Select datagram mode in the pair flags.
pub const DUPLEX_DATAGRAM: u32 = 1;
/// Non-blocking endpoint flag (the native descriptor flag value).
pub const O_NONBLOCK: u32 = 0o4000;
/// Close-on-exec endpoint flag (recorded for the future exec sweep).
pub const O_CLOEXEC: u32 = 0o2000000;

/// `shutdown(SHUT_RD)`.
pub const SHUT_RD: u8 = 0;
/// `shutdown(SHUT_WR)`.
pub const SHUT_WR: u8 = 1;
/// `shutdown(SHUT_RDWR)`.
pub const SHUT_RDWR: u8 = 2;

/// POSIX `SIGPIPE`.
const SIGPIPE: u32 = 13;
/// Wait-queue key for readers of one direction.
const WAIT_READ: u64 = 1;
/// Wait-queue key for writers of one direction.
const WAIT_WRITE: u64 = 2;

/// Next pair id (the wait-queue key space).
static NEXT_DUPLEX_ID: AtomicU64 = AtomicU64::new(1);

/// One side of a duplex pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The first endpoint returned in the pair.
    Left,
    /// The second endpoint returned in the pair.
    Right,
}

impl Side {
    /// The endpoint on the other end.
    pub const fn peer(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

/// One handle to a duplex endpoint.
pub struct DuplexEnd {
    pair: Arc<Duplex>,
    side: Side,
    nonblock: bool,
    cloexec: bool,
    datagram: bool,
}

impl DuplexEnd {
    /// True when the endpoint was created with `O_NONBLOCK`.
    #[allow(dead_code)] // F_GETFL reports it when fcntl lands
    pub const fn is_nonblock(&self) -> bool {
        self.nonblock
    }

    /// True when the endpoint records `O_CLOEXEC`.
    pub const fn is_cloexec(&self) -> bool {
        self.cloexec
    }

    /// Wake and remove every duplex waiter owned by `process_id`.
    pub fn wake_process(&self, process_id: u64) {
        let _ = self.pair.left_readers.wake_process(process_id);
        let _ = self.pair.left_writers.wake_process(process_id);
        let _ = self.pair.right_readers.wake_process(process_id);
        let _ = self.pair.right_writers.wake_process(process_id);
    }

    /// True when both handles belong to the same pair.
    pub fn same_pair(&self, other: &DuplexEnd) -> bool {
        Arc::ptr_eq(&self.pair, &other.pair)
    }

    /// Mark both sides closed and wake every waiter.
    ///
    /// Teardown calls this after the last live process reference is gone. A blocked syscall may
    /// still hold an endpoint `Arc`, so the buffers stay allocated until that frame returns.
    pub fn retire(&self) {
        {
            let mut state = self.pair.state.lock();
            state.left_ends = 0;
            state.right_ends = 0;
            state.left_read_shutdown = true;
            state.left_write_shutdown = true;
            state.right_read_shutdown = true;
            state.right_write_shutdown = true;
        }
        self.pair.wake_all();
        event::notify_all();
    }

    /// Current EVQ readiness bits for this endpoint.
    pub fn readiness(&self) -> u32 {
        let state = self.pair.state.lock();
        let peer = self.side.peer();
        let local_read_shutdown = state.read_shutdown(self.side);
        let local_write_shutdown = state.write_shutdown(self.side);
        let peer_read_shutdown = state.read_shutdown(peer);
        let peer_write_shutdown = state.write_shutdown(peer);
        let peer_ends = state.ends(peer);
        let incoming_empty = state.incoming(self.side).is_empty();
        let outgoing_writable = state.outgoing(self.side).writable();
        let mut events = 0;
        if local_read_shutdown || peer_write_shutdown || peer_ends == 0 || !incoming_empty {
            events |= READABLE;
        }
        if local_read_shutdown || peer_write_shutdown || peer_ends == 0 {
            events |= HUP;
        }
        if !local_write_shutdown && !peer_read_shutdown && peer_ends != 0 && outgoing_writable {
            events |= WRITABLE;
        }
        events
    }
}

impl Drop for DuplexEnd {
    fn drop(&mut self) {
        let (left, right) = {
            let mut state = self.pair.state.lock();
            match self.side {
                Side::Left => state.left_ends = state.left_ends.saturating_sub(1),
                Side::Right => state.right_ends = state.right_ends.saturating_sub(1),
            }
            (state.left_ends == 0, state.right_ends == 0)
        };
        // Closing the left side closes the peer's reader and writer; the reverse is symmetric.
        if left {
            self.pair.right_readers.wake_all();
            self.pair.right_writers.wake_all();
        }
        if right {
            self.pair.left_readers.wake_all();
            self.pair.left_writers.wake_all();
        }
        event::notify_all();
    }
}

struct Duplex {
    id: u64,
    state: Mutex<DuplexState>,
    left_readers: WaitQueue,
    left_writers: WaitQueue,
    right_readers: WaitQueue,
    right_writers: WaitQueue,
}

impl Duplex {
    fn readers(&self, side: Side) -> &WaitQueue {
        match side {
            Side::Left => &self.left_readers,
            Side::Right => &self.right_readers,
        }
    }

    fn writers(&self, side: Side) -> &WaitQueue {
        match side {
            Side::Left => &self.left_writers,
            Side::Right => &self.right_writers,
        }
    }

    fn wake_readers(&self, side: Side) {
        self.readers(side).wake_all();
    }

    fn wake_writers(&self, side: Side) {
        self.writers(side).wake_all();
    }

    fn wake_all(&self) {
        self.left_readers.wake_all();
        self.left_writers.wake_all();
        self.right_readers.wake_all();
        self.right_writers.wake_all();
    }
}

struct DuplexState {
    left_to_right: Buffer,
    right_to_left: Buffer,
    left_ends: u32,
    right_ends: u32,
    left_read_shutdown: bool,
    left_write_shutdown: bool,
    right_read_shutdown: bool,
    right_write_shutdown: bool,
}

impl DuplexState {
    fn ends(&self, side: Side) -> u32 {
        match side {
            Side::Left => self.left_ends,
            Side::Right => self.right_ends,
        }
    }

    fn read_shutdown(&self, side: Side) -> bool {
        match side {
            Side::Left => self.left_read_shutdown,
            Side::Right => self.right_read_shutdown,
        }
    }

    fn write_shutdown(&self, side: Side) -> bool {
        match side {
            Side::Left => self.left_write_shutdown,
            Side::Right => self.right_write_shutdown,
        }
    }

    fn set_read_shutdown(&mut self, side: Side) {
        match side {
            Side::Left => self.left_read_shutdown = true,
            Side::Right => self.right_read_shutdown = true,
        }
    }

    fn set_write_shutdown(&mut self, side: Side) {
        match side {
            Side::Left => self.left_write_shutdown = true,
            Side::Right => self.right_write_shutdown = true,
        }
    }

    /// The queue a read on `side` drains.
    fn incoming(&self, side: Side) -> &Buffer {
        match side {
            Side::Left => &self.right_to_left,
            Side::Right => &self.left_to_right,
        }
    }

    fn incoming_mut(&mut self, side: Side) -> &mut Buffer {
        match side {
            Side::Left => &mut self.right_to_left,
            Side::Right => &mut self.left_to_right,
        }
    }

    /// The queue a write on `side` fills.
    fn outgoing(&self, side: Side) -> &Buffer {
        match side {
            Side::Left => &self.left_to_right,
            Side::Right => &self.right_to_left,
        }
    }

    fn outgoing_mut(&mut self, side: Side) -> &mut Buffer {
        match side {
            Side::Left => &mut self.left_to_right,
            Side::Right => &mut self.right_to_left,
        }
    }
}

enum Buffer {
    Stream(ring::Ring),
    Datagram(DatagramBuffer),
}

impl Buffer {
    fn new(datagram: bool) -> Result<Self, Errno> {
        if datagram {
            Ok(Self::Datagram(DatagramBuffer {
                queue: VecDeque::new(),
                bytes: 0,
            }))
        } else {
            ring::Ring::pipe().map(Self::Stream).ok_or(Errno::ENOMEM)
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Stream(ring) => ring.is_empty(),
            Self::Datagram(queue) => queue.queue.is_empty(),
        }
    }

    fn writable(&self) -> bool {
        match self {
            Self::Stream(ring) => ring.free() != 0,
            Self::Datagram(queue) => {
                queue.queue.len() < DUPLEX_DATAGRAM_MAX_MESSAGES
                    && queue.bytes < DUPLEX_DATAGRAM_CAPACITY
            }
        }
    }

    fn datagram_fits(&self, length: usize) -> bool {
        match self {
            Self::Stream(_) => false,
            Self::Datagram(queue) => {
                queue.queue.len() < DUPLEX_DATAGRAM_MAX_MESSAGES
                    && queue.bytes.saturating_add(length) <= DUPLEX_DATAGRAM_CAPACITY
            }
        }
    }
}

struct DatagramBuffer {
    queue: VecDeque<Vec<u8>>,
    bytes: usize,
}

/// Create one duplex pair and its two endpoint handles.
///
/// The flags accept `DUPLEX_DATAGRAM`, `O_NONBLOCK` and `O_CLOEXEC`. Unknown flags return
/// `EINVAL`; failure to allocate either direction returns `ENOMEM`.
pub fn create(flags: u32) -> Result<(Arc<DuplexEnd>, Arc<DuplexEnd>), Errno> {
    if flags & !(DUPLEX_DATAGRAM | O_NONBLOCK | O_CLOEXEC) != 0 {
        return Err(Errno::EINVAL);
    }
    let datagram = flags & DUPLEX_DATAGRAM != 0;
    let left_to_right = Buffer::new(datagram)?;
    let right_to_left = Buffer::new(datagram)?;
    let id = NEXT_DUPLEX_ID.fetch_add(1, Ordering::Relaxed);
    let pair = Arc::new(Duplex {
        id,
        state: Mutex::new(DuplexState {
            left_to_right,
            right_to_left,
            left_ends: 1,
            right_ends: 1,
            left_read_shutdown: false,
            left_write_shutdown: false,
            right_read_shutdown: false,
            right_write_shutdown: false,
        }),
        left_readers: WaitQueue::new(),
        left_writers: WaitQueue::new(),
        right_readers: WaitQueue::new(),
        right_writers: WaitQueue::new(),
    });
    let nonblock = flags & O_NONBLOCK != 0;
    let cloexec = flags & O_CLOEXEC != 0;
    let left = Arc::new(DuplexEnd {
        pair: Arc::clone(&pair),
        side: Side::Left,
        nonblock,
        cloexec,
        datagram,
    });
    let right = Arc::new(DuplexEnd {
        pair,
        side: Side::Right,
        nonblock,
        cloexec,
        datagram,
    });
    Ok((left, right))
}

/// Read one chunk or one datagram from `end`.
///
/// A closed or read-shutdown endpoint returns 0. An empty queue with a live peer blocks, or
/// returns `EAGAIN` with `O_NONBLOCK`; an empty queue with no peer returns 0. A datagram read
/// copies at most `destination.length()` bytes of the front message and consumes the whole
/// message, so a short buffer truncates it.
pub fn read(end: &Arc<DuplexEnd>, destination: UserSlice) -> Result<usize, Errno> {
    if destination.is_empty() {
        return Ok(0);
    }
    let limit = usize::try_from(destination.length()).map_err(|_| Errno::EINVAL)?;
    loop {
        let mut state = end.pair.state.lock();
        if state.read_shutdown(end.side) {
            return Ok(0);
        }
        let peer = end.side.peer();
        if state.incoming(end.side).is_empty() {
            if state.write_shutdown(peer) || state.ends(peer) == 0 {
                return Ok(0);
            }
            if end.nonblock {
                return Err(Errno::EAGAIN);
            }
            drop(state);
            wait(end.pair.readers(end.side), end.pair.id, WAIT_READ, || {
                let state = end.pair.state.lock();
                Ok(state.incoming(end.side).is_empty()
                    && !state.read_shutdown(end.side)
                    && !state.write_shutdown(peer)
                    && state.ends(peer) != 0)
            })?;
            continue;
        }
        let incoming = state.incoming_mut(end.side);
        let copied = match incoming {
            Buffer::Stream(ring) => read_stream(ring, destination, limit)?,
            Buffer::Datagram(buffer) => read_datagram(buffer, destination, limit)?,
        };
        drop(state);
        end.pair.wake_writers(peer);
        event::notify_all();
        return Ok(copied);
    }
}

fn read_stream(
    ring: &mut ring::Ring,
    destination: UserSlice,
    limit: usize,
) -> Result<usize, Errno> {
    let want = ring.len().min(limit);
    let (first, second) = ring.read_parts();
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
    ring.consume(copied);
    Ok(copied)
}

fn read_datagram(
    buffer: &mut DatagramBuffer,
    destination: UserSlice,
    limit: usize,
) -> Result<usize, Errno> {
    let Some(message) = buffer.queue.front() else {
        return Ok(0);
    };
    let copied = message.len().min(limit);
    if let Some(bytes) = message.get(..copied) {
        copy_out(destination.address(), bytes)?;
    }
    let length = message.len();
    if buffer.queue.pop_front().is_some() {
        buffer.bytes = buffer.bytes.saturating_sub(length);
    }
    Ok(copied)
}

/// Write one buffer to `end`.
///
/// A stream write fills the 64 KiB ring and waits for the peer to drain it. A datagram write
/// queues one complete message atomically. A write after local write shutdown, peer read shutdown
/// or peer close queues `SIGPIPE` and returns `EPIPE`.
pub fn write(end: &Arc<DuplexEnd>, source: UserSlice) -> Result<usize, Errno> {
    if source.is_empty() {
        return Ok(0);
    }
    if end.datagram {
        return write_datagram(end, source);
    }
    write_stream(end, source)
}

fn write_stream(end: &Arc<DuplexEnd>, source: UserSlice) -> Result<usize, Errno> {
    let total = usize::try_from(source.length()).map_err(|_| Errno::EINVAL)?;
    let mut done = 0usize;
    loop {
        let mut state = end.pair.state.lock();
        let peer = end.side.peer();
        if state.write_shutdown(end.side) || state.read_shutdown(peer) || state.ends(peer) == 0 {
            drop(state);
            raise_sigpipe();
            return Err(Errno::EPIPE);
        }
        let free = match state.outgoing(end.side) {
            Buffer::Stream(ring) => ring.free(),
            Buffer::Datagram(_) => 0,
        };
        if free == 0 {
            if end.nonblock {
                return if done == 0 {
                    Err(Errno::EAGAIN)
                } else {
                    Ok(done)
                };
            }
            drop(state);
            if let Err(error) = wait(end.pair.writers(end.side), end.pair.id, WAIT_WRITE, || {
                let state = end.pair.state.lock();
                let outgoing = state.outgoing(end.side);
                let free = match outgoing {
                    Buffer::Stream(ring) => ring.free(),
                    Buffer::Datagram(_) => 0,
                };
                Ok(free == 0
                    && !state.write_shutdown(end.side)
                    && !state.read_shutdown(peer)
                    && state.ends(peer) != 0)
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
        let outgoing = state.outgoing_mut(end.side);
        let copied = match outgoing {
            Buffer::Stream(ring) => write_stream_ring(ring, source, done, chunk)?,
            Buffer::Datagram(_) => 0,
        };
        drop(state);
        end.pair.wake_readers(peer);
        event::notify_all();
        done = done.saturating_add(copied);
        if done >= total {
            return Ok(done);
        }
        if end.nonblock {
            return Ok(done);
        }
    }
}

fn write_stream_ring(
    ring: &mut ring::Ring,
    source: UserSlice,
    done: usize,
    chunk: usize,
) -> Result<usize, Errno> {
    let (first, second) = ring.write_parts();
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
    ring.produce(copied);
    Ok(copied)
}

fn write_datagram(end: &Arc<DuplexEnd>, source: UserSlice) -> Result<usize, Errno> {
    let total = usize::try_from(source.length()).map_err(|_| Errno::EINVAL)?;
    if total > DUPLEX_DATAGRAM_MAX {
        return Err(Errno::EMSGSIZE);
    }
    let mut message = Vec::new();
    message
        .try_reserve_exact(total)
        .map_err(|_| Errno::ENOMEM)?;
    message.resize(total, 0);
    copy_from_user(&mut message, source)?;
    let mut pending = Some(message);
    loop {
        let mut state = end.pair.state.lock();
        let peer = end.side.peer();
        if state.write_shutdown(end.side) || state.read_shutdown(peer) || state.ends(peer) == 0 {
            drop(state);
            raise_sigpipe();
            return Err(Errno::EPIPE);
        }
        let fits = state.outgoing(end.side).datagram_fits(total);
        if !fits {
            if end.nonblock {
                return Err(Errno::EAGAIN);
            }
            drop(state);
            wait(end.pair.writers(end.side), end.pair.id, WAIT_WRITE, || {
                let state = end.pair.state.lock();
                Ok(!state.outgoing(end.side).datagram_fits(total)
                    && !state.write_shutdown(end.side)
                    && !state.read_shutdown(peer)
                    && state.ends(peer) != 0)
            })?;
            continue;
        }
        if let Buffer::Datagram(buffer) = state.outgoing_mut(end.side) {
            let Some(message) = pending.take() else {
                return Err(Errno::EINVAL);
            };
            buffer.bytes = buffer.bytes.saturating_add(message.len());
            buffer.queue.push_back(message);
        } else {
            return Err(Errno::EINVAL);
        }
        drop(state);
        end.pair.wake_readers(peer);
        event::notify_all();
        return Ok(total);
    }
}

/// Apply `shutdown(how)` to one endpoint.
///
/// `SHUT_RD` makes local reads return 0 and peer writes fail with `EPIPE`; `SHUT_WR` makes local
/// writes fail with `EPIPE` and peer reads return 0; `SHUT_RDWR` does both.
pub fn shutdown(end: &Arc<DuplexEnd>, how: u8) -> Result<(), Errno> {
    let (read, write) = match how {
        SHUT_RD => (true, false),
        SHUT_WR => (false, true),
        SHUT_RDWR => (true, true),
        _ => return Err(Errno::EINVAL),
    };
    let peer = end.side.peer();
    {
        let mut state = end.pair.state.lock();
        if read {
            state.set_read_shutdown(end.side);
        }
        if write {
            state.set_write_shutdown(end.side);
        }
    }
    if read {
        end.pair.wake_readers(end.side);
        end.pair.wake_writers(peer);
    }
    if write {
        end.pair.wake_writers(end.side);
        end.pair.wake_readers(peer);
    }
    event::notify_all();
    Ok(())
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

/// Queue `SIGPIPE` on the calling process.
fn raise_sigpipe() {
    if let Some(caller) = process::current() {
        caller.queue_signal(SIGPIPE, 0);
    }
}

/// Block the current thread while `condition` is true.
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
