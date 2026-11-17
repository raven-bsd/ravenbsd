// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cross-process span sink: a fixed-capacity multi-producer/single-reader ring.
//!
//! Q3/Q3b prove a trace id crosses a call; this is the shared sink a `trace` command reads the
//! spans from. It is the pure, dependency-free core of the Q4 demo: a versioned header and
//! fixed-size records over caller-supplied storage, with an explicit overwrite and gap detection.

use core::sync::atomic::{AtomicU16, AtomicU32, AtomicU64, Ordering};

/// The ring magic ("RVNTRING").
pub const RING_MAGIC: u64 = 0x5256_4E54_5249_4E47;
/// The ring format version.
pub const RING_VERSION: u32 = 1;
/// How many times a reader retries a transiently-torn slot before reporting [`RingError::Torn`].
const RETRIES: u32 = 1_000_000;

/// What a span records about a crossing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum SpanKind {
    /// The client side of a call.
    Client = 0,
    /// The server side of a call.
    Server = 1,
    /// A kernel boundary crossing.
    Kernel = 2,
    /// A bulk-data (ring/io) operation.
    Io = 3,
}

/// A span to record: the logical payload a producer emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// The trace id; zero means "not traced".
    pub trace_id: u64,
    /// The span id.
    pub span_id: u64,
    /// The parent span id; zero for a root.
    pub parent_span_id: u64,
    /// Monotonic timestamp in nanoseconds.
    pub timestamp_ns: u64,
    /// The producing process id.
    pub pid: u32,
    /// The span kind.
    pub kind: SpanKind,
    /// The opcode (or status) this span records.
    pub opcode: u16,
}

/// One fixed-size record in the ring's storage.
///
/// Fields are atomic so the producers and the reader share the storage safely behind `&self`; the
/// per-producer `sequence` and the seqlock `generation` counter let the reader detect gaps and torn
/// records. `generation` is even when the slot is stable and odd while a producer is writing it.
#[repr(C)]
pub struct SpanRecord {
    /// Seqlock generation: even = stable, odd = writing.
    pub generation: AtomicU64,
    /// Per-producer monotonic sequence.
    pub sequence: AtomicU64,
    /// Trace id.
    pub trace_id: AtomicU64,
    /// Span id.
    pub span_id: AtomicU64,
    /// Parent span id.
    pub parent_span_id: AtomicU64,
    /// Monotonic timestamp in nanoseconds.
    pub timestamp_ns: AtomicU64,
    /// Producing process id.
    pub pid: AtomicU32,
    /// Span kind.
    pub kind: AtomicU16,
    /// Opcode or status.
    pub opcode: AtomicU16,
}

const _: () = {
    let _ = core::mem::size_of::<SpanRecord>();
};

impl Default for SpanRecord {
    fn default() -> Self {
        Self {
            generation: AtomicU64::new(0),
            sequence: AtomicU64::new(0),
            trace_id: AtomicU64::new(0),
            span_id: AtomicU64::new(0),
            parent_span_id: AtomicU64::new(0),
            timestamp_ns: AtomicU64::new(0),
            pid: AtomicU32::new(0),
            kind: AtomicU16::new(0),
            opcode: AtomicU16::new(0),
        }
    }
}

impl SpanRecord {
    /// Read the record as a logical [`Span`]. The caller must have observed a stable even
    /// generation with `Acquire` first, so every field read is ordered after the producer's write.
    fn load_span(&self) -> Span {
        Span {
            trace_id: self.trace_id.load(Ordering::Relaxed),
            span_id: self.span_id.load(Ordering::Relaxed),
            parent_span_id: self.parent_span_id.load(Ordering::Relaxed),
            timestamp_ns: self.timestamp_ns.load(Ordering::Relaxed),
            pid: self.pid.load(Ordering::Relaxed),
            kind: SpanKind::from_raw(self.kind.load(Ordering::Relaxed)),
            opcode: self.opcode.load(Ordering::Relaxed),
        }
    }
}

impl SpanKind {
    const fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::Client,
            1 => Self::Server,
            2 => Self::Kernel,
            _ => Self::Io,
        }
    }
}

/// The ring header: fixed metadata plus the free-running reserve/commit cursors.
#[repr(C)]
pub struct RingHeader {
    magic: u64,
    version: u32,
    capacity: u32,
    record_size: u32,
    producer_count: u32,
    reserve_cursor: AtomicU64,
    commit_cursor: AtomicU64,
}

impl RingHeader {
    /// A header for a ring of `capacity` records and `producer_count` producers.
    pub fn new(capacity: u32, producer_count: u32) -> Self {
        Self {
            magic: RING_MAGIC,
            version: RING_VERSION,
            capacity,
            record_size: core::mem::size_of::<SpanRecord>() as u32,
            producer_count,
            reserve_cursor: AtomicU64::new(0),
            commit_cursor: AtomicU64::new(0),
        }
    }

