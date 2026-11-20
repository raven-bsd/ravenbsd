// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `recv`, `recvfrom`, `recvmsg` and `recvmmsg`.

use core::ffi::{c_int, c_uint, c_void};

use raven_libc_abi::structs::Msghdr;

use super::*;

/// `recv(2)`.
///
/// # Safety
/// `buf` points to `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn recv(fd: c_int, buf: *mut c_void, len: usize, flags: c_int) -> isize {
    if buf.is_null() && len != 0 {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    // SAFETY: the caller's writable buffer; `recv` has no source address.
    let data = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), len) };
    match with_net(|net| net.recvmsg(fd, &mut [], &mut [data], &mut [], flags)) {
        Ok(message) => isize::try_from(message.len).unwrap_or(isize::MAX),
        Err(error) => crate::ffi::ret::ssize(Err(error)),
    }
}

/// `recvfrom(2)`.
///
/// # Safety
/// `buf`/`addr`/`addrlen` follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn recvfrom(
    fd: c_int,
    buf: *mut c_void,
    len: usize,
    flags: c_int,
    addr: *mut c_void,
    addrlen: *mut SockLen,
) -> isize {
    if buf.is_null() && len != 0 {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    let capacity = if addrlen.is_null() {
        0
    } else {
        // SAFETY: the caller's in/out socklen.
        unsafe { *addrlen }
    };
    // SAFETY: the caller's writable buffers.
    let data = unsafe { crate::ffi::cast::bytes_mut(buf.cast(), len) };
    let name = match unsafe { crate::ffi::sockaddr::view_mut(addr, capacity) } {
        Ok(name) => name,
        Err(error) => return crate::ffi::ret::ssize(Err(error)),
    };
    match with_net(|net| net.recvmsg(fd, name, &mut [data], &mut [], flags)) {
        Ok(message) => {
            // SAFETY: the caller's in/out socklen.
            unsafe { crate::ffi::sockaddr::write_len(addrlen, message.name_len, capacity) };
            isize::try_from(message.len).unwrap_or(isize::MAX)
        }
        Err(error) => crate::ffi::ret::ssize(Err(error)),
    }
}

/// `recvmsg(2)`.
///
/// # Safety
/// `message` and every buffer it names follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn recvmsg(fd: c_int, message: *mut Msghdr, flags: c_int) -> isize {
    if message.is_null() {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    // SAFETY: the caller's msghdr.
    let header = unsafe { &mut *message };
    let capacity = header.msg_namelen;
    // SAFETY: the optional name buffer and its capacity.
    let name = match unsafe { crate::ffi::sockaddr::view_mut(header.msg_name, capacity) } {
        Ok(name) => name,
        Err(error) => return crate::ffi::ret::ssize(Err(error)),
    };
    // SAFETY: the iovec array contract.
    let mut iov =
        match unsafe { crate::ffi::iov::slices_mut(header.msg_iov.cast(), header.msg_iovlen) } {
            Ok(iov) => iov,
            Err(error) => return crate::ffi::ret::ssize(Err(error)),
        };
    let control_capacity = header.msg_controllen as usize;
    if control_capacity != 0 && header.msg_control.is_null() {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    // SAFETY: the caller supplied `msg_controllen` writable control bytes; a zero length does
    // not dereference the pointer.
    let control: &mut [u8] = if control_capacity == 0 {
        &mut []
    } else {
        unsafe {
            core::slice::from_raw_parts_mut(header.msg_control.cast::<u8>(), control_capacity)
        }
    };
    match with_net(|net| net.recvmsg(fd, name, &mut iov, control, flags)) {
        Ok(reply) => {
            // SAFETY: the caller's in/out socklen.
            unsafe {
                crate::ffi::sockaddr::write_len(&mut header.msg_namelen, reply.name_len, capacity)
            };
            header.msg_controllen = u32::try_from(reply.control_len).unwrap_or(u32::MAX);
            header.msg_flags = reply.flags;
            isize::try_from(reply.len).unwrap_or(isize::MAX)
        }
        Err(error) => crate::ffi::ret::ssize(Err(error)),
    }
}

/// `recvmmsg(2)`.
///
/// # Safety
/// `messages` points to `count` writable `mmsghdr`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn recvmmsg(
    fd: c_int,
    messages: *mut c_void,
    count: c_uint,
    flags: c_int,
    _timeout: *mut c_void,
) -> c_int {
    if messages.is_null() && count != 0 {
        return crate::ffi::ret::int(Err(fault()));
    }
    let mut received = 0u32;
    while received < count {
        // SAFETY: the caller's array of mmsghdr.
        let message = unsafe { messages.cast::<Mmsghdr>().add(received as usize) };
        // SAFETY: the current mmsghdr is writable.
        let length = unsafe { recvmsg(fd, core::ptr::addr_of_mut!((*message).header), flags) };
        if length < 0 {
            if received == 0 {
                return length as c_int;
            }
            break;
        }
        // SAFETY: the current mmsghdr is writable.
        unsafe { (*message).len = u32::try_from(length).unwrap_or(u32::MAX) };
        received = received.saturating_add(1);
    }
    c_int::try_from(received).unwrap_or(c_int::MAX)
}
