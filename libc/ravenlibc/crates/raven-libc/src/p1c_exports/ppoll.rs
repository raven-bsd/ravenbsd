// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ppoll`.

use super::*;
use core::mem::size_of;
#[cfg(target_os = "none")]
use raven_libc_signal::kernel::{KICK_CHECK, SysKernel};
#[cfg(target_os = "none")]
use raven_libc_signal::{APP_MASK, SigKernel, SigSet, raw, run_direct};

/// True when `[address, address + length)` is inside the user address window.
fn user_range(address: usize, length: usize) -> bool {
    raven_libc_platform::poll::validate_user_range(address as u64, length as u64).is_ok()
}

/// Poll with the caller's signal mask for the duration of the wait.
///
/// The kernel's `EVQ_WAIT` has no mask argument (PG-26), so the mask is installed in the thread
/// control block before the wait and the previous mask is restored before returning. A real
/// signal queues a sticky kick, which makes the wait return `EINTR`; the signal entry then runs
/// here with the temporary mask so the handler sees the unblocked set. A mask change alone does
/// **not** kick, so an idle wait still sleeps to its timeout. This is the `sigsuspend` pattern
/// (`raven-libc-signal` `wait.rs`) applied to the poll wait.
///
/// The kernel entry is only present in the freestanding shell; the host test build keeps the
/// platform's no-mask call so the wrappers remain testable.
fn poll_with_mask(
    records: &mut [Pollfd],
    timeout: Option<&Timespec>,
    mask: Option<SigsetT>,
) -> Result<usize, raven_libc_platform::Errno> {
    #[cfg(target_os = "none")]
    if let Some(value) = mask {
        let set = SigSet(value.bytes);
        let kernel = SysKernel::new();
        let thread_pointer = kernel.thread_pointer();
        let previous = raw::mask_swap(thread_pointer, set.app_word() & APP_MASK);
        let result = raven_libc_platform::poll::ppoll(global(), records, timeout, None);
        if matches!(&result, Err(raven_libc_platform::Errno::EINTR)) {
            // The temporary mask is still installed: the handler runs with the unblocked set, and
            // a spurious kick with nothing deliverable is a no-op.
            let _ = run_direct(&kernel, &crate::signal::DISP, KICK_CHECK, None);
        }
        raw::mask_store(thread_pointer, previous);
        return result;
    }
    #[cfg(not(target_os = "none"))]
    let _ = mask;
    raven_libc_platform::poll::ppoll(global(), records, timeout, None)
}

/// Poll with an atomic signal mask.
///
/// # Safety
/// `fds` has `nfds` writable records; `timeout`/`sigmask` are NULL or valid.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ppoll(
    fds: *mut Pollfd,
    nfds: u64,
    timeout: *const Timespec,
    sigmask: *const SigsetT,
) -> c_int {
    let count = usize::try_from(nfds).unwrap_or(usize::MAX);
    // FreeBSD `kern_poll`/`kern_poll_kfds` copies the record array and the timespec in, and the
    // sigmask through `kern_sigprocmask`; each bad pointer is EFAULT. This shell reads them in
    // user mode, so every pointer outside the user window is rejected before it forms a slice or
    // dereferences the timespec/sigmask (the donor `(struct pollfd *)-1` case).
    let records_valid = count == 0
        || count
            .checked_mul(size_of::<Pollfd>())
            .is_some_and(|bytes| user_range(fds as usize, bytes));
    if !records_valid
        || (!timeout.is_null() && !user_range(timeout as usize, size_of::<Timespec>()))
        || (!sigmask.is_null() && !user_range(sigmask as usize, size_of::<SigsetT>()))
    {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    let records = if count == 0 {
        &mut []
    } else {
        // SAFETY: `count` records at `fds` lie inside the user window (checked above) and the
        // caller supplied them as writable pollfd values; an unmapped page inside the window is
        // the kernel copyin gap, not something user mode can probe without faulting.
        unsafe { core::slice::from_raw_parts_mut(fds, count) }
    };
    // SAFETY: NULL is `None`; otherwise the timespec address passed the user-window check above.
    let timeout = unsafe { timeout.as_ref() };
    // SAFETY: same contract as `timeout`.
    let mask = unsafe { sigmask.as_ref() }.copied();
    ret::platform_int(
        poll_with_mask(records, timeout, mask)
            .map(|ready| c_int::try_from(ready).unwrap_or(c_int::MAX)),
    )
}