    /// The declared capacity.
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    /// The declared producer count.
    pub const fn producer_count(&self) -> u32 {
        self.producer_count
    }

    /// The declared record size.
    pub const fn record_size(&self) -> u32 {
        self.record_size
    }

    /// Whether the header's magic and version match this crate.
    pub const fn is_valid(&self) -> bool {
        self.magic == RING_MAGIC && self.version == RING_VERSION
    }
}

/// Why a ring operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    /// The header magic or version does not match.
    InvalidHeader,
    /// The header record size does not match [`SpanRecord`].
    BadRecordSize,
    /// The declared capacity is not a non-zero power of two.
    BadCapacity,
    /// The producer index is out of range.
    BadProducer,
    /// No committed record is available to read.
    Empty,
    /// The reader fell behind and `missed` records were overwritten.
    Gap {
        /// Number of records overwritten since the reader's last read.
        missed: u64,
    },
    /// The next record is reserved but not committed (torn).
    Torn,
}

impl RingError {
    /// A stable short name for logs and telemetry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidHeader => "invalid-header",
            Self::BadRecordSize => "bad-record-size",
            Self::BadCapacity => "bad-capacity",
            Self::BadProducer => "bad-producer",
            Self::Empty => "empty",
            Self::Gap { .. } => "gap",
            Self::Torn => "torn",
        }
    }
}

impl core::fmt::Display for RingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A fixed-capacity multi-producer/single-reader span ring over caller-supplied storage.
///
/// # Protocol
///
/// A producer reserves a slot with a lock-free increment of the reserve cursor, writes its record
/// (publishing `committed = 1` with `Release`), then advances the commit cursor in slot order — it
/// spins until the previous slot is committed. One reader iterates the contiguous committed prefix
/// in commit order. Overwrite is explicit: a reader that falls behind detects a [`RingError::Gap`]
/// instead of returning a stale record; a reserved-but-uncommitted record is a [`RingError::Torn`].
/// The record path never allocates.
#[derive(Clone, Copy)]
pub struct SpanRing<'a> {
    header: &'a RingHeader,
    records: &'a [SpanRecord],
    mask: u64,
}

impl<'a> SpanRing<'a> {
    /// Borrow `header` and `records` (at least `capacity` records) as a ring.
    pub fn new(header: &'a RingHeader, records: &'a [SpanRecord]) -> Result<Self, RingError> {
        if !header.is_valid() {
            return Err(RingError::InvalidHeader);
        }
        if header.record_size != core::mem::size_of::<SpanRecord>() as u32 {
            return Err(RingError::BadRecordSize);
        }
        let capacity = header.capacity;
        if capacity == 0 || !capacity.is_power_of_two() {
            return Err(RingError::BadCapacity);
        }
        if records.len() < capacity as usize {
            return Err(RingError::BadCapacity);
        }
        Ok(Self {
            header,
            records,
            mask: u64::from(capacity - 1),
        })
    }

    /// The ring capacity.
    pub const fn capacity(&self) -> u32 {
        self.header.capacity
    }

    /// The committed record count (free-running; wrap-safe via `wrapping_sub`).
    pub fn committed(&self) -> u64 {
        self.header.commit_cursor.load(Ordering::Acquire)
    }

    /// Reserve the next slot; returns the free-running slot number.
    ///
    /// Marks the slot's generation odd, so a reader that reaches it mid-write observes a torn
    /// record rather than stale fields.
    pub fn reserve(&self) -> u64 {
        let slot = self.header.reserve_cursor.fetch_add(1, Ordering::Relaxed);
        self.records[(slot & self.mask) as usize]
            .generation
            .fetch_add(1, Ordering::Acquire);
        slot
    }

    /// Write `span` with per-producer `sequence` into a reserved `slot`, publishing the record.
    pub fn write(&self, slot: u64, sequence: u64, span: &Span) {
        let record = &self.records[(slot & self.mask) as usize];
        record.sequence.store(sequence, Ordering::Relaxed);
        record.trace_id.store(span.trace_id, Ordering::Relaxed);
        record.span_id.store(span.span_id, Ordering::Relaxed);
        record
            .parent_span_id
            .store(span.parent_span_id, Ordering::Relaxed);
        record
            .timestamp_ns
            .store(span.timestamp_ns, Ordering::Relaxed);
        record.pid.store(span.pid, Ordering::Relaxed);
        record.kind.store(span.kind as u16, Ordering::Relaxed);
        record.opcode.store(span.opcode, Ordering::Relaxed);
        // Publish the record: the generation becomes even again.
        record.generation.fetch_add(1, Ordering::Release);
    }

