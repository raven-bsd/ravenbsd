// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Safe device-region access for RavenBSD userspace drivers (B13a, B13c).
//!
//! Drivers are ordinary `no_std` servers and must keep `#![forbid(unsafe_code)]`, but a device
//! driver has to touch memory the CPU does not treat as ordinary RAM: every register access must
//! be a single volatile access of the right width, and every offset must be inside the mapped
//! window. This crate is the one place that owns that `unsafe`; the review checklist already
//! names `crates/raven-driver` as the allowed home for MMIO and DMA code.
//!
//! The types:
//!
//! * [`MmioRegion`] wraps a **writable** mapping (`&'static mut [u8]`, obtained from the runtime's
//!   `map_mmio_mut`) and offers bounded volatile reads and writes.
//! * [`ConfigRegion`] wraps a **read-only** mapping (`&'static [u8]`, the PCI configuration space
//!   of one function) and offers the bounded reads a capability walk needs.
//! * [`DmaRegion`] wraps a **writable** DMA buffer mapping and remembers the device-visible bus
//!   address; [`DmaRegion::publish`] and [`DmaRegion::acquire`] are the release/acquire fences a
//!   descriptor ring needs around a doorbell.
//! * [`Irq`] wraps the transferred `Irq` handle and exposes `wait` and `ack`.
//!
//! Every region validates offset and alignment before touching memory; an out-of-range or
//! misaligned access is an [`AccessError`], never a fault. The widths follow the device model:
//! 8, 16, 32 and 64 bits, little endian, exactly one machine access each.

#![no_std]
#![allow(unsafe_code)] // volatile device access; every block has a SAFETY comment.
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

use core::sync::atomic::{Ordering, fence};

use raven_abi::types::Handle;

#[cfg(target_os = "none")]
use raven_abi::errno::Errno;

/// Page size used by the kernel's device mappings.
pub const PAGE: usize = 4096;

/// Largest PCI capability chain this crate walks before giving up.
pub const MAX_CAPABILITIES: usize = 48;

/// PCI configuration-space size of one function.
pub const CONFIG_LEN: usize = 4096;

/// Why an access was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessError {
    /// The access would leave the mapped region.
    Bounds,
    /// The natural alignment of the access width was not met.
    Alignment,
}

/// A writable device window.
pub struct MmioRegion {
    window: &'static mut [u8],
}

impl MmioRegion {
    /// Wrap a writable mapping of a device window.
    ///
    /// The caller must pass a region the kernel mapped for this process and must never touch the
    /// underlying bytes through another alias while the region lives.
    pub fn new(window: &'static mut [u8]) -> Self {
        Self { window }
    }

    /// Length of the window in bytes.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// True when the window is empty.
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Read one byte.
    pub fn read_u8(&self, offset: usize) -> Result<u8, AccessError> {
        let pointer = self.pointer(offset, 1, 1)?;
        // SAFETY: `pointer` is inside the mapped window and aligned for a one-byte volatile read.
        Ok(unsafe { core::ptr::read_volatile(pointer) })
    }

