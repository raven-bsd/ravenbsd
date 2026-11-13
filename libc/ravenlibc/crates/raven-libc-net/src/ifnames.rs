// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The resolver's `IfNames` implementation.

use alloc::string::String;

use raven_libc_abi::errno::Errno;
use raven_libc_resolv::IfNames;
use raven_proto_net::IfaceResponse;

use crate::iface::list;
use crate::{Net, NetBackend, NetEnv, SocketFds, UnixBackend};

/// The resolver interface-name implementation; borrows one [`Net`].
pub struct NetIfNames<'a, B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend = crate::NoUnix> {
    net: &'a Net<B, F, E, U>,
}

impl<'a, B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> NetIfNames<'a, B, F, E, U> {
    pub(crate) fn new(net: &'a Net<B, F, E, U>) -> Self {
        Self { net }
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> IfNames
    for NetIfNames<'_, B, F, E, U>
{
    fn name_to_index(&self, name: &str) -> Option<u32> {
        crate::iface::nametoindex(self.net.backend(), name.as_bytes()).ok()
    }

    fn index_to_name(&self, idx: u32) -> Option<String> {
        let list = list(self.net.backend()).ok()?;
        for (position, info) in list.iter().enumerate() {
            let candidate = u32::try_from(position.saturating_add(1)).ok()?;
            if candidate == idx {
                return Some(String::from(info.name.as_str()));
            }
        }
        None
    }
}

impl<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend> Net<B, F, E, U> {
    /// The resolver interface-name seam.
    #[must_use]
    pub fn ifnames(&self) -> NetIfNames<'_, B, F, E, U> {
        NetIfNames::new(self)
    }

    /// The control-channel `LIST` stream as `(index, name)` pairs.
    pub fn iface_entries(&self) -> Result<alloc::vec::Vec<(u32, alloc::vec::Vec<u8>)>, Errno> {
        let entries = crate::iface::nameindex(self.backend())?;
        let _ = IfaceResponse::Ok;
        Ok(entries)
    }
}

#[cfg(test)]
#[path = "ifnames_ut.rs"]
mod tests;