    /// Publish a written `slot`: advance the commit cursor in slot order, spinning until the
    /// previous slot is committed.
    pub fn publish(&self, slot: u64) {
        while self.header.commit_cursor.load(Ordering::Acquire) != slot {
            core::hint::spin_loop();
        }
        self.header
            .commit_cursor
            .store(slot.wrapping_add(1), Ordering::Release);
    }

    /// Emit `span` as producer `producer` with per-producer sequence `sequence`.
    ///
    /// Overwrites the oldest unread record when the reader is behind (the reader reports the gap).
    pub fn emit(&self, producer: u32, sequence: u64, span: &Span) -> Result<(), RingError> {
        if producer >= self.header.producer_count {
            return Err(RingError::BadProducer);
        }
        let slot = self.reserve();
        self.write(slot, sequence, span);
        self.publish(slot);
        Ok(())
    }

    /// Read the next committed record, advancing `cursor` (a free-running read position).
    ///
    /// A slot that is reserved but not yet written (a producer is mid-write, or an overwrite is in
    /// progress) is retried a bounded number of times; if it never becomes stable the reader
    /// reports [`RingError::Torn`]. A reader that falls behind gets [`RingError::Gap`] instead of a
    /// stale record.
    pub fn read(&self, cursor: &mut u64) -> Result<Span, RingError> {
        for _ in 0..RETRIES {
            let commit = self.header.commit_cursor.load(Ordering::Acquire);
            let head = *cursor;
            if head == commit {
                return Err(RingError::Empty);
            }
            let capacity = u64::from(self.header.capacity);
            if commit.wrapping_sub(head) > capacity {
                let missed = commit.wrapping_sub(head) - capacity;
                *cursor = commit.wrapping_sub(capacity);
                return Err(RingError::Gap { missed });
            }
            let record = &self.records[(head & self.mask) as usize];
            // Seqlock: read a stable generation, the fields, then the generation again.
            let generation = record.generation.load(Ordering::Acquire);
            if !generation.is_multiple_of(2) {
                // A producer is writing this slot; retry.
                core::hint::spin_loop();
                continue;
            }
            let span = record.load_span();
            if record.generation.load(Ordering::Acquire) != generation {
                // The slot was re-written during the read; retry.
                core::hint::spin_loop();
                continue;
            }
            // Re-check the gap: the commit cursor may have advanced past this slot while we read
            // it, overwriting it. Skip rather than return a stale record.
            let commit = self.header.commit_cursor.load(Ordering::Acquire);
            if commit.wrapping_sub(head) > capacity {
                let missed = commit.wrapping_sub(head) - capacity;
                *cursor = commit.wrapping_sub(capacity);
                return Err(RingError::Gap { missed });
            }
            *cursor = head.wrapping_add(1);
            return Ok(span);
        }
        Err(RingError::Torn)
    }
}

/// A producer's view of the ring: its id, its monotonic sequence and an enable flag.
///
/// When disabled, [`TraceSink::emit`] writes nothing — the disabled path is one branch and no
/// record is touched.
pub struct TraceSink<'a> {
    ring: SpanRing<'a>,
    producer: u32,
    sequence: u64,
    enabled: bool,
}

impl<'a> TraceSink<'a> {
    /// A producer sink for `producer`.
    pub fn new(ring: SpanRing<'a>, producer: u32) -> Self {
        Self {
            ring,
            producer,
            sequence: 0,
            enabled: true,
        }
    }

    /// Whether the sink is enabled.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the enable flag.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// The producer id.
    pub const fn producer(&self) -> u32 {
        self.producer
    }

    /// Emit `span`, recording it into the ring; the disabled path writes nothing and returns `Ok`.
    pub fn emit(&mut self, span: &Span) -> Result<(), RingError> {
        if !self.enabled {
            return Ok(());
        }
        let sequence = self.sequence;
        self.ring.emit(self.producer, sequence, span)?;
        self.sequence = self.sequence.wrapping_add(1);
        Ok(())
    }
}

/// The single reader's view of the ring: a free-running cursor.
pub struct TraceReader<'a> {
    ring: SpanRing<'a>,
    cursor: u64,
}

impl<'a> TraceReader<'a> {
    /// A reader over `ring`.
    pub fn new(ring: SpanRing<'a>) -> Self {
        Self { ring, cursor: 0 }
    }

    /// Read the next committed record, advancing the cursor.
    pub fn read(&mut self) -> Result<Span, RingError> {
        self.ring.read(&mut self.cursor)
    }

    /// The current read position (free-running).
    pub const fn cursor(&self) -> u64 {
        self.cursor
    }
}

#[cfg(test)]
#[path = "sink_ut.rs"]
mod tests;
