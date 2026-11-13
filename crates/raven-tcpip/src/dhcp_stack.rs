// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The DHCPv4 client wired into the stack: UDP framing from the unconfigured address, lease
//! application and the poll timer.

use alloc::vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, MacAddr};
use raven_tcpip_wire::ipv4::{Ipv4Builder, Ipv4Header};
use raven_tcpip_wire::udp::{UdpBuilder, UdpPacket};

use crate::device::Device;
use crate::dhcp::{self, CLIENT_PORT, Dhcp, Event, Lease, Outgoing, SERVER_PORT};
use crate::ipv4_stack::to_wire;
use crate::stack::{Ipv4Config, Stack};
use crate::{Instant, Ipv4Addr};

impl<D: Device> Stack<D> {
    /// Start the DHCPv4 client: it discovers a server, leases an address and configures the
    /// interface, renewing before the lease ends. A later [`Stack::set_ipv4`] stops it.
    ///
    /// # Errors
    /// `ENETUNREACH` when the stack has no hardware address to identify itself with.
    pub fn dhcp_enable(&mut self) -> Result<(), Errno> {
        if self.dhcp.is_some() {
            return Ok(());
        }
        let caps = self.device.capabilities();
        let mac = self
            .config
            .hardware_addr
            .or(caps.hardware_addr)
            .ok_or(Errno::ENETUNREACH)?;
        let mut seed = [0u8; 8];
        self.rng.fill(&mut seed);
        let mut client = Dhcp::new(mac, u64::from_le_bytes(seed));
        client.set_retry_config(self.dhcp_retry);
        self.dhcp = Some(client);
        Ok(())
    }

    /// Change the DHCP retry timers; applies to the running client and to one started later.
    pub fn dhcp_set_retry_config(&mut self, retry: crate::dhcp::RetryConfig) {
        self.dhcp_retry = retry;
        if let Some(client) = self.dhcp.as_mut() {
            client.set_retry_config(retry);
        }
    }

    /// Whether the DHCPv4 client is running.
    #[must_use]
    pub fn dhcp_active(&self) -> bool {
        self.dhcp.is_some()
    }

    /// The current lease, once the client has one.
    #[must_use]
    pub fn dhcp_lease(&self) -> Option<&Lease> {
        self.dhcp.as_ref().and_then(Dhcp::lease)
    }

    /// The current lease's absolute T1/T2/expiry timers, once the client has one.
    #[must_use]
    pub fn dhcp_timers(&self) -> Option<crate::dhcp::DhcpTimers> {
        self.dhcp.as_ref().and_then(Dhcp::timers)
    }

    /// When the client next needs a poll, for [`Stack::next_poll_delay`].
    pub(crate) fn dhcp_deadline(&self) -> Option<Instant> {
        self.dhcp
            .as_ref()
            .map(|client| Instant::from_millis(client.poll_at()))
    }

    /// Run the client's timers, apply its events and transmit what it wants sent.
    pub(crate) fn dhcp_step(&mut self, now: Instant) {
        self.apply_dhcp_events();
        let Some(client) = self.dhcp.as_mut() else {
            return;
        };
        let outgoing = client.poll(now.as_millis());
        self.apply_dhcp_events();
        if let Some(outgoing) = outgoing {
            let _ = self.dhcp_send(&outgoing);
        }
    }

    fn apply_dhcp_events(&mut self) {
        while let Some(event) = self.dhcp.as_mut().and_then(Dhcp::take_event) {
            match event {
                Event::Configured(lease) => {
                    let config = Ipv4Config {
                        addr: lease.address,
                        prefix_len: lease.prefix_len,
                        gateway: lease.router,
                    };
                    let _ = self.apply_ipv4(config);
                }
                Event::Deconfigured => {
                    self.ipv4 = None;
                    self.routes.clear();
                }
            }
        }
    }

    /// Hand a received DHCP datagram to the client; true when it was one.
    pub(crate) fn dhcp_input(
        &mut self,
        source: Ipv4Addr,
        destination: Ipv4Addr,
        udp: &[u8],
        now: Instant,
    ) -> bool {
        let context = ChecksumContext::ipv4(to_wire(source), to_wire(destination));
        let Ok(datagram) = UdpPacket::parse(udp, context) else {
            return false;
        };
        if datagram.dst_port() != CLIENT_PORT || datagram.src_port() != SERVER_PORT {
            return false;
        }
        if let Some(client) = self.dhcp.as_mut() {
            client.on_packet(now.as_millis(), source, datagram.payload());
        }
        self.dhcp_step(now);
        true
    }

    fn dhcp_send(&mut self, outgoing: &Outgoing) -> Result<(), Errno> {
        let Some(mac) = self.dhcp.as_ref().map(Dhcp::mac) else {
            return Ok(());
        };
        let caps = self.device.capabilities();
        let mtu = u16::try_from(caps.mtu.saturating_sub(14)).unwrap_or(1500);
        let message = dhcp::encode(outgoing, mac, mtu);
        let source = outgoing.ciaddr;
        let destination = outgoing.unicast_to.unwrap_or(Ipv4Addr::new([255; 4]));
        let mut datagram = vec![0u8; 8 + message.len()];
        {
            let context = ChecksumContext::ipv4(to_wire(source), to_wire(destination));
            let mut builder = UdpBuilder::new(
                &mut datagram,
                context,
                CLIENT_PORT,
                SERVER_PORT,
                message.len(),
            )
            .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(&message);
            builder.finish();
        }
        if outgoing.unicast_to.is_some() {
            return self.send_ipv4(&datagram, destination, 17);
        }
        let header = Ipv4Header {
            id: self.next_ip_id,
            ttl: 64,
            protocol: 17,
            src: to_wire(source),
            dst: to_wire(destination),
            ..Ipv4Header::default()
        };
        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let mut packet = vec![0u8; 20 + datagram.len()];
        {
            let mut builder = Ipv4Builder::new(&mut packet, &header, datagram.len())
                .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(&datagram);
        }
        self.transmit_ethernet(MacAddr::BROADCAST, EtherType::Ipv4, &packet)
    }
}
