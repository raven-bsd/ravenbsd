// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable B39 `nic:` frame service for the GENET driver.
//!
//! The service owns the GENET frame backend and one shared `raven-proto-net` ring region. It maps
//! the B39 RX pool buffers 0..7 to GENET RX slots 0..7 and the B39 TX pool buffers 8..10 to the
//! three GENET TX slots, so a frame is delivered and transmitted in place: the GENET device and
//! `netd` see the same DMA bytes. RX completions are published as B39 descriptors; TX descriptors
//! are queued to GENET and retired in FIFO order.
//!
//! The hardware descriptor-register programming and the board `devd` platform launch are the
//! target glue; this module is the part that can be driven by host tests. No board result is
//! implied.

use core::fmt;

use crate::frame::FrameBackend;
use crate::platform::{PACKET_BUFFER_LEN, PlatformLayout, PlatformSeam};
use crate::release::ReleaseSession;
use crate::{DriverError, Seam, SeamError};
use raven_genet::b39::{
    BUFFER_SIZE, DESC_ERROR, DESC_FIRST, DESC_LAST, Desc, LayoutError, Ring, RingConsumer,
    RingError, RingHeader, RingLayout, RingProducer,
};
use raven_genet::frame::FrameError;
use raven_genet::irq::InterruptStatus;
use raven_genet::link::LinkEvent;
use raven_genet::release::{Lease, PingReport, ReleaseTranscript};

/// Control scheme served to `netd`.
pub const NIC_SCHEME: &str = "nic";
/// Machine-readable control summary.
pub const NIC_INFO: &str = "info";
/// MAC address line.
pub const NIC_MAC: &str = "mac";
/// MTU line.
pub const NIC_MTU: &str = "mtu";
/// Link state line.
pub const NIC_LINK: &str = "link";
/// The shared ring attachment.
pub const NIC_RING: &str = "ring";
/// Arm the frame pump.
pub const NIC_START: &str = "start";
/// Pause the frame pump.
pub const NIC_STOP: &str = "stop";
/// Feed and read the release link/DHCP/ping evidence.
pub const NIC_RELEASE: &str = "release";
/// Test-only clean shutdown.
pub const NIC_QUIT: &str = "quit";

/// Offset of the B39 ring region inside the DMA window.
pub const RING_REGION_OFFSET: usize = 0x1000;
/// Length of the B39 ring region.
pub const RING_REGION_LEN: usize = 0x6000;
/// RX descriptors in the B39 ring and GENET RX slots in use.
pub const RING_RX_COUNT: u16 = 8;
/// TX descriptors in the B39 ring.
pub const RING_TX_COUNT: u16 = 8;
/// Pool buffers reserved for the device RX queue.
pub const RING_RX_BUFFERS: usize = 8;
/// Pool buffers available for the TX queue (the remaining three 2 KiB slots).
pub const RING_TX_BUFFERS: usize = 3;
/// Ethernet payload MTU advertised to `netd`.
pub const DEFAULT_MTU: u16 = 1500;

/// Why a `nic:` operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NicError {
    /// The frame backend rejected the operation.
    Driver(DriverError),
    /// The frame ownership path rejected the operation.
    Frame(FrameError),
    /// The platform seam rejected the access.
    Seam(SeamError),
    /// The B39 ring layout rejected the region.
    Layout(LayoutError),
    /// The B39 ring rejected the descriptor.
    Ring(RingError),
    /// The configured ring counts do not match the B39 service.
    BadLayout,
    /// The device tree or caller supplied no usable MAC.
    MissingMac,
    /// A `netd` TX descriptor is malformed.
    BadDescriptor,
}

impl From<DriverError> for NicError {
    fn from(error: DriverError) -> Self {
        Self::Driver(error)
    }
}

impl From<FrameError> for NicError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

impl From<SeamError> for NicError {
    fn from(error: SeamError) -> Self {
        Self::Seam(error)
    }
}

impl From<LayoutError> for NicError {
    fn from(error: LayoutError) -> Self {
        Self::Layout(error)
    }
}

impl From<RingError> for NicError {
    fn from(error: RingError) -> Self {
        Self::Ring(error)
    }
}

