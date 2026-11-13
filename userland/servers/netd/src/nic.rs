// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target-only adapter from the persistent `nic:` frame service to [`raven_tcpip::Device`].
//!
//! The driver owns the virtio queues and exposes a B39 shared ring: `nic:info` describes the DMA
//! window, `nic:ring` returns the mapping attachment, and `nic:start` arms the pump. This module
//! maps that window, drains the RX ring and fills the TX ring without a per-device thread.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::{String, ToString};

use raven_abi::types::Handle;
use raven_netcore::{ChecksumOffload, Device, DeviceCaps, Instant, Medium, TxError};
use raven_proto_net::ring::{
    BUFFER_SIZE, DESC_FIRST, DESC_LAST, Desc, Ring, RingConsumer, RingHeader, RingLayout,
    RingProducer,
};
use raven_scheme::Client;
use virtio_net::{ControlInfo, MAX_FRAME_LEN, RING_RX_BUFFERS, frame_desc};

use crate::{Loopback, Netd, NicConfig};

/// Address QEMU user-mode assigns to the guest.
pub const GUEST_ADDR: [u8; 4] = [10, 0, 2, 15];
/// Address QEMU user-mode uses for the gateway/host alias.
pub const GATEWAY_ADDR: [u8; 4] = [10, 0, 2, 2];
/// Prefix length of the QEMU user-mode network.
pub const GUEST_PREFIX: u8 = 24;

const OPEN_READ: u32 = 1;
const OPEN_WRITE: u32 = 2;

/// Attach the `nic:` service as the second core.
pub fn attach(netd: &mut Netd<Loopback>, now_ms: i64) -> Result<NicConfig, String> {
    let (device, config) = NicDevice::open()?;
    netd.attach_nic(device, config, now_ms);
    Ok(config)
}

struct NicDevice {
    _ring_handle: Handle,
    window: &'static mut [u8],
    ring_offset: usize,
    ring_len: usize,
    layout: RingLayout,
    tx_free: VecDeque<usize>,
    tx_inflight: VecDeque<usize>,
    tx_cons_seen: u32,
    mac: [u8; 6],
    mtu: u16,
}

impl NicDevice {
    fn ring(&self) -> &[u8] {
        let start = self.ring_offset;
        let end = start + self.ring_len;
        &self.window[start..end]
    }

    fn ring_mut(&mut self) -> &mut [u8] {
        let start = self.ring_offset;
        let end = start + self.ring_len;
        &mut self.window[start..end]
    }
}

impl NicDevice {
    fn open() -> Result<(Self, NicConfig), String> {
        let client = Client::new();
        let info = read_info(&client)?;
        if !info.link {
            return Err("link is down".to_string());
        }
        let ring_handle = client
            .open("nic:ring", OPEN_READ)
            .map_err(|error| format!("open nic:ring: {}", error.description()))?;
        let window = raven_rt::map_mmio_mut(ring_handle, info.dma_len)
            .map_err(|error| format!("map nic ring: {}", error.description()))?;
        let end = info
            .ring_offset
            .checked_add(info.ring_len)
            .ok_or_else(|| "ring range overflow".to_string())?;
        if end > window.len() {
            return Err("ring range is outside the DMA window".to_string());
        }
        let layout = RingLayout::new(info.ring_len, info.rx_count, info.tx_count)
            .map_err(|error| format!("ring layout: {error}"))?;
        let header = RingHeader::read_from(&window[info.ring_offset..end])
            .map_err(|error| format!("ring header: {error}"))?;
        if header.rx_count != info.rx_count || header.tx_count != info.tx_count {
            return Err("ring header counts differ from nic:info".to_string());
        }
        if layout.buffer_count() <= RING_RX_BUFFERS {
            return Err("ring pool has no TX buffers".to_string());
        }
        let mut tx_free = VecDeque::new();
        for index in RING_RX_BUFFERS..layout.buffer_count() {
            tx_free.push_back(index);
        }
        write_control(&client, "nic:start")?;
        let config = NicConfig {
            mac: info.mac.0,
            mtu: info.mtu,
            addr: GUEST_ADDR,
            prefix: GUEST_PREFIX,
            gateway: Some(GATEWAY_ADDR),
            dhcp: true,
        };
        Ok((
            Self {
                _ring_handle: ring_handle,
                window,
                ring_offset: info.ring_offset,
                ring_len: info.ring_len,
                layout,
                tx_free,
                tx_inflight: VecDeque::new(),
                tx_cons_seen: header.tx_cons,
                mac: info.mac.0,
                mtu: info.mtu,
            },
            config,
        ))
    }

