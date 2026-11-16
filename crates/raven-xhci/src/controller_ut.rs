// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the xHCI capability/port controller.

use super::*;

struct FakeSeam {
    regs: [u32; 0x4000],
    dma: [u8; MIN_DMA_LEN],
    bus: u64,
}

impl FakeSeam {
    fn new() -> Self {
        let mut seam = Self {
            regs: [0; 0x4000],
            dma: [0; MIN_DMA_LEN],
            bus: 0x20_0000,
        };
        seam.regs[0] = 0x40 | (0x0110 << 16);
        seam.regs[1] = 32 | (2 << 24);
        seam.regs[4] = 1 | (1 << 2);
        seam.regs[5] = 0x2000;
        seam.regs[6] = 0x3000;
        seam
    }

    fn set_port(&mut self, port: u8, value: u32) {
        let offset = 0x40 + 0x400 + usize::from(port.saturating_sub(1)) * 0x10;
        self.regs[offset / 4] = value;
    }
}

impl Seam for FakeSeam {
    fn read32(&mut self, offset: usize) -> Result<u32, XhciError> {
        self.regs.get(offset / 4).copied().ok_or(XhciError::Access)
    }
    fn write32(&mut self, offset: usize, value: u32) -> Result<(), XhciError> {
        *self.regs.get_mut(offset / 4).ok_or(XhciError::Access)? = value;
        Ok(())
    }
    fn read64(&mut self, offset: usize) -> Result<u64, XhciError> {
        let low = self.read32(offset)?;
        let high = self.read32(offset + 4)?;
        Ok(u64::from(high) << 32 | u64::from(low))
    }
    fn write64(&mut self, offset: usize, value: u64) -> Result<(), XhciError> {
        self.write32(offset, value as u32)?;
        self.write32(offset + 4, (value >> 32) as u32)
    }
    fn dma_bus(&self) -> u64 {
        self.bus
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
fn the_controller_decodes_the_qemu_capability() {
    let controller = Controller::new(FakeSeam::new()).expect("controller");
    let cap = controller.capability();
    assert_eq!(cap.hci_version, 0x0110);
    assert_eq!(cap.max_slots, 32);
    assert_eq!(cap.max_ports, 2);
    assert_eq!(controller.context_size(), 64);
    assert_eq!(controller.slot(), 0);
    assert_eq!(controller.port(), 0);
}

#[test]
fn a_connected_port_reports_its_speed() {
    let mut seam = FakeSeam::new();
    seam.set_port(2, PORTSC_CCS | (3 << 10));
    let mut controller = Controller::new(seam).expect("controller");
    assert_eq!(controller.wait_for_port(2), Ok(2));
    assert_eq!(controller.port(), 2);
    assert_eq!(controller.speed(), 3);
}

#[test]
fn a_disconnected_port_times_out() {
    let mut controller = Controller::new(FakeSeam::new()).expect("controller");
    assert_eq!(controller.wait_for_port(1), Err(XhciError::NoPort));
}

#[test]
fn the_qemu_xhci_identity_is_recorded() {
    assert_eq!(QEMU_XHCI_VENDOR, 0x1b36);
    assert_eq!(QEMU_XHCI_DEVICE, 0x000d);
}
