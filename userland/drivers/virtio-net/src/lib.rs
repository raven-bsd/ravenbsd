// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable core of the virtio-net driver: split virtqueue layout, descriptor encoding,
//! virtio-net headers, feature bits and the ARP frame used by the qtest.
//!
//! The device layer (`main.rs`) owns the MMIO and DMA windows; this crate owns the byte-level
//! rules so they can be tested on the host. Nothing here touches hardware, allocates or panics:
//! every function takes slices/arrays and returns `Option`/`Err` for a bad input.
//!
//! `virtio-net` speaks the modern (1.0) virtio-pci interface: a common configuration structure, a
//! notify region, an ISR byte and a device-specific region, all found through PCI capabilities.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use raven_proto_net::ring as netring;

/// Length of the virtio-net header prepended to every frame.
pub const NET_HEADER_LEN: usize = 12;

/// Largest Ethernet frame this driver accepts (1514 bytes without a VLAN tag).
pub const MAX_FRAME_LEN: usize = 1514;

/// Smallest queue size the driver programs; the device must offer at least this many entries.
pub const MIN_QUEUE_SIZE: u16 = 4;

/// Largest queue size the driver programs, so two queues fit in the 64 KiB resource.
pub const MAX_QUEUE_SIZE: u16 = 8;

/// Descriptor flags.
pub const DESC_F_NEXT: u16 = 1;
/// The device writes into this buffer (RX descriptors).
pub const DESC_F_WRITE: u16 = 2;

/// virtio-pci capability types.
pub const CAP_COMMON_CFG: u8 = 1;
/// The notify region.
pub const CAP_NOTIFY_CFG: u8 = 2;
/// The ISR byte.
pub const CAP_ISR_CFG: u8 = 3;
/// The device-specific region.
pub const CAP_DEVICE_CFG: u8 = 4;

/// Device status bits.
pub const STATUS_ACKNOWLEDGE: u8 = 1;
/// The driver knows how to drive the device.
pub const STATUS_DRIVER: u8 = 2;
/// The driver is ready for the device to process queues.
pub const STATUS_DRIVER_OK: u8 = 4;
/// The driver accepted the feature set.
pub const STATUS_FEATURES_OK: u8 = 8;

/// virtio-net feature: device-provided MAC address (word 0, bit 5).
pub const NET_F_MAC: u32 = 1 << 5;
/// virtio-net feature: the device config carries an MTU (word 0, bit 3).
pub const NET_F_MTU: u32 = 1 << 3;
/// virtio-net feature: link status in the device config (word 0, bit 16).
pub const NET_F_STATUS: u32 = 1 << 16;
/// `VIRTIO_F_VERSION_1` lives in the second feature word (bit 32).
pub const F_VERSION_1: u32 = 1 << 0;

/// A MAC address with a dotted-hex display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacAddress(pub [u8; 6]);

impl core::fmt::Display for MacAddress {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let [a, b, c, d, e, f] = self.0;
        write!(formatter, "{a:02x}:{b:02x}:{c:02x}:{d:02x}:{e:02x}:{f:02x}")
    }
}

/// A 64-bit feature set split into the two device-feature words.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Features {
    /// Word 0 (feature bits 0..31).
    pub low: u32,
    /// Word 1 (feature bits 32..63).
    pub high: u32,
}

impl Features {
    /// True when feature `bit` is set.
    pub fn has(&self, bit: u32) -> bool {
        if bit < 32 {
            self.low & (1 << bit) != 0
        } else if bit < 64 {
            self.high & (1 << (bit - 32)) != 0
        } else {
            false
        }
    }
}

