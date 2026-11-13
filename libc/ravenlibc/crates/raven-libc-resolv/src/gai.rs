// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getaddrinfo`, the address-family probes and `gai_strerror`.

use alloc::vec::Vec;

use crate::consts::{AI_ADDRCONFIG, AI_MASK, MAXADDRS, MAXSERVS};
use crate::errors::Eai;
use crate::lookup::{HostOrder, lookup_name};
use crate::serv::lookup_serv;
use crate::traits::Env;
use crate::types::{AddrInfoRec, Family, Hints, SockAddr};

/// Convert a raw errno to `EAI_SYSTEM`.
#[must_use]
pub const fn eai_from_io(_errno: crate::Errno) -> Eai {
    Eai::System
}

/// The musl `gai_strerror` table.
const GAI_MESSAGES: [&str; 12] = [
    "Invalid flags",
    "Name does not resolve",
    "Try again",
    "Non-recoverable error",
    "Name has no usable address",
    "Unrecognized address family or invalid length",
    "Unrecognized socket type",
    "Unrecognized service",
    "Unknown error",
    "Out of memory",
    "System error",
    "Overflow",
];

/// The musl error text for `code`; unknown values give `"Unknown error"`.
#[must_use]
pub fn gai_strerror(code: i32) -> &'static str {
    if !(-12..=-1).contains(&code) {
        return "Unknown error";
    }
    let index = usize::try_from(code.saturating_neg().saturating_sub(1)).unwrap_or(0);
    GAI_MESSAGES.get(index).copied().unwrap_or("Unknown error")
}

fn addrconfig(hints: &Hints, env: &Env) -> (i32, bool) {
    let mut family = hints.family;
    let mut no_family = false;
    if hints.flags & AI_ADDRCONFIG != 0 {
        for candidate in [Family::V4, Family::V6] {
            if family == candidate.other().af() {
                continue;
            }
            if env.probe.family_usable(candidate) {
                continue;
            }
            if family == candidate.af() {
                no_family = true;
            }
            family = candidate.other().af();
        }
    }
    (family, no_family)
}

/// `getaddrinfo`: the `serv` x `name` cross product, address-major.
pub fn getaddrinfo(
    host: Option<&str>,
    serv: Option<&str>,
    hints: &Hints,
    env: &Env,
    order: &HostOrder,
) -> Result<Vec<AddrInfoRec>, Eai> {
    if host.is_none() && serv.is_none() {
        return Err(Eai::NoName);
    }
    if hints.flags & !AI_MASK != 0 {
        return Err(Eai::BadFlags);
    }
    if !hints.family_valid() {
        return Err(Eai::Family);
    }
    let (family, no_family) = addrconfig(hints, env);

    let services = lookup_serv(env.files, serv, hints.protocol, hints.socktype, hints.flags)?;
    let name = lookup_name(host, family, hints.flags, env, order)?;
    if no_family {
        return Err(Eai::NoData);
    }
    let mut out: Vec<AddrInfoRec> = Vec::new();
    let canon = if name.canon.is_empty() {
        None
    } else {
        Some(name.canon.clone())
    };
    for address in name.addrs.iter().take(MAXADDRS) {
        for service in services.iter().take(MAXSERVS) {
            if out.len() >= MAXADDRS.saturating_mul(MAXSERVS) {
                return Ok(out);
            }
            let sockaddr = SockAddr::with_scope(address.ip(), service.port, address.scope);
            out.push(AddrInfoRec {
                flags: hints.flags,
                family: address.family.af(),
                socktype: service.socktype,
                protocol: service.proto,
                addr: sockaddr,
                canonname: canon.clone(),
            });
        }
    }
    if out.is_empty() {
        return Err(Eai::NoName);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "gai_ut.rs"]
mod tests;
