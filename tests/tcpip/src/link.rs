// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A deterministic two-endpoint simulated link.

use std::cell::RefCell;
use std::rc::Rc;

use raven_prng::{Generator, seed_from_entropy};
use raven_tcpip::{ChecksumOffload, Device, DeviceCaps, Instant, Medium, TxError};

use crate::trace::{Direction, Trace};

/// Simulated link properties.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkConfig {
    /// Round-trip time; each direction adds half of it.
    pub rtt_ms: u64,
    /// Optional bit rate. Frames are spaced by their serialization time.
    pub bandwidth_bps: Option<u64>,
    /// Maximum frame size; larger frames are rejected with [`TxError::TooLong`].
    pub mtu: usize,
    /// Percentage of frames lost.
    pub loss_percent: f32,
    /// Percentage of frames duplicated.
    pub duplicate_percent: f32,
    /// Percentage of frames given an extra random delay.
    pub reorder_percent: f32,
    /// Percentage of frames with one random bit flipped.
    pub corrupt_percent: f32,
    /// Byte cap for each direction's in-flight queue.
    pub queue_bytes: usize,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            rtt_ms: 100,
            bandwidth_bps: None,
            mtu: 1500,
            loss_percent: 0.0,
            duplicate_percent: 0.0,
            reorder_percent: 0.0,
            corrupt_percent: 0.0,
            queue_bytes: 256 * 1024,
        }
    }
}

struct Frame {
    due: i64,
    order: u64,
    data: Vec<u8>,
}

struct Wire {
    config: LinkConfig,
    rng: Generator,
    queues: [Vec<Frame>; 2],
    queued_bytes: [usize; 2],
    next_departure: [i64; 2],
    next_order: u64,
    trace: Trace,
    drops: usize,
}

impl Wire {
    fn roll(&mut self, percent: f32) -> bool {
        if percent <= 0.0 {
            return false;
        }
        if percent >= 100.0 {
            return true;
        }
        let mut bytes = [0u8; 8];
        self.rng.fill(&mut bytes);
        let value = u64::from_le_bytes(bytes);
        let basis_points = (percent * 100.0) as u64;
        value % 10_000 < basis_points
    }

    fn random_below(&mut self, limit: u64) -> u64 {
        if limit == 0 {
            return 0;
        }
        let mut bytes = [0u8; 8];
        self.rng.fill(&mut bytes);
        u64::from_le_bytes(bytes) % limit
    }
}

/// A handle to a link, usable for configuration and trace inspection.
#[derive(Clone)]
pub struct Link {
    wire: Rc<RefCell<Wire>>,
}

impl Link {
    /// Create a link and its two endpoints.
    ///
    /// The C04 contract names this constructor `new` even though it returns the endpoint pair.
    #[must_use]
    #[allow(clippy::new_ret_no_self)]
    pub fn new(config: LinkConfig, seed: u64) -> (Endpoint, Endpoint) {
        let wire = Rc::new(RefCell::new(Wire {
            config,
            rng: Generator::from_seed(&seed_from_entropy(seed, 0)),
            queues: [Vec::new(), Vec::new()],
            queued_bytes: [0, 0],
            next_departure: [0, 0],
            next_order: 0,
            trace: Trace::new(),
            drops: 0,
        }));
        (
            Endpoint {
                wire: Rc::clone(&wire),
                side: 0,
            },
            Endpoint {
                wire: Rc::clone(&wire),
                side: 1,
            },
        )
    }

    /// Change the link properties mid-test.
    pub fn set_config(&self, config: LinkConfig) {
        self.wire.borrow_mut().config = config;
    }

    /// The current properties.
    #[must_use]
    pub fn config(&self) -> LinkConfig {
        self.wire.borrow().config
    }

    /// The trace of frames put on the wire.
    #[must_use]
    pub fn trace(&self) -> Trace {
        self.wire.borrow().trace.clone()
    }

    /// The trace rendered as stable text.
    #[must_use]
    pub fn trace_text(&self) -> String {
        self.wire.borrow().trace.to_text()
    }

    /// Frames dropped because the queue was full.
    #[must_use]
    pub fn drops(&self) -> usize {
        self.wire.borrow().drops
    }

    /// In-flight bytes in each direction.
    #[must_use]
    pub fn queued_bytes(&self) -> (usize, usize) {
        let wire = self.wire.borrow();
        (wire.queued_bytes[0], wire.queued_bytes[1])
    }