    /// Read one 16-bit little-endian register.
    pub fn read_u16(&self, offset: usize) -> Result<u16, AccessError> {
        let pointer = self.pointer(offset, 2, 2)?;
        // SAFETY: `pointer` is inside the mapped window and 2-byte aligned.
        Ok(u16::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u16>())
        }))
    }

    /// Read one 32-bit little-endian register.
    pub fn read_u32(&self, offset: usize) -> Result<u32, AccessError> {
        let pointer = self.pointer(offset, 4, 4)?;
        // SAFETY: `pointer` is inside the mapped window and 4-byte aligned.
        Ok(u32::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u32>())
        }))
    }

    /// Read one 64-bit little-endian register.
    pub fn read_u64(&self, offset: usize) -> Result<u64, AccessError> {
        let pointer = self.pointer(offset, 8, 8)?;
        // SAFETY: `pointer` is inside the mapped window and 8-byte aligned.
        Ok(u64::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u64>())
        }))
    }

    /// Write one byte.
    pub fn write_u8(&mut self, offset: usize, value: u8) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 1, 1)?;
        // SAFETY: `pointer` is inside the mapped window and aligned for a one-byte volatile write.
        unsafe { core::ptr::write_volatile(pointer, value) };
        Ok(())
    }

    /// Write one 16-bit little-endian register.
    pub fn write_u16(&mut self, offset: usize, value: u16) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 2, 2)?;
        // SAFETY: `pointer` is inside the mapped window and 2-byte aligned.
        unsafe { core::ptr::write_volatile(pointer.cast::<u16>(), value.to_le()) };
        Ok(())
    }

    /// Write one 32-bit little-endian register.
    pub fn write_u32(&mut self, offset: usize, value: u32) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 4, 4)?;
        // SAFETY: `pointer` is inside the mapped window and 4-byte aligned.
        unsafe { core::ptr::write_volatile(pointer.cast::<u32>(), value.to_le()) };
        Ok(())
    }

    /// Write one 64-bit little-endian register.
    pub fn write_u64(&mut self, offset: usize, value: u64) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 8, 8)?;
        // SAFETY: `pointer` is inside the mapped window and 8-byte aligned.
        unsafe { core::ptr::write_volatile(pointer.cast::<u64>(), value.to_le()) };
        Ok(())
    }

    /// Copy `bytes` into the window at `offset`.
    pub fn write_bytes(&mut self, offset: usize, bytes: &[u8]) -> Result<(), AccessError> {
        self.pointer(offset, bytes.len(), 1)?;
        for (index, byte) in bytes.iter().enumerate() {
            let position = offset.checked_add(index).ok_or(AccessError::Bounds)?;
            let pointer = self.pointer(position, 1, 1)?;
            // SAFETY: `pointer` is inside the mapped window.
            unsafe { core::ptr::write_volatile(pointer, *byte) };
        }
        Ok(())
    }

    /// Copy the window at `offset` into `out`.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), AccessError> {
        self.pointer(offset, out.len(), 1)?;
        for (index, slot) in out.iter_mut().enumerate() {
            let position = offset.checked_add(index).ok_or(AccessError::Bounds)?;
            let pointer = self.pointer(position, 1, 1)?;
            // SAFETY: `pointer` is inside the mapped window.
            *slot = unsafe { core::ptr::read_volatile(pointer) };
        }
        Ok(())
    }

    fn pointer(&self, offset: usize, len: usize, align: usize) -> Result<*mut u8, AccessError> {
        let base = self.window.as_ptr() as usize;
        checked_pointer(base, self.window.len(), offset, len, align)
    }
}

/// A read-only PCI configuration region of one function.
pub struct ConfigRegion {
    window: &'static [u8],
}

impl ConfigRegion {
    /// Wrap a read-only mapping of one function's 4 KiB configuration space.
    pub fn new(window: &'static [u8]) -> Self {
        Self { window }
    }

    /// Length of the window in bytes.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// True when the window is empty.
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// Read one byte, for example a capability id.
    pub fn read_u8(&self, offset: usize) -> Result<u8, AccessError> {
        let pointer = self.pointer(offset, 1, 1)?;
        // SAFETY: `pointer` is inside the mapped configuration window.
        Ok(unsafe { core::ptr::read_volatile(pointer) })
    }

