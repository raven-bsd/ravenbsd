// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `__lookup_name`: the hosts/DNS backends, the search list and the RFC 6724 sort.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_libc_abi::errno::{EACCES, ENOENT, ENOTDIR};
use raven_libc_nss::{Database, Nsswitch, Source, SourceEntry};

use crate::conf::{ResolvConf, parse_resolv_conf};
use crate::consts::{
    AI_ALL, AI_NUMERICHOST, AI_V4MAPPED, MAX_ORDER, MAXADDRS, RR_A, RR_AAAA, RR_CNAME,
};
use crate::engine::msend;
use crate::errors::Eai;
use crate::hosts;
use crate::literal::lookup_ipliteral;
use crate::msg::{append_edns0, res_mkquery};
use crate::name::{dn_expand, is_valid_hostname};
use crate::traits::{Env, Probe};
use crate::types::{Family, IpAddr, SockAddr};

/// One resolved address with the donor's sort key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Address {
    /// The family.
    pub family: Family,
    /// The IPv6 scope id.
    pub scope: u32,
    /// The address in a 16-byte buffer.
    pub addr: [u8; 16],
    /// The RFC 6724 sort key (0 before sorting).
    pub sortkey: i32,
}

impl Address {
    /// A zeroed address of the given family.
    #[must_use]
    pub const fn new(family: Family) -> Self {
        Self {
            family,
            scope: 0,
            addr: [0u8; 16],
            sortkey: 0,
        }
    }

    /// A copy with a concrete address.
    #[must_use]
    pub const fn with_addr(family: Family, addr: [u8; 16], scope: u32) -> Self {
        Self {
            family,
            scope,
            addr,
            sortkey: 0,
        }
    }

    /// The address as an [`IpAddr`].
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

    /// The 16-byte form; IPv4 is v4-mapped.
    #[must_use]
    pub fn ip16(&self) -> [u8; 16] {
        match self.family {
            Family::V4 => self.ip().to_v4_mapped().octets16(),
            Family::V6 => self.addr,
        }
    }

    /// A socket address with `port`.
    #[must_use]
    pub fn sockaddr(&self, port: u16) -> SockAddr {
        SockAddr::with_scope(self.ip(), port, self.scope)
    }
}

/// One source and its `[NOTFOUND=return]` flag; a copy of the nss entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderEntry {
    /// The source.
    pub source: Source,
    /// Stop the search when this source reports not found.
    pub on_notfound_return: bool,
}

/// The hosts database order built from `nsswitch.conf`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostOrder {
    entries: [Option<OrderEntry>; MAX_ORDER],
    len: usize,
}

impl HostOrder {
    /// The musl default: `files dns`.
    #[must_use]
    pub fn defaults() -> Self {
        Self::from_sources(&[
            OrderEntry {
                source: Source::Files,
                on_notfound_return: false,
            },
            OrderEntry {
                source: Source::Dns,
                on_notfound_return: false,
            },
        ])
    }

    /// Build from explicit entries, keeping at most [`MAX_ORDER`].
    #[must_use]
    pub fn from_sources(entries: &[OrderEntry]) -> Self {
        let mut value = Self {
            entries: [None; MAX_ORDER],
            len: 0,
        };
        for entry in entries.iter().copied() {
            if value.len >= MAX_ORDER {
                break;
            }
            if let Some(slot) = value.entries.get_mut(value.len) {
                *slot = Some(entry);
                value.len = value.len.saturating_add(1);
            }
        }
        value
    }

    /// Build from the hosts list of a parsed `nsswitch.conf`.
    #[must_use]
    pub fn from_nss(nss: &Nsswitch) -> Self {
        let list = nss.sources(Database::Hosts);
        let mut entries: Vec<OrderEntry> = Vec::new();
        for entry in list.iter() {
            entries.push(OrderEntry {
                source: entry.source,
                on_notfound_return: entry.on_notfound_return,
            });
        }
        if entries.is_empty() {
            return Self::defaults();
        }
        Self::from_sources(&entries)
    }

    /// Build from one [`SourceEntry`] slice.
    #[must_use]
    pub fn from_entries(entries: &[SourceEntry]) -> Self {
        let mut order: Vec<OrderEntry> = Vec::new();
        for entry in entries.iter().copied() {
            order.push(OrderEntry {
                source: entry.source,
                on_notfound_return: entry.on_notfound_return,
            });
        }
        if order.is_empty() {
            return Self::defaults();
        }
        Self::from_sources(&order)
    }