    /// The earliest virtual delivery time, or `None` when both directions are empty.
    #[must_use]
    pub fn next_delivery(&self, now: Instant) -> Option<Instant> {
        let wire = self.wire.borrow();
        let mut earliest: Option<i64> = None;
        for queue in &wire.queues {
            for frame in queue {
                earliest = Some(match earliest {
                    Some(current) if current <= frame.due => current,
                    _ => frame.due,
                });
            }
        }
        let due = earliest?;
        Some(Instant::from_millis(due.max(now.as_millis())))
    }
}

/// One end of a [`Link`].
pub struct Endpoint {
    wire: Rc<RefCell<Wire>>,
    side: usize,
}

impl Endpoint {
    /// A handle to the shared link.
    #[must_use]
    pub fn link(&self) -> Link {
        Link {
            wire: Rc::clone(&self.wire),
        }
    }

    /// Whether this endpoint is side A.
    #[must_use]
    pub fn is_a(&self) -> bool {
        self.side == 0
    }

    /// Bytes waiting to be received by this endpoint.
    #[must_use]
    pub fn incoming_bytes(&self) -> usize {
        self.wire.borrow().queued_bytes[self.side]
    }
}

impl Device for Endpoint {
    fn capabilities(&self) -> DeviceCaps {
        let config = self.wire.borrow().config;
        DeviceCaps {
            medium: Medium::Ethernet,
            mtu: config.mtu,
            hardware_addr: Some(if self.side == 0 {
                [0x02, 0, 0, 0, 0, 0x0a]
            } else {
                [0x02, 0, 0, 0, 0, 0x0b]
            }),
            checksum_offload: ChecksumOffload::None,
        }
    }

    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let mut wire = self.wire.borrow_mut();
        let queue = &mut wire.queues[self.side];
        let mut selected: Option<usize> = None;
        for (index, frame) in queue.iter().enumerate() {
            if frame.due > now.as_millis() {
                continue;
            }
            selected = Some(match selected {
                Some(current) => {
                    let other = &queue[current];
                    if (frame.due, frame.order) < (other.due, other.order) {
                        index
                    } else {
                        current
                    }
                }
                None => index,
            });
        }
        let Some(index) = selected else {
            return false;
        };
        let frame = queue.remove(index);
        wire.queued_bytes[self.side] =
            wire.queued_bytes[self.side].saturating_sub(frame.data.len());
        consume(&frame.data);
        true
    }

    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        let mut wire = self.wire.borrow_mut();
        let config = wire.config;
        if len > config.mtu {
            wire.drops = wire.drops.saturating_add(1);
            return Err(TxError::TooLong);
        }
        let mut data = vec![0u8; len];
        fill(&mut data);

        if wire.roll(config.corrupt_percent) && len > 0 {
            let byte = wire.random_below(len as u64) as usize;
            let bit = wire.random_below(8) as u8;
            if let Some(value) = data.get_mut(byte) {
                *value ^= 1 << bit;
            }
        }

        let direction = if self.side == 0 {
            Direction::AtoB
        } else {
            Direction::BtoA
        };
        wire.trace.record(direction, now, &data);

        let lost = wire.roll(config.loss_percent);
        let duplicate = wire.roll(config.duplicate_percent);
        let reorder = wire.roll(config.reorder_percent);

        let departure = now.as_millis().max(wire.next_departure[self.side]);
        if let Some(bandwidth) = config.bandwidth_bps.filter(|value| *value > 0) {
            let bits = (len as u64).saturating_mul(8);
            let serialization_ms = bits.saturating_mul(1000).div_ceil(bandwidth);
            let serialization_ms = i64::try_from(serialization_ms).unwrap_or(i64::MAX);
            wire.next_departure[self.side] = departure.saturating_add(serialization_ms);
        }

        let base_due = departure.saturating_add(i64::try_from(config.rtt_ms / 2).unwrap_or(0));
        let mut due = base_due;
        if reorder {
            let extra = wire.random_below(config.rtt_ms.saturating_add(1));
            due = due.saturating_add(i64::try_from(extra).unwrap_or(0));
        }

        if lost {
            return Ok(());
        }

        let destination = 1 - self.side;
        let copies = if duplicate { 2 } else { 1 };
        for _ in 0..copies {
            if wire.queued_bytes[destination].saturating_add(data.len()) > config.queue_bytes {
                wire.drops = wire.drops.saturating_add(1);
                continue;
            }
            let order = wire.next_order;
            wire.next_order = wire.next_order.wrapping_add(1);
            wire.queued_bytes[destination] =
                wire.queued_bytes[destination].saturating_add(data.len());
            wire.queues[destination].push(Frame {
                due,
                order,
                data: data.clone(),
            });
        }
        Ok(())
    }
}
