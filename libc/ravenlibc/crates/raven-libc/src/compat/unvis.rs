// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the unvis export (ticket C65, `17` section 13). The logic is
// `raven-libc-compat::vis::unvis`; this file owns only the C signature and the errno write.
//! `unvis` C-ABI export (ticket C65).

use core::ffi::c_char;

use raven_libc_compat::vis;
use raven_libc_compat::vis::unvis;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn unvis(
    cp: *mut c_char,
    c: core::ffi::c_int,
    astate: *mut core::ffi::c_int,
    flag: core::ffi::c_int,
) -> core::ffi::c_int {
    if cp.is_null() || astate.is_null() {
        return vis::UNVIS_SYNBAD;
    }
    // SAFETY: the caller owns both pointers for the duration of the call.
    // SAFETY: `astate` is the caller's state word.
    let packed = unsafe { *astate };
    let mut machine = vis::Unvis::unpack(packed as u32);
    let step = machine.feed(c as u8, flag as u32);
    // SAFETY: `astate` is writable.
    unsafe { *astate = machine.pack() as core::ffi::c_int };
    match step {
        vis::UnvisStep::Valid(byte) => {
            // SAFETY: `cp` is writable.
            unsafe { *cp = byte as c_char };
            vis::UNVIS_VALID
        }
        vis::UnvisStep::ValidPush(byte) => {
            // SAFETY: `cp` is writable.
            unsafe { *cp = byte as c_char };
            vis::UNVIS_VALIDPUSH
        }
        vis::UnvisStep::NoChar => vis::UNVIS_NOCHAR,
        vis::UnvisStep::SynBad => vis::UNVIS_SYNBAD,
        vis::UnvisStep::Error => vis::UNVIS_ERROR,
    }
}