    /// The entries in order.
    pub fn iter(&self) -> impl Iterator<Item = OrderEntry> + '_ {
        self.entries.iter().take(self.len).flatten().copied()
    }

    /// Number of entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether there are no entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for HostOrder {
    fn default() -> Self {
        Self::defaults()
    }
}

/// The result of [`lookup_name`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NameResult {
    /// The addresses in their final order.
    pub addrs: Vec<Address>,
    /// The canonical name, empty for a numeric lookup.
    pub canon: String,
}

/// One RFC 6724 policy row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// The prefix bytes.
    pub addr: [u8; 16],
    /// Number of whole bytes compared before the masked byte.
    pub len: usize,
    /// The mask applied to byte `len`.
    pub mask: u8,
    /// The precedence.
    pub prec: u8,
    /// The label.
    pub label: u8,
}

/// The donor's `defpolicy` table (rules 3, 4 and 7 omitted, as in the donor).
pub static RFC6724_POLICY: [Policy; 6] = [
    Policy {
        addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
        len: 15,
        mask: 0xff,
        prec: 50,
        label: 0,
    },
    Policy {
        addr: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 0, 0, 0, 0],
        len: 11,
        mask: 0xff,
        prec: 35,
        label: 4,
    },
    Policy {
        addr: [0x20, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        len: 1,
        mask: 0xff,
        prec: 30,
        label: 2,
    },
    Policy {
        addr: [0x20, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        len: 3,
        mask: 0xff,
        prec: 5,
        label: 5,
    },
    Policy {
        addr: [0xfc, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        len: 0,
        mask: 0xfe,
        prec: 3,
        label: 13,
    },
    Policy {
        addr: [0u8; 16],
        len: 0,
        mask: 0,
        prec: 40,
        label: 1,
    },
];

fn byte(addr: &[u8; 16], index: usize) -> u8 {
    addr.get(index).copied().unwrap_or(0)
}

fn policy_of(addr: &[u8; 16]) -> &'static Policy {
    for policy in RFC6724_POLICY.iter() {
        let whole = policy
            .addr
            .get(..policy.len)
            .is_some_and(|head| addr.get(..policy.len) == Some(head));
        if !whole {
            continue;
        }
        if (byte(addr, policy.len) & policy.mask) != byte(&policy.addr, policy.len) {
            continue;
        }
        return policy;
    }
    match RFC6724_POLICY.last() {
        Some(policy) => policy,
        None => &RFC6724_POLICY[0],
    }
}

fn label_of(addr: &[u8; 16]) -> u8 {
    policy_of(addr).label
}

fn scope_of(addr: &[u8; 16]) -> u8 {
    let ip = IpAddr::V6(*addr);
    if byte(addr, 0) == 0xff {
        return byte(addr, 1) & 15;
    }
    if ip.is_link_local() || ip.is_loopback() {
        return 2;
    }
    if byte(addr, 0) == 0xfe && (byte(addr, 1) & 0xc0) == 0xc0 {
        return 5;
    }
    14
}

fn prefix_match(source: &[u8; 16], dest: &[u8; 16]) -> u16 {
    let mut bits = 0u16;
    for index in 0..16usize {
        let xor = byte(source, index) ^ byte(dest, index);
        if xor == 0 {
            bits = bits.saturating_add(8);
            continue;
        }
        let mut bit = 0u8;
        while bit < 8 {
            let mask = 0x80u8.wrapping_shr(u32::from(bit));
            if xor & mask != 0 {
                return bits.saturating_add(u16::from(bit));
            }
            bit = bit.saturating_add(1);
        }
        return bits.saturating_add(8);
    }
    bits
}

const DAS_USABLE: i32 = 0x4000_0000;
const DAS_MATCHINGSCOPE: i32 = 0x2000_0000;
const DAS_MATCHINGLABEL: i32 = 0x1000_0000;
const DAS_PREC_SHIFT: u32 = 20;
const DAS_SCOPE_SHIFT: u32 = 16;
const DAS_PREFIX_SHIFT: u32 = 8;

fn sort_key(probe: &dyn Probe, address: &Address, order: usize) -> i32 {
    let dest16 = address.ip16();
    let dst = SockAddr::with_scope(address.ip(), 65535, address.scope);
    let route = probe.udp_route(&dst);
    let dpolicy = policy_of(&dest16);
    let dscope = scope_of(&dest16);
    let mut key: i32 = 0;
    if let Some(source) = route {
        let source16 = match address.family {
            Family::V4 => source.to_v4_mapped().octets16(),
            Family::V6 => source.octets16(),
        };
        key |= DAS_USABLE;
        if dscope == scope_of(&source16) {
            key |= DAS_MATCHINGSCOPE;
        }
        if dpolicy.label == label_of(&source16) {
            key |= DAS_MATCHINGLABEL;
        }
        let prefix = prefix_match(&source16, &dest16);
        key |= i32::from(prefix).wrapping_shl(DAS_PREFIX_SHIFT);
    }
    key |= i32::from(dpolicy.prec).wrapping_shl(DAS_PREC_SHIFT);
    key |= i32::from(15u8.saturating_sub(dscope)).wrapping_shl(DAS_SCOPE_SHIFT);
    let order_key = MAXADDRS.saturating_sub(order);
    key |= i32::try_from(order_key).unwrap_or(0);
    key
}

/// Apply the RFC 6724 destination sort when at least two addresses mix families.
pub fn sort_addresses(addrs: &mut [Address], probe: &dyn Probe) {
    if addrs.len() < 2 {
        return;
    }
    if addrs.iter().all(|address| address.family == Family::V4) {
        return;
    }
    for (index, address) in addrs.iter_mut().enumerate() {
        address.sortkey = sort_key(probe, address, index);
    }
    addrs.sort_by_key(|address| core::cmp::Reverse(address.sortkey));
}

fn name_from_null(family: i32, flags: i32) -> Vec<Address> {
    let mut out = Vec::new();
    let passive = flags & crate::consts::AI_PASSIVE != 0;
    if family != crate::consts::AF_INET6 {
        let mut address = Address::new(Family::V4);
        if !passive {
            address.addr = IpAddr::V4([127, 0, 0, 1]).octets16();
        }
        out.push(address);
    }
    if family != crate::consts::AF_INET {
        let mut address = Address::new(Family::V6);
        if !passive {
            address.addr = IpAddr::V6(crate::inet::IN6ADDR_LOOPBACK).octets16();
        }
        out.push(address);
    }
    out
}

fn name_from_hosts(
    name: &str,
    family: i32,
    env: &Env,
    canon: &mut String,
) -> Result<Vec<Address>, Eai> {
    let file = match env.read("/etc/hosts") {
        Ok(bytes) => bytes,
        Err(errno) if errno == ENOENT || errno == ENOTDIR || errno == EACCES => {
            return Ok(Vec::new());
        }
        Err(_) => return Err(Eai::System),
    };
    match hosts::lookup(&file, name.as_bytes(), family, env.ifnames, canon) {
        Ok((addrs, badfam)) => {
            if addrs.is_empty() && badfam {
                Err(Eai::NoData)
            } else {
                Ok(addrs)
            }
        }
        Err(errno) => Err(crate::gai::eai_from_io(errno)),
    }
}

fn is_localhost(name: &str) -> bool {
    let trimmed = name.strip_suffix('.').unwrap_or(name);
    trimmed.eq_ignore_ascii_case("localhost")
        || trimmed
            .len()
            .checked_sub(".localhost".len())
            .and_then(|index| trimmed.get(index..))
            .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".localhost"))
}

