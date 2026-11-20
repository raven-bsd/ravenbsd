// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Static C records for the legacy `netdb` calls. The lookup logic is C51's; this module owns
//! the process scratch record and the `/etc/hosts` iteration cursor.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{ERANGE, Errno};
use raven_libc_nss::FileSource;
use raven_libc_resolv::dbfiles::{NetFile, ProtoFile, ServFile};
use raven_libc_resolv::hosts::{HostLine, parse_hosts};
use raven_libc_resolv::legacy::{self, HostEnt};
use raven_libc_resolv::netdb::{Proto, Protoent as CrateProtoent, Servent as CrateServent};
use raven_libc_resolv::{HostOrder, IpAddr};

use crate::ffi::lock::SpinLock;

static FILES: ShellFiles = ShellFiles;

/// The C `struct hostent`.
#[repr(C)]
pub(crate) struct Hostent {
    pub(crate) h_name: *mut c_char,
    pub(crate) h_aliases: *mut *mut c_char,
    pub(crate) h_addrtype: c_int,
    pub(crate) h_length: c_int,
    pub(crate) h_addr_list: *mut *mut c_char,
}

impl Hostent {
    pub(crate) const ZERO: Self = Self {
        h_name: core::ptr::null_mut(),
        h_aliases: core::ptr::null_mut(),
        h_addrtype: 0,
        h_length: 0,
        h_addr_list: core::ptr::null_mut(),
    };
}

/// The C `struct servent`.
#[repr(C)]
pub(crate) struct Servent {
    pub(crate) s_name: *mut c_char,
    pub(crate) s_aliases: *mut *mut c_char,
    pub(crate) s_port: c_int,
    pub(crate) s_proto: *mut c_char,
}

impl Servent {
    pub(crate) const ZERO: Self = Self {
        s_name: core::ptr::null_mut(),
        s_aliases: core::ptr::null_mut(),
        s_port: 0,
        s_proto: core::ptr::null_mut(),
    };
}

/// The C `struct protoent`.
#[repr(C)]
pub(crate) struct Protoent {
    pub(crate) p_name: *mut c_char,
    pub(crate) p_aliases: *mut *mut c_char,
    pub(crate) p_proto: c_int,
}

impl Protoent {
    pub(crate) const ZERO: Self = Self {
        p_name: core::ptr::null_mut(),
        p_aliases: core::ptr::null_mut(),
        p_proto: 0,
    };
}

/// The C `struct netent`.
#[repr(C)]
pub(crate) struct Netent {
    pub(crate) n_name: *mut c_char,
    pub(crate) n_aliases: *mut *mut c_char,
    pub(crate) n_addrtype: c_int,
    pub(crate) n_net: u32,
}

impl Netent {
    pub(crate) const ZERO: Self = Self {
        n_name: core::ptr::null_mut(),
        n_aliases: core::ptr::null_mut(),
        n_addrtype: 0,
        n_net: 0,
    };
}

struct ShellFiles;

impl FileSource for ShellFiles {
    fn read_file(&self, path: &str, max: usize) -> Result<Vec<u8>, i32> {
        let platform = crate::ffi::platform::global();
        let fd = platform
            .open(path.as_bytes(), raven_libc_abi::fcntl::O_RDONLY as u32, 0)
            .map_err(|error| i32::from(error.0))?;
        let mut out = Vec::new();
        let mut chunk = [0u8; 4096];
        while out.len() < max {
            let want = core::cmp::min(chunk.len(), max.saturating_sub(out.len()));
            let slice = match chunk.get_mut(..want) {
                Some(slice) => slice,
                None => break,
            };
            match platform.read(fd, slice) {
                Ok(0) => break,
                Ok(count) => {
                    if let Some(bytes) = slice.get(..count) {
                        out.extend_from_slice(bytes);
                    }
                }
                Err(error) => {
                    let _ = platform.close(fd);
                    return Err(i32::from(error.0));
                }
            }
        }
        platform.close(fd).map_err(|error| i32::from(error.0))?;
        Ok(out)
    }
}

struct Area {
    host: Hostent,
    host_buf: [u8; 2048],
    serv: Servent,
    serv_buf: [u8; 2048],
    proto: Protoent,
    proto_buf: [u8; 2048],
    net: Netent,
    net_buf: [u8; 2048],
    hosts: Vec<HostLine>,
    host_pos: usize,
    services: Vec<CrateServent>,
    service_pos: usize,
    protocols: Vec<CrateProtoent>,
    protocol_pos: usize,
    networks: Vec<raven_libc_resolv::dbfiles::Netent>,
    network_pos: usize,
}