/// Byte offsets inside `virtio_pci_common_cfg`.
pub mod common {
    /// `device_feature_select`.
    pub const DEVICE_FEATURE_SELECT: usize = 0;
    /// `device_feature`.
    pub const DEVICE_FEATURE: usize = 4;
    /// `driver_feature_select`.
    pub const DRIVER_FEATURE_SELECT: usize = 8;
    /// `driver_feature`.
    pub const DRIVER_FEATURE: usize = 12;
    /// `msix_config`.
    pub const MSIX_CONFIG: usize = 16;
    /// `num_queues`.
    pub const NUM_QUEUES: usize = 18;
    /// `device_status`.
    pub const DEVICE_STATUS: usize = 20;
    /// `config_generation`.
    pub const CONFIG_GENERATION: usize = 21;
    /// `queue_select`.
    pub const QUEUE_SELECT: usize = 22;
    /// `queue_size`.
    pub const QUEUE_SIZE: usize = 24;
    /// `queue_msix_vector`.
    pub const QUEUE_MSIX_VECTOR: usize = 26;
    /// `queue_enable`.
    pub const QUEUE_ENABLE: usize = 28;
    /// `queue_notify_off`.
    pub const QUEUE_NOTIFY_OFF: usize = 30;
    /// `queue_desc` (64-bit bus address).
    pub const QUEUE_DESC: usize = 32;
    /// `queue_avail` (64-bit bus address).
    pub const QUEUE_AVAIL: usize = 40;
    /// `queue_used` (64-bit bus address).
    pub const QUEUE_USED: usize = 48;
}

/// Layout of one split virtqueue inside a DMA window.
///
/// The descriptor table is 16-byte aligned, the available ring 2-byte and the used ring 4-byte,
/// as the specification requires; `end` is the first byte after the used ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingLayout {
    /// Offset of the descriptor table.
    pub desc: usize,
    /// Offset of the available ring.
    pub avail: usize,
    /// Offset of the used ring.
    pub used: usize,
    /// Number of entries in each ring.
    pub size: u16,
    /// One past the last byte the queue uses.
    pub end: usize,
}

impl RingLayout {
    /// Lay a queue out at `base`, rounding each region up to its alignment.
    pub fn new(base: usize, size: u16) -> Self {
        let count = usize::from(size);
        let desc = align_up(base, 16);
        let avail = align_up(desc + 16 * count, 2);
        let avail_len = 4 + 2 * count;
        let used = align_up(avail + avail_len, 4);
        let used_len = 4 + 8 * count;
        Self {
            desc,
            avail,
            used,
            size,
            end: used + used_len,
        }
    }

    /// Offset of available-ring slot `index` (`ring[index]`).
    pub fn avail_slot(&self, index: u16) -> Option<usize> {
        if index >= self.size {
            return None;
        }
        Some(self.avail + 4 + 2 * usize::from(index))
    }

    /// Offset of used-ring entry `index`.
    pub fn used_slot(&self, index: u16) -> Option<usize> {
        if index >= self.size {
            return None;
        }
        Some(self.used + 4 + 8 * usize::from(index))
    }

    /// Offset of the available-ring `idx` field.
    pub fn avail_idx(&self) -> usize {
        self.avail + 2
    }

    /// Offset of the used-ring `idx` field.
    pub fn used_idx(&self) -> usize {
        self.used + 2
    }
}

/// One split-virtqueue descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Descriptor {
    /// Device-visible buffer address.
    pub addr: u64,
    /// Buffer length in bytes.
    pub len: u32,
    /// [`DESC_F_NEXT`], [`DESC_F_WRITE`] and friends.
    pub flags: u16,
    /// The next descriptor when `DESC_F_NEXT` is set.
    pub next: u16,
}

/// Encode a descriptor in the ring's 16-byte little-endian format.
pub fn encode_descriptor(descriptor: Descriptor) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&descriptor.addr.to_le_bytes());
    bytes[8..12].copy_from_slice(&descriptor.len.to_le_bytes());
    bytes[12..14].copy_from_slice(&descriptor.flags.to_le_bytes());
    bytes[14..16].copy_from_slice(&descriptor.next.to_le_bytes());
    bytes
}

/// Decode one 16-byte descriptor.
pub fn decode_descriptor(bytes: &[u8]) -> Option<Descriptor> {
    if bytes.len() < 16 {
        return None;
    }
    Some(Descriptor {
        addr: u64::from_le_bytes(bytes.get(0..8)?.try_into().ok()?),
        len: u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?),
        flags: u16::from_le_bytes(bytes.get(12..14)?.try_into().ok()?),
        next: u16::from_le_bytes(bytes.get(14..16)?.try_into().ok()?),
    })
}

