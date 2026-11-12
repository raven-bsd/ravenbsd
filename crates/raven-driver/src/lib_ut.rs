// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the MMIO and configuration-region helpers.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use super::*;

/// A heap buffer standing in for a device window.
struct Window {
    bytes: Vec<u8>,
}

impl Window {
    fn new(len: usize) -> Self {
        Self {
            bytes: vec![0u8; len],
        }
    }

    fn mmio(&mut self) -> MmioRegion {
        // SAFETY: the buffer lives in the `Window` that outlives the region in each test, and no
        // other reference to it exists while the region is used.
        let slice: &'static mut [u8] =
            unsafe { core::slice::from_raw_parts_mut(self.bytes.as_mut_ptr(), self.bytes.len()) };
        MmioRegion::new(slice)
    }

    fn config(&self) -> ConfigRegion {
        // SAFETY: read-only access to a buffer that outlives the region in each test.
        let slice: &'static [u8] =
            unsafe { core::slice::from_raw_parts(self.bytes.as_ptr(), self.bytes.len()) };
        ConfigRegion::new(slice)
    }

    fn dma(&mut self, bus: u64) -> DmaRegion {
        // SAFETY: writable access to a buffer that outlives the region in each test.
        let slice: &'static mut [u8] =
            unsafe { core::slice::from_raw_parts_mut(self.bytes.as_mut_ptr(), self.bytes.len()) };
        DmaRegion::new(slice, bus)
    }
}

#[test]
fn a_fresh_region_reports_its_length() {
    let mut window = Window::new(64);
    let region = window.mmio();
    assert_eq!(region.len(), 64);
    assert!(!region.is_empty());
}

#[test]
fn reads_and_writes_round_trip_every_width() {
    let mut window = Window::new(64);
    let mut region = window.mmio();
    region.write_u8(0, 0x12).unwrap();
    region.write_u16(2, 0x3456).unwrap();
    region.write_u32(4, 0x789A_BCDE).unwrap();
    region.write_u64(8, 0x0123_4567_89AB_CDEF).unwrap();
    assert_eq!(region.read_u8(0), Ok(0x12));
    assert_eq!(region.read_u16(2), Ok(0x3456));
    assert_eq!(region.read_u32(4), Ok(0x789A_BCDE));
    assert_eq!(region.read_u64(8), Ok(0x0123_4567_89AB_CDEF));
}

#[test]
fn accesses_outside_the_window_are_refused() {
    let mut window = Window::new(16);
    let mut region = window.mmio();
    assert_eq!(region.read_u8(16), Err(AccessError::Bounds));
    assert_eq!(region.read_u32(13), Err(AccessError::Bounds));
    assert_eq!(region.write_u64(9, 0), Err(AccessError::Bounds));
    assert_eq!(region.read_u8(usize::MAX), Err(AccessError::Bounds));
}

#[test]
fn misaligned_accesses_are_refused() {
    let mut window = Window::new(64);
    let mut region = window.mmio();
    assert_eq!(region.read_u16(1), Err(AccessError::Alignment));
    assert_eq!(region.read_u32(2), Err(AccessError::Alignment));
    assert_eq!(region.write_u64(4, 0), Err(AccessError::Alignment));
}

#[test]
fn byte_operations_need_no_alignment() {
    let mut window = Window::new(16);
    let mut region = window.mmio();
    region.write_bytes(1, b"abc").unwrap();
    let mut out = [0u8; 3];
    region.read_bytes(1, &mut out).unwrap();
    assert_eq!(&out, b"abc");
}

#[test]
fn byte_copies_stay_inside_the_window() {
    let mut window = Window::new(8);
    let mut region = window.mmio();
    assert_eq!(region.write_bytes(6, b"abc"), Err(AccessError::Bounds));
    let mut out = [0u8; 4];
    assert_eq!(region.read_bytes(5, &mut out), Err(AccessError::Bounds));
}

#[test]
fn an_empty_region_refuses_everything() {
    let mut window = Window::new(0);
    let region = window.mmio();
    assert!(region.is_empty());
    assert_eq!(region.read_u8(0), Err(AccessError::Bounds));
}