    /// Read one 16-bit little-endian field, for example the vendor id.
    pub fn read_u16(&self, offset: usize) -> Result<u16, AccessError> {
        let pointer = self.pointer(offset, 2, 2)?;
        // SAFETY: `pointer` is inside the mapped configuration window and 2-byte aligned.
        Ok(u16::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u16>())
        }))
    }

    /// Read one 32-bit little-endian field, for example a BAR.
    pub fn read_u32(&self, offset: usize) -> Result<u32, AccessError> {
        let pointer = self.pointer(offset, 4, 4)?;
        // SAFETY: `pointer` is inside the mapped configuration window and 4-byte aligned.
        Ok(u32::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u32>())
        }))
    }

    /// Vendor id and device id from the standard header.
    pub fn ids(&self) -> Result<(u16, u16), AccessError> {
        Ok((self.read_u16(0x00)?, self.read_u16(0x02)?))
    }

    /// Class and subclass from the standard header.
    pub fn class(&self) -> Result<(u8, u8), AccessError> {
        Ok((self.read_u8(0x0B)?, self.read_u8(0x0A)?))
    }

    /// The first capability id and offset, if the function has a capability list.
    ///
    /// Returns `Ok(None)` when the status register does not advertise a capability list or the
    /// list is empty.
    pub fn first_capability(&self) -> Result<Option<Capability>, AccessError> {
        let status = self.read_u16(0x06)?;
        if status & (1 << 4) == 0 {
            return Ok(None);
        }
        let pointer = self.read_u8(0x34)? & 0xFC;
        if pointer == 0 {
            return Ok(None);
        }
        self.capability(pointer)
    }

    /// Read the capability at `offset`.
    pub fn capability(&self, offset: u8) -> Result<Option<Capability>, AccessError> {
        let offset = usize::from(offset);
        if !offset.is_multiple_of(4) || offset < 0x40 || offset + 1 >= CONFIG_LEN {
            return Ok(None);
        }
        let id = self.read_u8(offset)?;
        if id == 0 {
            return Ok(None);
        }
        let next = self.read_u8(offset + 1)? & 0xFC;
        Ok(Some(Capability {
            id,
            offset: offset as u8,
            next,
        }))
    }

    fn pointer(&self, offset: usize, len: usize, align: usize) -> Result<*const u8, AccessError> {
        let base = self.window.as_ptr() as usize;
        checked_pointer(base, self.window.len(), offset, len, align)
            .map(|pointer| pointer as *const u8)
    }
}

/// One entry of the PCI capability list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    /// Capability id (`0x11` is MSI-X, `0x09` is vendor-specific, ...).
    pub id: u8,
    /// Offset of the capability record in configuration space.
    pub offset: u8,
    /// Offset of the next capability, `0` for the end of the list.
    pub next: u8,
}

/// A bounded capability list.
#[derive(Clone, Copy, Debug)]
pub struct CapabilityList {
    entries: [Capability; MAX_CAPABILITIES],
    len: usize,
}

impl CapabilityList {
    const EMPTY: Capability = Capability {
        id: 0,
        offset: 0,
        next: 0,
    };

    /// An empty list.
    pub const fn new() -> Self {
        Self {
            entries: [Self::EMPTY; MAX_CAPABILITIES],
            len: 0,
        }
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the function has no capabilities.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The entry at `index`.
    pub fn get(&self, index: usize) -> Option<&Capability> {
        if index >= self.len {
            return None;
        }
        self.entries.get(index)
    }

    /// Iterate the entries in list order.
    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.entries.get(..self.len).unwrap_or(&[]).iter()
    }

    fn push(&mut self, entry: Capability) -> bool {
        match self.entries.get_mut(self.len) {
            Some(slot) => {
                *slot = entry;
                self.len += 1;
                true
            }
            None => false,
        }
    }
}

impl Default for CapabilityList {
    fn default() -> Self {
        Self::new()
    }
}

