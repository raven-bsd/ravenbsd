// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A minimal guest xHCI controller for the QEMU `qemu-xhci` + `usb-kbd` path.
//!
//! The controller owns the operational sequence a USB core needs: reset/run, DCBAAP/CRCR/ERST,
//! the port reset, Enable Slot, Address Device, Configure Endpoint and control/interrupt
//! transfers. The MMIO/DMA seam is a trait so the host tests can run the ring/context logic
//! against a deterministic memory double; the userland driver implements it with
//! `raven-driver`'s `MmioRegion`, `DmaRegion` and `Irq`.
//!
//! The QEMU device is a standard xHCI controller (`1b36:000d`); the Pi/RP1 DWC3 offset remains
//! UNVERIFIED (R4). This model deliberately programs only the standard registers.

use crate::context::{encode_address_input, encode_configure_input, input_context_size};
use crate::error::XhciError;
use crate::ring::{EventRing, ProducerRing};
use raven_rp1_xhci::Capability;
use raven_rp1_xhci::control::{ControlDirection, ControlTransfer};
use raven_rp1_xhci::regs::{
    CONFIG, CRCR, DCBAAP, ERDP, ERSTBA, ERSTSZ, IMAN, IMAN_IE, PORT_BASE, PORT_STRIDE, PORTSC_CCS,
    PORTSC_CSC, PORTSC_LWS, PORTSC_PEC, PORTSC_PED, PORTSC_PP, PORTSC_PR, PORTSC_PRC, USBCMD_INTE,
    USBCMD_RESET, USBCMD_RUN, USBSTS_CNR, USBSTS_HCE, USBSTS_HCH,
};
use raven_rp1_xhci::trb::{Trb, TrbType};

/// QEMU xHCI PCI vendor ID.
pub const QEMU_XHCI_VENDOR: u16 = 0x1b36;
/// QEMU xHCI PCI device ID.
pub const QEMU_XHCI_DEVICE: u16 = 0x000d;

/// DMA offset of the device-context base address array.
pub const DCBAAP_OFFSET: usize = 0x0000;
/// DMA offset of the device context (slot 1).
pub const DEVICE_CONTEXT_OFFSET: usize = 0x0800;
/// DMA offset of the input context.
pub const INPUT_CONTEXT_OFFSET: usize = 0x1000;
/// DMA offset of the command ring.
pub const COMMAND_RING_OFFSET: usize = 0x2000;
/// DMA offset of the event ring.
pub const EVENT_RING_OFFSET: usize = 0x3000;
/// DMA offset of the event ring segment table.
pub const ERST_OFFSET: usize = 0x3400;
/// DMA offset of the EP0 transfer ring.
pub const EP0_RING_OFFSET: usize = 0x4000;
/// DMA offset of the EP1 transfer ring.
pub const EP1_RING_OFFSET: usize = 0x5000;
/// DMA offset of the control-transfer buffer.
pub const CONTROL_BUFFER_OFFSET: usize = 0x6000;
/// DMA offset of the interrupt-IN buffer.
pub const INTERRUPT_BUFFER_OFFSET: usize = 0x6100;
/// Smallest DMA buffer this layout accepts.
pub const MIN_DMA_LEN: usize = 0x8000;
/// TRBs in the command/event/transfer rings.
pub const RING_TRBS: u16 = 64;
/// Control endpoint DCI.
pub const EP0_DCI: u8 = 1;
/// Interrupt-IN endpoint DCI (endpoint 1 IN).
pub const EP1_DCI: u8 = 3;
/// PORTSC port-speed field shift.
const PORTSC_PS_SHIFT: u32 = 10;
/// PORTSC port-speed field mask.
const PORTSC_PS_MASK: u32 = 0xf;

