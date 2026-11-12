// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The stack object, poll loop, timers and event transitions.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::fmt;

use raven_abi::errno::Errno;
use raven_prng::{Generator, seed_from_entropy};

use crate::device::Device;
use crate::event::{Event, PollResult, SocketId};
use crate::socket::{Kind, SocketTable};
use crate::time::{Duration, Instant};

/// Maximum number of armed timers. Exceeding it returns [`Errno::ENOMEM`] rather than allocating
/// without bound.
pub const MAX_TIMERS: usize = 256;

/// An IPv4 address at the stack boundary.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ipv4Addr([u8; 4]);

impl Ipv4Addr {
    /// The unspecified address `0.0.0.0`.
    pub const UNSPECIFIED: Self = Self([0; 4]);

    /// Construct an address from its four octets.
    #[must_use]
    pub const fn new(octets: [u8; 4]) -> Self {
        Self(octets)
    }

    /// Return the four octets in wire order.
    #[must_use]
    pub const fn octets(self) -> [u8; 4] {
        self.0
    }
}

impl From<[u8; 4]> for Ipv4Addr {
    fn from(octets: [u8; 4]) -> Self {
        Self(octets)
    }
}

impl fmt::Debug for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d] = self.0;
        write!(f, "{a}.{b}.{c}.{d}")
    }
}

/// The stack's resource limits and identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackConfig {
    /// Hardware address used for Ethernet framing, if the stack has one.
    pub hardware_addr: Option<[u8; 6]>,
    /// Maximum number of simultaneous sockets, including listeners.
    pub max_sockets: usize,
    /// Receive buffer cap per socket, in bytes.
    pub rx_queue_bytes: usize,
    /// Transmit buffer cap per socket, in bytes.
    pub tx_queue_bytes: usize,
    /// Global cap for reassembly buffers, in bytes.
    pub reassembly_bytes: usize,
    /// Maximum neighbour-cache entries.
    pub neighbor_cache_entries: usize,
}

impl Default for StackConfig {
    /// Modest host defaults: 64 sockets, 64 KiB per socket direction, 256 KiB of reassembly
    /// memory and 64 neighbour entries. They are small enough for a server and large enough for
    /// the conformance tests; every value is caller-overridable.
    fn default() -> Self {
        Self {
            hardware_addr: None,
            max_sockets: 64,
            rx_queue_bytes: 64 * 1024,
            tx_queue_bytes: 64 * 1024,
            reassembly_bytes: 256 * 1024,
            neighbor_cache_entries: 64,
        }
    }
}

/// A timer identifier, unique for the lifetime of the stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimerId(pub u64);

pub(crate) struct Timer {
    pub(crate) id: TimerId,
    pub(crate) deadline: Instant,
}

/// A standalone TCP/IP stack over one [`Device`].
pub struct Stack<D: Device> {
    pub(crate) device: D,
    pub(crate) config: StackConfig,
    pub(crate) sockets: SocketTable,
    pub(crate) events: VecDeque<Event>,
    pub(crate) timers: Vec<Timer>,
    pub(crate) next_timer_id: u64,
    pub(crate) expired: Vec<TimerId>,
    pub(crate) drops: usize,
    pub(crate) now: Instant,
    #[allow(dead_code)]
    pub(crate) rng: Generator,
    #[cfg(any(test, feature = "testutil"))]
    pub(crate) trace: Vec<Vec<u8>>,
    #[cfg(any(test, feature = "testutil"))]
    pub(crate) trace_enabled: bool,
}

impl<D: Device> Stack<D> {
    /// Create a stack over `device`.
    ///
    /// `seed` is the only entropy source; the same seed, inputs and `now` sequence reproduce the
    /// same bytes and event trace. `now` is the initial monotonic time.
    #[must_use]
    pub fn new(device: D, config: StackConfig, seed: u64, now: Instant) -> Self {
        let rng = Generator::from_seed(&seed_from_entropy(seed, 0));
        Self {
            device,
            config,
            sockets: SocketTable::new(config.max_sockets),
            events: VecDeque::new(),
            timers: Vec::new(),
            next_timer_id: 1,
            expired: Vec::new(),
            drops: 0,
            now,
            rng,
            #[cfg(any(test, feature = "testutil"))]
            trace: Vec::new(),
            #[cfg(any(test, feature = "testutil"))]
            trace_enabled: false,
        }
    }

    /// The configuration this stack was created with.
    #[must_use]
    pub const fn config(&self) -> &StackConfig {
        &self.config
    }

    /// The device capabilities.
    #[must_use]
    pub fn capabilities(&self) -> crate::DeviceCaps {
        self.device.capabilities()
    }