fn name_from_localhost(family: i32) -> Vec<Address> {
    let mut out = Vec::new();
    if family != crate::consts::AF_INET6 {
        out.push(Address::with_addr(
            Family::V4,
            IpAddr::V4([127, 0, 0, 1]).octets16(),
            0,
        ));
    }
    if family != crate::consts::AF_INET {
        out.push(Address::with_addr(
            Family::V6,
            crate::inet::IN6ADDR_LOOPBACK,
            0,
        ));
    }
    out
}

struct DnsContext<'a> {
    addrs: &'a mut Vec<Address>,
    canon: &'a mut String,
    rrtype: u8,
}

fn dns_callback(
    ctx: &mut DnsContext<'_>,
    rr: u8,
    rdata: &[u8],
    packet: &[u8],
    offset: usize,
) -> i32 {
    if rr == RR_CNAME {
        let mut name = [0u8; 256];
        if let Some((_, len)) = dn_expand(packet, offset, &mut name)
            && let Ok(text) = core::str::from_utf8(name.get(..len).unwrap_or(&[]))
            && is_valid_hostname(text)
        {
            *ctx.canon = String::from(text);
        }
        return 0;
    }
    if ctx.addrs.len() >= MAXADDRS {
        return 0;
    }
    if rr != ctx.rrtype {
        return 0;
    }
    match rr {
        RR_A => {
            if rdata.len() != 4 {
                return -1;
            }
            let mut addr = [0u8; 16];
            if let Some(slot) = addr.get_mut(12..) {
                slot.copy_from_slice(rdata);
            }
            ctx.addrs.push(Address::with_addr(Family::V4, addr, 0));
        }
        RR_AAAA => {
            if rdata.len() != 16 {
                return -1;
            }
            let mut addr = [0u8; 16];
            addr.copy_from_slice(rdata);
            ctx.addrs.push(Address::with_addr(Family::V6, addr, 0));
        }
        _ => {}
    }
    0
}

