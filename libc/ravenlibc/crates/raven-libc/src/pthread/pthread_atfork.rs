// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pthread_atfork(3)`.
use core::ffi::c_int;
use raven_libc_thread::thread::atfork;
#[unsafe(no_mangle)]
pub extern "C" fn pthread_atfork(
    prepare: Option<extern "C" fn()>,
    parent: Option<extern "C" fn()>,
    child: Option<extern "C" fn()>,
) -> c_int {
    let p: Option<fn()> =
        prepare.map(|f| unsafe { core::mem::transmute::<extern "C" fn(), fn()>(f) }); // SAFETY: the two fn() types share the C ABI; the hook is only invoked as prepare.
    let pa: Option<fn()> =
        parent.map(|f| unsafe { core::mem::transmute::<extern "C" fn(), fn()>(f) }); // SAFETY: the two fn() types share the C ABI; the hook is only invoked as parent.
    let c: Option<fn()> =
        child.map(|f| unsafe { core::mem::transmute::<extern "C" fn(), fn()>(f) }); // SAFETY: the two fn() types share the C ABI; the hook is only invoked as child.
    match atfork::pthread_atfork(p, pa, c) {
        Ok(()) => 0,
        Err(e) => super::common::err(e),
    }
}
