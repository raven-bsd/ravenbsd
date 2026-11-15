// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! BCM2835 DMA control blocks for the PWM FIFO audio ring.
//!
//! Four (or more) 32-byte control blocks live in the same DMA window as the duty buffer. Each
//! block transfers one period of 32-bit duty words to the PWM FIFO and points at the next block;
//! the last one wraps to the first. All addresses are device-visible 32-bit DMA addresses.

use crate::registers::{
    DMA_CHANNEL_STRIDE, DMA_CHANNELS, DMA_CS_ABORT, DMA_CS_ACTIVE, DMA_CS_ERROR, DMA_CS_INTMASK,
    DMA_CS_RESET, DMA_ENABLE, DMA_INT_STATUS, PWM_FIFO_BUS,
};

/// Size of one DMA control block in bytes.
pub const CONTROL_BLOCK_LEN: usize = 32;
/// Alignment of a control block in DMA memory.
pub const CONTROL_BLOCK_ALIGN: usize = 32;
/// Transfer-info word for the audio path.
pub const TI_AUDIO: u32 = 0x0405_2149;
/// Maximum number of control blocks in one ring.
pub const MAX_PERIODS: u16 = 16;
/// PWM FIFO destination bus address.
pub const FIFO_BUS: u32 = PWM_FIFO_BUS as u32;

/// One 32-byte DMA control block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlBlock {
    /// Transfer information.
    pub ti: u32,
    /// Source bus address.
    pub source: u32,
    /// Destination bus address.
    pub dest: u32,
    /// Transfer length in bytes.
    pub length: u32,
    /// Source/destination stride.
    pub stride: u32,
    /// Next control block bus address.
    pub next: u32,
}

impl ControlBlock {
    /// Build an audio block for one duty period.
    #[must_use]
    pub const fn audio(source: u32, length: u32, next: u32) -> Self {
        Self {
            ti: TI_AUDIO,
            source,
            dest: FIFO_BUS,
            length,
            stride: 0,
            next,
        }
    }

    /// Encode the block in the little-endian DMA layout.
    #[must_use]
    pub const fn encode(self) -> [u8; CONTROL_BLOCK_LEN] {
        let mut out = [0u8; CONTROL_BLOCK_LEN];
        let words = [
            self.ti,
            self.source,
            self.dest,
            self.length,
            self.stride,
            self.next,
        ];
        let mut index = 0usize;
        while index < words.len() {
            let bytes = words[index].to_le_bytes();
            let start = index * 4;
            out[start] = bytes[0];
            out[start + 1] = bytes[1];
            out[start + 2] = bytes[2];
            out[start + 3] = bytes[3];
            index += 1;
        }
        out
    }

    /// Decode a control block from the little-endian DMA layout.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut words = [0u32; 6];
        for (index, word) in words.iter_mut().enumerate() {
            let start = index * 4;
            let chunk = bytes.get(start..start + 4)?;
            *word = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        Some(Self {
            ti: words[0],
            source: words[1],
            dest: words[2],
            length: words[3],
            stride: words[4],
            next: words[5],
        })
    }
}

/// Why a DMA ring geometry was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaError {
    /// The period count or frame count is zero.
    ZeroPeriod,
    /// The period count is above [`MAX_PERIODS`].
    TooManyPeriods,
    /// The period has more frames than one 32-bit length can express.
    TooManyFrames,
    /// A control-block or duty-buffer offset/length is misaligned.
    Unaligned,
    /// The DMA window cannot hold the control blocks and duty buffer.
    RegionTooSmall,
    /// A bus address is above the 32-bit DMA field.
    AddressOverflow,
    /// The DMA channel is outside 0..14.
    InvalidChannel,
    /// The requested control block is outside the ring.
    NoBlock,
    /// The channel did not leave reset/abort within the bounded poll.
    ResetTimeout,
}

/// A circular DMA control-block and duty-buffer ring inside one DMA window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingGeometry {
    /// Device-visible base address of the DMA window.
    pub dma_bus_addr: u32,
    /// Byte offset of the first control block.
    pub cb_offset: usize,
    /// Byte offset of the first duty period.
    pub duty_offset: usize,
    /// Frames in one period.
    pub period_frames: u32,
    /// Number of DMA control blocks.
    pub period_count: u16,
    /// Main DMA controller channel.
    pub channel: u8,
    /// DMA window length.
    pub dma_len: usize,
}

