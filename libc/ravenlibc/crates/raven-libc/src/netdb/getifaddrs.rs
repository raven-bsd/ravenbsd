// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getifaddrs(3)` / `freeifaddrs(3)` over C63's `raven-libc-net` interface list.

use alloc::boxed::Box;
use alloc::ffi::CString;
use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr;

use raven_libc_abi::errno::Errno;
use raven_libc_net::ErrnoExt;
use raven_libc_net::addr::{self, SockAddr as NetSockAddr};
use raven_libc_net::iface::IfAddr;

/// The C `struct ifaddrs` union.
#[repr(C)]
pub(crate) union IfAddrsIfu {
    pub(crate) broadaddr: *mut c_void,
    pub(crate) dstaddr: *mut c_void,
}

/// The C `struct ifaddrs`.
#[repr(C)]
pub(crate) struct IfAddrs {
    ifa_next: *mut IfAddrs,
    ifa_name: *mut c_char,
    ifa_flags: c_uint,
    ifa_addr: *mut c_void,
    ifa_netmask: *mut c_void,
    ifa_ifu: IfAddrsIfu,
    ifa_data: *mut c_void,
}

fn sockaddr_in(ip: [u8; 4]) -> Option<*mut c_void> {
    let mut bytes = Box::new([0u8; 16]);
    let net = NetSockAddr::In { ip, port: 0 };
    if addr::write_sockaddr(&net, bytes.as_mut_slice()) == 0 {
        return None;
    }
    Some(Box::into_raw(bytes).cast())
}

fn build_one(item: &IfAddr) -> Option<*mut IfAddrs> {
    let name = CString::new(item.name.as_slice()).ok()?;
    let address = sockaddr_in(item.addr)?;
    let netmask = sockaddr_in(item.netmask)?;
    let broadaddr = match item.broadaddr {
        Some(ip) => sockaddr_in(ip)?,
        None => ptr::null_mut(),
    };
    let node = Box::new(IfAddrs {
        ifa_next: ptr::null_mut(),
        ifa_name: name.into_raw(),
        ifa_flags: item.flags,
        ifa_addr: address,
        ifa_netmask: netmask,
        ifa_ifu: IfAddrsIfu { broadaddr },
        ifa_data: ptr::null_mut(),
    });
    Some(Box::into_raw(node))
}

pub(crate) fn build_list(items: &[IfAddr]) -> Result<*mut IfAddrs, Errno> {
    let mut head: *mut IfAddrs = ptr::null_mut();
    let mut tail: *mut IfAddrs = ptr::null_mut();
    for item in items {
        let Some(node) = build_one(item) else {
            // SAFETY: `head` is the list built above and is not used again after the free.
            unsafe { free_list(head) };
            return Err(Errno::ENOMEM);
        };
        if tail.is_null() {
            head = node;
        } else {
            // SAFETY: `tail` is a live node of the list being built.
            unsafe { (*tail).ifa_next = node };
        }
        tail = node;
    }
    Ok(head)
}

/// Free a list returned by [`build_list`].
///
/// # Safety
/// `list` is NULL or a list built by [`build_list`] and not freed before.
pub(crate) unsafe fn free_list(list: *mut IfAddrs) {
    let mut current = list;
    while !current.is_null() {
        // SAFETY: the list invariant above; each node is freed exactly once.
        let node = unsafe { Box::from_raw(current) };
        let next = node.ifa_next;
        if !node.ifa_name.is_null() {
            // SAFETY: `ifa_name` came from `CString::into_raw` in `build_one`.
            unsafe { drop(CString::from_raw(node.ifa_name)) };
        }
        // SAFETY: the union was initialised with `broadaddr` in `build_one`.
        let broadaddr = unsafe { node.ifa_ifu.broadaddr };
        for pointer in [node.ifa_addr, node.ifa_netmask, broadaddr] {
            if !pointer.is_null() {
                // SAFETY: each sockaddr came from `Box<[u8; 16]>` in `sockaddr_in`.
                unsafe { drop(Box::from_raw(pointer.cast::<[u8; 16]>())) };
            }
        }
        current = next;
    }
}

/// `getifaddrs(3)`.
///
/// # Safety
/// `ifap` is NULL or writable for one pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getifaddrs(ifap: *mut *mut IfAddrs) -> c_int {
    if ifap.is_null() {
        crate::ffi::errno::set(Errno::EINVAL);
        return -1;
    }
    // SAFETY: the caller's writable output slot.
    unsafe { ifap.write(ptr::null_mut()) };
    let result = crate::socket::with_net(|net| raven_libc_net::iface::getifaddrs(net.backend()));
    match result {
        Ok(items) => match build_list(&items) {
            Ok(head) => {
                // SAFETY: the caller's writable output slot.
                unsafe { ifap.write(head) };
                0
            }
            Err(error) => {
                crate::ffi::errno::set(error);
                -1
            }
        },
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}

/// `freeifaddrs(3)`.
///
/// # Safety
/// `list` is NULL or a list returned by [`getifaddrs`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn freeifaddrs(list: *mut IfAddrs) {
    // SAFETY: the caller's list contract.
    unsafe { free_list(list) };
}

#[cfg(test)]
#[path = "getifaddrs_ut.rs"]
mod tests;