/// One used-ring entry: which descriptor the device finished and how many bytes it used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsedEntry {
    /// Descriptor index the device finished.
    pub id: u32,
    /// Number of bytes written (RX) or consumed (TX).
    pub len: u32,
}

/// Decode one 8-byte used-ring entry.
pub fn decode_used(bytes: &[u8]) -> Option<UsedEntry> {
    if bytes.len() < 8 {
        return None;
    }
    Some(UsedEntry {
        id: u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?),
        len: u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?),
    })
}

/// Choose the queue size the driver programs: the largest power of two up to
/// [`MAX_QUEUE_SIZE`] that does not exceed `device_max`.
pub fn queue_size(device_max: u16) -> Option<u16> {
    let capped = device_max.min(MAX_QUEUE_SIZE);
    if capped < MIN_QUEUE_SIZE {
        return None;
    }
    let mut size = MIN_QUEUE_SIZE;
    while size * 2 <= capped {
        size *= 2;
    }
    Some(size)
}

/// One `virtio_net_hdr` (the fields the driver sets are zero) in 12 bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetHeader {
    /// `flags`.
    pub flags: u8,
    /// `gso_type`.
    pub gso_type: u8,
    /// `hdr_len`.
    pub hdr_len: u16,
    /// `gso_size`.
    pub gso_size: u16,
    /// `csum_start`.
    pub csum_start: u16,
    /// `csum_offset`.
    pub csum_offset: u16,
}

impl NetHeader {
    /// Encode the header in the 12 bytes a frame is prefixed with.
    pub fn encode(&self) -> [u8; NET_HEADER_LEN] {
        let mut bytes = [0u8; NET_HEADER_LEN];
        bytes[0] = self.flags;
        bytes[1] = self.gso_type;
        bytes[2..4].copy_from_slice(&self.hdr_len.to_le_bytes());
        bytes[4..6].copy_from_slice(&self.gso_size.to_le_bytes());
        bytes[6..8].copy_from_slice(&self.csum_start.to_le_bytes());
        bytes[8..10].copy_from_slice(&self.csum_offset.to_le_bytes());
        bytes[10..12].copy_from_slice(&0u16.to_le_bytes()); // num_buffers (no MRG_RXBUF)
        bytes
    }

    /// Decode a header from the first 12 bytes of a received buffer.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < NET_HEADER_LEN {
            return None;
        }
        Some(Self {
            flags: bytes[0],
            gso_type: bytes[1],
            hdr_len: u16::from_le_bytes(bytes.get(2..4)?.try_into().ok()?),
            gso_size: u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?),
            csum_start: u16::from_le_bytes(bytes.get(6..8)?.try_into().ok()?),
            csum_offset: u16::from_le_bytes(bytes.get(8..10)?.try_into().ok()?),
        })
    }
}

/// Build an ARP request for `target_ip` from `src_mac`/`src_ip` into `out`.
///
/// Returns the frame length (42) or `None` when `out` cannot hold it. The caller prefixes it with
/// a [`NetHeader`] in the device buffer.
pub fn build_arp_request(
    src_mac: MacAddress,
    src_ip: [u8; 4],
    target_ip: [u8; 4],
    out: &mut [u8],
) -> Option<usize> {
    const LEN: usize = 42;
    let frame = out.get_mut(..LEN)?;
    frame[0..6].copy_from_slice(&[0xFF; 6]);
    frame[6..12].copy_from_slice(&src_mac.0);
    frame[12..14].copy_from_slice(&0x0806u16.to_be_bytes()); // ARP
    frame[14..16].copy_from_slice(&1u16.to_be_bytes()); // Ethernet
    frame[16..18].copy_from_slice(&0x0800u16.to_be_bytes()); // IPv4
    frame[18] = 6;
    frame[19] = 4;
    frame[20..22].copy_from_slice(&1u16.to_be_bytes()); // request
    frame[22..28].copy_from_slice(&src_mac.0);
    frame[28..32].copy_from_slice(&src_ip);
    frame[32..38].copy_from_slice(&[0u8; 6]);
    frame[38..42].copy_from_slice(&target_ip);
    Some(LEN)
}

