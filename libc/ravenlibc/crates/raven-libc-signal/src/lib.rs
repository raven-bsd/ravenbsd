// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! ravenlibc signals: `sigset_t`, the disposition table, the interposed entry path,
//! `setjmp`/`sigsetjmp`, `ucontext` and the send/wait families.
//!
//! Area document `donor-map/09-threads-tls-signals.md` sections 3.7, 3.8, 4.1 and 4.8 own the
//! design; the twelve-step entry path is section 4.5 of ticket C58. The kernel only kicks
//! (PG-7); libc owns masks, dispositions, the frame and the alternate stack. Nothing here
//! allocates, formats or takes a lock on the entry path (rule S1).
//!
//! Outside this crate: the C headers (`signal.h`, `setjmp.h`, `ucontext.h` are C37/C38), the
//! shell exports (`raven-libc`), `pthread_sigmask` as an exported name and `pthread_kill`
//! (C57), POSIX timers (C44) and `strsignal` text (C40).

#![cfg_attr(not(test), no_std)]
#![allow(unexpected_cfgs)] // `raven_va_softfloat` is the documented build-profile cfg (H-20)
#![deny(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects,
        clippy::unreachable,
        clippy::undocumented_unsafe_blocks,
        clippy::missing_safety_doc,
        clippy::print_stdout,
        clippy::print_stderr,
        unsafe_code
    )
)]

pub mod action;
pub mod altstack;
pub mod context;
pub mod critical;
pub mod defaults;
pub mod deliver;
pub mod dispositions;
pub mod entry;
pub mod errors;
pub mod frame;
pub mod kernel;
pub mod legacy;
pub mod mask;
pub mod maskops;
pub mod names;
pub mod psignal;
pub mod safe_list;
pub mod send;
pub mod sig2str;
pub mod sigjmp;
pub mod sigset;
pub mod wait;

#[allow(unsafe_code)] // DR-39: the single audited raw module (asm, frame copies, TCB view)
pub mod raw;

#[cfg(test)]
pub mod host_ut;

pub use action::sigaction;
pub use altstack::{on_altstack, sigaltstack};
pub use context::{getcontext, makecontext_core, setcontext, swapcontext};
pub use critical::{SigThreadState, critical_enter, critical_exit, defer_enter, defer_exit};
pub use defaults::{DefaultAction, default_action};
pub use dispositions::{
    DispTable, Disposition, SA_NODEFER, SA_ONSTACK, SA_RESETHAND, SA_RESTART, SA_SIGINFO,
};
pub use entry::{EntryOutcome, run_direct, signal_entry};
pub use errors::{
    EAGAIN_ERR, EINTR_ERR, EINVAL_ERR, ENOMEM_ERR, ENOSYS_ERR, EPERM_ERR, ETIMEDOUT_ERR, errno,
};
pub use frame::{si_addr, si_code, si_pid, si_signo, si_uid, si_value};
pub use kernel::{FaultKind, SigCaps, SigKernel, SysKernel};
pub use mask::{block_all, block_app, current_mask, restore};
pub use maskops::{pthread_sigmask, sigpending, sigprocmask};
pub use names::{name_of, number_of};
pub use psignal::psignal_text;
pub use safe_list::is_async_signal_safe;
pub use send::{kill, killpg, raise, sigqueue};
pub use sig2str::{SIG2STR_MAX, sig2str, str2sig};
pub use sigjmp::{LONGJMP, SETJMP, longjmp, setjmp, siglongjmp, sigsetjmp};
pub use sigset::{
    ALL_MASK, APP_MASK, NSIG, SIGCANCEL, SIGRTMAX, SIGRTMIN, SIGSYNCCALL, SIGTIMER, SigSet,
};
pub use wait::{sigsuspend, sigtimedwait, sigwait, sigwaitinfo};