fn entropy_id(env: &Env) -> Result<u16, Eai> {
    let mut bytes = [0u8; 2];
    if !env.random(&mut bytes) {
        return Err(Eai::Fail);
    }
    Ok(u16::from_be_bytes(bytes))
}

fn build_query(
    name: &[u8],
    rtype: u8,
    id: u16,
    edns0: bool,
    trust_ad: bool,
) -> Option<([u8; crate::consts::UDP_QUERY_MAX], usize)> {
    let mut buf = [0u8; crate::consts::UDP_QUERY_MAX];
    let len = res_mkquery(0, name, 1, u16::from(rtype), id, &mut buf)?;
    // lookup_name.c clears the AD bit; the ring-2 `trust-ad` option requests it.
    if let Some(slot) = buf.get_mut(3) {
        *slot = if trust_ad { 32 } else { 0 };
    }
    let len = if edns0 {
        append_edns0(&mut buf, len, edns0)?
    } else {
        len
    };
    Some((buf, len))
}

fn name_from_dns(
    name: &str,
    family: i32,
    canon: &mut String,
    conf: &ResolvConf,
    ext: &crate::conf::ConfExt,
    env: &Env,
) -> Result<Vec<Address>, Eai> {
    let mut qtypes: Vec<u8> = Vec::new();
    if family != crate::consts::AF_INET6 {
        qtypes.push(RR_A);
    }
    if family != crate::consts::AF_INET && !ext.no_aaaa {
        qtypes.push(RR_AAAA);
    }
    if qtypes.is_empty() {
        return Ok(Vec::new());
    }
    let mut first_id: Option<u16> = None;
    let mut storage: Vec<([u8; crate::consts::UDP_QUERY_MAX], usize)> = Vec::new();
    for rrtype in qtypes.iter().copied() {
        let mut id = entropy_id(env)?;
        if let Some(previous) = first_id
            && id == previous
        {
            id = id.wrapping_add(1);
        }
        if first_id.is_none() {
            first_id = Some(id);
        }
        let Some(built) = build_query(name.as_bytes(), rrtype, id, ext.edns0, ext.trust_ad) else {
            return Ok(Vec::new());
        };
        storage.push(built);
    }
    let mut queries: Vec<&[u8]> = Vec::new();
    for (buf, len) in storage.iter() {
        match buf.get(..*len) {
            Some(slice) => queries.push(slice),
            None => return Ok(Vec::new()),
        }
    }
    let mut answer_store: Vec<Vec<u8>> = Vec::new();
    for _ in queries.iter() {
        answer_store.push(alloc::vec![0u8; crate::consts::ABUF_SIZE]);
    }
    let mut answers: Vec<&mut [u8]> = answer_store.iter_mut().map(Vec::as_mut_slice).collect();
    let mut alens: Vec<usize> = alloc::vec![0usize; queries.len()];
    msend(env, conf, ext, &queries, &mut answers, &mut alens).map_err(|_| Eai::System)?;

    for (index, len) in alens.iter().copied().enumerate() {
        if len < 4 {
            return Err(Eai::Again);
        }
        let Some(answer) = answers.get(index).map(|slot| &**slot) else {
            return Err(Eai::System);
        };
        let rcode = crate::u8_at(answer, 3) & 15;
        if rcode == 2 {
            return Err(Eai::Again);
        }
        if rcode == 3 {
            return Ok(Vec::new());
        }
        if rcode != 0 {
            return Err(Eai::Fail);
        }
    }

    let mut addrs: Vec<Address> = Vec::new();
    for (index, rrtype) in qtypes.iter().copied().enumerate().rev() {
        let Some(answer) = answers.get(index).map(|slot| &**slot) else {
            continue;
        };
        let Some(len) = alens.get(index).copied() else {
            continue;
        };
        let cap = len.min(answer.len());
        let Some(packet) = answer.get(..cap) else {
            continue;
        };
        let mut ctx = DnsContext {
            addrs: &mut addrs,
            canon,
            rrtype,
        };
        let _ = crate::msg::dns_parse_offset(packet, |rr, rdata, full, offset| {
            dns_callback(&mut ctx, rr, rdata, full, offset)
        });
    }
    if addrs.is_empty() {
        return Err(Eai::NoData);
    }
    Ok(addrs)
}

