// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Packet-buffer DMA rules for GENET.
//!
//! The device sees the bus windows from the `/scb` `dma-ranges`; the CPU sees physical memory.
//! A buffer that fits one window is programmed with its translated bus address. A buffer outside
//! every window (or crossing a window edge) needs a bounce buffer that does fit; the caller
//! supplies that buffer and the model reports whether a copy is required. GENET is treated as
//! non-coherent: a ToDevice buffer is cleaned and a FromDevice buffer is invalidated unless the
//! platform explicitly declares the domain coherent.

use crate::dtb::DmaMap;

/// AArch64 cache line used for DMA ownership changes.
pub const DMA_CACHE_LINE: usize = 64;
/// Largest packet buffer the GENET rings use.
pub const MAX_DMA_BUFFER: usize = 2048;

/// Why a buffer cannot be routed to the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaError {
    /// A DMA buffer must carry at least one byte.
    ZeroLength,
    /// The length exceeds one packet buffer.
    TooLong(usize),
    /// The CPU address is zero or not cache-line aligned.
    Unaligned(u64),
    /// No bounce buffer was supplied for an unmapped range.
    BounceRequired,
    /// The supplied bounce buffer is shorter than the request.
    BounceTooSmall {
        /// Length supplied.
        provided: usize,
        /// Length required.
        needed: usize,
    },
    /// The supplied bounce buffer is not itself directly addressable.
    BounceNotDirect,
    /// The supplied bounce buffer's bus address does not match the DMA window.
    BounceBusMismatch,
}

/// The direction of a DMA buffer transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaDirection {
    /// CPU writes the frame and the device reads it.
    ToDevice,
    /// The device writes the frame and the CPU reads it.
    FromDevice,
}

/// Cache maintenance required around one transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaMaintenance {
    /// Clean the CPU cache before the device reads the buffer.
    pub clean: bool,
    /// Invalidate the CPU cache before the CPU reads the buffer.
    pub invalidate: bool,
}

impl DmaMaintenance {
    /// No maintenance, for a coherent domain.
    pub const NONE: Self = Self {
        clean: false,
        invalidate: false,
    };
}

/// A packet buffer mapped for the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaMapping {
    /// CPU physical address of the bytes the CPU reads or writes.
    pub cpu_addr: u64,
    /// Device-visible bus address.
    pub bus_addr: u64,
    /// Number of bytes used.
    pub len: usize,
    /// True when the CPU address and the bus address refer to different buffers.
    pub bounce: bool,
}

impl DmaMapping {
    /// The maintenance a non-coherent device needs for one transfer.
    #[must_use]
    pub const fn maintenance(self, direction: DmaDirection, coherent: bool) -> DmaMaintenance {
        if coherent {
            return DmaMaintenance::NONE;
        }
        match direction {
            DmaDirection::ToDevice => DmaMaintenance {
                clean: true,
                invalidate: false,
            },
            DmaDirection::FromDevice => DmaMaintenance {
                clean: false,
                invalidate: true,
            },
        }
    }
}

/// The DMA policy for one device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmaRules {
    map: DmaMap,
    coherent: bool,
}

impl DmaRules {
    /// A non-coherent policy over the device-tree DMA map.
    #[must_use]
    pub const fn new(map: DmaMap) -> Self {
        Self {
            map,
            coherent: false,
        }
    }

    /// A policy with an explicit coherence flag.
    #[must_use]
    pub const fn with_coherence(map: DmaMap, coherent: bool) -> Self {
        Self { map, coherent }
    }

    /// The DMA map.
    #[must_use]
    pub const fn map(self) -> DmaMap {
        self.map
    }

    /// True when the device is treated as cache-coherent.
    #[must_use]
    pub const fn coherent(self) -> bool {
        self.coherent
    }

    /// Route one packet buffer.
    ///
    /// `bounce` is an already-allocated buffer the caller can use for a copy when the requested
    /// range is not directly addressable. The returned mapping is either the request itself or
    /// the bounce buffer with [`DmaMapping::bounce`] set.
    pub fn route(
        self,
        cpu_addr: u64,
        len: usize,
        bounce: Option<DmaMapping>,
    ) -> Result<DmaMapping, DmaError> {
        if len == 0 {
            return Err(DmaError::ZeroLength);
        }
        if len > MAX_DMA_BUFFER {
            return Err(DmaError::TooLong(len));
        }
        if cpu_addr == 0 || !cpu_addr.is_multiple_of(DMA_CACHE_LINE as u64) {
            return Err(DmaError::Unaligned(cpu_addr));
        }
        if let Some(bus_addr) = direct_bus(self.map, cpu_addr, len) {
            return Ok(DmaMapping {
                cpu_addr,
                bus_addr,
                len,
                bounce: false,
            });
        }
        let Some(bounce) = bounce else {
            return Err(DmaError::BounceRequired);
        };
        if bounce.len < len {
            return Err(DmaError::BounceTooSmall {
                provided: bounce.len,
                needed: len,
            });
        }
        if bounce.bounce
            || bounce.cpu_addr == 0
            || !bounce.cpu_addr.is_multiple_of(DMA_CACHE_LINE as u64)
        {
            return Err(DmaError::BounceNotDirect);
        }
        let Some(bus_addr) = direct_bus(self.map, bounce.cpu_addr, len) else {
            return Err(DmaError::BounceNotDirect);
        };
        if bus_addr != bounce.bus_addr {
            return Err(DmaError::BounceBusMismatch);
        }
        Ok(DmaMapping {
            cpu_addr: bounce.cpu_addr,
            bus_addr,
            len,
            bounce: true,
        })
    }
}

fn direct_bus(map: DmaMap, cpu_addr: u64, len: usize) -> Option<u64> {
    if map.is_identity() {
        cpu_addr.checked_add(len as u64)?;
        return Some(cpu_addr);
    }
    let end = cpu_addr.checked_add(len as u64)?;
    for window in map.as_slice() {
        let window_end = window.cpu_base.checked_add(window.size)?;
        if cpu_addr >= window.cpu_base && end <= window_end {
            return window.bus_base.checked_add(cpu_addr - window.cpu_base);
        }
    }
    None
}

#[cfg(test)]
#[path = "dma_ut.rs"]
mod tests;
