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
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_handles::LineageNode;
use raven_usercopy::UserSlice;
use spin::Mutex;

use crate::event;
use crate::event::evq_core::{HUP, READABLE, WRITABLE};
use crate::process;
use crate::process::handle::Object;
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
/// `MSG_NOSIGNAL`: suppress the `SIGPIPE` queue on `EPIPE` for one send.
pub const DUPLEX_MSG_NOSIGNAL: u32 = 0x4000;
/// Maximum handles in one control message.
pub const DUPLEX_MAX_HANDLES: usize = 16;
/// Maximum pending control messages per direction.
pub const DUPLEX_CONTROL_QUEUE_MAX: usize = 64;
/// Maximum control bytes accepted from one send.
pub const DUPLEX_CONTROL_MAX: usize = 256;

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

/// Live duplex pair objects. The K496 matrix compares this with the reachable graph after every
/// operation; a pair that outlives every external handle is a retained cycle.
static LIVE_PAIRS: AtomicU64 = AtomicU64::new(0);
/// Live endpoint objects (two per pair while any handle or queued record holds one).
static LIVE_ENDS: AtomicU64 = AtomicU64::new(0);
/// Handles owned by a queued `SCM_RIGHTS` record or datagram message.
static LIVE_PENDING: AtomicU64 = AtomicU64::new(0);
/// Queued stream control records (one per `sendmsg` that carried handles).
static LIVE_CONTROLS: AtomicU64 = AtomicU64::new(0);

/// Observable duplex ownership counters for the K496 evidence suite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Live pair objects.
    pub pairs: u64,
    /// Live endpoint objects.
    pub ends: u64,
    /// Handles waiting in a control record or datagram message.
    pub pending: u64,
    /// Queued stream control records.
    pub controls: u64,
}

/// Snapshot the ownership counters.
pub fn stats() -> Stats {
    Stats {
        pairs: LIVE_PAIRS.load(Ordering::Acquire),
        ends: LIVE_ENDS.load(Ordering::Acquire),
        pending: LIVE_PENDING.load(Ordering::Acquire),
        controls: LIVE_CONTROLS.load(Ordering::Acquire),
    }
}

/// Weak live-object registry used only by the explicit cycle collector.
///
/// The collector is triggered by `DUPLEX_COLLECT`, never by read/write/send/recv. Pair creation
/// prunes the dead entries and registers the new pair/endpoints. The registry lock is the outermost
/// lock in the collector; it is released before any pair state lock is taken.
struct Registry {
    pairs: Vec<Weak<Duplex>>,
    ends: Vec<Weak<DuplexEnd>>,
}

impl Registry {
    fn prune(&mut self) {
        self.pairs.retain(|pair| pair.strong_count() > 0);
        self.ends.retain(|end| end.strong_count() > 0);
    }

    fn register(&mut self, pair: &Arc<Duplex>, left: &Arc<DuplexEnd>, right: &Arc<DuplexEnd>) {
        self.pairs.push(Arc::downgrade(pair));
        self.ends.push(Arc::downgrade(left));
        self.ends.push(Arc::downgrade(right));
    }
}

/// Live pair/endpoint registry for the collector.
static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    pairs: Vec::new(),
    ends: Vec::new(),
});

/// One endpoint snapshot used by the collector.
struct EndInfo {
    endpoint: Arc<DuplexEnd>,
}

/// A queue removed from an unreachable receive side, dropped after the state lock is released.
///
/// The payload is never read: the drop itself releases the pending handles and their endpoint
/// references outside the state lock.
#[allow(dead_code)]
enum Disposal {
    Controls(VecDeque<ControlMessage>),
    Datagrams(VecDeque<DatagramMessage>),
}

fn endpoint_index(info: &[EndInfo], pointer: usize) -> Option<usize> {
    info.iter()
        .position(|entry| Arc::as_ptr(&entry.endpoint) as usize == pointer)
}

