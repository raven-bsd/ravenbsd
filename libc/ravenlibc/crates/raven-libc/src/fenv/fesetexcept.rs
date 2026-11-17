// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `fesetexcept`.
#[unsafe(no_mangle)]
pub extern "C" fn fesetexcept(excepts: core::ffi::c_int) -> core::ffi::c_int {
    raven_libc_m::fesetexcept(excepts)
}

/// `fesetexceptflag`.
///
/// # Safety
/// `flagp` is a non-NULL readable `fexcept_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fesetexceptflag(
    flagp: *const super::FexceptC,
    excepts: core::ffi::c_int,
) -> core::ffi::c_int {
    // SAFETY: the C contract requires a readable fexcept_t.
    let flags = raven_libc_m::FexceptT(unsafe { u32::from(*flagp) });
    raven_libc_m::fesetexceptflag(&flags, excepts)
}

/// `fesetmode`.
///
/// # Safety
/// `modep` is a non-NULL readable `femode_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fesetmode(modep: *const raven_libc_m::Femode) -> core::ffi::c_int {
    // SAFETY: the C contract requires a valid pointer.
    unsafe { raven_libc_m::fesetmode(&*modep) }
}

/// `fesetround`.
#[unsafe(no_mangle)]
pub extern "C" fn fesetround(round: core::ffi::c_int) -> core::ffi::c_int {
    raven_libc_m::fesetround(round)
}
