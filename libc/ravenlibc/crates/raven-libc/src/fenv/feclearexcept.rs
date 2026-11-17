// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `feclearexcept`.
#[unsafe(no_mangle)]
pub extern "C" fn feclearexcept(excepts: core::ffi::c_int) -> core::ffi::c_int {
    raven_libc_m::feclearexcept(excepts)
}

/// `fegetenv`.
///
/// # Safety
/// `envp` is a non-NULL writable `fenv_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fegetenv(envp: *mut raven_libc_m::FenvT) -> core::ffi::c_int {
    // SAFETY: the C contract requires a valid pointer.
    unsafe { raven_libc_m::fegetenv(&mut *envp) }
}

/// `fegetexceptflag`.
///
/// # Safety
/// `flagp` is a non-NULL writable `fexcept_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fegetexceptflag(
    flagp: *mut super::FexceptC,
    excepts: core::ffi::c_int,
) -> core::ffi::c_int {
    let mut flags = raven_libc_m::FexceptT(0);
    let result = raven_libc_m::fegetexceptflag(&mut flags, excepts);
    // SAFETY: the C contract requires a writable fexcept_t.
    unsafe {
        *flagp = flags.0 as super::FexceptC;
    }
    result
}

/// `fegetmode`.
///
/// # Safety
/// `modep` is a non-NULL writable `femode_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fegetmode(modep: *mut raven_libc_m::Femode) -> core::ffi::c_int {
    // SAFETY: the C contract requires a valid pointer.
    unsafe { raven_libc_m::fegetmode(&mut *modep) }
}