/// The `nic:info` summary, encoded in the same `key=value` form as the virtio NIC service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlInfo {
    /// Device MAC.
    pub mac: [u8; 6],
    /// Ethernet payload MTU.
    pub mtu: u16,
    /// Link state.
    pub link: bool,
    /// DMA window length.
    pub dma_len: usize,
    /// Ring region offset inside the DMA window.
    pub ring_offset: usize,
    /// Ring region length.
    pub ring_len: usize,
    /// RX descriptor count.
    pub rx_count: u16,
    /// TX descriptor count.
    pub tx_count: u16,
    /// Buffer pool length.
    pub pool_len: usize,
}

impl ControlInfo {
    /// Encode the summary into `out`; returns the byte count.
    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        let mut writer = TextWriter { out, pos: 0 };
        let link = if self.link { "up" } else { "down" };
        core::fmt::write(
            &mut writer,
            format_args!(
                "mac={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n\
                 mtu={}\nlink={}\nring_offset={}\nring_len={}\nrx={}\ntx={}\npool={}\ndma={}\n",
                self.mac[0],
                self.mac[1],
                self.mac[2],
                self.mac[3],
                self.mac[4],
                self.mac[5],
                self.mtu,
                link,
                self.ring_offset,
                self.ring_len,
                self.rx_count,
                self.tx_count,
                self.pool_len,
                self.dma_len
            ),
        )
        .ok()?;
        Some(writer.pos)
    }
}

struct TextWriter<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl fmt::Write for TextWriter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.pos.checked_add(text.len()).ok_or(fmt::Error)?;
        let target = self.out.get_mut(self.pos..end).ok_or(fmt::Error)?;
        target.copy_from_slice(text.as_bytes());
        self.pos = end;
        Ok(())
    }
}

/// One board-probe command for the release evidence gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseCommand {
    /// A DHCP lease observed by the board probe.
    Dhcp {
        /// Validated lease.
        lease: Lease,
    },
    /// A gateway ping result observed by the board probe.
    Ping {
        /// Validated ping report.
        report: PingReport,
    },
}

impl ReleaseCommand {
    /// Parse one `nic:release` line.
    ///
    /// `dhcp <addr>/<prefix> <gateway> <dns|none>` and
    /// `ping <gateway> tx=<n> rx=<n> rtt_ms=<n>` are the only accepted forms.
    pub fn parse(text: &str) -> Option<Self> {
        let line = text.trim();
        let mut parts = line.split_whitespace();
        match parts.next()? {
            "dhcp" => {
                let address = parts.next()?;
                let (address, prefix) = address.split_once('/')?;
                let address = parse_ipv4(address)?;
                let prefix = prefix.parse().ok()?;
                let gateway = parse_ipv4(parts.next()?)?;
                let dns = match parts.next()? {
                    "none" => None,
                    server => Some(parse_ipv4(server)?),
                };
                if parts.next().is_some() {
                    return None;
                }
                Some(Self::Dhcp {
                    lease: Lease::new(address, prefix, gateway, dns).ok()?,
                })
            }
            "ping" => {
                let target = parse_ipv4(parts.next()?)?;
                let transmitted = parse_field(parts.next()?, "tx=")?.parse().ok()?;
                let received = parse_field(parts.next()?, "rx=")?.parse().ok()?;
                let rtt_ms = parse_field(parts.next()?, "rtt_ms=")?.parse().ok()?;
                if parts.next().is_some() {
                    return None;
                }
                Some(Self::Ping {
                    report: PingReport {
                        target,
                        transmitted,
                        received,
                        rtt_ms,
                    },
                })
            }
            _ => None,
        }
    }
}

fn parse_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.strip_prefix(key)
}

fn parse_ipv4(text: &str) -> Option<[u8; 4]> {
    let mut address = [0u8; 4];
    let mut count = 0usize;
    for part in text.split('.') {
        let slot = address.get_mut(count)?;
        *slot = part.parse().ok()?;
        count += 1;
    }
    (count == 4).then_some(address)
}

/// One bounded service pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NicReport {
    /// RX frames published to `netd`.
    pub rx_delivered: usize,
    /// RX frames dropped because the B39 RX ring was full.
    pub rx_dropped: usize,
    /// RX descriptors with a malformed status.
    pub rx_errors: usize,
    /// GENET RX descriptors re-armed.
    pub rx_refilled: usize,
    /// TX descriptors reclaimed from GENET.
    pub tx_completed: usize,
}

/// The host-tested frame service.
pub struct NicService<S> {
    backend: FrameBackend<S>,
    release: ReleaseSession,
    layout: RingLayout,
    region_bus: u64,
    dma_len: usize,
    mac: [u8; 6],
    link_up: bool,
    started: bool,
    tx_inflight: u16,
}

