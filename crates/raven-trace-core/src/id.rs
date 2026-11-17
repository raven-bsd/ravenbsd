// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Trace and span identifiers.

/// Number of counter bits in a [`TraceId`].
pub const COUNTER_BITS: u32 = 40;

/// Mask covering the counter field of a [`TraceId`].
pub const COUNTER_MASK: u64 = (1u64 << COUNTER_BITS) - 1;

/// A 64-bit trace identifier.
///
/// Layout (`R-OBS-7`): `boot_nonce:16 | cpu:8 | counter:40`. Zero means "not traced".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraceId(u64);

impl TraceId {
    /// The "not traced" id.
    pub const ZERO: Self = Self(0);

    /// Pack a trace id from its three fields; `counter` is truncated to 40 bits.
    pub const fn new(boot_nonce: u16, cpu: u8, counter: u64) -> Self {
        Self(((boot_nonce as u64) << 48) | ((cpu as u64) << 40) | (counter & COUNTER_MASK))
    }

    /// Interpret a raw value as a trace id.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw 64-bit value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// The per-boot nonce (bits 63..48).
    pub const fn boot_nonce(self) -> u16 {
        (self.0 >> 48) as u16
    }

    /// The generating CPU (bits 47..40).
    pub const fn cpu(self) -> u8 {
        (self.0 >> 40) as u8
    }

    /// The monotonic counter (bits 39..0).
    pub const fn counter(self) -> u64 {
        self.0 & COUNTER_MASK
    }

    /// Whether this is the "not traced" id.
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::ZERO
    }
}

/// A per-boot, per-CPU trace id generator.
///
/// [`TraceIdGen::next_id`] increments the 40-bit counter and never returns [`TraceId::ZERO`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceIdGen {
    boot_nonce: u16,
    cpu: u8,
    counter: u64,
}

impl TraceIdGen {
    /// A generator whose first [`TraceIdGen::next_id`] returns counter 1.
    pub const fn new(boot_nonce: u16, cpu: u8) -> Self {
        Self {
            boot_nonce,
            cpu,
            counter: 0,
        }
    }

    /// A generator starting at `counter` (truncated to 40 bits).
    pub const fn with_counter(boot_nonce: u16, cpu: u8, counter: u64) -> Self {
        Self {
            boot_nonce,
            cpu,
            counter: counter & COUNTER_MASK,
        }
    }

    /// The configured boot nonce.
    pub const fn boot_nonce(&self) -> u16 {
        self.boot_nonce
    }

    /// The configured CPU.
    pub const fn cpu(&self) -> u8 {
        self.cpu
    }

    /// The counter the next [`TraceIdGen::next_id`] would return (before the increment).
    pub const fn counter(&self) -> u64 {
        self.counter
    }

    /// Produce the next id, advancing the 40-bit counter. The counter wraps to 1 rather than 0,
    /// so the id is never zero.
    pub fn next_id(&mut self) -> TraceId {
        self.counter = (self.counter + 1) & COUNTER_MASK;
        if self.counter == 0 {
            self.counter = 1;
        }
        TraceId::new(self.boot_nonce, self.cpu, self.counter)
    }
}

/// A span identifier, unique within a [`Trace`](crate::Trace).
///
/// Zero is the "no span" value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpanId(u64);

impl SpanId {
    /// The "no span" id.
    pub const NONE: Self = Self(0);

    /// Interpret a raw value as a span id.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// The raw 64-bit value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Whether this is the "no span" id.
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
#[path = "id_ut.rs"]
mod tests;
