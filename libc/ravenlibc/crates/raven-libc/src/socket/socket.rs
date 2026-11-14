// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `listen`, `shutdown`, `sockatmark` and `socket`.

use core::ffi::c_int;

use super::*;

/// `socket(2)`.
#[unsafe(no_mangle)]
pub extern "C" fn socket(domain: c_int, ty: c_int, proto: c_int) -> c_int {
    socket_result(with_net(|net| net.socket(domain, ty, proto)))
}

/// `listen(2)`.
#[unsafe(no_mangle)]
pub extern "C" fn listen(fd: c_int, backlog: c_int) -> c_int {
    unit_result(with_net(|net| net.listen(fd, backlog)))
}

/// `shutdown(2)`.
#[unsafe(no_mangle)]
pub extern "C" fn shutdown(fd: c_int, how: c_int) -> c_int {
    unit_result(with_net(|net| net.shutdown(fd, how)))
}

/// `sockatmark(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn sockatmark(fd: c_int) -> c_int {
    socket_result(with_net(|net| net.sockatmark(fd)))
}