impl<S: Seam> NicService<S> {
    /// Attach the service to a ready frame backend and a validated B39 layout.
    pub fn new(
        backend: FrameBackend<S>,
        layout: RingLayout,
        region_bus: u64,
        dma_len: usize,
    ) -> Result<Self, NicError> {
        if layout.rx_count() != RING_RX_COUNT
            || layout.tx_count() != RING_TX_COUNT
            || layout.buffer_count() < RING_RX_BUFFERS + RING_TX_BUFFERS
            || backend.path().rx.count() != RING_RX_COUNT
            || backend.path().tx.count() != RING_TX_BUFFERS as u16
        {
            return Err(NicError::BadLayout);
        }
        let mac = backend
            .controller()
            .info()
            .mac
            .ok_or(NicError::MissingMac)?
            .bytes();
        Ok(Self {
            backend,
            release: ReleaseSession::new(),
            layout,
            region_bus,
            dma_len,
            mac,
            link_up: false,
            started: false,
            tx_inflight: 0,
        })
    }

    /// The B39 layout.
    #[must_use]
    pub const fn layout(&self) -> RingLayout {
        self.layout
    }

    /// The frame backend.
    #[must_use]
    pub const fn backend(&self) -> &FrameBackend<S> {
        &self.backend
    }

    /// Mutable access to the frame backend (target bring-up and host tests).
    pub fn backend_mut(&mut self) -> &mut FrameBackend<S> {
        &mut self.backend
    }

    /// The `nic:info` summary.
    #[must_use]
    pub fn info(&self) -> ControlInfo {
        ControlInfo {
            mac: self.mac,
            mtu: DEFAULT_MTU,
            link: self.link_up,
            dma_len: self.dma_len,
            ring_offset: RING_REGION_OFFSET,
            ring_len: RING_REGION_LEN,
            rx_count: RING_RX_COUNT,
            tx_count: RING_TX_COUNT,
            pool_len: self.layout.pool_len(),
        }
    }

    /// The release evidence transcript, once link/DHCP/ping are complete.
    #[must_use]
    pub fn transcript(&self) -> Option<ReleaseTranscript> {
        self.release.transcript()
    }

    /// The accepted link speed and duplex, if the PHY reported one.
    #[must_use]
    pub fn link(&self) -> Option<(raven_genet::phy::Speed, raven_genet::phy::Duplex)> {
        self.release.gate().link()
    }

    /// Arm the pump.
    pub fn start(&mut self) {
        self.started = true;
    }

    /// Pause the pump.
    pub fn stop(&mut self) {
        self.started = false;
    }

    /// Whether the pump is armed.
    #[must_use]
    pub const fn is_started(&self) -> bool {
        self.started
    }

    /// Write the B39 header into a zeroed region.
    pub fn init_region(&self, region: &mut [u8]) -> Result<(), NicError> {
        self.layout.write_header(region, 0, 0)?;
        Ok(())
    }

    /// Post every free GENET RX descriptor and write it to the hardware array.
    pub fn arm_rx(&mut self) -> Result<usize, NicError> {
        let mut posted = 0usize;
        while self.backend.path().rx.posted()? < usize::from(RING_RX_COUNT) {
            let post = self.backend.post_rx_next()?;
            self.backend.write_rx_descriptor(post)?;
            posted = posted.saturating_add(1);
            if posted > usize::from(RING_RX_COUNT) {
                break;
            }
        }
        self.backend.publish_rx_indices()?;
        Ok(posted)
    }

    /// Sample the PHY link and feed the release session.
    pub fn poll_link(&mut self) -> Result<Option<LinkEvent>, NicError> {
        let (bmsr, auxsts) = self.backend.sample_link()?;
        let event = self.backend.observe_link(bmsr, auxsts)?;
        if let Some(event) = event {
            self.link_up = !matches!(event, LinkEvent::Down);
            if self.link_up {
                self.release.link_event(event)?;
            }
        }
        Ok(event)
    }

    /// Record the DHCP lease that `netd` applied.
    pub fn dhcp_bound(&mut self, lease: Lease) -> Result<(), NicError> {
        self.release.dhcp_bound(lease)?;
        Ok(())
    }

    /// Record the `ping <gateway>` result.
    pub fn ping_reply(&mut self, report: PingReport) -> Result<(), NicError> {
        self.release.ping_reply(report)?;
        Ok(())
    }