/// The MMIO/DMA seam the controller drives.
pub trait Seam {
    /// Read a 32-bit MMIO register.
    fn read32(&mut self, offset: usize) -> Result<u32, XhciError>;
    /// Write a 32-bit MMIO register.
    fn write32(&mut self, offset: usize, value: u32) -> Result<(), XhciError>;
    /// Read a 64-bit MMIO register.
    fn read64(&mut self, offset: usize) -> Result<u64, XhciError>;
    /// Write a 64-bit MMIO register.
    fn write64(&mut self, offset: usize, value: u64) -> Result<(), XhciError>;
    /// Device-visible DMA base address.
    fn dma_bus(&self) -> u64;
    /// DMA buffer length.
    fn dma_len(&self) -> usize;
    /// Read a 32-bit DMA word.
    fn dma_read_u32(&self, offset: usize) -> Result<u32, XhciError>;
    /// Write a 32-bit DMA word.
    fn dma_write_u32(&mut self, offset: usize, value: u32) -> Result<(), XhciError>;
    /// Read a 64-bit DMA word.
    fn dma_read_u64(&self, offset: usize) -> Result<u64, XhciError>;
    /// Write a 64-bit DMA word.
    fn dma_write_u64(&mut self, offset: usize, value: u64) -> Result<(), XhciError>;
    /// Copy DMA bytes out.
    fn dma_read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), XhciError>;
    /// Copy bytes into DMA.
    fn dma_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), XhciError>;
    /// Release fence before a doorbell.
    fn publish(&self) {}
    /// Acquire fence after an interrupt.
    fn acquire(&self) {}
    /// Wait between polls.
    fn delay_us(&mut self, micros: u32);
    /// Wait for the controller interrupt, then acknowledge it. The default is a small delay.
    fn wait_irq(&mut self, _timeout_ns: u64) {
        self.delay_us(1000);
    }
}

/// The guest xHCI controller.
pub struct Controller<S: Seam> {
    seam: S,
    cap: Capability,
    context_size: usize,
    operational: usize,
    runtime: usize,
    doorbell: usize,
    command: ProducerRing,
    event: EventRing,
    ep0: ProducerRing,
    ep1: ProducerRing,
    slot: u8,
    port: u8,
    speed: u8,
    last_command_slot: u8,
}

impl<S: Seam> Controller<S> {
    /// Build a controller over `seam` and decode its capability registers.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::Capability`] for unusable capability values and
    /// [`XhciError::Length`] for a DMA buffer smaller than [`MIN_DMA_LEN`].
    pub fn new(seam: S) -> Result<Self, XhciError> {
        if seam.dma_len() < MIN_DMA_LEN {
            return Err(XhciError::Length);
        }
        let mut seam = seam;
        let cap_version = seam.read32(0)?;
        let caplength = (cap_version & 0xff) as u8;
        let hci_version = (cap_version >> 16) as u16;
        let hcsparams1 = seam.read32(0x04)?;
        let hccparams1 = seam.read32(0x10)?;
        let cap = Capability::decode(hci_version, hcsparams1, hccparams1);
        if caplength < 0x20 || cap.max_slots == 0 || cap.max_ports == 0 {
            return Err(XhciError::Capability);
        }
        let dboff = seam.read32(0x14)?;
        let rtsoff = seam.read32(0x18)?;
        let runtime = usize::try_from(Capability::runtime_offset(rtsoff))
            .map_err(|_| XhciError::Capability)?;
        let doorbell = usize::try_from(Capability::doorbell_offset(dboff))
            .map_err(|_| XhciError::Capability)?;
        if runtime == 0 || doorbell == 0 {
            return Err(XhciError::Capability);
        }
        let context_size = if cap.csz { 64 } else { 32 };
        Ok(Self {
            seam,
            cap,
            context_size,
            operational: usize::from(caplength),
            runtime,
            doorbell,
            command: ProducerRing::new(COMMAND_RING_OFFSET, RING_TRBS)?,
            event: EventRing::new(EVENT_RING_OFFSET, RING_TRBS)?,
            ep0: ProducerRing::new(EP0_RING_OFFSET, RING_TRBS)?,
            ep1: ProducerRing::new(EP1_RING_OFFSET, RING_TRBS)?,
            slot: 0,
            port: 0,
            speed: 0,
            last_command_slot: 0,
        })
    }

    /// Decoded capability registers.
    #[must_use]
    pub const fn capability(&self) -> Capability {
        self.cap
    }

    /// Context size in bytes.
    #[must_use]
    pub const fn context_size(&self) -> usize {
        self.context_size
    }

    /// Current slot, or zero before Enable Slot.
    #[must_use]
    pub const fn slot(&self) -> u8 {
        self.slot
    }

    /// Current port, or zero before a connection.
    #[must_use]
    pub const fn port(&self) -> u8 {
        self.port
    }

    /// Current port speed ID.
    #[must_use]
    pub const fn speed(&self) -> u8 {
        self.speed
    }

