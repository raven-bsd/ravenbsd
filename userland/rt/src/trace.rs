// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Safe attachment to the cross-process span sink.
//!
//! The launcher creates the sink memory object, initialises it once and transfers it to every
//! producer at a fixed handle. A producer calls [`Sink::attach`] with that handle; the runtime does
//! the one raw mapping and exposes only safe [`Sink::enabled`]/[`Sink::emit`] calls.
//!
//! The fixed layout is shared with `userland/bin/trace` and the Q4a ring: header at
//! [`SINK_ADDR`], records behind it, [`CAPACITY`] slots and [`PRODUCERS`] producers. The first
//! attach writes a fresh header when the page is still zero-filled; every later attach validates
//! it. A zero trace id is never recorded and the disabled path allocates nothing.

use core::mem::size_of;

use raven_abi::types::{Handle, map, prot};
use raven_trace_core::{RingHeader, SpanRecord, SpanRing};

pub use raven_trace_core::{Span, SpanKind};

/// The handle a launcher transfers the sink under.
pub const SINK_HANDLE: Handle = Handle::new(3, 0);
/// The fixed address every producer maps the sink at.
pub const SINK_ADDR: usize = 0x3000_0000;
/// Ring capacity in records.
pub const CAPACITY: u32 = 64;
/// Producers the ring admits.
pub const PRODUCERS: u32 = 4;
/// Page size used by the fixed mapping.
const PAGE: usize = 4096;
/// Ring header bytes.
pub const HEADER_BYTES: usize = size_of::<RingHeader>();
/// One record's bytes.
pub const RECORD_BYTES: usize = size_of::<SpanRecord>();
/// Total sink bytes (header plus records).
pub const SINK_BYTES: usize = HEADER_BYTES + CAPACITY as usize * RECORD_BYTES;
/// Sink bytes rounded up to whole pages.
pub const SINK_PAGES: usize = SINK_BYTES.div_ceil(PAGE);

/// Whether `producer` is a valid ring index.
pub const fn producer_valid(producer: u32) -> bool {
    producer < PRODUCERS
}

/// A process's safe handle on the shared sink.
///
/// `attach` never fails the caller: a missing handle, a mapping error or a bad header leaves the
/// sink disabled, so `emit` is a no-op. This keeps the probe call sites free of error paths and
/// keeps the disabled path allocation-free.
pub struct Sink {
    ring: Option<SpanRing<'static>>,
    producer: u32,
    sequence: u64,
}

impl Sink {
    /// Map the transferred sink and return a safe producer handle.
    ///
    /// `handle` is normally [`SINK_HANDLE`]. The first successful attach initialises the ring
    /// header; later attaches validate it.
    pub fn attach(handle: Handle, producer: u32) -> Self {
        if !producer_valid(producer) {
            return Self::disabled(producer);
        }
        let mapped = crate::sys::map_object_in(
            Handle::SELF_PROC,
            handle,
            0,
            SINK_ADDR,
            SINK_PAGES * PAGE,
            prot::READ | prot::WRITE,
            map::FIXED | map::SHARED,
        );
        if mapped.is_err() {
            return Self::disabled(producer);
        }
        // SAFETY: `map_object_in` mapped `SINK_PAGES` writable pages at the fixed address and the
        // mapping lives for the process. The layout is the Q4a header followed by `CAPACITY`
        // records. The first attach initialises a zeroed header before any producer starts; later
        // attach calls only read it.
        let ring = unsafe {
            let header = SINK_ADDR as *mut RingHeader;
            if !(*header).is_valid() {
                header.write(RingHeader::new(CAPACITY, PRODUCERS));
            }
            let records = (SINK_ADDR + HEADER_BYTES) as *const SpanRecord;
            let records = core::slice::from_raw_parts(records, CAPACITY as usize);
            SpanRing::new(&*header, records)
        };
        match ring {
            Ok(ring) => Self {
                ring: Some(ring),
                producer,
                sequence: 0,
            },
            Err(_) => Self::disabled(producer),
        }
    }

    /// Whether the sink is attached and the header validated.
    pub const fn enabled(&self) -> bool {
        self.ring.is_some()
    }

    /// Record `span` as this producer; returns whether it was written.
    ///
    /// A disabled sink returns `false` without allocating. A zero trace id is a valid untraced
    /// span (the Q4b fixture records both traced and untraced requests); probes that want only
    /// traced traffic check `span.trace_id != 0` at the call site.
    pub fn emit(&mut self, span: Span) -> bool {
        let Some(ring) = self.ring.as_ref() else {
            return false;
        };
        let emitted = ring.emit(self.producer, self.sequence, &span).is_ok();
        self.sequence = self.sequence.saturating_add(1);
        emitted
    }

    const fn disabled(producer: u32) -> Self {
        Self {
            ring: None,
            producer,
            sequence: 0,
        }
    }
}

#[cfg(test)]
#[path = "trace_ut.rs"]
mod tests;
