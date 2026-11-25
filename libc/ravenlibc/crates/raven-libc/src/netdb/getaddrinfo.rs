// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Runtime `getaddrinfo`, `freeaddrinfo` and `getnameinfo` over C63's sockets.

use alloc::boxed::Box;
use alloc::ffi::CString;
use core::ffi::{c_char, c_int, c_void};

use raven_libc_net::addr::{self, SockAddr as NetSockAddr};
use raven_libc_net::netdb;
use raven_libc_resolv::{AddrInfoRec, Hints, IpAddr, SockAddr};

use crate::pwd::source::ShellFiles;

static FILES: ShellFiles = ShellFiles;

/// The C `struct addrinfo`.
#[repr(C)]
pub struct AddrInfo {
    pub(crate) ai_flags: c_int,
    pub(crate) ai_family: c_int,
    pub(crate) ai_socktype: c_int,
    pub(crate) ai_protocol: c_int,
    pub(crate) ai_addrlen: u32,
    pub(crate) ai_addr: *mut c_void,
    pub(crate) ai_canonname: *mut c_char,
    pub(crate) ai_next: *mut AddrInfo,
}

fn net_addr(value: &SockAddr) -> NetSockAddr {
    match value.ip {
        IpAddr::V4(ip) => NetSockAddr::In {
            ip,
            port: value.port,
        },
        IpAddr::V6(ip) => NetSockAddr::In6 {
            ip,
            port: value.port,
            scope: value.scope,
            flowinfo: 0,
        },
    }
}

fn node(record: &AddrInfoRec) -> Option<*mut AddrInfo> {
    let mut sockaddr = Box::new([0u8; 28]);
    let net = net_addr(&record.addr);
    let length = addr::write_sockaddr(&net, sockaddr.as_mut_slice());
    let canonname = match &record.canonname {
        Some(name) => CString::new(name.as_bytes()).ok()?.into_raw(),
        None => core::ptr::null_mut(),
    };
    let value = Box::new(AddrInfo {
        ai_flags: record.flags,
        ai_family: netdb::public_family(record.family),
        ai_socktype: record.socktype,
        ai_protocol: record.protocol,
        ai_addrlen: u32::try_from(length).ok()?,
        ai_addr: Box::into_raw(sockaddr).cast(),
        ai_canonname: canonname,
        ai_next: core::ptr::null_mut(),
    });
    Some(Box::into_raw(value))
}

/// `getaddrinfo(3)`.
///
/// # Safety
/// `node`, `service` and `hints` are NULL or follow the C string/record contracts; `result` is
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getaddrinfo(
    node_name: *const c_char,
    service: *const c_char,
    hints: *const AddrInfo,
    result: *mut *mut AddrInfo,
) -> c_int {
    if result.is_null() {
        return raven_libc_resolv::Eai::Fail.code();
    }
    // SAFETY: the caller's writable result slot.
    unsafe { result.write(core::ptr::null_mut()) };
    let hints = if hints.is_null() {
        Hints::default()
    } else {
        // SAFETY: the caller's hints record.
        let value = unsafe { &*hints };
        Hints {
            flags: value.ai_flags,
            family: value.ai_family,
            socktype: value.ai_socktype,
            protocol: value.ai_protocol,
        }
    };
    // SAFETY: the optional name strings are NUL terminated.
    let host = unsafe { crate::ffi::cstr::slice(node_name) };
    let service = unsafe { crate::ffi::cstr::slice(service) };
    let host = if host.is_empty() {
        None
    } else {
        match core::str::from_utf8(host) {
            Ok(value) => Some(value),
            Err(_) => return raven_libc_resolv::Eai::NoName.code(),
        }
    };
    let service = if service.is_empty() {
        None
    } else {
        match core::str::from_utf8(service) {
            Ok(value) => Some(value),
            Err(_) => return raven_libc_resolv::Eai::Service.code(),
        }
    };
    // The public hints carry FreeBSD-valued families; the resolver's records are Linux-valued.
    let family = match netdb::resolver_family(hints.family) {
        Some(family) => family,
        None => return raven_libc_resolv::Eai::Family.code(),
    };
    let hints = Hints { family, ..hints };
    let records = match netdb::getaddrinfo(crate::socket::net(), &FILES, host, service, &hints) {
        Ok(records) => records,
        Err(error) => return error.code(),
    };
    let mut head: *mut AddrInfo = core::ptr::null_mut();
    let mut tail: *mut AddrInfo = core::ptr::null_mut();
    for record in &records {
        let Some(new) = node(record) else {
            // SAFETY: `head` is the list built above.
            unsafe { freeaddrinfo(head) };
            return raven_libc_resolv::Eai::Memory.code();
        };
        if tail.is_null() {
            head = new;
        } else {
            // SAFETY: `tail` is a live node of the list being built.
            unsafe { (*tail).ai_next = new };
        }
        tail = new;
    }
    // SAFETY: the caller's writable result slot.
    unsafe { result.write(head) };
    0
}

