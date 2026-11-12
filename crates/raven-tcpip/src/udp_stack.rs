// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! UDP input dispatch and datagram delivery.

use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::udp::UdpPacket;

use crate::device::Device;
use crate::ipv4_stack::to_wire;
use crate::socket::{Datagram, Kind};
use crate::{Ipv4Addr, SocketId, Stack};

impl<D: Device> Stack<D> {
    pub(crate) fn handle_udp(&mut self, source: Ipv4Addr, destination: Ipv4Addr, payload: &[u8]) {
        let context = ChecksumContext::ipv4(to_wire(source), to_wire(destination));
        let Ok(datagram) = UdpPacket::parse(payload, context) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        let destination_port = datagram.dst_port();
        let socket = self.sockets.records.iter().find_map(|(raw_id, record)| {
            if record.closed {
                return None;
            }
            match &record.kind {
                Kind::Udp { local_port, .. } if *local_port == destination_port => {
                    Some(SocketId(*raw_id))
                }
                _ => None,
            }
        });
        let Some(socket) = socket else {
            self.drops = self.drops.saturating_add(1);
            if !destination.is_broadcast() && !destination.is_multicast() {
                self.send_port_unreachable(source, destination, datagram.as_bytes());
            }
            return;
        };
        let Ok(record) = self.sockets.record_mut(socket) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        if datagram.payload_len() > record.rx_cap {
            self.drops = self.drops.saturating_add(1);
            return;
        }
        let Kind::Udp { datagrams, .. } = &mut record.kind else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        datagrams.push_back(Datagram {
            data: datagram.payload().to_vec(),
            src: (crate::ipv6_stack::IpAddr::V4(source), datagram.src_port()),
        });
    }
}
