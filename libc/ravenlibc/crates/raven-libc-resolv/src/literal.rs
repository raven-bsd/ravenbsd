// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The numeric literal backend: IPv4 first, then IPv6 with a `%scope` suffix.

use alloc::string::String;

use crate::consts::{AF_INET, AF_INET6, AF_UNSPEC, DNS_LABEL_MAX};
use crate::errors::Eai;
use crate::inet::{inet_aton, inet_pton};
use crate::traits::IfNames;
use crate::types::{Family, IpAddr};

/// One parsed literal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Literal {
    /// The address family.
    pub family: Family,
    /// The address bytes (4 or 16 meaningful bytes in a 16-byte buffer).
    pub addr: [u8; 16],
    /// The IPv6 scope id.
    pub scope: u32,
}

impl Literal {
    /// The literal as an [`IpAddr`].
    #[must_use]
    pub fn ip(&self) -> IpAddr {
        match self.family {
            Family::V4 => {
                let mut bytes = [0u8; 4];
                if let Some(src) = self.addr.get(12..) {
                    bytes.copy_from_slice(src);
                }
                IpAddr::V4(bytes)
            }
            Family::V6 => IpAddr::V6(self.addr),
        }
    }
}

/// Parse a `%`-suffixed scope: all digits, or an interface name for link-local addresses.
fn parse_scope(text: &[u8], addr: &[u8; 16], ifnames: &dyn IfNames) -> Result<u32, Eai> {
    let first = text.first().copied().unwrap_or(0);
    if first.is_ascii_digit() {
        let mut value: u64 = 0;
        for byte in text.iter().copied() {
            if !byte.is_ascii_digit() {
                return Err(Eai::NoName);
            }
            value = value
                .saturating_mul(10)
                .saturating_add(u64::from(byte.saturating_sub(b'0')));
        }
        if value > u64::from(u32::MAX) {
            return Err(Eai::NoName);
        }
        return u32::try_from(value).map_err(|_| Eai::NoName);
    }
    let link = IpAddr::V6(*addr);
    if !link.is_link_local() && !link.is_mc_link_local() {
        return Err(Eai::NoName);
    }
    let Ok(name) = core::str::from_utf8(text) else {
        return Err(Eai::NoName);
    };
    ifnames.name_to_index(name).ok_or(Eai::NoName)
}

/// `__lookup_ipliteral`: one literal or `Ok(None)` when the text is not numeric.
///
/// A literal of the wrong family is `EAI_NODATA`; a bad scope is `EAI_NONAME`.
pub fn lookup_ipliteral(
    name: &[u8],
    family: i32,
    ifnames: &dyn IfNames,
) -> Result<Option<Literal>, Eai> {
    if let Some(v4) = inet_aton(name) {
        if family == AF_INET6 {
            return Err(Eai::NoData);
        }
        let mut addr = [0u8; 16];
        if let Some(slot) = addr.get_mut(12..) {
            slot.copy_from_slice(&v4);
        }
        return Ok(Some(Literal {
            family: Family::V4,
            addr,
            scope: 0,
        }));
    }

    let (text, scope_text) = match name.iter().position(|byte| *byte == b'%') {
        Some(index) if index < 64 => {
            let text = name.get(..index).unwrap_or(&[]);
            let scope = name.get(index.saturating_add(1)..).unwrap_or(&[]);
            (text, Some(scope))
        }
        _ => (name, None),
    };
    if text.len() > DNS_LABEL_MAX.saturating_mul(4) {
        return Ok(None);
    }
    let mut addr = [0u8; 16];
    if inet_pton(AF_INET6, text, &mut addr) != 1 {
        return Ok(None);
    }
    if family == AF_INET {
        return Err(Eai::NoData);
    }
    let scope = match scope_text {
        Some(value) => parse_scope(value, &addr, ifnames)?,
        None => 0,
    };
    if family != AF_INET6 && family != AF_UNSPEC {
        return Err(Eai::Family);
    }
    Ok(Some(Literal {
        family: Family::V6,
        addr,
        scope,
    }))
}

/// Whether `name` is a numeric literal of either family.
#[must_use]
pub fn is_numeric(name: &[u8]) -> bool {
    inet_aton(name).is_some() || {
        let mut addr = [0u8; 16];
        inet_pton(AF_INET6, name, &mut addr) == 1
    }
}

/// A scope name or number as text, for the `%scope` fallback in `getnameinfo`.
#[must_use]
pub fn scope_text(idx: u32, ifnames: &dyn IfNames, numeric: bool) -> String {
    if !numeric && let Some(name) = ifnames.index_to_name(idx) {
        return name;
    }
    alloc::format!("{idx}")
}

#[cfg(test)]
#[path = "literal_ut.rs"]
mod tests;