    /// Reset and start the controller, program the rings and power the ports.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::Timeout`] if the controller does not leave reset/run, or an access
    /// error from the seam.
    pub fn init(&mut self) -> Result<(), XhciError> {
        self.seam.write32(self.operational, USBCMD_RESET)?;
        self.wait_status(USBSTS_CNR, 0, 200)?;
        self.seam.write32(self.operational, 0)?;

        self.seam.write32(
            self.operational + usize::from(CONFIG),
            u32::from(self.cap.max_slots),
        )?;
        self.seam.write64(
            self.operational + usize::from(DCBAAP),
            self.dma(DEVICE_CONTEXT_OFFSET),
        )?;
        self.seam.write64(
            self.operational + usize::from(CRCR),
            self.dma(COMMAND_RING_OFFSET) | 1,
        )?;
        self.command.init(&mut self.seam)?;
        self.ep0.init(&mut self.seam)?;
        self.ep1.init(&mut self.seam)?;

        self.seam
            .dma_write_u64(ERST_OFFSET, self.dma(EVENT_RING_OFFSET))?;
        self.seam
            .dma_write_u32(ERST_OFFSET + 8, u32::from(RING_TRBS))?;
        self.seam.dma_write_u32(ERST_OFFSET + 12, 0)?;
        self.seam
            .write32(self.runtime + 0x20 + usize::from(ERSTSZ), 1)?;
        self.seam.write64(
            self.runtime + 0x20 + usize::from(ERSTBA),
            self.dma(ERST_OFFSET),
        )?;
        self.seam.write64(
            self.runtime + 0x20 + usize::from(ERDP),
            self.dma(EVENT_RING_OFFSET),
        )?;
        self.seam
            .write32(self.runtime + 0x20 + usize::from(IMAN), IMAN_IE)?;
        self.seam
            .write32(self.operational, USBCMD_RUN | USBCMD_INTE)?;
        self.wait_status(USBSTS_HCH, 0, 200)?;

        for port in 1..=self.cap.max_ports {
            let offset = self.port_offset(port);
            let status = self.seam.read32(offset)?;
            self.seam.write32(offset, status | PORTSC_PP)?;
        }
        Ok(())
    }

    /// Poll for a connected port for up to `timeout_ms`.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::NoPort`] when no port is connected.
    pub fn wait_for_port(&mut self, timeout_ms: u32) -> Result<u8, XhciError> {
        for _ in 0..timeout_ms {
            for port in 1..=self.cap.max_ports {
                let status = self.seam.read32(self.port_offset(port))?;
                if status & PORTSC_CCS != 0 {
                    self.port = port;
                    self.speed = ((status >> PORTSC_PS_SHIFT) & PORTSC_PS_MASK) as u8;
                    return Ok(port);
                }
            }
            self.seam.delay_us(1000);
        }
        Err(XhciError::NoPort)
    }

    /// Reset the connected port and return its speed ID.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::NoPort`] when the port is disconnected or [`XhciError::Timeout`] if
    /// the reset does not complete.
    pub fn reset_port(&mut self, port: u8) -> Result<u8, XhciError> {
        let offset = self.port_offset(port);
        let status = self.seam.read32(offset)?;
        if status & PORTSC_CCS == 0 {
            return Err(XhciError::NoPort);
        }
        let value =
            (status & !(PORTSC_LWS | PORTSC_CSC | PORTSC_PEC | PORTSC_PRC)) | PORTSC_PP | PORTSC_PR;
        self.seam.write32(offset, value)?;
        for _ in 0..2000 {
            let status = self.seam.read32(offset)?;
            if status & (PORTSC_PRC | PORTSC_PED) != 0 {
                let clear = (status & !(PORTSC_PR | PORTSC_LWS))
                    | PORTSC_PRC
                    | PORTSC_PEC
                    | PORTSC_CSC
                    | PORTSC_PP;
                self.seam.write32(offset, clear)?;
                self.port = port;
                self.speed = ((status >> PORTSC_PS_SHIFT) & PORTSC_PS_MASK) as u8;
                return Ok(self.speed);
            }
            self.seam.delay_us(1000);
        }
        Err(XhciError::Timeout)
    }

    /// Enable Slot and return the assigned slot.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::Slot`] for slot zero or a controller error.
    pub fn enable_slot(&mut self) -> Result<u8, XhciError> {
        self.command(Trb::enable_slot())?;
        let slot = self.last_command_slot();
        if slot == 0 {
            return Err(XhciError::Slot(slot));
        }
        self.slot = slot;
        Ok(slot)
    }