    fn reclaim_tx(&mut self) {
        let Ok(header) = RingHeader::read_from(self.ring()) else {
            return;
        };
        let mut pending = header.tx_cons.wrapping_sub(self.tx_cons_seen) as usize;
        while pending > 0 {
            let Some(index) = self.tx_inflight.pop_front() else {
                // The driver consumed entries this producer does not know about; resynchronise.
                self.tx_cons_seen = header.tx_cons;
                return;
            };
            self.tx_free.push_back(index);
            self.tx_cons_seen = self.tx_cons_seen.wrapping_add(1);
            pending -= 1;
        }
    }
}

impl Device for NicDevice {
    fn capabilities(&self) -> DeviceCaps {
        DeviceCaps {
            medium: Medium::Ethernet,
            mtu: usize::from(self.mtu).saturating_add(14).min(MAX_FRAME_LEN),
            hardware_addr: Some(self.mac),
            checksum_offload: ChecksumOffload::None,
        }
    }

    fn receive(&mut self, _now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let mut delivered = false;
        for _ in 0..self.layout.rx_count() {
            let desc = {
                let layout = self.layout;
                let Ok(consumer) = RingConsumer::new(self.ring_mut(), layout, Ring::Rx) else {
                    break;
                };
                let Some(desc) = consumer.peek() else {
                    break;
                };
                desc
            };
            let mut frame = [0u8; MAX_FRAME_LEN];
            let mut length = 0usize;
            if let Some(bytes) = frame_desc(self.ring(), &self.layout, desc)
                && bytes.len() <= MAX_FRAME_LEN
            {
                frame[..bytes.len()].copy_from_slice(bytes);
                length = bytes.len();
            }
            let layout = self.layout;
            if let Ok(mut consumer) = RingConsumer::new(self.ring_mut(), layout, Ring::Rx) {
                let _ = consumer.pop();
            }
            if length > 0 {
                consume(&frame[..length]);
                delivered = true;
            }
        }
        delivered
    }

    fn transmit(
        &mut self,
        _now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        if len > self.capabilities().mtu || len > BUFFER_SIZE {
            return Err(TxError::TooLong);
        }
        self.reclaim_tx();
        let Some(index) = self.tx_free.pop_front() else {
            return Err(TxError::Busy);
        };
        let Some(offset) = self.layout.buffer_offset(index) else {
            return Err(TxError::TooLong);
        };
        let Some(end) = offset.checked_add(len) else {
            return Err(TxError::TooLong);
        };
        let Some(buffer) = self.ring_mut().get_mut(offset..end) else {
            return Err(TxError::TooLong);
        };
        fill(buffer);
        let descriptor = Desc::new(offset as u64, len as u32, DESC_FIRST | DESC_LAST);
        let layout = self.layout;
        let pushed = match RingProducer::new(self.ring_mut(), layout, Ring::Tx) {
            Ok(mut producer) => producer.push(descriptor),
            Err(_) => {
                self.tx_free.push_front(index);
                return Err(TxError::Down);
            }
        };
        match pushed {
            Ok(()) => {
                self.tx_inflight.push_back(index);
                Ok(())
            }
            Err(_) => {
                self.tx_free.push_front(index);
                Err(TxError::Busy)
            }
        }
    }
}

fn read_info(client: &Client) -> Result<ControlInfo, String> {
    let handle = client
        .open("nic:info", OPEN_READ)
        .map_err(|error| format!("open nic:info: {}", error.description()))?;
    let mut data = [0u8; 256];
    let length = client
        .read(handle, 0, &mut data)
        .map_err(|error| format!("read nic:info: {}", error.description()))?;
    let _ = client.close(handle);
    let text = core::str::from_utf8(
        data.get(..length)
            .ok_or_else(|| "short nic:info".to_string())?,
    )
    .map_err(|_| "nic:info is not UTF-8".to_string())?;
    ControlInfo::decode(text).ok_or_else(|| "decode nic:info".to_string())
}

fn write_control(client: &Client, path: &str) -> Result<(), String> {
    let handle = client
        .open(path, OPEN_WRITE)
        .map_err(|error| format!("open {path}: {}", error.description()))?;
    client
        .write(handle, 0, b"1")
        .map_err(|error| format!("write {path}: {}", error.description()))?;
    let _ = client.close(handle);
    Ok(())
}