    /// Feed one parsed `nic:release` command.
    pub fn feed_release(&mut self, command: ReleaseCommand) -> Result<(), NicError> {
        match command {
            ReleaseCommand::Dhcp { lease } => self.dhcp_bound(lease),
            ReleaseCommand::Ping { report } => self.ping_reply(report),
        }
    }

    /// Write the complete evidence transcript into `out`; returns the byte count.
    pub fn transcript_text(&self, out: &mut [u8]) -> Option<usize> {
        let transcript = self.transcript()?;
        let mut writer = TextWriter { out, pos: 0 };
        core::fmt::write(&mut writer, format_args!("{transcript}")).ok()?;
        Some(writer.pos)
    }

    /// Service one interrupt-status pass and refill the GENET RX ring.
    pub fn service(&mut self, region: &mut [u8], budget: usize) -> Result<NicReport, NicError> {
        let mut report = NicReport::default();
        if !self.started {
            return Ok(report);
        }
        let status = InterruptStatus::from_raw(self.backend.interrupt_status()?);
        if status.rx_done() {
            for _ in 0..budget {
                if self.backend.path().rx.posted()? == 0 {
                    break;
                }
                let rx_status = self.backend.rx_consumer_status()?;
                let completion = match self.backend.complete_rx(rx_status) {
                    Ok(Some(completion)) => completion,
                    Ok(None) => break,
                    Err(DriverError::Frame(FrameError::StillOwned(_))) => break,
                    Err(error) => return Err(error.into()),
                };
                match completion.frame {
                    Ok(frame) => {
                        if frame.length > 0 && frame.length <= BUFFER_SIZE {
                            match self.publish_rx(region, completion.index, frame.length) {
                                Ok(()) => {
                                    report.rx_delivered = report.rx_delivered.saturating_add(1)
                                }
                                Err(NicError::Ring(RingError::Full)) => {
                                    report.rx_dropped = report.rx_dropped.saturating_add(1);
                                }
                                Err(error) => return Err(error),
                            }
                        } else {
                            report.rx_dropped = report.rx_dropped.saturating_add(1);
                        }
                    }
                    Err(_) => report.rx_errors = report.rx_errors.saturating_add(1),
                }
            }
            report.rx_refilled = self.arm_rx()?;
        }
        if status.tx_done() {
            for _ in 0..budget {
                if self.tx_inflight == 0 {
                    break;
                }
                let tx_status = self.backend.tx_consumer_status()?;
                match self.backend.reclaim_tx(tx_status) {
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(DriverError::Frame(FrameError::StillOwned(_))) => break,
                    Err(error) => return Err(error.into()),
                }
                let mut consumer =
                    RingConsumer::new(region, self.layout, Ring::Tx).map_err(NicError::Layout)?;
                consumer.pop().ok_or(NicError::BadDescriptor)?;
                self.tx_inflight = self.tx_inflight.saturating_sub(1);
                report.tx_completed = report.tx_completed.saturating_add(1);
            }
            self.backend.publish_tx_indices()?;
        }
        let ack = status.ack_bits();
        if ack != 0 {
            self.backend.ack_interrupt(ack)?;
        }
        Ok(report)
    }

