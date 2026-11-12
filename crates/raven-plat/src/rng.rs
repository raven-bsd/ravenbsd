// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RNG200 health/read state machine.
//!
//! The hardware block is polled, never blocked on: a request is bounded by
//! [`MAX_RNG_BYTES`], the status bits are checked before each read, and an empty FIFO returns
//! `EAGAIN` instead of spinning in the server loop. `platd` implements [`Registers`] over the
//! mapped MMIO capability; the fake backend implements [`Rng`] directly.

use raven_abi::errno::Errno;

/// Largest RNG read one request may ask for.
pub const MAX_RNG_BYTES: usize = 256;

/// RNG200 `CONTROL`.
pub const RNG_CONTROL: usize = 0x00;
/// RNG200 `RNG_RESET`.
pub const RNG_RNG_RESET: usize = 0x04;
/// RNG200 `RBG_RESET`.
pub const RNG_RBG_RESET: usize = 0x08;
/// RNG200 `STATUS`.
pub const RNG_STATUS: usize = 0x18;
/// RNG200 `DATA`.
pub const RNG_DATA: usize = 0x20;
/// RNG200 `COUNT`.
pub const RNG_COUNT: usize = 0x24;

/// RBG mask field in `CONTROL`.
pub const CONTROL_RBG_MASK: u32 = 0x1fff;
/// RBG enable bit in `CONTROL`.
pub const CONTROL_RBG_ENABLE: u32 = 1 << 1;
/// `STATUS` master failure bit.
pub const STATUS_MASTER_FAIL: u32 = 1 << 31;
/// `STATUS` NIST failure bit.
pub const STATUS_NIST_FAIL: u32 = 1 << 5;

/// Bounded number of FIFO words read per request.
const MAX_WORDS: usize = MAX_RNG_BYTES / 4;

/// A bounded entropy source.
pub trait Rng {
    /// Fill up to `out.len()` bytes; returns the number written.
    fn fill(&mut self, out: &mut [u8]) -> Result<usize, Errno>;
}

/// Register access used by [`Rng200`].
pub trait Registers {
    /// Read one 32-bit register.
    fn read32(&mut self, offset: usize) -> Result<u32, Errno>;
    /// Write one 32-bit register.
    fn write32(&mut self, offset: usize, value: u32) -> Result<(), Errno>;
}

/// RNG200 driver over a bounded register window.
pub struct Rng200<R> {
    registers: R,
}

impl<R: Registers> Rng200<R> {
    /// Wrap the RNG200 register window.
    pub const fn new(registers: R) -> Self {
        Self { registers }
    }

    /// Reset and enable the RBG. Call once before [`Rng::fill`].
    pub fn init(&mut self) -> Result<(), Errno> {
        let control = self.registers.read32(RNG_CONTROL)?;
        self.registers
            .write32(RNG_CONTROL, control & !CONTROL_RBG_MASK)?;
        self.registers.write32(RNG_STATUS, u32::MAX)?;
        let rbg = self.registers.read32(RNG_RBG_RESET)?;
        self.registers.write32(RNG_RBG_RESET, rbg | 1)?;
        let rng = self.registers.read32(RNG_RNG_RESET)?;
        self.registers.write32(RNG_RNG_RESET, rng | 1)?;
        let control = self.registers.read32(RNG_CONTROL)?;
        self.registers
            .write32(RNG_CONTROL, control | CONTROL_RBG_ENABLE)
    }

    fn healthy(&mut self) -> Result<bool, Errno> {
        let status = self.registers.read32(RNG_STATUS)?;
        Ok(status & (STATUS_MASTER_FAIL | STATUS_NIST_FAIL) == 0)
    }
}

impl<R: Registers> Rng for Rng200<R> {
    fn fill(&mut self, out: &mut [u8]) -> Result<usize, Errno> {
        if out.is_empty() {
            return Ok(0);
        }
        if out.len() > MAX_RNG_BYTES {
            return Err(Errno::E2BIG);
        }
        let mut written = 0usize;
        let mut words = 0usize;
        while written < out.len() && words < MAX_WORDS {
            if !self.healthy()? {
                let _ = self.init();
                return Err(Errno::EIO);
            }
            let count = self.registers.read32(RNG_COUNT)? & 0xff;
            if count == 0 {
                break;
            }
            let word = self.registers.read32(RNG_DATA)?;
            let take = core::cmp::min(4, out.len() - written);
            if let (Some(destination), Some(source)) = (
                out.get_mut(written..written + take),
                word.to_le_bytes().get(..take),
            ) {
                destination.copy_from_slice(source);
            }
            written += take;
            words += 1;
        }
        if written == 0 {
            Err(Errno::EAGAIN)
        } else {
            Ok(written)
        }
    }
}

/// A deterministic host/QEMU entropy source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FakeRng {
    state: u64,
}

impl FakeRng {
    /// A fake source with a fixed non-zero seed.
    pub const fn new() -> Self {
        Self {
            state: 0x9e37_79b9_7f4a_7c15,
        }
    }
}

impl Default for FakeRng {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng for FakeRng {
    fn fill(&mut self, out: &mut [u8]) -> Result<usize, Errno> {
        if out.len() > MAX_RNG_BYTES {
            return Err(Errno::E2BIG);
        }
        for chunk in out.chunks_mut(8) {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            let bytes = self.state.to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
        Ok(out.len())
    }
}

#[cfg(test)]
#[path = "rng_ut.rs"]
mod tests;