/// `freeaddrinfo(3)`.
///
/// # Safety
/// `list` is NULL or a list returned by [`getaddrinfo`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeaddrinfo(list: *mut AddrInfo) {
    let mut current = list;
    while !current.is_null() {
        // SAFETY: the list was built by `node` and is freed exactly once.
        let node = unsafe { Box::from_raw(current) };
        let next = node.ai_next;
        if !node.ai_addr.is_null() {
            // SAFETY: `ai_addr` came from `Box<[u8; 28]>` in `node`.
            unsafe { drop(Box::from_raw(node.ai_addr.cast::<[u8; 28]>())) };
        }
        if !node.ai_canonname.is_null() {
            // SAFETY: `ai_canonname` came from `CString::into_raw` in `node`.
            unsafe { drop(CString::from_raw(node.ai_canonname)) };
        }
        current = next;
    }
}

/// A caller's output string, `EAI_OVERFLOW` when it does not fit.
///
/// # Safety
/// `out` is NULL with `len == 0`, or writable for `len` bytes.
unsafe fn write_name(out: *mut c_char, len: usize, value: Option<&str>) -> c_int {
    let Some(value) = value else {
        return 0;
    };
    let needed = value.len().saturating_add(1);
    if out.is_null() || len < needed {
        return raven_libc_resolv::Eai::Overflow.code();
    }
    // SAFETY: the caller granted `len` writable bytes and `needed <= len`.
    unsafe {
        core::ptr::copy_nonoverlapping(value.as_ptr(), out.cast::<u8>(), value.len());
        out.cast::<u8>().add(value.len()).write(0);
    }
    0
}

/// `getnameinfo(3)`.
///
/// # Safety
/// `addr` points to `addrlen` readable bytes; `host`/`service` follow their C buffer contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getnameinfo(
    addr: *const c_void,
    addrlen: u32,
    host: *mut c_char,
    hostlen: u32,
    service: *mut c_char,
    servicelen: u32,
    flags: c_int,
) -> c_int {
    // SAFETY: the caller's sockaddr contract.
    let bytes = match unsafe { crate::ffi::sockaddr::view(addr, addrlen) } {
        Ok(bytes) => bytes,
        Err(_) => return raven_libc_resolv::Eai::Family.code(),
    };
    let parsed = match addr::parse_sockaddr(bytes) {
        Ok(NetSockAddr::In { ip, port }) => SockAddr::new(IpAddr::V4(ip), port),
        Ok(NetSockAddr::In6 {
            ip, port, scope, ..
        }) => SockAddr::with_scope(IpAddr::V6(ip), port, scope),
        Ok(NetSockAddr::Un { .. }) | Err(_) => {
            return raven_libc_resolv::Eai::Family.code();
        }
    };
    let (host_name, service_name) = match netdb::getnameinfo(
        crate::socket::net(),
        &FILES,
        &parsed,
        hostlen as usize,
        servicelen as usize,
        flags,
    ) {
        Ok(value) => value,
        Err(error) => return error.code(),
    };
    // SAFETY: the caller's output buffers.
    let first = unsafe { write_name(host, hostlen as usize, host_name.as_deref()) };
    if first != 0 {
        return first;
    }
    // SAFETY: the caller's output buffers.
    unsafe { write_name(service, servicelen as usize, service_name.as_deref()) }
}