    /// Drain the device, run timers and refresh readiness transitions.
    ///
    /// The protocol pipeline is empty in this skeleton: received frames are counted and dropped.
    pub fn poll(&mut self, now: Instant) -> PollResult {
        self.now = now;
        let mut result = PollResult::default();

        let mut index = 0;
        while index < self.timers.len() {
            if self.timers[index].deadline <= now {
                let timer = self.timers.remove(index);
                self.expired.push(timer.id);
            } else {
                index += 1;
            }
        }

        #[cfg(any(test, feature = "testutil"))]
        let trace_enabled = self.trace_enabled;
        let device = &mut self.device;
        #[cfg(any(test, feature = "testutil"))]
        let trace = &mut self.trace;
        loop {
            let mut count = 0usize;
            let received = device.receive(now, &mut |_frame| {
                count += 1;
                #[cfg(any(test, feature = "testutil"))]
                if trace_enabled {
                    trace.push(_frame.to_vec());
                }
            });
            result.rx_frames = result.rx_frames.saturating_add(count);
            if !received || count == 0 {
                break;
            }
        }

        self.refresh_events();
        result.dropped = self.drops;
        self.drops = 0;
        result
    }

    /// Return how long the caller may sleep before the next [`Self::poll`], or `None` when no
    /// timer is armed.
    #[must_use]
    pub fn next_poll_delay(&mut self, now: Instant) -> Option<Duration> {
        let mut earliest: Option<Instant> = None;
        for timer in &self.timers {
            earliest = Some(match earliest {
                Some(deadline) if deadline <= timer.deadline => deadline,
                _ => timer.deadline,
            });
        }
        let deadline = earliest?;
        if deadline <= now {
            Some(Duration::ZERO)
        } else {
            Some(deadline.saturating_since(now))
        }
    }

    /// Arm a timer. A deadline at or before `now` fires on the next poll.
    ///
    /// Returns [`Errno::ENOMEM`] when [`MAX_TIMERS`] are already armed.
    pub fn set_timer(&mut self, deadline: Instant) -> Result<TimerId, Errno> {
        if self.timers.len() >= MAX_TIMERS {
            return Err(Errno::ENOMEM);
        }
        let id = TimerId(self.next_timer_id);
        self.next_timer_id = self.next_timer_id.wrapping_add(1);
        self.timers.push(Timer { id, deadline });
        Ok(id)
    }

    /// Cancel an armed timer. Returns whether it was found.
    pub fn cancel_timer(&mut self, id: TimerId) -> bool {
        match self.timers.iter().position(|timer| timer.id == id) {
            Some(index) => {
                self.timers.remove(index);
                true
            }
            None => false,
        }
    }

    /// Drain timers that expired in previous polls.
    pub fn take_expired_timers(&mut self) -> impl Iterator<Item = TimerId> + '_ {
        self.expired.drain(..)
    }

    /// Drain readiness events accumulated since the previous call.
    pub fn take_events(&mut self) -> impl Iterator<Item = Event> + '_ {
        self.events.drain(..)
    }

    /// The number of armed timers.
    #[must_use]
    pub fn timer_count(&self) -> usize {
        self.timers.len()
    }

    fn refresh_events(&mut self) {
        let mut events: Vec<Event> = Vec::new();
        for (raw_id, record) in &mut self.sockets.records {
            let id = SocketId(*raw_id);
            let readable = record.forced_readable
                || !record.rx.is_empty()
                || match &record.kind {
                    Kind::Udp { datagrams, .. } => !datagrams.is_empty(),
                    _ => false,
                };
            let writable = !record.closed
                && !record.tx_shutdown
                && record.tx.len() < record.tx_cap
                && !matches!(record.kind, Kind::Listener { .. });
            let closed = record.closed || record.peer_closed;

            if readable && !record.prev_readable {
                events.push(Event::Readable(id));
            }
            record.prev_readable = readable;
            if writable && !record.prev_writable {
                events.push(Event::Writable(id));
            }
            record.prev_writable = writable;
            if closed && !record.prev_closed {
                events.push(Event::Closed(id));
            }
            record.prev_closed = closed;
            if let Some(error) = record.error
                && record.prev_error != Some(error)
            {
                events.push(Event::Error(id, error));
                record.prev_error = Some(error);
            }
            if let Kind::Listener {
                pending, reported, ..
            } = &mut record.kind
            {
                for connection in pending.iter().skip(*reported) {
                    events.push(Event::Accepted {
                        listener: id,
                        conn: *connection,
                    });
                }
                *reported = pending.len();
            }
        }
        self.events.extend(events);
    }
}