fn name_from_dns_search(
    name: &str,
    family: i32,
    canon: &mut String,
    env: &Env,
) -> Result<Vec<Address>, Eai> {
    let raw = env.read("/etc/resolv.conf").ok();
    let conf = parse_resolv_conf(raw.as_deref());
    let ext = crate::conf::parse_options_ext(raw.as_deref().unwrap_or(&[]));

    let bytes = name.as_bytes();
    let mut dots = 0usize;
    for byte in bytes.iter().copied() {
        if byte == b'.' {
            dots = dots.saturating_add(1);
        }
    }
    let ends_dot = bytes.last().copied() == Some(b'.');
    let search_enabled = dots < usize::try_from(conf.ndots).unwrap_or(usize::MAX) && !ends_dot;

    let mut length = bytes.len();
    if ends_dot {
        length = length.saturating_sub(1);
    }
    let Some(trimmed) = bytes.get(..length) else {
        return Err(Eai::NoName);
    };
    if trimmed.is_empty() || trimmed.last().copied() == Some(b'.') {
        return Err(Eai::NoName);
    }
    let Some(base) = core::str::from_utf8(trimmed).ok().map(String::from) else {
        return Err(Eai::NoName);
    };

    if search_enabled {
        for domain in conf.search.split_ascii_whitespace() {
            if domain.is_empty() {
                continue;
            }
            let needed = base.len().saturating_add(1).saturating_add(domain.len());
            if needed >= 256 {
                continue;
            }
            let full = format!("{base}.{domain}");
            *canon = full.clone();
            match name_from_dns(&full, family, canon, &conf, &ext, env) {
                Ok(addrs) if !addrs.is_empty() => return Ok(addrs),
                Ok(_) => {}
                Err(Eai::NoName) => {}
                Err(other) => return Err(other),
            }
        }
    }
    *canon = base.clone();
    name_from_dns(&base, family, canon, &conf, &ext, env)
}

fn finish(
    mut addrs: Vec<Address>,
    canon: String,
    family: i32,
    flags: i32,
    env: &Env,
) -> NameResult {
    if flags & AI_V4MAPPED != 0 {
        if flags & AI_ALL == 0 && addrs.iter().any(|address| address.family == Family::V6) {
            addrs.retain(|address| address.family == Family::V6);
        }
        for address in addrs.iter_mut() {
            if address.family == Family::V4 {
                let mapped = address.ip().to_v4_mapped();
                if let IpAddr::V6(bytes) = mapped {
                    address.addr = bytes;
                    address.family = Family::V6;
                }
            }
        }
    }
    if family != crate::consts::AF_INET {
        sort_addresses(&mut addrs, env.probe);
    }
    NameResult { addrs, canon }
}

