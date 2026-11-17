// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `fetestexcept`.
#[unsafe(no_mangle)]
pub extern "C" fn fetestexcept(excepts: core::ffi::c_int) -> core::ffi::c_int {
    raven_libc_m::fetestexcept(excepts)
}

/// `fetestexceptflag`.
///
/// # Safety
/// `flagp` is a non-NULL readable `fexcept_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fetestexceptflag(
    flagp: *const super::FexceptC,
    excepts: core::ffi::c_int,
) -> core::ffi::c_int {
    // SAFETY: the C contract requires a readable fexcept_t.
    let flags = raven_libc_m::FexceptT(unsafe { u32::from(*flagp) });
    raven_libc_m::fetestexceptflag(&flags, excepts)
}

/// `feupdateenv`.
///
/// # Safety
/// `envp` is a non-NULL readable `fenv_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn feupdateenv(envp: *const raven_libc_m::FenvT) -> core::ffi::c_int {
    // SAFETY: the C contract requires a valid pointer.
    unsafe { raven_libc_m::feupdateenv(&*envp) }
}
