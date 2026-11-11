// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 system-call entry support.
//!
//! Unlike x86-64's `syscall` MSRs, `svc #0` needs no per-CPU configuration: EL0 may always
//! execute it while `HCR_EL2.TGE` is clear (the loader runs without a hypervisor). The exception
//! arrives in the lower-EL AArch64 synchronous vector with `ESR_EL1.EC = 0x15` and is dispatched
//! by [`crate::syscall::svc_dispatch`].

/// No architecture setup is required for `svc`.
pub fn init() {}