/// `__lookup_name`: the hosts and DNS backends with the search list.
pub fn lookup_name(
    name: Option<&str>,
    family: i32,
    flags: i32,
    env: &Env,
    order: &HostOrder,
) -> Result<NameResult, Eai> {
    let mut canon = String::new();
    if let Some(value) = name {
        let length = value.len().min(255);
        if length == 0 || length >= 255 {
            return Err(Eai::NoName);
        }
        canon.push_str(value);
    }
    let mut family = family;
    let mut flags = flags;
    if flags & AI_V4MAPPED != 0 {
        if family == crate::consts::AF_INET6 {
            family = crate::consts::AF_UNSPEC;
        } else {
            flags &= !AI_V4MAPPED;
        }
    }

    if name.is_none() {
        let addrs = name_from_null(family, flags);
        return Ok(finish(addrs, canon, family, flags, env));
    }
    if let Some(value) = name {
        match lookup_ipliteral(value.as_bytes(), family, env.ifnames) {
            Ok(Some(literal)) => {
                let addr = literal.ip();
                let octets = match addr {
                    IpAddr::V4(bytes) => IpAddr::V4(bytes).to_v4_mapped().octets16(),
                    IpAddr::V6(bytes) => bytes,
                };
                let address = Address::with_addr(literal.family, octets, literal.scope);
                return Ok(finish(alloc::vec![address], canon, family, flags, env));
            }
            Ok(None) => {}
            Err(error) => return Err(error),
        }
        if flags & AI_NUMERICHOST != 0 {
            return Err(Eai::NoName);
        }
        let mut temporary_canon = String::new();
        let result = dispatch_sources(value, family, env, order, &mut temporary_canon)?;
        if !result.addrs.is_empty() {
            if !temporary_canon.is_empty() {
                canon = temporary_canon;
            }
            return Ok(finish(result.addrs, canon, family, flags, env));
        }
        return Err(Eai::NoName);
    }
    Err(Eai::NoName)
}

fn dispatch_sources(
    name: &str,
    family: i32,
    env: &Env,
    order: &HostOrder,
    canon: &mut String,
) -> Result<NameResult, Eai> {
    let mut addresses: Vec<Address> = Vec::new();
    let mut again = false;
    let mut failed = false;
    let mut system = false;
    let mut nodata = false;
    for entry in order.iter() {
        match entry.source {
            Source::Skipped => {}
            Source::Files => {
                let mut hosts_canon = String::new();
                match name_from_hosts(name, family, env, &mut hosts_canon) {
                    Ok(found) if !found.is_empty() => {
                        *canon = hosts_canon;
                        addresses = found;
                        break;
                    }
                    Ok(_) => {
                        if entry.on_notfound_return {
                            return Err(Eai::NoName);
                        }
                    }
                    Err(Eai::NoName) => {
                        if entry.on_notfound_return {
                            return Err(Eai::NoName);
                        }
                    }
                    Err(Eai::NoData) => {
                        nodata = true;
                    }
                    Err(Eai::System) => {
                        system = true;
                    }
                    Err(other) => {
                        failed = true;
                        if other == Eai::Again {
                            again = true;
                        }
                    }
                }
            }
            Source::Dns => {
                if is_localhost(name) {
                    let found = name_from_localhost(family);
                    if !found.is_empty() {
                        *canon = name.strip_suffix('.').unwrap_or(name).into();
                        addresses = found;
                        break;
                    }
                }
                match name_from_dns_search(name, family, canon, env) {
                    Ok(found) if !found.is_empty() => {
                        addresses = found;
                        break;
                    }
                    Ok(_) => {
                        if entry.on_notfound_return {
                            return Err(Eai::NoName);
                        }
                    }
                    Err(Eai::NoName) => {
                        if entry.on_notfound_return {
                            return Err(Eai::NoName);
                        }
                    }
                    Err(Eai::NoData) => {
                        nodata = true;
                    }
                    Err(Eai::Again) => {
                        again = true;
                    }
                    Err(Eai::System) => {
                        system = true;
                    }
                    Err(other) => {
                        failed = true;
                        if other == Eai::Again {
                            again = true;
                        }
                    }
                }
            }
        }
    }
    if !addresses.is_empty() {
        return Ok(NameResult {
            addrs: addresses,
            canon: canon.clone(),
        });
    }
    if system {
        Err(Eai::System)
    } else if again {
        Err(Eai::Again)
    } else if failed {
        Err(Eai::Fail)
    } else if nodata {
        Err(Eai::NoData)
    } else {
        Err(Eai::NoName)
    }
}

#[cfg(test)]
#[path = "lookup_ut.rs"]
mod tests;
