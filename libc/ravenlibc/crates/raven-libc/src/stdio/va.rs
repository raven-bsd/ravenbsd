// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `va_list` bridge between C59's trampolines and the `v*` targets.
//!
//! One alias per architecture: the trampoline passes a pointer to the platform list; the
//! reader copies it and never writes back (C59 RD-new 4).

use crate::va::NativeReader;

#[cfg(target_arch = "x86_64")]
pub(crate) type List = crate::va::VaListSysV;
#[cfg(target_arch = "aarch64")]
pub(crate) type List = crate::va::VaListAapcs64;
#[cfg(target_arch = "riscv64")]
pub(crate) type List = crate::va::VaListRiscv64;

/// The reader over the trampoline's list pointer.
///
/// # Safety
/// `list` points to the list the stub built and stays readable for the call.
pub(crate) unsafe fn reader(list: *mut List) -> NativeReader {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: the x86_64 stub passes `&VaListSysV`.
        unsafe { crate::va::SysvReader::from_ptr(list) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: the aarch64 stub passes `&VaListAapcs64`.
        unsafe { crate::va::Aapcs64Reader::from_ptr(list) }
    }
    #[cfg(target_arch = "riscv64")]
    {
        // SAFETY: the riscv64 stub passes a pointer to the cursor value.
        unsafe { crate::va::RiscvReader::from_cursor(*list) }
    }
}
