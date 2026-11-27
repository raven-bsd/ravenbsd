// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `SIOC*` ioctls belong to the network area (C63); this module only classifies them so the
//! shell routes them without duplicating the numbers.
//!
//! The request values are the pinned FreeBSD `sys/sys/sockio.h` encodings:
//! `_IOW`/`_IOWR('i', number, struct ifreq)` for a 32-byte `struct ifreq` and
//! `_IOWR('i', 36, struct ifconf)` for the 16-byte `struct ifconf`. The 32-bit request word
//! carries the 13-bit parameter length and the `i` command group exactly as `sys/sys/ioccom.h`
//! defines them.

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

/// `sizeof(struct ifreq)`, the FreeBSD public record (`net/if.h`).
const IFREQ_LEN: u32 = 32;
/// `sizeof(struct ifconf)`.
const IFCONF_LEN: u32 = 16;
/// `IOC_INOUT`: copy parameters in and out.
const IOC_INOUT: u32 = 0xc000_0000;
/// `IOC_IN`: copy parameters in.
const IOC_IN: u32 = 0x8000_0000;
/// Build a FreeBSD ioctl request (`sys/sys/ioccom.h` `_IOC`).
const fn ioc(direction: u32, number: u32, len: u32) -> u32 {
    direction | ((len & 0x1fff) << 16) | ((b'i' as u32) << 8) | number
}

const SIOCGIFCONF: u32 = ioc(IOC_INOUT, 36, IFCONF_LEN);
const SIOCGIFFLAGS: u32 = ioc(IOC_INOUT, 17, IFREQ_LEN);
const SIOCSIFFLAGS: u32 = ioc(IOC_IN, 16, IFREQ_LEN);
const SIOCGIFADDR: u32 = ioc(IOC_INOUT, 33, IFREQ_LEN);
const SIOCSIFADDR: u32 = ioc(IOC_IN, 12, IFREQ_LEN);
const SIOCGIFNETMASK: u32 = ioc(IOC_INOUT, 37, IFREQ_LEN);
const SIOCGIFMTU: u32 = ioc(IOC_INOUT, 51, IFREQ_LEN);
/// The `i` command group assigned to the interface ioctls.
const IFACE_GROUP: u32 = b'i' as u32;

/// Classify a request; `None` is not a network ioctl.
pub const fn classify(request: u32) -> Option<NetIoctl> {
    let action = match request {
        SIOCGIFCONF => NetIoctl::GetIfConf,
        SIOCGIFFLAGS => NetIoctl::GetIfFlags,
        SIOCSIFFLAGS => NetIoctl::SetIfFlags,
        SIOCGIFADDR => NetIoctl::GetIfAddr,
        SIOCSIFADDR => NetIoctl::SetIfAddr,
        SIOCGIFNETMASK => NetIoctl::GetIfNetmask,
        SIOCGIFMTU => NetIoctl::GetIfMtu,
        value if (value >> 8) & 0xff == IFACE_GROUP => NetIoctl::Other,
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