fn config_with_header(vendor: u16, device: u16, capability: Option<u8>) -> Window {
    config_with_header_size(CONFIG_LEN, vendor, device, capability)
}

fn config_with_header_size(len: usize, vendor: u16, device: u16, capability: Option<u8>) -> Window {
    let mut window = Window::new(len);
    window.bytes[0..2].copy_from_slice(&vendor.to_le_bytes());
    window.bytes[2..4].copy_from_slice(&device.to_le_bytes());
    window.bytes[0x0A] = 0x00;
    window.bytes[0x0B] = 0x02; // network controller
    if let Some(offset) = capability {
        let status = (1u16 << 4).to_le_bytes();
        window.bytes[0x06..0x08].copy_from_slice(&status);
        window.bytes[0x34] = offset;
    }
    window
}

#[test]
fn the_header_fields_are_read() {
    let window = config_with_header(0x1AF4, 0x1000, None);
    let config = window.config();
    assert_eq!(config.ids(), Ok((0x1AF4, 0x1000)));
    assert_eq!(config.class(), Ok((0x02, 0x00)));
}

#[test]
fn a_function_without_capabilities_reports_none() {
    let window = config_with_header(0x1234, 0x11E8, None);
    let config = window.config();
    assert_eq!(config.first_capability(), Ok(None));
    assert!(capabilities(&config).unwrap().is_empty());
}

#[test]
fn a_capability_chain_is_walked_in_order() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x40));
    window.bytes[0x40] = 0x11; // MSI-X
    window.bytes[0x41] = 0x60;
    window.bytes[0x60] = 0x09; // vendor specific
    window.bytes[0x61] = 0x00;
    let config = window.config();
    let list = capabilities(&config).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(
        list.get(0).map(|cap| (cap.id, cap.offset)),
        Some((0x11, 0x40))
    );
    assert_eq!(
        list.get(1).map(|cap| (cap.id, cap.offset)),
        Some((0x09, 0x60))
    );
}

#[test]
fn a_prepended_descending_capability_chain_is_walked_in_order() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x80));
    window.bytes[0x80] = 0x11; // MSI-X at the highest offset
    window.bytes[0x81] = 0x60;
    window.bytes[0x60] = 0x09; // vendor-specific in the middle
    window.bytes[0x61] = 0x40;
    window.bytes[0x40] = 0x05; // MSI at the lowest offset
    window.bytes[0x41] = 0x00;
    let config = window.config();
    let list = capabilities(&config).unwrap();
    assert_eq!(list.len(), 3);
    let seen: Vec<(u8, u8)> = list.iter().map(|cap| (cap.id, cap.offset)).collect();
    assert_eq!(seen, vec![(0x11, 0x80), (0x09, 0x60), (0x05, 0x40)]);
}

#[test]
fn a_single_capability_is_walked() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x40));
    window.bytes[0x40] = 0x11;
    window.bytes[0x41] = 0x00;
    let config = window.config();
    let list = capabilities(&config).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).map(|cap| cap.offset), Some(0x40));
}

#[test]
fn a_two_entry_capability_cycle_ends_after_both_entries() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x40));
    window.bytes[0x40] = 0x11;
    window.bytes[0x41] = 0x60;
    window.bytes[0x60] = 0x09;
    window.bytes[0x61] = 0x40;
    let config = window.config();
    let list = capabilities(&config).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.get(0).map(|cap| cap.offset), Some(0x40));
    assert_eq!(list.get(1).map(|cap| cap.offset), Some(0x60));
}

#[test]
fn a_cyclic_capability_chain_ends() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x40));
    window.bytes[0x40] = 0x11;
    window.bytes[0x41] = 0x40; // points at itself
    let config = window.config();
    let list = capabilities(&config).unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn a_next_pointer_outside_the_mapped_config_region_keeps_the_error() {
    let mut window = config_with_header_size(0x80, 0x1AF4, 0x1000, Some(0x40));
    window.bytes[0x40] = 0x11;
    window.bytes[0x41] = 0x80; // past the 0x80-byte mapping
    let config = window.config();
    assert!(matches!(capabilities(&config), Err(AccessError::Bounds)));
}