    /// Move pending `netd` TX descriptors into the GENET TX ring.
    pub fn drain_tx(&mut self, region: &mut [u8], budget: usize) -> Result<usize, NicError> {
        if !self.started {
            return Ok(0);
        }
        let header = RingHeader::read_from(region).map_err(NicError::Layout)?;
        let available = header
            .tx_prod
            .wrapping_sub(header.tx_cons)
            .min(u32::from(RING_TX_COUNT)) as usize;
        let tx_start = self
            .layout
            .buffer_offset(RING_RX_BUFFERS)
            .ok_or(NicError::BadLayout)?;
        let mut queued = 0usize;
        while self.tx_inflight < RING_TX_BUFFERS as u16
            && (self.tx_inflight as usize) < available
            && queued < budget
        {
            let slot = ((header.tx_cons + u32::from(self.tx_inflight)) % u32::from(RING_TX_COUNT))
                as usize;
            let offset = self
                .layout
                .desc_offset(Ring::Tx, slot)
                .ok_or(NicError::BadDescriptor)?;
            let end = offset
                .checked_add(Desc::LEN)
                .ok_or(NicError::BadDescriptor)?;
            let bytes = region.get(offset..end).ok_or(NicError::BadDescriptor)?;
            let desc = Desc::read_from(bytes).ok_or(NicError::BadDescriptor)?;
            let both = DESC_FIRST | DESC_LAST;
            if desc.flags & both != both || desc.flags & DESC_ERROR != 0 {
                return Err(NicError::BadDescriptor);
            }
            let addr = usize::try_from(desc.addr).map_err(|_| NicError::BadDescriptor)?;
            let len = usize::try_from(desc.len).map_err(|_| NicError::BadDescriptor)?;
            let buffer_end = addr.checked_add(len).ok_or(NicError::BadDescriptor)?;
            if len == 0
                || len > BUFFER_SIZE
                || addr < tx_start
                || !addr.is_multiple_of(64)
                || buffer_end > self.layout.region_len()
            {
                return Err(NicError::BadDescriptor);
            }
            let bus = self
                .region_bus
                .checked_add(addr as u64)
                .ok_or(NicError::BadDescriptor)?;
            let post = self.backend.queue_tx_address(bus, len)?;
            self.backend.write_tx_descriptor(post)?;
            self.tx_inflight = self.tx_inflight.saturating_add(1);
            queued = queued.saturating_add(1);
        }
        if queued != 0 {
            self.backend.publish_tx_indices()?;
        }
        Ok(queued)
    }

    fn publish_rx(&self, region: &mut [u8], index: u16, length: usize) -> Result<(), NicError> {
        let offset = self
            .layout
            .buffer_offset(usize::from(index))
            .ok_or(NicError::BadDescriptor)?;
        let desc = Desc::new(
            offset as u64,
            u32::try_from(length).map_err(|_| NicError::BadDescriptor)?,
            DESC_FIRST | DESC_LAST,
        );
        let mut producer =
            RingProducer::new(region, self.layout, Ring::Rx).map_err(NicError::Layout)?;
        producer.push(desc)?;
        Ok(())
    }
}

impl NicService<PlatformSeam> {
    /// Copy the B39 region out of the DMA window.
    pub fn read_region(&self, region: &mut [u8]) -> Result<(), NicError> {
        if region.len() != RING_REGION_LEN {
            return Err(NicError::BadLayout);
        }
        self.backend.seam().read_dma(RING_REGION_OFFSET, region)?;
        Ok(())
    }

    /// Copy the B39 region back into the DMA window.
    pub fn write_region(&mut self, region: &[u8]) -> Result<(), NicError> {
        if region.len() != RING_REGION_LEN {
            return Err(NicError::BadLayout);
        }
        self.backend
            .seam_mut()
            .write_dma(RING_REGION_OFFSET, region)?;
        Ok(())
    }
}

/// Build the B39 layout used by the GENET `nic:` service.
pub fn ring_layout() -> Result<RingLayout, NicError> {
    RingLayout::new(RING_REGION_LEN, RING_RX_COUNT, RING_TX_COUNT).map_err(NicError::Layout)
}

/// Build the `PlatformLayout` that maps the B39 RX/TX pool buffers to GENET slots.
pub fn platform_layout(layout: RingLayout, dma_len: usize) -> Result<PlatformLayout, NicError> {
    if layout.rx_count() != RING_RX_COUNT
        || layout.tx_count() != RING_TX_COUNT
        || layout.buffer_count() < RING_RX_BUFFERS + RING_TX_BUFFERS
    {
        return Err(NicError::BadLayout);
    }
    let rx_offset = RING_REGION_OFFSET
        .checked_add(layout.pool_offset())
        .ok_or(NicError::BadLayout)?;
    let tx_offset = RING_REGION_OFFSET
        .checked_add(
            layout
                .buffer_offset(RING_RX_BUFFERS)
                .ok_or(NicError::BadLayout)?,
        )
        .ok_or(NicError::BadLayout)?;
    if RING_REGION_OFFSET
        .checked_add(RING_REGION_LEN)
        .is_none_or(|end| end > dma_len)
    {
        return Err(NicError::BadLayout);
    }
    let platform = PlatformLayout {
        rx_offset,
        tx_offset,
        rx_count: usize::from(RING_RX_COUNT),
        tx_count: RING_TX_BUFFERS,
        buffer_len: PACKET_BUFFER_LEN,
        stride: PACKET_BUFFER_LEN,
    };
    platform.validate(dma_len)?;
    Ok(platform)
}

#[cfg(test)]
#[path = "nic_ut.rs"]
mod tests;
