// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Thread-pointer accessors.
//!
//! x86-64 reads the self pointer from `%fs:0` and needs `THREAD_SET_TLS` (PG-1) to change the
//! base; until the kernel lands it, [`set_thread_pointer`] returns `ENOSYS`. AArch64 and RISC-V
//! write their user-writable register directly; the kernel must save and restore it per thread
//! (PG-1), so multi-thread on-target tests are marked `# needs PG-1`.

#[cfg(feature = "host-fake")]
use core::sync::atomic::{AtomicUsize, Ordering};

use raven_abi::errno::Errno;

/// Return the calling thread's thread pointer.
#[inline]
pub fn thread_pointer() -> *mut u8 {
    #[cfg(feature = "host-fake")]
    {
        FAKE_TP.load(Ordering::Relaxed) as *mut u8
    }
    #[cfg(all(target_arch = "x86_64", not(feature = "host-fake")))]
    {
        let tp: *mut u8;
        // SAFETY: reading `fs:0` is the x86-64 self-pointer convention and does not access
        // memory outside the current thread's segment base.
        unsafe {
            core::arch::asm!(
                "mov {}, qword ptr fs:[0]",
                out(reg) tp,
                options(nostack, nomem, preserves_flags),
            );
        }
        tp
    }
    #[cfg(all(target_arch = "aarch64", not(feature = "host-fake")))]
    {
        let tp: *mut u8;
        // SAFETY: reading `TPIDR_EL0` is side-effect free.
        unsafe {
            core::arch::asm!(
                "mrs {}, tpidr_el0",
                out(reg) tp,
                options(nostack, nomem, preserves_flags),
            );
        }
        tp
    }
    #[cfg(all(target_arch = "riscv64", not(feature = "host-fake")))]
    {
        let tp: *mut u8;
        // SAFETY: reading `tp` is side-effect free.
        unsafe {
            core::arch::asm!(
                "mv {}, tp",
                out(reg) tp,
                options(nostack, nomem, preserves_flags),
            );
        }
        tp
    }
}

/// Set the calling thread's thread pointer.
///
/// On x86-64 this is PG-1 and returns `ENOSYS` until the kernel provides `THREAD_SET_TLS`.
///
/// # Safety
///
/// `tp` must point to a valid thread control block that outlives the thread's use of TLS.
pub unsafe fn set_thread_pointer(tp: *mut u8) -> Result<(), Errno> {
    #[cfg(feature = "host-fake")]
    {
        FAKE_TP.store(tp as usize, Ordering::Relaxed);
        Ok(())
    }
    #[cfg(all(target_arch = "x86_64", not(feature = "host-fake")))]
    {
        let _ = tp;
        Err(Errno::ENOSYS)
    }
    #[cfg(all(target_arch = "aarch64", not(feature = "host-fake")))]
    {
        // SAFETY: `TPIDR_EL0` is user-writable at EL0; the caller guarantees `tp` is a valid
        // thread control block for this thread. The kernel must save and restore it per thread
        // (PG-1); until then multi-thread on-target use is not supported.
        unsafe {
            core::arch::asm!(
                "msr tpidr_el0, {}",
                in(reg) tp,
                options(nostack, preserves_flags),
            );
        }
        Ok(())
    }
    #[cfg(all(target_arch = "riscv64", not(feature = "host-fake")))]
    {
        // SAFETY: `tp` is a user-writable register; the caller guarantees `tp` is a valid thread
        // control block. The kernel must save and restore it per thread (PG-1).
        unsafe {
            core::arch::asm!(
                "mv tp, {}",
                in(reg) tp,
                options(nostack, preserves_flags),
            );
        }
        Ok(())
    }
}

#[cfg(feature = "host-fake")]
static FAKE_TP: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
#[path = "tp_ut.rs"]
mod tests;
