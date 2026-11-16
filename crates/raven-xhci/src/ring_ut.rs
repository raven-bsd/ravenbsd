// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the xHCI DMA rings.

use super::*;
use crate::controller::{MIN_DMA_LEN, Seam};
use crate::error::XhciError;
use raven_rp1_xhci::TrbType;

struct FakeSeam {
    dma: [u8; MIN_DMA_LEN],
}

impl FakeSeam {
    fn new() -> Self {
        Self {
            dma: [0; MIN_DMA_LEN],
        }
    }
}

impl Seam for FakeSeam {
    fn read32(&mut self, _offset: usize) -> Result<u32, XhciError> {
        Ok(0)
    }
    fn write32(&mut self, _offset: usize, _value: u32) -> Result<(), XhciError> {
        Ok(())
    }
    fn read64(&mut self, _offset: usize) -> Result<u64, XhciError> {
        Ok(0)
    }
    fn write64(&mut self, _offset: usize, _value: u64) -> Result<(), XhciError> {
        Ok(())
    }
    fn dma_bus(&self) -> u64 {
        0x1000
    }
    fn dma_len(&self) -> usize {
        self.dma.len()
    }
    fn dma_read_u32(&self, offset: usize) -> Result<u32, XhciError> {
        let mut word = [0u8; 4];
        word.copy_from_slice(self.dma.get(offset..offset + 4).ok_or(XhciError::Access)?);
        Ok(u32::from_le_bytes(word))
    }
    fn dma_write_u32(&mut self, offset: usize, value: u32) -> Result<(), XhciError> {
        self.dma
            .get_mut(offset..offset + 4)
            .ok_or(XhciError::Access)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    fn dma_read_u64(&self, offset: usize) -> Result<u64, XhciError> {
        let mut word = [0u8; 8];
        word.copy_from_slice(self.dma.get(offset..offset + 8).ok_or(XhciError::Access)?);
        Ok(u64::from_le_bytes(word))
    }
    fn dma_write_u64(&mut self, offset: usize, value: u64) -> Result<(), XhciError> {
        self.dma
            .get_mut(offset..offset + 8)
            .ok_or(XhciError::Access)?
            .copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    fn dma_read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), XhciError> {
        out.copy_from_slice(
            self.dma
                .get(offset..offset + out.len())
                .ok_or(XhciError::Access)?,
        );
        Ok(())
    }
    fn dma_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), XhciError> {
        self.dma
            .get_mut(offset..offset + data.len())
            .ok_or(XhciError::Access)?
            .copy_from_slice(data);
        Ok(())
    }
    fn delay_us(&mut self, _micros: u32) {}
}

#[test]
fn a_producer_ring_writes_a_link_and_wraps_the_cycle() {
    let mut seam = FakeSeam::new();
    let mut ring = ProducerRing::new(0x100, 4).expect("ring");
    ring.init(&mut seam).expect("init");

    let link = read_trb(&seam, 0x100 + 3 * 16).expect("link");
    assert_eq!(link.kind(), TrbType::Link);
    assert!(link.cycle());

    for index in 0..3 {
        assert_eq!(
            ring.push(&mut seam, Trb::normal(0x2000 + index, 1, false)),
            Ok(index as u16)
        );
    }
    assert_eq!(ring.enqueue(), 3);
    assert!(read_trb(&seam, 0x100).expect("trb 0").cycle());

    // The fourth push wraps through the Link and starts the next producer cycle.
    assert_eq!(ring.push(&mut seam, Trb::normal(0x3000, 1, false)), Ok(0));
    assert_eq!(ring.enqueue(), 1);
    assert!(!ring.cycle());
    assert!(!read_trb(&seam, 0x100).expect("wrapped trb 0").cycle());
    assert!(read_trb(&seam, 0x100 + 3 * 16).expect("link").cycle());
}

#[test]
fn an_event_ring_consumes_in_cycle_order() {
    let mut seam = FakeSeam::new();
    let mut ring = EventRing::new(0x200, 4).expect("event ring");
    assert_eq!(ring.pop(&seam).expect("empty"), None);

    write_trb(
        &mut seam,
        0x200,
        Trb::normal(0x4000, 8, true).with_cycle(true),
    )
    .expect("event");
    let event = ring.pop(&seam).expect("pop").expect("event");
    assert_eq!(event.parameter, 0x4000);
    assert_eq!(ring.consumer(), 1);
    assert_eq!(ring.pop(&seam).expect("empty"), None);
    assert_eq!(ring.dequeue_address(0x1000), 0x1210);

    write_trb(
        &mut seam,
        0x210,
        Trb::normal(0x5000, 8, true).with_cycle(true),
    )
    .expect("event");
    assert_eq!(
        ring.pop(&seam).expect("pop").map(|trb| trb.parameter),
        Some(0x5000)
    );
}

#[test]
fn ring_sizes_are_validated() {
    assert_eq!(ProducerRing::new(0, 1).unwrap_err(), XhciError::Length);
    assert_eq!(ProducerRing::new(0, 1025).unwrap_err(), XhciError::Length);
    assert_eq!(EventRing::new(0, 1).unwrap_err(), XhciError::Length);
    assert!(ProducerRing::new(0, 2).is_ok());
    assert!(EventRing::new(0, 1024).is_ok());
}