impl Area {
    const fn new() -> Self {
        Self {
            host: Hostent::ZERO,
            host_buf: [0; 2048],
            serv: Servent::ZERO,
            serv_buf: [0; 2048],
            proto: Protoent::ZERO,
            proto_buf: [0; 2048],
            net: Netent::ZERO,
            net_buf: [0; 2048],
            hosts: Vec::new(),
            host_pos: 0,
            services: Vec::new(),
            service_pos: 0,
            protocols: Vec::new(),
            protocol_pos: 0,
            networks: Vec::new(),
            network_pos: 0,
        }
    }
}

/// The netdb scratch area is allocated on first use: the static keeps only a pointer, so the
/// process image pays 8 bytes of BSS instead of the 8 KiB `Area`. The C API already allocates
/// for the parsed database vectors, so this adds no new failure mode.
struct Cell(UnsafeCell<*mut Area>);

// SAFETY: access is serialised by `LOCK`; the pointer is written once under the lock and never
// freed.
unsafe impl Sync for Cell {}

static LOCK: SpinLock = SpinLock::new();
static AREA: Cell = Cell(UnsafeCell::new(core::ptr::null_mut()));

/// Run `f` over the process netdb scratch area.
fn with_area<R>(f: impl FnOnce(&mut Area) -> R) -> R {
    LOCK.acquire();
    // SAFETY: `LOCK` serialises this cell and the slot is private to this module.
    let slot = unsafe { &mut *AREA.0.get() };
    if slot.is_null() {
        *slot = Box::into_raw(Box::new(Area::new()));
    }
    // SAFETY: the pointer was published under `LOCK` and is never freed.
    let area = unsafe { &mut **slot };
    let result = f(area);
    LOCK.release();
    result
}

fn align8(value: usize) -> Option<usize> {
    value.checked_add(7).map(|value| value & !7usize)
}

fn write_bytes(buf: &mut [u8], cursor: &mut usize, bytes: &[u8]) -> Result<(), Errno> {
    let end = cursor
        .checked_add(bytes.len())
        .ok_or(Errno(ERANGE as u16))?;
    let slot = buf.get_mut(*cursor..end).ok_or(Errno(ERANGE as u16))?;
    slot.copy_from_slice(bytes);
    *cursor = end;
    Ok(())
}

fn write_cstr(buf: &mut [u8], cursor: &mut usize, value: &str) -> Result<usize, Errno> {
    let start = *cursor;
    write_bytes(buf, cursor, value.as_bytes())?;
    let end = cursor.checked_add(1).ok_or(Errno(ERANGE as u16))?;
    let slot = buf.get_mut(*cursor..end).ok_or(Errno(ERANGE as u16))?;
    slot.fill(0);
    *cursor = end;
    Ok(start)
}

fn write_pointer(buf: &mut [u8], at: usize, pointer: usize) -> Result<(), Errno> {
    let bytes = pointer.to_ne_bytes();
    let slot = buf
        .get_mut(at..at.checked_add(8).ok_or(Errno(ERANGE as u16))?)
        .ok_or(Errno(ERANGE as u16))?;
    slot.copy_from_slice(&bytes);
    Ok(())
}

fn pointer_table(buf: &mut [u8], cursor: &mut usize, values: &[usize]) -> Result<usize, Errno> {
    let start = align8(*cursor).ok_or(Errno(ERANGE as u16))?;
    let end = start
        .checked_add(values.len().saturating_add(1).saturating_mul(8))
        .ok_or(Errno(ERANGE as u16))?;
    if end > buf.len() {
        return Err(Errno(ERANGE as u16));
    }
    for (index, value) in values.iter().enumerate() {
        write_pointer(buf, start.saturating_add(index.saturating_mul(8)), *value)?;
    }
    write_pointer(buf, start.saturating_add(values.len().saturating_mul(8)), 0)?;
    *cursor = end;
    Ok(start)
}

