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
        if raw::with_state(tp, |state| state.ctx_area()) != 0 {
            return Ok(());
        }
        let area = raw::signal_context_area(tp).ok_or(ENOMEM_ERR)?;
        raw::set_ctx_area(tp, area.context());
        raw::set_entry_function(shell_signal_entry as *const () as usize);
        // K548 rev355: the kernel must never receive an entry that is not the asm stub. Report
        // the value and its first four bytes at registration so a symbol/runtime mismatch is a
        // visible fact, then (after this diagnostic run) refuse it.
        let entry = raw::sigentry_address();
        // SAFETY: the entry is the package's own asm symbol, so the first four bytes are mapped.
        let prologue = unsafe { core::ptr::read_unaligned(entry as *const u32) }.to_le_bytes();
        report_entry(entry, prologue);
        // The install path uses the contract crate's syscall number; the entry is the asm stub
        // and the context area is the one the kernel validates for this thread.
        // SAFETY: SIG_SETUP takes three scalar words; the kernel validates the executable entry
        // and the context window before storing either.
        let result = unsafe {
            raven_libc_sys::arch::syscall3(
                raven_abi::signal::SIG_SETUP,
                entry,
                area.argument(),
                area.flags() as usize,
            )
        };
        raven_libc_sys::errno::ret(result).map_err(|error| Errno(error.0))?;
    }
    Ok(())
}

/// Report the registered `SIG_SETUP` entry and its first four bytes on standard error.
///
/// The entry is the package's own `__raven_sigentry` asm symbol; the kernel must see its
/// `and $-16,%rsp` prologue on x86-64. A mismatch is reported loudly before SIG_SETUP.
#[cfg(target_os = "none")]
fn report_entry(entry: usize, bytes: [u8; 4]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = *b"sigentry: __raven_sigentry=0x0000000000000000 bytes=00 00 00 00\n";
    let mut at = 29;
    let mut shift = 60;
    loop {
        out[at] = HEX[((entry >> shift) & 0xf) as usize];
        at += 1;
        if shift == 0 {
            break;
        }
        shift -= 4;
    }
    let mut at = 52;
    for (index, byte) in bytes.iter().enumerate() {
        out[at] = HEX[(byte >> 4) as usize];
        out[at + 1] = HEX[(byte & 0xf) as usize];
        at += 2;
        if index < 3 {
            out[at] = b' ';
            at += 1;
        }
    }
    let platform = crate::ffi::platform::global();
    let mut written = 0usize;
    while let Some(rest) = out.get(written..) {
        match platform.write(2, rest) {
            Ok(0) | Err(_) => break,
            Ok(count) => written = written.saturating_add(count),
        }
    }
}
