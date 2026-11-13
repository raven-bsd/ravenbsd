// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `SIOC*` ioctls belong to the network area (C63); this module only classifies them so the
//! shell routes them without duplicating the numbers.

use raven_abi::errno::Errno;

/// A network ioctl request the network area owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetIoctl {
    /// `SIOCGIFCONF`.
    GetIfConf,
    /// `SIOCGIFFLAGS`.
    GetIfFlags,
    /// `SIOCSIFFLAGS`.
    SetIfFlags,
    /// `SIOCGIFADDR`.
    GetIfAddr,
    /// `SIOCSIFADDR`.
    SetIfAddr,
    /// `SIOCGIFNETMASK`.
    GetIfNetmask,
    /// `SIOCGIFMTU`.
    GetIfMtu,
    /// Any other `SIOC*` request.
    Other,
}

/// Classify a request; `None` is not a network ioctl.
pub const fn classify(request: u32) -> Option<NetIoctl> {
    let action = match request {
        0x8912 => NetIoctl::GetIfConf,
        0x8913 => NetIoctl::GetIfFlags,
        0x8914 => NetIoctl::SetIfFlags,
        0x8915 => NetIoctl::GetIfAddr,
        0x8916 => NetIoctl::SetIfAddr,
        0x891b => NetIoctl::GetIfNetmask,
        0x8921 => NetIoctl::GetIfMtu,
        value if value & 0xffff_ff00 == 0x8900_0000 => NetIoctl::Other,
        _ => return None,
    };
    Some(action)
}

/// Route one network ioctl: the network area owns the implementation, so this layer only
/// reports that the request is recognised but not implementable here.
pub fn net_ioctl(request: u32) -> Errno {
    match classify(request) {
        Some(_) => Errno::ENOSYS,
        None => Errno::ENOTTY,
    }
}