impl RingGeometry {
    /// Validate and build a ring geometry.
    pub fn new(
        dma_bus_addr: u32,
        dma_len: usize,
        cb_offset: usize,
        duty_offset: usize,
        period_frames: u32,
        period_count: u16,
        channel: u8,
    ) -> Result<Self, DmaError> {
        if period_frames == 0 || period_count == 0 {
            return Err(DmaError::ZeroPeriod);
        }
        if period_count > MAX_PERIODS {
            return Err(DmaError::TooManyPeriods);
        }
        if !cb_offset.is_multiple_of(CONTROL_BLOCK_ALIGN) || !duty_offset.is_multiple_of(64) {
            return Err(DmaError::Unaligned);
        }
        if channel >= DMA_CHANNELS {
            return Err(DmaError::InvalidChannel);
        }
        let period_bytes = period_bytes(period_frames)?;
        let duty_bytes = period_bytes
            .checked_mul(usize::from(period_count))
            .ok_or(DmaError::TooManyFrames)?;
        let cb_bytes = CONTROL_BLOCK_LEN
            .checked_mul(usize::from(period_count))
            .ok_or(DmaError::TooManyFrames)?;
        let cb_end = cb_offset
            .checked_add(cb_bytes)
            .ok_or(DmaError::RegionTooSmall)?;
        let duty_end = duty_offset
            .checked_add(duty_bytes)
            .ok_or(DmaError::RegionTooSmall)?;
        if cb_end > dma_len || duty_end > dma_len {
            return Err(DmaError::RegionTooSmall);
        }
        if cb_end > duty_offset && duty_end > cb_offset {
            return Err(DmaError::RegionTooSmall);
        }
        if u64::from(dma_bus_addr)
            .checked_add(duty_end as u64)
            .is_none_or(|end| end > u32::MAX as u64)
        {
            return Err(DmaError::AddressOverflow);
        }
        Ok(Self {
            dma_bus_addr,
            cb_offset,
            duty_offset,
            period_frames,
            period_count,
            channel,
            dma_len,
        })
    }

    /// Bytes in one duty period.
    pub fn period_bytes(&self) -> usize {
        period_bytes(self.period_frames).unwrap_or(0)
    }

    /// Bytes in the whole duty buffer.
    pub fn duty_bytes(&self) -> usize {
        self.period_bytes()
            .saturating_mul(usize::from(self.period_count))
    }

    /// Bytes in the control-block ring.
    pub fn cb_bytes(&self) -> usize {
        CONTROL_BLOCK_LEN * usize::from(self.period_count)
    }

    /// Bus address of control block `index`, or `None` when outside the ring.
    pub fn cb_bus(&self, index: u16) -> Option<u32> {
        if index >= self.period_count {
            return None;
        }
        let offset = self
            .cb_offset
            .checked_add(CONTROL_BLOCK_LEN * usize::from(index))?;
        self.dma_bus_addr.checked_add(offset as u32)
    }

    /// Bus address of duty period `index`, or `None` when outside the ring.
    pub fn duty_bus(&self, index: u16) -> Option<u32> {
        if index >= self.period_count {
            return None;
        }
        let offset = self
            .duty_offset
            .checked_add(self.period_bytes() * usize::from(index))?;
        self.dma_bus_addr.checked_add(offset as u32)
    }

    /// Control block `index`, or `None` when outside the ring.
    pub fn control_block(&self, index: u16) -> Option<ControlBlock> {
        let source = self.duty_bus(index)?;
        let next_index = if index + 1 == self.period_count {
            0
        } else {
            index + 1
        };
        let next = self.cb_bus(next_index)?;
        Some(ControlBlock::audio(
            source,
            self.period_bytes() as u32,
            next,
        ))
    }

    /// The DMA channel register base offset in the controller window.
    pub fn channel_base(&self) -> Option<usize> {
        if self.channel >= DMA_CHANNELS {
            return None;
        }
        Some(DMA_CHANNEL_STRIDE * usize::from(self.channel))
    }

    /// The device bit in the global `ENABLE` register.
    pub fn enable_bit(&self) -> u32 {
        1 << self.channel
    }

    /// The global DMA `ENABLE` register offset.
    pub const fn enable_register() -> usize {
        DMA_ENABLE
    }

    /// The global DMA interrupt-status register offset.
    pub const fn interrupt_status_register() -> usize {
        DMA_INT_STATUS
    }
}

/// Bytes for one stereo S16LE period expressed as duty words.
pub fn period_bytes(period_frames: u32) -> Result<usize, DmaError> {
    if period_frames == 0 {
        return Err(DmaError::ZeroPeriod);
    }
    let frames = usize::try_from(period_frames).map_err(|_| DmaError::TooManyFrames)?;
    frames.checked_mul(8).ok_or(DmaError::TooManyFrames)
}

/// The channel CS flags to clear after one end/interrupt.
pub const CS_COMPLETION_CLEAR: u32 = DMA_CS_INTMASK | DMA_CS_ERROR;

/// Reset the channel: set `RESET`, then clear it.
pub const CS_RESET_START: u32 = DMA_CS_RESET;

/// Abort an active channel.
pub const CS_ABORT: u32 = DMA_CS_ABORT;

/// Start an active channel after a reset.
pub const CS_ACTIVE: u32 = DMA_CS_ACTIVE;

#[cfg(test)]
#[path = "dma_ut.rs"]
mod tests;