/// Pack a `Hostent` into a caller record and buffer.
pub(crate) fn pack_host(
    entry: &HostEnt,
    record: &mut Hostent,
    buf: &mut [u8],
) -> Result<(), Errno> {
    if entry.addrs.len() > 16 || entry.aliases.len() > 16 {
        return Err(Errno(ERANGE as u16));
    }
    let mut cursor = 0usize;
    let name = write_cstr(buf, &mut cursor, &entry.name)?;
    let mut aliases = Vec::with_capacity(entry.aliases.len());
    for alias in &entry.aliases {
        aliases.push(write_cstr(buf, &mut cursor, alias)?);
    }
    let addr_len = entry.addrs.first().map_or(0, Vec::len);
    let mut addrs = Vec::with_capacity(entry.addrs.len());
    for addr in &entry.addrs {
        let offset = cursor;
        write_bytes(buf, &mut cursor, addr)?;
        addrs.push(offset);
    }
    let alias_table = pointer_table(buf, &mut cursor, &aliases)?;
    let addr_table = pointer_table(buf, &mut cursor, &addrs)?;
    let base = buf.as_mut_ptr() as usize;
    record.h_name = base.wrapping_add(name) as *mut c_char;
    record.h_aliases = base.wrapping_add(alias_table) as *mut *mut c_char;
    record.h_addrtype = entry.addr_type;
    record.h_length = i32::try_from(addr_len).map_err(|_| Errno(ERANGE as u16))?;
    record.h_addr_list = base.wrapping_add(addr_table) as *mut *mut c_char;
    Ok(())
}

/// Pack a `servent` into a caller record and buffer.
pub(crate) fn pack_serv(
    entry: &CrateServent,
    record: &mut Servent,
    buf: &mut [u8],
) -> Result<(), Errno> {
    if entry.aliases.len() > 16 {
        return Err(Errno(ERANGE as u16));
    }
    let mut cursor = 0usize;
    let name = write_cstr(buf, &mut cursor, &entry.name)?;
    let mut aliases = Vec::with_capacity(entry.aliases.len());
    for alias in &entry.aliases {
        aliases.push(write_cstr(buf, &mut cursor, alias)?);
    }
    let table = pointer_table(buf, &mut cursor, &aliases)?;
    let base = buf.as_mut_ptr() as usize;
    record.s_name = base.wrapping_add(name) as *mut c_char;
    record.s_aliases = base.wrapping_add(table) as *mut *mut c_char;
    record.s_port = c_int::from(entry.port.to_be());
    record.s_proto = match entry.proto {
        Proto::Tcp => b"tcp\0".as_ptr() as *mut c_char,
        Proto::Udp => b"udp\0".as_ptr() as *mut c_char,
    };
    Ok(())
}

/// Pack a `protoent` into a caller record and buffer.
pub(crate) fn pack_proto(
    entry: &CrateProtoent,
    record: &mut Protoent,
    buf: &mut [u8],
) -> Result<(), Errno> {
    if entry.aliases.len() > 16 {
        return Err(Errno(ERANGE as u16));
    }
    let mut cursor = 0usize;
    let name = write_cstr(buf, &mut cursor, &entry.name)?;
    let mut aliases = Vec::with_capacity(entry.aliases.len());
    for alias in &entry.aliases {
        aliases.push(write_cstr(buf, &mut cursor, alias)?);
    }
    let table = pointer_table(buf, &mut cursor, &aliases)?;
    let base = buf.as_mut_ptr() as usize;
    record.p_name = base.wrapping_add(name) as *mut c_char;
    record.p_aliases = base.wrapping_add(table) as *mut *mut c_char;
    record.p_proto = c_int::from(entry.proto);
    Ok(())
}

/// Pack a `netent` into a caller record and buffer.
pub(crate) fn pack_net(
    entry: &raven_libc_resolv::dbfiles::Netent,
    record: &mut Netent,
    buf: &mut [u8],
) -> Result<(), Errno> {
    if entry.aliases.len() > 16 {
        return Err(Errno(ERANGE as u16));
    }
    let mut cursor = 0usize;
    let name = write_cstr(buf, &mut cursor, &entry.name)?;
    let mut aliases = Vec::with_capacity(entry.aliases.len());
    for alias in &entry.aliases {
        aliases.push(write_cstr(buf, &mut cursor, alias)?);
    }
    let table = pointer_table(buf, &mut cursor, &aliases)?;
    let base = buf.as_mut_ptr() as usize;
    record.n_name = base.wrapping_add(name) as *mut c_char;
    record.n_aliases = base.wrapping_add(table) as *mut *mut c_char;
    record.n_addrtype = 2; // AF_INET
    record.n_net = entry.addr;
    Ok(())
}