fn scan_handles(
    handles: &[PendingHandle],
    owner: usize,
    info: &[EndInfo],
    pending: &mut [usize],
    edges: &mut Vec<(usize, usize)>,
) {
    for handle in handles {
        if let Object::Duplex(end) = &handle.object {
            let pointer = Arc::as_ptr(end) as usize;
            if let Some(target) = endpoint_index(info, pointer) {
                if let Some(slot) = pending.get_mut(target) {
                    *slot = slot.saturating_add(1);
                }
                edges.push((owner, target));
            }
        }
    }
}

fn scan_records(
    records: &ControlRecords,
    owner: usize,
    info: &[EndInfo],
    pending: &mut [usize],
    edges: &mut Vec<(usize, usize)>,
) {
    for record in records {
        scan_handles(record, owner, info, pending, edges);
    }
}

fn scan_controls(
    controls: &VecDeque<ControlMessage>,
    owner: usize,
    info: &[EndInfo],
    pending: &mut [usize],
    edges: &mut Vec<(usize, usize)>,
) {
    for control in controls {
        scan_records(&control.records, owner, info, pending, edges);
    }
}

fn scan_datagrams(
    messages: &VecDeque<DatagramMessage>,
    owner: usize,
    info: &[EndInfo],
    pending: &mut [usize],
    edges: &mut Vec<(usize, usize)>,
) {
    for message in messages {
        scan_records(&message.records, owner, info, pending, edges);
    }
}

