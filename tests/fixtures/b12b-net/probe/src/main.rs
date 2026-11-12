// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B39 ring consumer for the `net-b` qtest.
//!
//! The probe starts after `scheme:nic`, maps the DMA buffer the driver attaches at `nic:ring`,
//! sends one ARP request through the TX ring and receives QEMU's reply from the RX ring.  It then
//! asks the driver to stop (test mode), waits for the next devd launch to publish `nic:` again and
//! repeats the exchange, so the qtest covers both launch/exit cycles.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_proto_net::ring::{
        DESC_FIRST, DESC_LAST, Desc, Ring, RingConsumer, RingLayout, RingProducer,
    };
    use raven_rt::entry;
    use raven_scheme::Client;
    use virtio_net::{
        ArpReply, ControlInfo, RING_RX_BUFFERS, build_arp_request, frame_desc, parse_arp_reply,
    };

    entry!(main);

    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;
    const GUEST_IP: [u8; 4] = [10, 0, 2, 15];
    const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];
    const ARP_LEN: usize = 42;
    const POLL_ROUNDS: usize = 500;
    const POLL_NS: u64 = 10_000_000;
    const RECONNECT_ROUNDS: usize = 600;
    const RECONNECT_NS: u64 = 100_000_000;
    const DEVD_ROUNDS: usize = 600;
    const DEVD_NS: u64 = 50_000_000;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("net-b: error: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        for cycle in 1..=2u32 {
            let info = info(&client)?;
            let handle = client
                .open("nic:ring", OPEN_READ)
                .map_err(|error| format!("open nic:ring: {}", error.description()))?;
            let window = raven_rt::map_mmio_mut(handle, info.dma_len)
                .map_err(|error| format!("map ring: {}", error.description()))?;
            let end = info
                .ring_offset
                .checked_add(info.ring_len)
                .ok_or_else(|| "ring range".to_string())?;
            let region = window
                .get_mut(info.ring_offset..end)
                .ok_or_else(|| "ring slice".to_string())?;
            let layout = RingLayout::new(info.ring_len, info.rx_count, info.tx_count)
                .map_err(|error| format!("ring layout: {error}"))?;
            raven_rt::println!(
                "net-b: cycle {cycle} mac={} mtu={} link={} ring offset={:#x} len={} rx={} tx={} dma={}",
                info.mac,
                info.mtu,
                if info.link { "up" } else { "down" },
                info.ring_offset,
                info.ring_len,
                info.rx_count,
                info.tx_count,
                info.dma_len
            );
            start(&client)?;
            exchange(region, &layout, &info, cycle)?;
            quit(&client)?;
            raven_rt::println!("net-b: cycle {cycle} quit");
            if cycle == 1 {
                wait_for_nic(&client)?;
            }
        }
        wait_for_devd(&client)?;
        raven_rt::println!("net-b: probe ok");
        Ok(())
    }

    fn info(client: &Client) -> Result<ControlInfo, String> {
        let handle = client
            .open("nic:info", OPEN_READ)
            .map_err(|error| format!("open nic:info: {}", error.description()))?;
        let mut data = [0u8; 256];
        let length = client
            .read(handle, 0, &mut data)
            .map_err(|error| format!("read nic:info: {}", error.description()))?;
        let _ = client.close(handle);
        let text = core::str::from_utf8(data.get(..length).ok_or("info length")?)
            .map_err(|_| "info is not UTF-8".to_string())?;
        ControlInfo::decode(text).ok_or_else(|| "decode nic:info".to_string())
    }

    fn start(client: &Client) -> Result<(), String> {
        write_control(client, "nic:start")
    }

    fn quit(client: &Client) -> Result<(), String> {
        write_control(client, "nic:quit")
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

    fn exchange(
        region: &mut [u8],
        layout: &RingLayout,
        info: &ControlInfo,
        cycle: u32,
    ) -> Result<(), String> {
        let offset = layout
            .buffer_offset(RING_RX_BUFFERS)
            .ok_or_else(|| "TX pool buffer".to_string())?;
        let mut frame = [0u8; ARP_LEN];
        build_arp_request(info.mac, GUEST_IP, GATEWAY_IP, &mut frame)
            .ok_or_else(|| "ARP frame".to_string())?;
        region
            .get_mut(offset..offset + ARP_LEN)
            .ok_or_else(|| "TX frame slice".to_string())?
            .copy_from_slice(&frame);
        {
            let mut producer = RingProducer::new(region, *layout, Ring::Tx)
                .map_err(|error| format!("TX ring: {error}"))?;
            producer
                .push(Desc::new(
                    offset as u64,
                    ARP_LEN as u32,
                    DESC_FIRST | DESC_LAST,
                ))
                .map_err(|error| format!("TX push: {error}"))?;
        }
        raven_rt::println!("net-b: cycle {cycle} tx frame {ARP_LEN} bytes");

        for _ in 0..POLL_ROUNDS {
            let desc = {
                let mut consumer = RingConsumer::new(region, *layout, Ring::Rx)
                    .map_err(|error| format!("RX ring: {error}"))?;
                consumer.pop()
            };
            if let Some(desc) = desc {
                let frame = frame_desc(region, layout, desc)
                    .ok_or_else(|| "RX descriptor out of range".to_string())?;
                if let Some(reply) = parse_arp_reply(frame) {
                    print_reply(&reply, frame.len(), cycle);
                    // The TX completion frees the pool buffer; wait for it before quitting.
                    wait_tx(region, layout)?;
                    return Ok(());
                }
            }
            let _ = raven_rt::nanosleep(POLL_NS);
        }
        Err(format!("cycle {cycle}: no ARP reply"))
    }

    fn print_reply(reply: &ArpReply, length: usize, cycle: u32) {
        raven_rt::println!("net-b: cycle {cycle} rx frame {length} bytes");
        raven_rt::println!(
            "net-b: cycle {cycle} arp reply from {} ({}.{}.{}.{})",
            reply.sender_mac,
            reply.sender_ip[0],
            reply.sender_ip[1],
            reply.sender_ip[2],
            reply.sender_ip[3]
        );
    }

    /// Wait until the driver retired the TX descriptor (the highest `tx_cons` seen).
    fn wait_tx(region: &mut [u8], layout: &RingLayout) -> Result<(), String> {
        for _ in 0..POLL_ROUNDS {
            let consumer = RingConsumer::new(region, *layout, Ring::Tx)
                .map_err(|error| format!("TX ring: {error}"))?;
            if consumer.is_empty() {
                return Ok(());
            }
            let _ = raven_rt::nanosleep(POLL_NS);
        }
        Err("TX descriptor was not retired".to_string())
    }

    /// Wait for the next devd launch to publish `nic:` again.
    fn wait_for_nic(client: &Client) -> Result<(), String> {
        for _ in 0..RECONNECT_ROUNDS {
            if let Ok(handle) = client.open("nic:info", OPEN_READ) {
                let _ = client.close(handle);
                raven_rt::println!("net-b: nic: reappeared");
                return Ok(());
            }
            let _ = raven_rt::nanosleep(RECONNECT_NS);
        }
        Err("nic: did not reappear for the second cycle".to_string())
    }

    /// Keep this readiness-`exit` service alive until devd has finished its second cycle, so svcd
    /// cannot settle (and the qtest cannot end) before the driver's cycle-2 teardown lines.
    fn wait_for_devd(client: &Client) -> Result<(), String> {
        for _ in 0..DEVD_ROUNDS {
            if let Ok(line) = devd_state(client)
                && (line.contains(" stopped")
                    || line.contains(" failed")
                    || line.contains(" gave-up"))
            {
                raven_rt::println!("net-b: devd done");
                return Ok(());
            }
            let _ = raven_rt::nanosleep(DEVD_NS);
        }
        Err("devd did not finish the second cycle".to_string())
    }

    fn devd_state(client: &Client) -> Result<String, String> {
        let handle = client
            .open("svc:/devd", OPEN_READ)
            .map_err(|error| format!("open svc:/devd: {}", error.description()))?;
        let mut data = [0u8; 128];
        let length = client
            .read(handle, 0, &mut data)
            .map_err(|error| format!("read svc:/devd: {}", error.description()))?;
        let _ = client.close(handle);
        core::str::from_utf8(data.get(..length).ok_or("svc length")?)
            .map(|line| line.to_string())
            .map_err(|_| "svc line is not UTF-8".to_string())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