/// Fill the static hostent record.
pub(crate) fn fill_host(entry: &HostEnt) -> *mut Hostent {
    with_area(
        |area| match pack_host(entry, &mut area.host, &mut area.host_buf) {
            Ok(()) => &raw mut area.host,
            Err(error) => {
                crate::ffi::errno::set(error);
                core::ptr::null_mut()
            }
        },
    )
}

/// Fill the static servent record.
pub(crate) fn fill_serv(entry: &CrateServent) -> *mut Servent {
    with_area(
        |area| match pack_serv(entry, &mut area.serv, &mut area.serv_buf) {
            Ok(()) => &raw mut area.serv,
            Err(error) => {
                crate::ffi::errno::set(error);
                core::ptr::null_mut()
            }
        },
    )
}

/// Fill the static protoent record.
pub(crate) fn fill_proto(entry: &CrateProtoent) -> *mut Protoent {
    with_area(
        |area| match pack_proto(entry, &mut area.proto, &mut area.proto_buf) {
            Ok(()) => &raw mut area.proto,
            Err(error) => {
                crate::ffi::errno::set(error);
                core::ptr::null_mut()
            }
        },
    )
}

/// Fill the static netent record.
pub(crate) fn fill_net(entry: &raven_libc_resolv::dbfiles::Netent) -> *mut Netent {
    with_area(
        |area| match pack_net(entry, &mut area.net, &mut area.net_buf) {
            Ok(()) => &raw mut area.net,
            Err(error) => {
                crate::ffi::errno::set(error);
                core::ptr::null_mut()
            }
        },
    )
}