/// Reclaim unreachable duplex cycles.
///
/// Roots are endpoints with a strong reference that is not owned by a queued handle. The mark
/// follows every receive queue that a reachable endpoint can read; a queue whose receiver is
/// unmarked is taken out and dropped after the state lock is released. Reachable cyclic graphs are
/// therefore never touched. Returns the number of receive queues cleared.
///
/// Cost: O(live pairs + queued handles), plus one allocation per snapshot. The registry lock is
/// never held with a state lock and no wake happens under a state lock.
pub fn collect_cycles() -> usize {
    let (pairs, endpoints) = {
        let mut registry = REGISTRY.lock();
        registry.prune();
        let mut pairs = Vec::new();
        let mut endpoints = Vec::new();
        if pairs.try_reserve(registry.pairs.len()).is_err()
            || endpoints.try_reserve(registry.ends.len()).is_err()
        {
            return 0;
        }
        for weak in &registry.pairs {
            if let Some(pair) = weak.upgrade() {
                pairs.push(pair);
            }
        }
        for weak in &registry.ends {
            if let Some(endpoint) = weak.upgrade() {
                endpoints.push(endpoint);
            }
        }
        (pairs, endpoints)
    };
    if pairs.is_empty() {
        return 0;
    }

    let mut info = Vec::new();
    if info.try_reserve(endpoints.len()).is_err() {
        return 0;
    }
    for endpoint in endpoints {
        info.push(EndInfo { endpoint });
    }
    let mut pending = Vec::new();
    if pending.try_reserve(info.len()).is_err() {
        return 0;
    }
    pending.resize(info.len(), 0);

    let mut edges = Vec::new();
    if edges.try_reserve(info.len().saturating_mul(8)).is_err() {
        return 0;
    }

    for pair in &pairs {
        let mut left = None;
        let mut right = None;
        for (index, entry) in info.iter().enumerate() {
            if Arc::ptr_eq(&entry.endpoint.pair, pair) {
                match entry.endpoint.side {
                    Side::Left => left = Some(index),
                    Side::Right => right = Some(index),
                }
            }
        }
        let state = pair.state.lock();
        if let Some(owner) = right {
            match &state.left_to_right {
                Buffer::Stream(buffer) => {
                    scan_controls(&buffer.controls, owner, &info, &mut pending, &mut edges)
                }
                Buffer::Datagram(buffer) => {
                    scan_datagrams(&buffer.queue, owner, &info, &mut pending, &mut edges)
                }
            }
        }
        if let Some(owner) = left {
            match &state.right_to_left {
                Buffer::Stream(buffer) => {
                    scan_controls(&buffer.controls, owner, &info, &mut pending, &mut edges)
                }
                Buffer::Datagram(buffer) => {
                    scan_datagrams(&buffer.queue, owner, &info, &mut pending, &mut edges)
                }
            }
        }
    }

    let mut marked = Vec::new();
    if marked.try_reserve(info.len()).is_err() {
        return 0;
    }
    marked.resize(info.len(), false);
    for (index, entry) in info.iter().enumerate() {
        let strong = Arc::strong_count(&entry.endpoint);
        let queued = pending.get(index).copied().unwrap_or(0);
        let external = strong.saturating_sub(queued.saturating_add(1));
        if external > 0
            && let Some(slot) = marked.get_mut(index)
        {
            *slot = true;
        }
    }
    loop {
        let mut changed = false;
        for (owner, target) in &edges {
            let owner_marked = marked.get(*owner).copied().unwrap_or(false);
            let target_marked = marked.get(*target).copied().unwrap_or(false);
            if owner_marked && !target_marked {
                if let Some(slot) = marked.get_mut(*target) {
                    *slot = true;
                }
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut disposals = Vec::new();
    for pair in &pairs {
        let mut left = None;
        let mut right = None;
        for (index, entry) in info.iter().enumerate() {
            if Arc::ptr_eq(&entry.endpoint.pair, pair) {
                match entry.endpoint.side {
                    Side::Left => left = Some(index),
                    Side::Right => right = Some(index),
                }
            }
        }
        let mut found = Vec::new();
        {
            let mut state = pair.state.lock();
            if right.is_none_or(|index| !marked.get(index).copied().unwrap_or(false)) {
                match &mut state.left_to_right {
                    Buffer::Stream(buffer) => {
                        found.push(Disposal::Controls(core::mem::take(&mut buffer.controls)));
                    }
                    Buffer::Datagram(buffer) => {
                        found.push(Disposal::Datagrams(core::mem::take(&mut buffer.queue)));
                        buffer.bytes = 0;
                    }
                }
            }
            if left.is_none_or(|index| !marked.get(index).copied().unwrap_or(false)) {
                match &mut state.right_to_left {
                    Buffer::Stream(buffer) => {
                        found.push(Disposal::Controls(core::mem::take(&mut buffer.controls)));
                    }
                    Buffer::Datagram(buffer) => {
                        found.push(Disposal::Datagrams(core::mem::take(&mut buffer.queue)));
                        buffer.bytes = 0;
                    }
                }
            }
        }
        disposals.extend(found);
    }

    let cleared = disposals.len();
    drop(disposals);
    cleared
}

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

/// Kernel-stamped identity of the process that created the pair.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PeerCredentials {
    /// Process id.
    pub pid: u64,
    /// Real user id at creation.
    pub uid: u32,
    /// Real group id at creation.
    pub gid: u32,
}

/// One handle carried by an `SCM_RIGHTS` control message, waiting for the receiver.
pub(crate) struct PendingHandle {
    pub(crate) object: Object,
    pub(crate) rights: Rights,
    pub(crate) lineage: Arc<LineageNode>,
}

impl PendingHandle {
    pub(crate) fn new(object: Object, rights: Rights, lineage: Arc<LineageNode>) -> Self {
        LIVE_PENDING.fetch_add(1, Ordering::AcqRel);
        Self {
            object,
            rights,
            lineage,
        }
    }
}

impl PendingHandle {
    /// Take the owned object/rights/lineage without running the counter drop.
    ///
    /// The caller installs the object in a handle table, so the in-flight reference ends here and
    /// the live counter is decremented exactly once.
    pub(crate) fn into_parts(self) -> (Object, Rights, Arc<LineageNode>) {
        let this = core::mem::ManuallyDrop::new(self);
        // SAFETY: `this` is consumed here, every field is read exactly once and the original
        // `PendingHandle` is never dropped, so no field is dropped twice.
        let (object, lineage) = unsafe {
            (
                core::ptr::read(&this.object),
                core::ptr::read(&this.lineage),
            )
        };
        LIVE_PENDING.fetch_sub(1, Ordering::AcqRel);
        (object, this.rights, lineage)
    }
}

impl Drop for PendingHandle {
    fn drop(&mut self) {
        LIVE_PENDING.fetch_sub(1, Ordering::AcqRel);
    }
}

/// All control records carried by one send, keeping the sender's cmsg boundaries (K521).
pub(crate) type ControlRecords = Vec<Vec<PendingHandle>>;

/// Total handles carried across every record.
fn record_handles(records: &ControlRecords) -> usize {
    records
        .iter()
        .fold(0usize, |total, record| total.saturating_add(record.len()))
}

struct ControlMessage {
    /// Stream byte offset this ancillary record is attached to.
    offset: u64,
    records: ControlRecords,
}

impl ControlMessage {
    fn new(offset: u64, records: ControlRecords) -> Self {
        LIVE_CONTROLS.fetch_add(1, Ordering::AcqRel);
        Self { offset, records }
    }

    fn into_records(mut self) -> ControlRecords {
        core::mem::take(&mut self.records)
    }
}

impl Drop for ControlMessage {
    fn drop(&mut self) {
        LIVE_CONTROLS.fetch_sub(1, Ordering::AcqRel);
    }
}

struct StreamBuffer {
    ring: ring::Ring,
    /// Total bytes written into the ring by the sender.
    written: u64,
    /// Total bytes consumed by the receiver.
    read: u64,
    controls: VecDeque<ControlMessage>,
}

impl StreamBuffer {
    fn control_ready(&self) -> bool {
        self.controls
            .front()
            .is_some_and(|control| control.offset <= self.read)
    }

    fn control_full(&self) -> bool {
        self.controls.len() >= DUPLEX_CONTROL_QUEUE_MAX
    }

    fn discard_controls_at_read(&mut self) {
        // A control at `offset == read` belongs to the next unread byte, not to a byte this
        // read consumed. A plain `read()` that stops exactly at that byte must leave the
        // control for the following `recvmsg()`; only controls strictly before `read` are lost.
        while self
            .controls
            .front()
            .is_some_and(|control| control.offset < self.read)
        {
            let _ = self.controls.pop_front();
        }
    }

    fn discard_controls_before_read(&mut self) {
        while self
            .controls
            .front()
            .is_some_and(|control| control.offset < self.read)
        {
            let _ = self.controls.pop_front();
        }
    }
}

struct DatagramMessage {
    data: Vec<u8>,
    records: ControlRecords,
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
    /// Watcher key shared by both ends of this duplex pair.
    pub(crate) fn event_key(&self) -> u64 {
        crate::event::duplex_key(self.pair.id)
    }

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

    /// Kernel-stamped credentials of the process that created the pair.
    pub fn credentials(&self) -> PeerCredentials {
        self.pair.peer
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
        event::notify_source(self.event_key());
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
        LIVE_ENDS.fetch_sub(1, Ordering::AcqRel);
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
        event::notify_source(self.event_key());
    }
}

struct Duplex {
    id: u64,
    peer: PeerCredentials,
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

impl Drop for Duplex {
    fn drop(&mut self) {
        LIVE_PAIRS.fetch_sub(1, Ordering::AcqRel);
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
    Stream(StreamBuffer),
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
            let ring = ring::Ring::pipe().ok_or(Errno::ENOMEM)?;
            Ok(Self::Stream(StreamBuffer {
                ring,
                written: 0,
                read: 0,
                controls: VecDeque::new(),
            }))
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Stream(buffer) => buffer.ring.is_empty() && buffer.controls.is_empty(),
            Self::Datagram(queue) => queue.queue.is_empty(),
        }
    }

    fn data_empty(&self) -> bool {
        match self {
            Self::Stream(buffer) => buffer.ring.is_empty(),
            Self::Datagram(queue) => queue.queue.is_empty(),
        }
    }

    fn control_ready(&self) -> bool {
        match self {
            Self::Stream(buffer) => buffer.control_ready(),
            Self::Datagram(_) => false,
        }
    }

    fn discard_controls_at_read(&mut self) {
        if let Self::Stream(buffer) = self {
            buffer.discard_controls_at_read();
        }
    }

    fn writable(&self) -> bool {
        match self {
            Self::Stream(buffer) => buffer.ring.free() != 0,
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
    queue: VecDeque<DatagramMessage>,
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
    let peer = process::current().map_or(PeerCredentials::default(), |caller| {
        let ids = caller.ids.lock();
        PeerCredentials {
            pid: caller.id,
            uid: ids.uid,
            gid: ids.gid,
        }
    });
    LIVE_PAIRS.fetch_add(1, Ordering::AcqRel);
    let pair = Arc::new(Duplex {
        id,
        peer,
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
    LIVE_ENDS.fetch_add(2, Ordering::AcqRel);
    let left = Arc::new(DuplexEnd {
        pair: Arc::clone(&pair),
        side: Side::Left,
        nonblock,
        cloexec,
        datagram,
    });
    let right = Arc::new(DuplexEnd {
        pair: Arc::clone(&pair),
        side: Side::Right,
        nonblock,
        cloexec,
        datagram,
    });
    {
        let mut registry = REGISTRY.lock();
        registry.prune();
        registry.register(&pair, &left, &right);
    }
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
        state.incoming_mut(end.side).discard_controls_at_read();
        if state.incoming(end.side).data_empty() {
            if state.write_shutdown(peer) || state.ends(peer) == 0 {
                return Ok(0);
            }
            if end.nonblock {
                return Err(Errno::EAGAIN);
            }
            drop(state);
            wait(end.pair.readers(end.side), end.pair.id, WAIT_READ, || {
                let state = end.pair.state.lock();
                Ok(state.incoming(end.side).data_empty()
                    && !state.read_shutdown(end.side)
                    && !state.write_shutdown(peer)
                    && state.ends(peer) != 0)
            })?;
            continue;
        }
        let incoming = state.incoming_mut(end.side);
        let copied = match incoming {
            Buffer::Stream(buffer) => read_stream(buffer, destination, limit)?,
            Buffer::Datagram(buffer) => read_datagram(buffer, destination, limit)?,
        };
        state.incoming_mut(end.side).discard_controls_at_read();
        drop(state);
        end.pair.wake_writers(peer);
        event::notify_source(end.event_key());
        return Ok(copied);
    }
}

fn read_stream(
    buffer: &mut StreamBuffer,
    destination: UserSlice,
    limit: usize,
) -> Result<usize, Errno> {
    let want = buffer.ring.len().min(limit);
    let (first, second) = buffer.ring.read_parts();
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
    buffer.ring.consume(copied);
    buffer.read = buffer.read.saturating_add(copied as u64);
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
    let copied = message.data.len().min(limit);
    if let Some(bytes) = message.data.get(..copied) {
        copy_out(destination.address(), bytes)?;
    }
    let length = message.data.len();
    if buffer.queue.pop_front().is_some() {
        buffer.bytes = buffer.bytes.saturating_sub(length);
    }
    Ok(copied)
}

/// Receive one chunk or datagram plus the `SCM_RIGHTS` records attached to it.
///
/// The control capacity is checked before consuming data or handles; a too-small buffer returns
/// `EMSGSIZE` and leaves the message queued. The caller either passes a capacity large enough for
/// every record and truncates whole records itself (the syscall layer, K521), or gets the old
/// all-or-nothing rejection. Records keep the sender's cmsg boundaries. The returned handles are
/// installed by the caller after this function returns.
pub fn recv(
    end: &Arc<DuplexEnd>,
    destination: UserSlice,
    control_capacity: usize,
) -> Result<(usize, ControlRecords), Errno> {
    let limit = usize::try_from(destination.length()).map_err(|_| Errno::EINVAL)?;
    loop {
        let mut state = end.pair.state.lock();
        if state.read_shutdown(end.side) {
            return Ok((0, Vec::new()));
        }
        let peer = end.side.peer();
        if let Buffer::Stream(buffer) = state.incoming_mut(end.side) {
            buffer.discard_controls_before_read();
        }
        let data_empty = state.incoming(end.side).data_empty();
        let control_ready = state.incoming(end.side).control_ready();
        if data_empty && !control_ready {
            if state.write_shutdown(peer) || state.ends(peer) == 0 {
                return Ok((0, Vec::new()));
            }
            if end.nonblock {
                return Err(Errno::EAGAIN);
            }
            drop(state);
            wait(end.pair.readers(end.side), end.pair.id, WAIT_READ, || {
                let state = end.pair.state.lock();
                Ok(state.incoming(end.side).data_empty()
                    && !state.incoming(end.side).control_ready()
                    && !state.read_shutdown(end.side)
                    && !state.write_shutdown(peer)
                    && state.ends(peer) != 0)
            })?;
            continue;
        }
        let (copied, records) = match state.incoming_mut(end.side) {
            Buffer::Stream(buffer) => {
                if buffer.control_ready() {
                    let needed = buffer
                        .controls
                        .front()
                        .map_or(0, |control| control_size(record_handles(&control.records)));
                    if control_capacity < needed {
                        return Err(Errno::EMSGSIZE);
                    }
                    let copied = read_stream(buffer, destination, limit)?;
                    let control = buffer.controls.pop_front().ok_or(Errno::EINVAL)?;
                    (copied, control.into_records())
                } else {
                    (read_stream(buffer, destination, limit)?, Vec::new())
                }
            }
            Buffer::Datagram(buffer) => {
                let Some(message) = buffer.queue.front() else {
                    return Err(Errno::EINVAL);
                };
                let needed = control_size(record_handles(&message.records));
                if control_capacity < needed {
                    return Err(Errno::EMSGSIZE);
                }
                let copied = message.data.len().min(limit);
                if let Some(bytes) = message.data.get(..copied) {
                    copy_out(destination.address(), bytes)?;
                }
                let length = message.data.len();
                let Some(message) = buffer.queue.pop_front() else {
                    return Err(Errno::EINVAL);
                };
                buffer.bytes = buffer.bytes.saturating_sub(length);
                (copied, message.records)
            }
        };
        drop(state);
        end.pair.wake_writers(peer);
        event::notify_source(end.event_key());
        return Ok((copied, records));
    }
}

fn control_size(handles: usize) -> usize {
    if handles == 0 {
        0
    } else {
        16usize.saturating_add(handles.saturating_mul(4))
    }
}

/// Write one buffer to `end`.
///
/// A stream write fills the 64 KiB ring and waits for the peer to drain it. A datagram write
/// queues one complete message atomically. A write after local write shutdown, peer read shutdown
/// or peer close queues `SIGPIPE` and returns `EPIPE`.
pub fn write(end: &Arc<DuplexEnd>, source: UserSlice) -> Result<usize, Errno> {
    send(end, source, Vec::new(), 0)
}

/// Write `source` with optional `SCM_RIGHTS` records.
///
/// `flags` currently accepts [`DUPLEX_MSG_NOSIGNAL`]; unknown flags are rejected by the syscall
/// layer. Stream mode attaches the records to the first byte of this send; a zero-length send is
/// a control-only record. Datagram mode stores the records with the one message. The sender's
/// cmsg boundaries are preserved through the queue and on receive (K521).
pub fn send(
    end: &Arc<DuplexEnd>,
    source: UserSlice,
    records: ControlRecords,
    flags: u32,
) -> Result<usize, Errno> {
    if source.is_empty() && records.is_empty() {
        return Ok(0);
    }
    let signal = flags & DUPLEX_MSG_NOSIGNAL == 0;
    if end.datagram {
        write_datagram(end, source, records, signal)
    } else {
        write_stream(end, source, records, signal)
    }
}

fn write_stream(
    end: &Arc<DuplexEnd>,
    source: UserSlice,
    records: ControlRecords,
    signal: bool,
) -> Result<usize, Errno> {
    let total = usize::try_from(source.length()).map_err(|_| Errno::EINVAL)?;
    let mut pending = if records.is_empty() {
        None
    } else {
        Some(records)
    };
    let mut done = 0usize;
    loop {
        let mut state = end.pair.state.lock();
        let peer = end.side.peer();
        if state.write_shutdown(end.side) || state.read_shutdown(peer) || state.ends(peer) == 0 {
            drop(state);
            raise_sigpipe(signal);
            return Err(Errno::EPIPE);
        }
        let has_control = pending.is_some();
        let no_data = done >= total;
        let (free, control_full) = match state.outgoing(end.side) {
            Buffer::Stream(buffer) => (buffer.ring.free(), has_control && buffer.control_full()),
            Buffer::Datagram(_) => return Err(Errno::EINVAL),
        };
        if (!no_data && free == 0) || control_full {
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
                let (free, control_full) = match state.outgoing(end.side) {
                    Buffer::Stream(buffer) => {
                        (buffer.ring.free(), has_control && buffer.control_full())
                    }
                    Buffer::Datagram(_) => (0, false),
                };
                Ok(((!no_data && free == 0) || control_full)
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
        let buffer = match state.outgoing_mut(end.side) {
            Buffer::Stream(buffer) => buffer,
            Buffer::Datagram(_) => return Err(Errno::EINVAL),
        };
        let offset = buffer.written;
        let copied = if no_data {
            0
        } else {
            write_stream_ring(buffer, source, done, chunk)?
        };
        if let Some(records) = pending.take() {
            buffer
                .controls
                .push_back(ControlMessage::new(offset, records));
        }
        drop(state);
        end.pair.wake_readers(peer);
        event::notify_source(end.event_key());
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
    buffer: &mut StreamBuffer,
    source: UserSlice,
    done: usize,
    chunk: usize,
) -> Result<usize, Errno> {
    let (first, second) = buffer.ring.write_parts();
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
    buffer.ring.produce(copied);
    buffer.written = buffer.written.saturating_add(copied as u64);
    Ok(copied)
}

fn write_datagram(
    end: &Arc<DuplexEnd>,
    source: UserSlice,
    records: ControlRecords,
    signal: bool,
) -> Result<usize, Errno> {
    let total = usize::try_from(source.length()).map_err(|_| Errno::EINVAL)?;
    if total > DUPLEX_DATAGRAM_MAX || record_handles(&records) > DUPLEX_MAX_HANDLES {
        return Err(Errno::EMSGSIZE);
    }
    let mut data = Vec::new();
    data.try_reserve_exact(total).map_err(|_| Errno::ENOMEM)?;
    data.resize(total, 0);
    copy_from_user(&mut data, source)?;
    let mut pending = Some(DatagramMessage { data, records });
    loop {
        let mut state = end.pair.state.lock();
        let peer = end.side.peer();
        if state.write_shutdown(end.side) || state.read_shutdown(peer) || state.ends(peer) == 0 {
            drop(state);
            raise_sigpipe(signal);
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
            buffer.bytes = buffer.bytes.saturating_add(message.data.len());
            buffer.queue.push_back(message);
        } else {
            return Err(Errno::EINVAL);
        }
        drop(state);
        end.pair.wake_readers(peer);
        event::notify_source(end.event_key());
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
    event::notify_source(end.event_key());
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

/// Queue `SIGPIPE` on the calling process unless the caller suppressed it.
fn raise_sigpipe(signal: bool) {
    if signal && let Some(caller) = process::current() {
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