    /// Address the device on `slot`.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] for an invalid slot or a command failure.
    pub fn address_device(
        &mut self,
        slot: u8,
        port: u8,
        speed: u8,
        max_packet: u16,
    ) -> Result<(), XhciError> {
        if slot == 0 || slot > self.cap.max_slots {
            return Err(XhciError::Slot(slot));
        }
        let mut context = [0u8; 32 + 32 * 64];
        let size = input_context_size(self.context_size);
        encode_address_input(
            self.context_size,
            port,
            speed,
            self.dma(EP0_RING_OFFSET),
            max_packet,
            &mut context[..size],
        )?;
        self.seam
            .dma_write_bytes(INPUT_CONTEXT_OFFSET, &context[..size])?;
        let input = self.dma(INPUT_CONTEXT_OFFSET);
        let trb = Trb::address_device(slot, input).ok_or(XhciError::Slot(slot))?;
        self.command(trb)?;
        self.slot = slot;
        self.port = port;
        self.speed = speed;
        Ok(())
    }

    /// Configure the interrupt-IN endpoint (DCI 3) on `slot`.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] for an invalid slot or a command failure.
    pub fn configure_interrupt(
        &mut self,
        slot: u8,
        port: u8,
        speed: u8,
        max_packet: u16,
        interval: u8,
    ) -> Result<(), XhciError> {
        if slot == 0 || slot > self.cap.max_slots {
            return Err(XhciError::Slot(slot));
        }
        let mut context = [0u8; 32 + 32 * 64];
        let size = input_context_size(self.context_size);
        encode_configure_input(
            self.context_size,
            port,
            speed,
            self.dma(EP0_RING_OFFSET),
            self.dma(EP1_RING_OFFSET),
            max_packet,
            interval,
            &mut context[..size],
        )?;
        self.seam
            .dma_write_bytes(INPUT_CONTEXT_OFFSET, &context[..size])?;
        let input = self.dma(INPUT_CONTEXT_OFFSET);
        let trb = Trb::configure_endpoint(slot, input).ok_or(XhciError::Slot(slot))?;
        self.command(trb)?;
        Ok(())
    }