/// Walk at most [`MAX_CAPABILITIES`] entries, starting at the first one.
///
/// PCI capability lists are singly linked and need not be numerically ordered: firmware and
/// emulators can prepend entries, so a valid chain may run from a high offset to a low one. The
/// walk therefore follows every `next` pointer in list order. It is bounded by
/// [`MAX_CAPABILITIES`] and cycle-safe without allocation: one bit per possible 8-bit
/// configuration-space offset is marked before the entry is visited. A zero next pointer, a
/// visited offset, a malformed entry or a next pointer that leaves the mapped configuration
/// region ends the walk (an out-of-range read keeps the existing [`AccessError`]).
pub fn capabilities(region: &ConfigRegion) -> Result<CapabilityList, AccessError> {
    let mut found = CapabilityList::new();
    let Some(mut current) = region.first_capability()? else {
        return Ok(found);
    };
    let mut visited = [0u64; 4];
    loop {
        if !mark_visited(&mut visited, current.offset) || !found.push(current) {
            break;
        }
        if current.next == 0 {
            break;
        }
        match region.capability(current.next)? {
            Some(next) => current = next,
            None => break,
        }
    }
    Ok(found)
}

/// Mark one 8-bit configuration offset; false when it was already visited.
fn mark_visited(visited: &mut [u64; 4], offset: u8) -> bool {
    let index = (offset as usize) / 64;
    let mask = 1u64 << (offset % 64);
    if visited[index] & mask != 0 {
        return false;
    }
    visited[index] |= mask;
    true
}

/// A writable DMA buffer mapping.
///
/// The window comes from `raven_rt::map_mmio_mut` (or any MAP of the transferred `DmaBuffer`
/// handle) and `bus_address` from `raven_rt::dma_info`. Accesses are volatile, bounded and
/// alignment-checked exactly like [`MmioRegion`]; the difference is the recorded bus address and
/// the two fences a device/driver handshake needs.
pub struct DmaRegion {
    window: &'static mut [u8],
    bus_address: u64,
}

impl DmaRegion {
    /// Wrap a writable DMA buffer mapping and its device-visible bus address.
    pub fn new(window: &'static mut [u8], bus_address: u64) -> Self {
        Self {
            window,
            bus_address,
        }
    }

    /// Length of the buffer in bytes.
    pub fn len(&self) -> usize {
        self.window.len()
    }

