// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `fegetround`.
#[unsafe(no_mangle)]
pub extern "C" fn fegetround() -> core::ffi::c_int {
    raven_libc_m::fegetround()
}

/// `feholdexcept`.
///
/// # Safety
/// `envp` is a non-NULL writable `fenv_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn feholdexcept(envp: *mut raven_libc_m::FenvT) -> core::ffi::c_int {
    // SAFETY: the C contract requires a valid pointer.
    unsafe { raven_libc_m::feholdexcept(&mut *envp) }
}

/// `feraiseexcept`.
#[unsafe(no_mangle)]
pub extern "C" fn feraiseexcept(excepts: core::ffi::c_int) -> core::ffi::c_int {
    raven_libc_m::feraiseexcept(excepts)
}

/// `fesetenv`.
///
/// # Safety
/// `envp` is a non-NULL readable `fenv_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fesetenv(envp: *const raven_libc_m::FenvT) -> core::ffi::c_int {
    // SAFETY: the C contract requires a valid pointer.
    unsafe { raven_libc_m::fesetenv(&*envp) }
}
