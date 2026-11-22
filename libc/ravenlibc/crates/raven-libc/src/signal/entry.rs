// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The shell's production signal entry and its one-time kernel registration.
//!
//! `__raven_sigentry` is the assembly stub the kernel resumes at; it calls the address stored in
//! `__rl_entry_fn`. The shell owns the disposition table, so this wrapper is the function that
//! binds the stub to `signal_entry` and the process table.

use core::sync::atomic::{AtomicBool, Ordering};

use raven_libc_abi::errno::Errno;
use raven_libc_signal::kernel::FaultRecord;
use raven_libc_signal::{ENOMEM_ERR, SigKernel, SysKernel, raw, signal_entry};

use super::DISP;

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The C-ABI body called by `__raven_sigentry` with `(bits, fault_record)`.
unsafe extern "C" fn shell_signal_entry(bits: usize, fault: *const FaultRecord) -> ! {
    // SAFETY: the kernel passes either zero or a readable `FaultRecord` in the registered
    // context area; a kick passes zero.
    let fault = if fault.is_null() {
        None
    } else {
        Some(unsafe { *fault })
    };
    signal_entry(&SysKernel::new(), &DISP, bits as u32, fault)
}

/// Register the entry with the kernel on the first handler installation.
///
/// The disposition table itself is always usable; only asynchronous delivery needs PG-7.
pub(crate) fn ensure_entry() -> Result<(), Errno> {
    if INSTALLED.load(Ordering::Acquire) {
        return Ok(());
    }
    #[cfg(target_os = "none")]
    {
        let kernel = SysKernel::new();
        let tp = kernel.thread_pointer();
        let (context, offset) = raw::signal_context_area(tp).ok_or(ENOMEM_ERR)?;
        raw::set_ctx_area(tp, context);
        raw::set_entry_function(shell_signal_entry as *const () as usize);
        kernel.sig_setup(raw::sigentry_address(), offset, 0)?;
    }
    INSTALLED.store(true, Ordering::Release);
    Ok(())
}