    /// Run one control transfer (EP0).
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::Length`] for a length mismatch, a controller timeout or a non-success
    /// completion code.
    pub fn control(
        &mut self,
        setup: [u8; 8],
        data: &mut [u8],
        direction: ControlDirection,
    ) -> Result<usize, XhciError> {
        let length = u16::try_from(data.len()).map_err(|_| XhciError::Length)?;
        if direction == ControlDirection::NoData && length != 0 {
            return Err(XhciError::Length);
        }
        let address = self.dma(CONTROL_BUFFER_OFFSET);
        if direction == ControlDirection::Out {
            self.seam.dma_write_bytes(CONTROL_BUFFER_OFFSET, data)?;
        } else if direction == ControlDirection::In {
            let zeros = [0u8; 1024];
            let take = data.len().min(zeros.len());
            self.seam
                .dma_write_bytes(CONTROL_BUFFER_OFFSET, &zeros[..take])?;
        }
        let transfer =
            ControlTransfer::new(setup, address, length, direction).ok_or(XhciError::Length)?;
        let (trbs, count) = transfer.trbs();
        for (index, trb) in trbs.iter().take(usize::from(count)).enumerate() {
            let mut trb = *trb;
            if index + 1 < usize::from(count) {
                trb.control |= raven_rp1_xhci::TRB_CHAIN;
            }
            let trb = trb.with_endpoint(EP0_DCI).ok_or(XhciError::Unsupported)?;
            self.ep0.push(&mut self.seam, trb.with_slot(self.slot))?;
        }
        self.ring_doorbell(self.slot, u32::from(EP0_DCI))?;
        let event = self.wait_transfer(EP0_DCI)?;
        let code = event.completion_code();
        if code != 1 && code != 13 {
            return Err(XhciError::Completion(code));
        }
        if direction == ControlDirection::In {
            self.seam.dma_read_bytes(CONTROL_BUFFER_OFFSET, data)?;
        }
        Ok(usize::from(length))
    }

    /// Run a no-data control transfer.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::control`].
    pub fn control_no_data(&mut self, setup: [u8; 8]) -> Result<(), XhciError> {
        self.control(setup, &mut [], ControlDirection::NoData)?;
        Ok(())
    }

    /// Queue one interrupt-IN transfer on EP1.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] when the ring cannot accept the TRB.
    pub fn interrupt_in(&mut self, length: u8) -> Result<(), XhciError> {
        let trb = Trb::normal(self.dma(INTERRUPT_BUFFER_OFFSET), u16::from(length), true)
            .with_endpoint(EP1_DCI)
            .ok_or(XhciError::Unsupported)?
            .with_slot(self.slot);
        self.ep1.push(&mut self.seam, trb)?;
        self.ring_doorbell(self.slot, u32::from(EP1_DCI))
    }

    /// Poll one event from the event ring.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] when the DMA access fails.
    pub fn poll_event(&mut self) -> Result<Option<Trb>, XhciError> {
        let event = self.event.pop(&self.seam)?;
        if let Some(event) = event {
            let address = self.event.dequeue_address(self.seam.dma_bus());
            self.seam
                .write64(self.runtime + 0x20 + usize::from(ERDP), address)?;
            return Ok(Some(event));
        }
        Ok(None)
    }

    /// Read the interrupt-IN buffer after a completed transfer.
    ///
    /// # Errors
    ///
    /// Returns an [`XhciError`] when the DMA access fails.
    pub fn read_interrupt(&self, out: &mut [u8]) -> Result<(), XhciError> {
        self.seam.dma_read_bytes(INTERRUPT_BUFFER_OFFSET, out)
    }

    /// Wait for the next EP1 interrupt-IN transfer event.
    ///
    /// This is the bounded wait the HID adapter uses after an interrupt-IN transfer
    /// has been primed. It acknowledges the controller interrupt and advances the
    /// event-ring dequeue pointer, exactly like the control-transfer wait path.
    ///
    /// # Errors
    ///
    /// Returns [`XhciError::Timeout`] when no EP1 transfer event arrives within the
    /// bounded wait, or the DMA error from the event ring.
    pub fn wait_interrupt(&mut self) -> Result<Trb, XhciError> {
        self.wait_transfer(EP1_DCI)
    }

    /// Wait for a transfer event for `endpoint`.
    fn wait_transfer(&mut self, endpoint: u8) -> Result<Trb, XhciError> {
        for _ in 0..2000 {
            if let Some(event) = self.poll_event()?
                && event.kind() == TrbType::TransferEvent
                && event.endpoint() == endpoint
            {
                return Ok(event);
            }
            self.seam.wait_irq(1_000_000);
            self.seam.delay_us(500);
        }
        Err(XhciError::Timeout)
    }

    fn command(&mut self, trb: Trb) -> Result<(), XhciError> {
        self.command.push(&mut self.seam, trb)?;
        self.seam.publish();
        self.ring_doorbell(0, 0)?;
        for _ in 0..2000 {
            if let Some(event) = self.poll_event()?
                && event.kind() == TrbType::CommandCompletion
            {
                self.last_command_slot = event.slot();
                let code = event.completion_code();
                if code != 1 {
                    return Err(XhciError::Completion(code));
                }
                return Ok(());
            }
            self.seam.wait_irq(1_000_000);
            self.seam.delay_us(500);
        }
        Err(XhciError::Timeout)
    }

    fn last_command_slot(&self) -> u8 {
        self.last_command_slot
    }

    fn ring_doorbell(&mut self, slot: u8, target: u32) -> Result<(), XhciError> {
        let offset = self
            .doorbell
            .checked_add(usize::from(slot).saturating_mul(4))
            .ok_or(XhciError::Length)?;
        self.seam.write32(offset, target)
    }

    fn port_offset(&self, port: u8) -> usize {
        self.operational
            + usize::from(PORT_BASE)
            + usize::from(port.saturating_sub(1)) * usize::from(PORT_STRIDE)
    }

    fn dma(&self, offset: usize) -> u64 {
        self.seam.dma_bus().saturating_add(offset as u64)
    }

    fn wait_status(&mut self, mask: u32, expected: u32, attempts: u32) -> Result<(), XhciError> {
        for _ in 0..attempts {
            let status = self.seam.read32(self.operational + 0x04)?;
            if status & mask == expected {
                return Ok(());
            }
            if status & USBSTS_HCE != 0 {
                return Err(XhciError::Capability);
            }
            self.seam.delay_us(1000);
        }
        Err(XhciError::Timeout)
    }
}

#[cfg(test)]
#[path = "controller_ut.rs"]
mod tests;
