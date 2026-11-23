// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The shell's production signal entry and its one-time kernel registration.
//!
//! `__raven_sigentry` is the assembly stub the kernel resumes at; it calls the address stored in
//! `__rl_entry_fn`. The shell owns the disposition table, so this wrapper is the function that
//! binds the stub to `signal_entry` and the process table.

use raven_libc_abi::errno::Errno;
use raven_libc_signal::kernel::FaultRecord;
use raven_libc_signal::{ENOMEM_ERR, SigKernel, SysKernel, raw, signal_entry};

use super::DISP;

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

/// Register the entry with the kernel for the calling thread.
///
/// `SIG_SETUP` is per thread, so this runs once per thread that can receive a signal. The
/// disposition table itself is always usable; only asynchronous delivery needs PG-7. A thread
/// with a mapped TLS block gets a dedicated absolute context page (K443); the builtin main-thread
/// TLS keeps the tail of its page.
pub(crate) fn ensure_entry() -> Result<(), Errno> {
    #[cfg(target_os = "none")]
    {
        let kernel = SysKernel::new();
        let tp = kernel.thread_pointer();
        // A thread with no TLS cannot run the libc entry (it reads its per-thread state through
        // this pointer). Do not register it; the process-directed signal then goes to a thread
        // that can handle it instead of taking this one down with a null TLS.
        if tp == 0 {
            return Err(ENOMEM_ERR);
        }
        if raw::with_state(tp, |state| state.ctx_area()) != 0 {
            return Ok(());
        }
        let area = raw::signal_context_area(tp).ok_or(ENOMEM_ERR)?;
        raw::set_ctx_area(tp, area.context());
        raw::set_entry_function(shell_signal_entry as *const () as usize);
        // The install path uses the contract crate's syscall number; the entry is the asm stub
        // and the context area is the one the kernel validates for this thread.
        // SAFETY: SIG_SETUP takes three scalar words; the kernel validates the executable entry
        // and the context window before storing either.
        let result = unsafe {
            raven_libc_sys::arch::syscall3(
                raven_abi::signal::SIG_SETUP,
                shell_signal_entry as *const () as usize,
                area.argument(),
                area.flags() as usize,
            )
        };
        raven_libc_sys::errno::ret(result).map_err(|error| Errno(error.0))?;
    }
    Ok(())
}
