// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The resolver's route and family probes.

use raven_libc_abi::socket::SOCK_DGRAM;
use raven_libc_resolv::{Family, IpAddr, Probe, SockAddr};
use raven_proto_net::{Call, Endpoint, IF_LINK_UP, IF_LOOPBACK, IF_UP, IfaceInfo};

use crate::iface::list;
use crate::msg::bare_path;
use crate::{CallReply, Net, NetBackend, NetEnv, SocketFds, UnixBackend};

/// The resolver probe implementation; borrows one [`Net`].
pub struct NetProbe<'a, B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend = crate::NoUnix> {
    net: &'a Net<B, F, E, U>,
}

impl<'a, B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> NetProbe<'a, B, F, E, U> {
    pub(crate) fn new(net: &'a Net<B, F, E, U>) -> Self {
        Self { net }
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Probe for NetProbe<'_, B, F, E, U> {
    fn udp_route(&self, dst: &SockAddr) -> Option<IpAddr> {
        let IpAddr::V4(destination) = dst.ip else {
            return None;
        };
        if self.net.backend().caps().udp_route_fail {
            return self.kernel_route(destination, dst.port);
        }
        let interfaces = list(self.net.backend()).ok()?;
        approximation(&interfaces, destination)
    }

    fn family_usable(&self, f: Family) -> bool {
        if f == Family::V6 && !self.net.backend.caps().ipv6 {
            return false;
        }
        let Ok(interfaces) = list(self.net.backend()) else {
            return false;
        };
        interfaces.iter().any(usable)
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> NetProbe<'_, B, F, E, U> {
    fn kernel_route(&self, destination: [u8; 4], port: u16) -> Option<IpAddr> {
        let mut buffer = [0u8; 16];
        let path = bare_path(SOCK_DGRAM, &mut buffer).ok()?;
        let sock = self.net.backend().open(path).ok()?;
        let endpoint = Endpoint {
            ip: destination,
            port: port.max(1),
        };
        let result = self
            .net
            .backend()
            .call(sock, Call::Connect(endpoint))
            .ok()?;
        if result != CallReply::Ok {
            self.net.backend().close(sock);
            return None;
        }
        let local = match self.net.backend().call(sock, Call::GetSockName) {
            Ok(CallReply::Endpoint(endpoint)) => Some(IpAddr::V4(endpoint.ip)),
            _ => None,
        };
        self.net.backend().close(sock);
        local
    }
}

fn usable(info: &IfaceInfo) -> bool {
    info.flags & IF_UP != 0 && (info.flags & IF_LINK_UP != 0 || info.flags & IF_LOOPBACK != 0)
}

/// The deterministic approximation used before PG-41.
#[must_use]
pub fn approximation(interfaces: &[IfaceInfo], destination: [u8; 4]) -> Option<IpAddr> {
    if destination[0] == 127 {
        return interfaces
            .iter()
            .find(|info| info.flags & IF_LOOPBACK != 0 && usable(info))
            .map(|info| IpAddr::V4(info.addr));
    }
    for info in interfaces {
        if !usable(info) || info.flags & IF_LOOPBACK != 0 {
            continue;
        }
        let mask = crate::iface::prefix_mask(info.prefix);
        if masked(destination, mask) == masked(info.addr, mask) {
            return Some(IpAddr::V4(info.addr));
        }
    }
    interfaces
        .iter()
        .find(|info| usable(info) && info.flags & IF_LINK_UP != 0 && info.flags & IF_LOOPBACK == 0)
        .map(|info| IpAddr::V4(info.addr))
}

fn masked(address: [u8; 4], mask: [u8; 4]) -> [u8; 4] {
    [
        address[0] & mask[0],
        address[1] & mask[1],
        address[2] & mask[2],
        address[3] & mask[3],
    ]
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Net<B, F, E, U> {
    /// The resolver route probe.
    #[must_use]
    pub fn probe(&self) -> NetProbe<'_, B, F, E, U> {
        NetProbe::new(self)
    }
}

#[cfg(test)]
#[path = "probe_ut.rs"]
mod tests;