/// A parsed ARP reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArpReply {
    /// Sender hardware address.
    pub sender_mac: MacAddress,
    /// Sender protocol address.
    pub sender_ip: [u8; 4],
    /// Target protocol address (the requester).
    pub target_ip: [u8; 4],
}

/// Parse an Ethernet II ARP reply.
///
/// Returns `None` for a truncated frame, a non-ARP ethertype, a non-Ethernet/IPv4 ARP or a reply
/// whose length fields disagree with the frame.
pub fn parse_arp_reply(frame: &[u8]) -> Option<ArpReply> {
    if frame.len() < 42 {
        return None;
    }
    if u16::from_be_bytes(frame.get(12..14)?.try_into().ok()?) != 0x0806 {
        return None;
    }
    if u16::from_be_bytes(frame.get(14..16)?.try_into().ok()?) != 1
        || u16::from_be_bytes(frame.get(16..18)?.try_into().ok()?) != 0x0800
    {
        return None;
    }
    if *frame.get(18)? != 6 || *frame.get(19)? != 4 {
        return None;
    }
    if u16::from_be_bytes(frame.get(20..22)?.try_into().ok()?) != 2 {
        return None;
    }
    let mut sender_mac = [0u8; 6];
    sender_mac.copy_from_slice(frame.get(22..28)?);
    let mut ip = [0u8; 4];
    ip.copy_from_slice(frame.get(28..32)?);
    let mut target_ip = [0u8; 4];
    target_ip.copy_from_slice(frame.get(38..42)?);
    Some(ArpReply {
        sender_mac: MacAddress(sender_mac),
        sender_ip: ip,
        target_ip,
    })
}

/// Parse a colon-separated MAC address (`52:54:00:12:34:56`).
pub fn parse_mac(text: &str) -> Option<MacAddress> {
    let bytes = text.as_bytes();
    if bytes.len() != 17 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (index, slot) in mac.iter_mut().enumerate() {
        let offset = index * 3;
        if index != 0 && bytes.get(offset - 1) != Some(&b':') {
            return None;
        }
        let high = hex_digit(*bytes.get(offset)?)?;
        let low = hex_digit(*bytes.get(offset + 1)?)?;
        *slot = (high << 4) | low;
    }
    Some(MacAddress(mac))
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Control scheme and paths served by the persistent driver.
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
/// Arm the ring pump.
pub const NIC_START: &str = "start";
/// Stop the ring pump.
pub const NIC_STOP: &str = "stop";
/// Test-only clean shutdown.
pub const NIC_QUIT: &str = "quit";

/// Offset of the B39 ring region inside the 64 KiB DMA buffer.
pub const RING_REGION_OFFSET: usize = 0x1000;
/// Length of the B39 ring region.
pub const RING_REGION_LEN: usize = 0x6000;
/// RX descriptors in the B39 ring.
pub const RING_RX_COUNT: u16 = 8;
/// TX descriptors in the B39 ring.
pub const RING_TX_COUNT: u16 = 8;
/// Pool buffers reserved for the device RX queue (one per RX descriptor).
pub const RING_RX_BUFFERS: usize = 8;
/// Bytes the device writes before an RX frame (the virtio-net header).
pub const VIRTIO_NET_HEADER: usize = NET_HEADER_LEN;

/// The B39 layout used by the driver and its consumer.
pub fn ring_layout() -> Result<netring::RingLayout, netring::LayoutError> {
    netring::RingLayout::new(RING_REGION_LEN, RING_RX_COUNT, RING_TX_COUNT)
}

/// The control summary returned by `nic:info`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlInfo {
    /// Device MAC.
    pub mac: MacAddress,
    /// Device MTU.
    pub mtu: u16,
    /// Link state.
    pub link: bool,
    /// DMA buffer length in bytes.
    pub dma_len: usize,
    /// Ring region offset inside the DMA buffer.
    pub ring_offset: usize,
    /// Ring region length.
    pub ring_len: usize,
    /// RX descriptor count.
    pub rx_count: u16,
    /// TX descriptor count.
    pub tx_count: u16,
    /// Buffer pool length inside the region.
    pub pool_len: usize,
}

