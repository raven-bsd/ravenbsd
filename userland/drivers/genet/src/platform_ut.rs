// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the platform MMIO/DMA seam.

use super::*;
use std::boxed::Box;
use std::vec;

fn leak(len: usize) -> &'static mut [u8] {
    Box::leak(vec![0u8; len].into_boxed_slice())
}

fn seam() -> PlatformSeam {
    PlatformSeam::new(
        MmioRegion::new(leak(0x1000)),
        DmaRegion::new(leak(0x10_0000), 0x1000_0000),
        PlatformLayout::pi4_default(),
    )
    .expect("seam")
}

#[test]
fn the_pi4_layout_fits_one_megabyte() {
    let layout = PlatformLayout::pi4_default();
    assert_eq!(layout.rx_count, 256);
    assert_eq!(layout.tx_count, 256);
    assert_eq!(layout.buffer_len, 2048);
    assert_eq!(layout.stride, 2048);
    assert_eq!(layout.tx_offset, 0x8_0000);
    assert_eq!(layout.validate(0x10_0000), Ok(()));
}

#[test]
fn the_layout_rejects_a_small_region() {
    assert_eq!(
        PlatformLayout::pi4_default().validate(0x0f_ffff),
        Err(SeamError::Bounds)
    );
}

#[test]
fn the_layout_rejects_misaligned_offsets_and_strides() {
    let mut layout = PlatformLayout::pi4_default();
    layout.rx_offset = 1;
    assert_eq!(layout.validate(0x10_0000), Err(SeamError::Alignment));
    let mut layout = PlatformLayout::pi4_default();
    layout.stride = 2049;
    assert_eq!(layout.validate(0x10_0000), Err(SeamError::Alignment));
}

#[test]
fn the_layout_rejects_overlapping_pools() {
    let mut layout = PlatformLayout::pi4_default();
    layout.tx_offset = 0x7_0000;
    assert_eq!(layout.validate(0x10_0000), Err(SeamError::Bounds));
}

#[test]
fn the_layout_rejects_zero_counts_and_oversized_counts() {
    let mut layout = PlatformLayout::pi4_default();
    layout.rx_count = 0;
    assert_eq!(layout.validate(0x10_0000), Err(SeamError::Bounds));
    let mut layout = PlatformLayout::pi4_default();
    layout.tx_count = 257;
    assert_eq!(layout.validate(0x10_0000), Err(SeamError::Bounds));
}

#[test]
fn register_access_round_trips() {
    let mut seam = seam();
    seam.write32(0x10, 0x1234_5678).expect("write");
    assert_eq!(seam.read32(0x10), Ok(0x1234_5678));
}

#[test]
fn register_access_is_bounded_and_aligned() {
    let mut seam = seam();
    assert_eq!(seam.read32(0x1000), Err(SeamError::Bounds));
    assert_eq!(seam.write32(0x1000, 0), Err(SeamError::Bounds));
    assert_eq!(seam.read32(2), Err(SeamError::Alignment));
    assert_eq!(seam.write32(2, 0), Err(SeamError::Alignment));
}

#[test]
fn rx_buffers_map_to_the_dma_bus_window() {
    let mut seam = seam();
    assert_eq!(
        seam.rx_buffer(0),
        Some(Buffer {
            bus_addr: 0x1000_0000,
            len: 2048,
        })
    );
    assert_eq!(
        seam.rx_buffer(255),
        Some(Buffer {
            bus_addr: 0x1000_0000 + 255 * 2048,
            len: 2048,
        })
    );
    assert_eq!(seam.rx_buffer(256), None);
}

#[test]
fn tx_buffers_start_after_the_rx_pool() {
    let mut seam = seam();
    assert_eq!(
        seam.tx_buffer(0),
        Some(Buffer {
            bus_addr: 0x1000_0000 + 0x8_0000,
            len: 2048,
        })
    );
    assert_eq!(
        seam.tx_buffer(255),
        Some(Buffer {
            bus_addr: 0x1000_0000 + 0x8_0000 + 255 * 2048,
            len: 2048,
        })
    );
    assert_eq!(seam.tx_buffer(256), None);
}

#[test]
fn the_seam_reports_its_layout_and_dma_length() {
    let seam = seam();
    assert_eq!(seam.layout(), PlatformLayout::pi4_default());
    assert_eq!(seam.dma_len(), 0x10_0000);
}
