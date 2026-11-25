// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `socketpair`, `send`, `sendto` and `sendmsg`.

use core::ffi::{c_int, c_void};

use raven_libc_abi::structs::Msghdr;

use super::*;

/// `socketpair(2)`.
///
/// # Safety
/// `sv` points to two writable `int`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn socketpair(
    domain: c_int,
    ty: c_int,
    proto: c_int,
    sv: *mut c_int,
) -> c_int {
    if sv.is_null() {
        return crate::ffi::ret::int(Err(fault()));
    }
    match with_net(|net| net.socketpair(domain, ty, proto)) {
        Ok([left, right]) => {
            // SAFETY: the caller supplied two writable ints.
            unsafe {
                sv.write(left);
                sv.add(1).write(right);
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

/// `send(2)`.
///
/// # Safety
/// `buf` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn send(fd: c_int, buf: *const c_void, len: usize, flags: c_int) -> isize {
    if buf.is_null() && len != 0 {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    // SAFETY: the caller's readable buffer.
    let data = unsafe { crate::ffi::cast::bytes(buf.cast(), len) };
    send_result(with_net(|net| net.sendmsg(fd, None, &[data], &[], flags)))
}

/// `sendto(2)`.
///
/// # Safety
/// `buf`/`addr` follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendto(
    fd: c_int,
    buf: *const c_void,
    len: usize,
    flags: c_int,
    addr: *const c_void,
    addrlen: SockLen,
) -> isize {
    if buf.is_null() && len != 0 {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    // SAFETY: the caller's readable buffer and sockaddr.
    let data = unsafe { crate::ffi::cast::bytes(buf.cast(), len) };
    let name = match unsafe { crate::ffi::sockaddr::view(addr, addrlen) } {
        Ok(name) => name,
        Err(error) => return crate::ffi::ret::ssize(Err(error)),
    };
    send_result(with_net(|net| {
        net.sendmsg(fd, Some(name), &[data], &[], flags)
    }))
}

/// `sendmsg(2)`.
///
/// # Safety
/// `message` and every buffer it names follow their C contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendmsg(fd: c_int, message: *const Msghdr, flags: c_int) -> isize {
    if message.is_null() {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    // SAFETY: the caller's msghdr.
    let header = unsafe { &*message };
    // SAFETY: the iovec array contract.
    let iov = match unsafe { crate::ffi::iov::slices(header.msg_iov.cast(), header.msg_iovlen) } {
        Ok(iov) => iov,
        Err(error) => return crate::ffi::ret::ssize(Err(error)),
    };
    // SAFETY: the optional name buffer.
    let name = match unsafe { crate::ffi::sockaddr::view(header.msg_name, header.msg_namelen) } {
        Ok(name) if !name.is_empty() => Some(name),
        Ok(_) => None,
        Err(error) => return crate::ffi::ret::ssize(Err(error)),
    };
    // SAFETY: the optional control buffer.
    let control = match unsafe {
        crate::ffi::sockaddr::control(header.msg_control.cast_const(), header.msg_controllen)
    } {
        Ok(control) => control,
        Err(error) => return crate::ffi::ret::ssize(Err(error)),
    };
    send_result(with_net(|net| net.sendmsg(fd, name, &iov, control, flags)))
}