fn with_env<R>(f: impl FnOnce(&raven_libc_resolv::Env<'_>, &HostOrder) -> R) -> R {
    let net = crate::socket::net();
    let dns = net.dns_io();
    let probe = net.probe();
    let ifnames = net.ifnames();
    let env = raven_libc_net::netdb::make_env(&FILES, &dns, &dns, &dns, &probe, &ifnames);
    let order = HostOrder::from_nss(&raven_libc_nss::load(&FILES));
    f(&env, &order)
}

/// `res_query` over the process resolver seam.
pub(crate) fn resolver_query(
    name: &[u8],
    class: u16,
    rtype: u16,
    answer: &mut [u8],
) -> Result<usize, raven_libc_resolv::ResError> {
    with_env(|env, _order| raven_libc_resolv::res_query(name, class, rtype, answer, env))
}

/// Look up a host by name.
pub(crate) fn host_by_name(name: &[u8], af: i32) -> Option<HostEnt> {
    let name = core::str::from_utf8(name).ok()?;
    with_env(|env, order| legacy::gethostbyname2(name, af, env, order).ok())
}

/// Look up a host by address.
pub(crate) fn host_by_addr(addr: &[u8], af: i32) -> Option<HostEnt> {
    with_env(|env, order| legacy::gethostbyaddr(addr, af, env, order).ok())
}

fn host_line(line: &HostLine) -> HostEnt {
    let mut names = line.names.iter();
    let name = names.next().cloned().unwrap_or_default();
    let aliases: Vec<_> = names.cloned().collect();
    let mut addrs = Vec::new();
    let addr_type = match line.addr {
        IpAddr::V4(bytes) => {
            addrs.push(bytes.to_vec());
            2
        }
        IpAddr::V6(bytes) => {
            addrs.push(bytes.to_vec());
            10
        }
    };
    HostEnt {
        name,
        aliases,
        addr_type,
        addrs,
    }
}

/// The next `/etc/hosts` entry, or `None` at the end.
pub(crate) fn next_host() -> Option<HostEnt> {
    with_area(|area| {
        if area.hosts.is_empty() {
            let bytes = FILES.read_file("/etc/hosts", 1 << 20).ok()?;
            let ifnames = crate::socket::net().ifnames();
            area.hosts = parse_hosts(&bytes, &ifnames);
            area.host_pos = 0;
        }
        let line = area.hosts.get(area.host_pos).cloned()?;
        area.host_pos = area.host_pos.saturating_add(1);
        Some(host_line(&line))
    })
}

/// Rewind the hosts cursor.
pub(crate) fn set_hosts() {
    with_area(|area| {
        area.hosts.clear();
        area.host_pos = 0;
    });
}

/// Finish the hosts cursor.
pub(crate) fn end_hosts() {
    with_area(|area| {
        area.hosts.clear();
        area.host_pos = 0;
    });
}

/// Look up one service.
pub(crate) fn serv_by_name(name: &[u8], proto: Option<&[u8]>) -> Option<CrateServent> {
    let name = core::str::from_utf8(name).ok()?;
    let proto = proto.and_then(|bytes| core::str::from_utf8(bytes).ok());
    raven_libc_resolv::getservbyname(&FILES, name, proto)
        .ok()
        .flatten()
}

/// Look up one service by port.
pub(crate) fn serv_by_port(port: u16, proto: Option<&[u8]>) -> Option<CrateServent> {
    let proto = proto.and_then(|bytes| core::str::from_utf8(bytes).ok());
    raven_libc_resolv::getservbyport(&FILES, port, proto)
        .ok()
        .flatten()
}

/// Load the service iterator.
pub(crate) fn set_services() {
    with_area(|area| {
        area.services = ServFile::open(&FILES).entries().to_vec();
        area.service_pos = 0;
    });
}

/// The next service entry.
pub(crate) fn next_service() -> Option<CrateServent> {
    with_area(|area| {
        if area.services.is_empty() {
            area.services = ServFile::open(&FILES).entries().to_vec();
            area.service_pos = 0;
        }
        let entry = area.services.get(area.service_pos).cloned()?;
        area.service_pos = area.service_pos.saturating_add(1);
        Some(entry)
    })
}

/// Finish the service iterator.
pub(crate) fn end_services() {
    with_area(|area| {
        area.services.clear();
        area.service_pos = 0;
    });
}

/// Look up one protocol.
pub(crate) fn proto_by_name(name: &[u8]) -> Option<CrateProtoent> {
    let name = core::str::from_utf8(name).ok()?;
    raven_libc_resolv::getprotobyname(&FILES, name)
        .ok()
        .flatten()
}

/// Look up one protocol by number.
pub(crate) fn proto_by_number(number: u8) -> Option<CrateProtoent> {
    raven_libc_resolv::getprotobynumber(&FILES, number)
        .ok()
        .flatten()
}

/// Load the protocol iterator.
pub(crate) fn set_protocols() {
    with_area(|area| {
        area.protocols = ProtoFile::open(&FILES).entries().to_vec();
        area.protocol_pos = 0;
    });
}

/// The next protocol entry.
pub(crate) fn next_protocol() -> Option<CrateProtoent> {
    with_area(|area| {
        if area.protocols.is_empty() {
            area.protocols = ProtoFile::open(&FILES).entries().to_vec();
            area.protocol_pos = 0;
        }
        let entry = area.protocols.get(area.protocol_pos).cloned()?;
        area.protocol_pos = area.protocol_pos.saturating_add(1);
        Some(entry)
    })
}

/// Finish the protocol iterator.
pub(crate) fn end_protocols() {
    with_area(|area| {
        area.protocols.clear();
        area.protocol_pos = 0;
    });
}

fn network_entries() -> Vec<raven_libc_resolv::dbfiles::Netent> {
    NetFile::open(&FILES).entries().to_vec()
}

/// Look up one network by name.
pub(crate) fn net_by_name(name: &[u8]) -> Option<raven_libc_resolv::dbfiles::Netent> {
    let name = core::str::from_utf8(name).ok()?;
    network_entries().into_iter().find(|entry| {
        entry.name.eq_ignore_ascii_case(name)
            || entry.aliases.iter().any(|a| a.eq_ignore_ascii_case(name))
    })
}

/// Look up one network by number.
pub(crate) fn net_by_addr(addr: u32) -> Option<raven_libc_resolv::dbfiles::Netent> {
    network_entries()
        .into_iter()
        .find(|entry| entry.addr == addr)
}

/// Load the network iterator.
pub(crate) fn set_networks() {
    with_area(|area| {
        area.networks = network_entries();
        area.network_pos = 0;
    });
}

/// The next network entry.
pub(crate) fn next_network() -> Option<raven_libc_resolv::dbfiles::Netent> {
    with_area(|area| {
        if area.networks.is_empty() {
            area.networks = network_entries();
            area.network_pos = 0;
        }
        let entry = area.networks.get(area.network_pos).cloned()?;
        area.network_pos = area.network_pos.saturating_add(1);
        Some(entry)
    })
}

/// Finish the network iterator.
pub(crate) fn end_networks() {
    with_area(|area| {
        area.networks.clear();
        area.network_pos = 0;
    });
}

/// Pack a hostent into a caller record and buffer.
pub(crate) fn pack_host_r(
    entry: &HostEnt,
    record: &mut Hostent,
    buf: &mut [u8],
) -> Result<(), Errno> {
    pack_host(entry, record, buf)
}