#[test]
fn a_capability_pointer_below_the_header_is_ignored() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x40));
    window.bytes[0x40] = 0x11;
    window.bytes[0x41] = 0x20; // below the standard header end
    let config = window.config();
    assert_eq!(config.capability(0x20), Ok(None));
    assert_eq!(capabilities(&config).unwrap().len(), 1);
}

#[test]
fn a_misaligned_capability_pointer_is_ignored() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x42));
    window.bytes[0x42] = 0x11;
    window.bytes[0x43] = 0x00;
    let config = window.config();
    assert_eq!(config.first_capability(), Ok(None));
}

#[test]
fn a_zero_capability_id_ends_the_walk() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x40));
    window.bytes[0x40] = 0x00;
    window.bytes[0x41] = 0x00;
    let config = window.config();
    assert!(capabilities(&config).unwrap().is_empty());
}

#[test]
fn config_reads_outside_the_window_are_refused() {
    let window = Window::new(64);
    let config = window.config();
    assert_eq!(config.read_u8(64), Err(AccessError::Bounds));
    assert_eq!(config.read_u16(63), Err(AccessError::Bounds));
}

#[test]
fn the_capability_limit_is_enforced() {
    let mut window = config_with_header(0x1AF4, 0x1000, Some(0x40));
    let mut offset = 0x40usize;
    while offset + 4 < 0x100 {
        window.bytes[offset] = 0x09;
        window.bytes[offset + 1] = (offset + 4) as u8;
        offset += 4;
    }
    window.bytes[offset] = 0x09;
    window.bytes[offset + 1] = 0;
    let config = window.config();
    let list = capabilities(&config).unwrap();
    assert!(list.len() <= MAX_CAPABILITIES);
    assert_eq!(list.len(), MAX_CAPABILITIES);
}

#[test]
fn a_dma_region_reports_its_length_and_bus_address() {
    let mut window = Window::new(4096);
    let region = window.dma(0x1_2345_6000);
    assert_eq!(region.len(), 4096);
    assert!(!region.is_empty());
    assert_eq!(region.bus_address(), 0x1_2345_6000);
}

#[test]
fn dma_accesses_round_trip_every_width() {
    let mut window = Window::new(64);
    let mut region = window.dma(0x1000);
    region.write_u8(0, 0x5A).unwrap();
    region.write_u16(2, 0x1234).unwrap();
    region.write_u32(4, 0xDEAD_BEEF).unwrap();
    region.write_u64(8, 0x0102_0304_0506_0708).unwrap();
    assert_eq!(region.read_u8(0), Ok(0x5A));
    assert_eq!(region.read_u16(2), Ok(0x1234));
    assert_eq!(region.read_u32(4), Ok(0xDEAD_BEEF));
    assert_eq!(region.read_u64(8), Ok(0x0102_0304_0506_0708));
}

#[test]
fn dma_byte_copies_round_trip() {
    let mut window = Window::new(32);
    let mut region = window.dma(0x2000);
    region.write_bytes(8, b"descriptor").unwrap();
    let mut out = [0u8; 10];
    region.read_bytes(8, &mut out).unwrap();
    assert_eq!(&out, b"descriptor");
}

#[test]
fn dma_accesses_outside_the_buffer_are_refused() {
    let mut window = Window::new(16);
    let mut region = window.dma(0x3000);
    assert_eq!(region.read_u8(16), Err(AccessError::Bounds));
    assert_eq!(region.write_u32(14, 0), Err(AccessError::Bounds));
    assert_eq!(region.read_u16(1), Err(AccessError::Alignment));
    assert_eq!(region.write_bytes(10, b"abcdefg"), Err(AccessError::Bounds));
}

#[test]
fn a_dma_region_can_be_fenced_and_an_irq_keeps_its_handle() {
    let mut window = Window::new(16);
    let region = window.dma(0x4000);
    region.publish();
    region.acquire();
    let irq = Irq::new(Handle::new(6, 0));
    assert_eq!(irq.handle().index(), 6);
}
