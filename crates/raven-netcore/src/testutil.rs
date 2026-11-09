// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! In-memory Ethernet link for host tests.
//!
//! Two [`PairDevice`] endpoints share a queue pair. Loss, reordering and delay are driven by a
//! deterministic PRNG, so a run is reproducible from the configuration seed.

use alloc::collections::VecDeque;
use alloc::rc::Rc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

/// Link behaviour for a [`PairDevice`] pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PairConfig {
    /// Drop probability in parts per thousand (20 = 2%).
    pub loss_per_mille: u16,
    /// Probability, in parts per thousand, of adding an extra delay to a frame.
    pub reorder_per_mille: u16,
    /// Base delivery delay in milliseconds.
    pub delay_ms: u64,
    /// Maximum extra delay in milliseconds.
    pub jitter_ms: u64,
    /// PRNG seed; the same seed and traffic produce the same delivery decisions.
    pub seed: u64,
}

impl PairConfig {
    /// A lossless, zero-delay link.
    pub const fn reliable() -> Self {
        Self {
            loss_per_mille: 0,
            reorder_per_mille: 0,
            delay_ms: 0,
            jitter_ms: 0,
            seed: 0x5eed_1234_5678_9abc,
        }
    }

    /// A lossless link with `delay_ms` one-way delay and random extra jitter.
    pub const fn delayed(delay_ms: u64, jitter_ms: u64, seed: u64) -> Self {
        Self {
            loss_per_mille: 0,
            reorder_per_mille: 0,
            delay_ms,
            jitter_ms,
            seed,
        }
    }
}

impl Default for PairConfig {
    fn default() -> Self {
        Self::reliable()
    }
}

/// One endpoint of a two-endpoint in-memory Ethernet link.
pub struct PairDevice {
    link: Rc<RefCell<Link>>,
    side: Side,
}

impl PairDevice {
    /// Create two connected endpoints.
    pub fn pair(config: PairConfig) -> (Self, Self) {
        let link = Rc::new(RefCell::new(Link::new(config)));
        (
            Self {
                link: Rc::clone(&link),
                side: Side::A,
            },
            Self {
                link,
                side: Side::B,
            },
        )
    }

    /// Replace the link behaviour. The PRNG is reseeded with the new configuration.
    pub fn set_config(&self, config: PairConfig) {
        *self.link.borrow_mut() = Link::new(config);
    }
}

impl Device for PairDevice {
    type RxToken<'a>
        = PairRxToken
    where
        Self: 'a;
    type TxToken<'a>
        = PairTxToken
    where
        Self: 'a;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut capabilities = DeviceCapabilities::default();
        capabilities.medium = Medium::Ethernet;
        capabilities.max_transmission_unit = 1514;
        capabilities
    }

    fn receive(&mut self, timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let bytes = self
            .link
            .borrow_mut()
            .dequeue(self.side, timestamp.total_millis())?;
        Some((
            PairRxToken { bytes },
            PairTxToken {
                link: Rc::clone(&self.link),
                side: self.side,
                timestamp: timestamp.total_millis(),
            },
        ))
    }

    fn transmit(&mut self, timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(PairTxToken {
            link: Rc::clone(&self.link),
            side: self.side,
            timestamp: timestamp.total_millis(),
        })
    }
}

pub struct PairRxToken {
    bytes: Vec<u8>,
}

impl RxToken for PairRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.bytes)
    }
}

pub struct PairTxToken {
    link: Rc<RefCell<Link>>,
    side: Side,
    timestamp: i64,
}

impl TxToken for PairTxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut bytes = vec![0; len];
        let result = f(&mut bytes);
        self.link
            .borrow_mut()
            .enqueue(self.side, self.timestamp, bytes);
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    A,
    B,
}

struct Link {
    a_to_b: VecDeque<Frame>,
    b_to_a: VecDeque<Frame>,
    config: PairConfig,
    rng: u64,
}

struct Frame {
    deliver_at: i64,
    bytes: Vec<u8>,
}

impl Link {
    fn new(config: PairConfig) -> Self {
        Self {
            a_to_b: VecDeque::new(),
            b_to_a: VecDeque::new(),
            config,
            rng: if config.seed == 0 {
                0x9e37_79b9_7f4a_7c15
            } else {
                config.seed
            },
        }
    }

    fn enqueue(&mut self, side: Side, timestamp: i64, bytes: Vec<u8>) {
        if self.chance(self.config.loss_per_mille) {
            return;
        }
        let mut delay = self.config.delay_ms;
        if self.config.jitter_ms > 0 {
            delay = delay.saturating_add(self.next_u64() % (self.config.jitter_ms + 1));
        }
        if self.chance(self.config.reorder_per_mille) {
            delay = delay.saturating_add(self.config.delay_ms.max(1));
        }
        let frame = Frame {
            deliver_at: timestamp.saturating_add(delay as i64),
            bytes,
        };
        match side {
            Side::A => self.a_to_b.push_back(frame),
            Side::B => self.b_to_a.push_back(frame),
        }
    }

    fn dequeue(&mut self, receiver: Side, now: i64) -> Option<Vec<u8>> {
        let queue = match receiver {
            Side::A => &mut self.b_to_a,
            Side::B => &mut self.a_to_b,
        };
        let mut best: Option<(usize, i64)> = None;
        for (index, frame) in queue.iter().enumerate() {
            if frame.deliver_at <= now
                && best.is_none_or(|(_, deliver_at)| frame.deliver_at < deliver_at)
            {
                best = Some((index, frame.deliver_at));
            }
        }
        best.and_then(|(index, _)| queue.remove(index))
            .map(|frame| frame.bytes)
    }

    fn chance(&mut self, per_mille: u16) -> bool {
        per_mille > 0 && (self.next_u64() % 1000) < u64::from(per_mille)
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.rng;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.rng = value;
        value
    }
}
