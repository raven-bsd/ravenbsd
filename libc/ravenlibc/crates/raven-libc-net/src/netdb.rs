// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! netdb glue over C51's resolver.

use alloc::vec::Vec;

use raven_libc_nss::FileSource;
use raven_libc_resolv::{
    AddrInfoRec, Clock, DnsIo, Eai, Entropy, Env, Hints, HostOrder, IfNames, Probe, SockAddr,
};

use crate::{Net, NetBackend, NetEnv, SocketFds, UnixBackend};

/// Bundle the six resolver seams from one `Net`.
#[must_use]
pub fn make_env<'a>(
    files: &'a dyn FileSource,
    io: &'a dyn DnsIo,
    clock: &'a dyn Clock,
    entropy: &'a dyn Entropy,
    probe: &'a dyn Probe,
    ifnames: &'a dyn IfNames,
) -> Env<'a> {
    Env {
        files,
        io,
        clock,
        entropy,
        probe,
        ifnames,
    }
}

/// `getaddrinfo` over the real net seams.
pub fn getaddrinfo<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend>(
    net: &Net<B, F, E, U>,
    files: &dyn FileSource,
    host: Option<&str>,
    serv: Option<&str>,
    hints: &Hints,
) -> Result<Vec<AddrInfoRec>, Eai> {
    let dns = net.dns_io();
    let probe = net.probe();
    let ifnames = net.ifnames();
    let environment = make_env(files, &dns, &dns, &dns, &probe, &ifnames);
    let nss = raven_libc_nss::load(files);
    let order = HostOrder::from_nss(&nss);
    raven_libc_resolv::getaddrinfo(host, serv, hints, &environment, &order)
}

/// `getnameinfo` over the real net seams.
pub fn getnameinfo<B: NetBackend, F: SocketFds, E: NetEnv, U: UnixBackend>(
    net: &Net<B, F, E, U>,
    files: &dyn FileSource,
    addr: &SockAddr,
    hostlen: usize,
    servlen: usize,
    flags: i32,
) -> Result<(Option<alloc::string::String>, Option<alloc::string::String>), Eai> {
    let dns = net.dns_io();
    let probe = net.probe();
    let ifnames = net.ifnames();
    let environment = make_env(files, &dns, &dns, &dns, &probe, &ifnames);
    let nss = raven_libc_nss::load(files);
    let order = HostOrder::from_nss(&nss);
    raven_libc_resolv::getnameinfo(addr, hostlen, servlen, flags, &environment, &order)
}

#[cfg(test)]
#[path = "netdb_ut.rs"]
mod tests;