impl ControlInfo {
    /// Encode the `key=value` summary into `out`; returns the byte count.
    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        let mut writer = TextWriter { out, pos: 0 };
        let link = if self.link { "up" } else { "down" };
        core::fmt::write(
            &mut writer,
            format_args!(
                "mac={}\nmtu={}\nlink={}\nring_offset={}\nring_len={}\nrx={}\ntx={}\npool={}\ndma={}\n",
                self.mac,
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

    /// Decode the summary; every key is required and unknown keys are rejected.
    pub fn decode(text: &str) -> Option<Self> {
        let mut mac = None;
        let mut mtu = None;
        let mut link = None;
        let mut ring_offset = None;
        let mut ring_len = None;
        let mut rx_count = None;
        let mut tx_count = None;
        let mut pool_len = None;
        let mut dma_len = None;
        for line in text.lines() {
            let (key, value) = line.split_once('=')?;
            match key {
                "mac" => mac = Some(parse_mac(value)?),
                "mtu" => mtu = Some(value.parse().ok()?),
                "link" => {
                    link = Some(match value {
                        "up" => true,
                        "down" => false,
                        _ => return None,
                    })
                }
                "ring_offset" => ring_offset = Some(value.parse().ok()?),
                "ring_len" => ring_len = Some(value.parse().ok()?),
                "rx" => rx_count = Some(value.parse().ok()?),
                "tx" => tx_count = Some(value.parse().ok()?),
                "pool" => pool_len = Some(value.parse().ok()?),
                "dma" => dma_len = Some(value.parse().ok()?),
                _ => return None,
            }
        }
        Some(Self {
            mac: mac?,
            mtu: mtu?,
            link: link?,
            dma_len: dma_len?,
            ring_offset: ring_offset?,
            ring_len: ring_len?,
            rx_count: rx_count?,
            tx_count: tx_count?,
            pool_len: pool_len?,
        })
    }
}

struct TextWriter<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl core::fmt::Write for TextWriter<'_> {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let end = self.pos.checked_add(text.len()).ok_or(core::fmt::Error)?;
        let target = self.out.get_mut(self.pos..end).ok_or(core::fmt::Error)?;
        target.copy_from_slice(text.as_bytes());
        self.pos = end;
        Ok(())
    }
}

/// Validate one B39 TX descriptor and return the frame bytes inside `region`.
///
/// The descriptor must be a complete frame (`FIRST|LAST`, no `ERROR`), inside the buffer pool and
/// no larger than one pool buffer.
pub fn frame_desc<'a>(
    region: &'a [u8],
    layout: &netring::RingLayout,
    desc: netring::Desc,
) -> Option<&'a [u8]> {
    let both = netring::DESC_FIRST | netring::DESC_LAST;
    if desc.flags & both != both || desc.flags & netring::DESC_ERROR != 0 {
        return None;
    }
    let addr = usize::try_from(desc.addr).ok()?;
    let len = usize::try_from(desc.len).ok()?;
    if len == 0 || len > netring::BUFFER_SIZE || addr < layout.pool_offset() {
        return None;
    }
    let end = addr.checked_add(len)?;
    if end > layout.region_len() {
        return None;
    }
    region.get(addr..end)
}

/// Build the RX descriptor for one delivered pool buffer.
///
/// The device writes the virtio-net header first, so the descriptor points after it and carries
/// only the Ethernet frame.
pub fn rx_desc(
    layout: &netring::RingLayout,
    buffer: usize,
    frame_len: usize,
) -> Option<netring::Desc> {
    if frame_len == 0 || frame_len > MAX_FRAME_LEN || buffer >= RING_RX_BUFFERS {
        return None;
    }
    let start = layout
        .buffer_offset(buffer)?
        .checked_add(VIRTIO_NET_HEADER)?;
    let end = start.checked_add(frame_len)?;
    if end > layout.region_len() {
        return None;
    }
    Some(netring::Desc::new(
        start as u64,
        frame_len as u32,
        netring::DESC_FIRST | netring::DESC_LAST,
    ))
}

fn align_up(value: usize, alignment: usize) -> usize {
    if alignment <= 1 {
        return value;
    }
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