    /// True when the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.window.is_empty()
    }

    /// The device-visible address of the first byte.
    pub fn bus_address(&self) -> u64 {
        self.bus_address
    }

    /// Release fence: call after writing descriptors and before ringing the doorbell.
    pub fn publish(&self) {
        fence(Ordering::Release);
    }

    /// Acquire fence: call after the device signals completion and before reading the rings.
    pub fn acquire(&self) {
        fence(Ordering::Acquire);
    }

    /// Read one byte.
    pub fn read_u8(&self, offset: usize) -> Result<u8, AccessError> {
        let pointer = self.pointer(offset, 1, 1)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer.
        Ok(unsafe { core::ptr::read_volatile(pointer) })
    }

    /// Read one 16-bit little-endian word.
    pub fn read_u16(&self, offset: usize) -> Result<u16, AccessError> {
        let pointer = self.pointer(offset, 2, 2)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer and 2-byte aligned.
        Ok(u16::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u16>())
        }))
    }

    /// Read one 32-bit little-endian word.
    pub fn read_u32(&self, offset: usize) -> Result<u32, AccessError> {
        let pointer = self.pointer(offset, 4, 4)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer and 4-byte aligned.
        Ok(u32::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u32>())
        }))
    }

    /// Read one 64-bit little-endian word.
    pub fn read_u64(&self, offset: usize) -> Result<u64, AccessError> {
        let pointer = self.pointer(offset, 8, 8)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer and 8-byte aligned.
        Ok(u64::from_le(unsafe {
            core::ptr::read_volatile(pointer.cast::<u64>())
        }))
    }

    /// Write one byte.
    pub fn write_u8(&mut self, offset: usize, value: u8) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 1, 1)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer.
        unsafe { core::ptr::write_volatile(pointer, value) };
        Ok(())
    }

    /// Write one 16-bit little-endian word.
    pub fn write_u16(&mut self, offset: usize, value: u16) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 2, 2)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer and 2-byte aligned.
        unsafe { core::ptr::write_volatile(pointer.cast::<u16>(), value.to_le()) };
        Ok(())
    }

    /// Write one 32-bit little-endian word.
    pub fn write_u32(&mut self, offset: usize, value: u32) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 4, 4)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer and 4-byte aligned.
        unsafe { core::ptr::write_volatile(pointer.cast::<u32>(), value.to_le()) };
        Ok(())
    }

    /// Write one 64-bit little-endian word.
    pub fn write_u64(&mut self, offset: usize, value: u64) -> Result<(), AccessError> {
        let pointer = self.pointer(offset, 8, 8)?;
        // SAFETY: `pointer` is inside the mapped DMA buffer and 8-byte aligned.
        unsafe { core::ptr::write_volatile(pointer.cast::<u64>(), value.to_le()) };
        Ok(())
    }

    /// Copy `bytes` into the buffer at `offset`.
    pub fn write_bytes(&mut self, offset: usize, bytes: &[u8]) -> Result<(), AccessError> {
        self.pointer(offset, bytes.len(), 1)?;
        for (index, byte) in bytes.iter().enumerate() {
            let position = offset.checked_add(index).ok_or(AccessError::Bounds)?;
            let pointer = self.pointer(position, 1, 1)?;
            // SAFETY: `pointer` is inside the mapped DMA buffer.
            unsafe { core::ptr::write_volatile(pointer, *byte) };
        }
        Ok(())
    }

    /// Copy the buffer at `offset` into `out`.
    pub fn read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), AccessError> {
        self.pointer(offset, out.len(), 1)?;
        for (index, slot) in out.iter_mut().enumerate() {
            let position = offset.checked_add(index).ok_or(AccessError::Bounds)?;
            let pointer = self.pointer(position, 1, 1)?;
            // SAFETY: `pointer` is inside the mapped DMA buffer.
            *slot = unsafe { core::ptr::read_volatile(pointer) };
        }
        Ok(())
    }

    fn pointer(&self, offset: usize, len: usize, align: usize) -> Result<*mut u8, AccessError> {
        let base = self.window.as_ptr() as usize;
        checked_pointer(base, self.window.len(), offset, len, align)
    }
}

/// The interrupt handle `devd` transfers to a driver.
///
/// The wrapper keeps the raw handle and forwards to the runtime; it carries no state, so a
/// driver can hold one and still keep `#![forbid(unsafe_code)]`.
pub struct Irq {
    handle: Handle,
}

impl Irq {
    /// Wrap a transferred `Irq` handle.
    pub const fn new(handle: Handle) -> Self {
        Self { handle }
    }

    /// The underlying handle, for logging or a hand-off.
    pub const fn handle(&self) -> Handle {
        self.handle
    }

    /// Wait up to `timeout_ns` for an interrupt; `Ok(0)` means the timeout expired.
    #[cfg(target_os = "none")]
    pub fn wait(&self, timeout_ns: u64) -> Result<usize, Errno> {
        raven_rt::irq_wait(self.handle, timeout_ns)
    }

    /// Acknowledge the handled interrupt and re-arm (or mask) the source.
    #[cfg(target_os = "none")]
    pub fn ack(&self) -> Result<(), Errno> {
        raven_rt::irq_ack(self.handle)
    }
}

fn checked_pointer(
    base: usize,
    length: usize,
    offset: usize,
    len: usize,
    align: usize,
) -> Result<*mut u8, AccessError> {
    let end = offset.checked_add(len).ok_or(AccessError::Bounds)?;
    if end > length {
        return Err(AccessError::Bounds);
    }
    let address = base.checked_add(offset).ok_or(AccessError::Bounds)?;
    if align > 1 && !address.is_multiple_of(align) {
        return Err(AccessError::Alignment);
    }
    Ok(address as *mut u8)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
