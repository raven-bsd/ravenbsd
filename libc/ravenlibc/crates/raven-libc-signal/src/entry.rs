// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The twelve-step entry path (ticket 4.5, CLEANROOM from `09` 4.1 and `06` A-sol-19/22).
//!
//! Step 1 is the per-architecture asm stub (`raw/entry_*`), step 12 is `SIGRETURN`. The body here
//! saves `errno`, COPY-INs the kernel area into a stack `ucontext_t` (freeing the area for a
//! nested kick), handles the `CANCEL`/`SYNCCALL` bits, computes the deliverable set, honours the
//! critical-section deferral, applies the default action or calls the handler on the selected
//! stack, restores the mask and COPY-OUTs the possibly modified machine context.

use raven_libc_abi::generated::signal;
use raven_libc_abi::structs::{SiginfoT, StackT, UcontextT};

use crate::defaults::DefaultAction;
use crate::deliver::{self, Decision};
use crate::dispositions::{Disposition, SA_ONSTACK, SA_RESETHAND, SA_SIGINFO};
use crate::frame;
use crate::kernel::{FaultRecord, KICK_CANCEL, KICK_SYNCCALL};
use crate::mask;
use crate::raw;
use crate::sigset::APP_MASK;
use crate::{DispTable, SigKernel};

/// What one entry did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntryOutcome {
    /// Number of handlers that ran.
    pub delivered: u32,
    /// A kick with nothing deliverable.
    pub spurious: bool,
}

/// The production body invoked from the asm stub: process, then `SIGRETURN` (never returns).
pub fn signal_entry<K: SigKernel>(
    kernel: &K,
    table: &DispTable,
    bits: u32,
    fault: Option<FaultRecord>,
) -> ! {
    process(kernel, table, bits, fault, true, true);
    if !kernel.caps().sigreturn && !kernel.caps().entry {
        // No PG-7: this body is not reachable from a kernel entry in production. Terminating is
        // the only honest action; the host fake implements `sigreturn` by unwinding.
        kernel.terminate(signal::SIGSEGV, true);
    }
    kernel.sigreturn()
}

/// The libc-only direct entry used by `raise` and self `pthread_kill` (PG-7 absent fallback).
pub fn run_direct<K: SigKernel>(
    kernel: &K,
    table: &DispTable,
    bits: u32,
    fault: Option<FaultRecord>,
) -> EntryOutcome {
    process(kernel, table, bits, fault, false, false)
}

fn process<K: SigKernel>(
    kernel: &K,
    table: &DispTable,
    bits: u32,
    fault: Option<FaultRecord>,
    copy_out: bool,
    from_kernel: bool,
) -> EntryOutcome {
    let tp = kernel.thread_pointer();
    let saved_errno = raven_libc_sys::errno::errno();
    raw::with_state(tp, |state| {
        state.set_saved_errno(saved_errno);
    });
    // Step 2: COPY-IN. The area is free from this point; a nested kick reuses it. A direct
    // entry (`raise`) has no kernel area: it describes the current stack instead.
    let mut context = UcontextT::ZERO;
    if from_kernel {
        raw::copy_ctx_in(tp, &mut context);
    } else {
        let mut machine = raven_libc_abi::structs::McontextT::ZERO;
        frame::set_ctx_sp(&mut machine, raw::current_stack_pointer());
        raw::copy_mcontext_in(&machine, &mut context);
    }
    let mask = raw::mask_load(tp);
    frame::set_uc_sigmask_word(&mut context, mask);
    let (alt_base, alt_size, alt_flags) = raw::altstack_get(tp);
    frame::set_uc_stack(
        &mut context,
        &StackT {
            ss_sp: alt_base as *mut core::ffi::c_void,
            ss_flags: alt_flags,
            ss_size: alt_size as u64,
        },
    );
    // Step 3: CANCEL may rewrite `pc` inside the `syscall_cp` window.
    if bits & KICK_CANCEL != 0 {
        let mut pc = frame::ctx_pc(&raw::read_mcontext(&context));
        if raw::call_cancel_hook(&mut pc, bits) {
            let mut machine = raw::read_mcontext(&context);
            frame::set_ctx_pc(&mut machine, pc);
            raw::copy_mcontext_in(&machine, &mut context);
        }
    }
    // Step 4: SYNCCALL.
    if bits & KICK_SYNCCALL != 0 {
        raw::call_synccall_hook();
    }
    let mut fault_state = fault.and_then(|record| deliver::fault_signal(&record));
    let mut delivered = 0u32;
    let mut spurious = true;

    loop {
        // Step 5: the deliverable set; a fault ignores the mask.
        let mask_now = raw::mask_load(tp);
        let mut pending = raw::pending_load(tp);
        let mut info: Option<SiginfoT> = None;
        let is_fault = fault_state.is_some();
        if let Some((sig, record)) = fault_state {
            pending |= crate::critical::signal_bit(sig);
            info = Some(record);
        } else {
            let allowed = !mask_now & APP_MASK;
            if let Some((sig, record)) = kernel.take_process_pending(allowed) {
                pending |= crate::critical::signal_bit(sig);
                info = Some(record);
            }
        }
        let ready = if is_fault {
            pending
        } else {
            deliver::deliverable(pending, mask_now)
        };
        if ready == 0 {
            break;
        }
        let sig = deliver::lowest(ready);
        if sig == 0 {
            break;
        }
        // Step 6: defer inside a critical section; never call the handler from the exit code.
        if raw::with_state(tp, |state| state.critical()) > 0 {
            let record = info.unwrap_or_else(|| crate::frame::user_siginfo(sig, signal::SI_USER));
            crate::critical::defer_signal(kernel, sig, &record, mask_now);
            frame::set_uc_sigmask_word(&mut context, mask_now | APP_MASK);
            break;
        }
        spurious = false;
        if is_fault {
            fault_state = None;
        } else {
            raw::pending_and(tp, !crate::critical::signal_bit(sig));
        }
        let record = info.unwrap_or_else(|| {
            let saved = raw::load_pending_info(tp);
            if saved.si_signo == sig {
                saved
            } else {
                crate::frame::user_siginfo(sig, signal::SI_USER)
            }
        });
        let disposition = table.read(sig);
        // Step 8: IGN, DFL or handler.
        match deliver::decide(disposition.handler, sig, false, is_fault) {
            Decision::Nothing | Decision::Ignore => continue,
            Decision::Defer => break,
            Decision::Default(action) => match action {
                DefaultAction::Ignore | DefaultAction::Continue => continue,
                DefaultAction::Stop => {
                    raw::call_stop_hook(sig);
                    continue;
                }
                DefaultAction::Terminate => kernel.terminate(sig, false),
                DefaultAction::Core => kernel.terminate(sig, true),
            },
            Decision::Handler(address) => {
                // Step 9: stack, mask, SA_RESETHAND before the handler.
                let old = raw::mask_load(tp);
                frame::set_uc_sigmask_word(&mut context, old);
                let mut handler_context = context;
                let new_mask = mask::handler_mask(old, disposition.mask, sig, disposition.flags);
                if disposition.flags & SA_RESETHAND != 0 {
                    table.write(sig, Disposition::DEFAULT);
                }
                raw::mask_store(tp, new_mask);
                let interrupted_sp = if from_kernel {
                    frame::ctx_sp(&raw::read_mcontext(&context))
                } else {
                    raw::current_stack_pointer()
                };
                let stack = handler_stack(
                    alt_base,
                    alt_size,
                    alt_flags,
                    disposition.flags,
                    interrupted_sp,
                );
                // Step 10: call through the asm helper (16-byte aligned at the call).
                let arg1 = sig as usize;
                let arg2 = if disposition.flags & SA_SIGINFO != 0 {
                    (&record as *const SiginfoT) as usize
                } else {
                    0
                };
                let arg3 = if disposition.flags & SA_SIGINFO != 0 {
                    (&mut handler_context as *mut UcontextT) as usize
                } else {
                    0
                };
                let _ = raw::call_handler(stack, address, arg1, arg2, arg3);
                // Step 11: the handler may have edited `uc_sigmask` and `uc_mcontext`.
                context = handler_context;
                raw::mask_store(tp, frame::uc_sigmask_word(&context) & APP_MASK);
                delivered = delivered.saturating_add(1);
            }
        }
    }
    // Step 12: errno and COPY-OUT.
    raven_libc_sys::errno::set_errno(saved_errno);
    raw::with_state(tp, |state| state.set_saved_errno(saved_errno));
    if copy_out {
        raw::copy_ctx_out(tp, &context);
    }
    EntryOutcome {
        delivered,
        spurious: spurious && delivered == 0,
    }
}

/// Choose the handler stack: the alternate stack when asked and not already on it, else the
/// interrupted stack below the x86-64 red zone.
fn handler_stack(
    alt_base: usize,
    alt_size: usize,
    alt_flags: i32,
    flags: u32,
    interrupted_sp: usize,
) -> usize {
    let enabled = alt_flags & crate::altstack::SS_DISABLE == 0 && alt_size != 0;
    let inside =
        enabled && interrupted_sp >= alt_base && interrupted_sp < alt_base.wrapping_add(alt_size);
    if flags & SA_ONSTACK != 0 && enabled && !inside {
        return alt_base.wrapping_add(alt_size) & !15usize;
    }
    #[cfg(target_arch = "x86_64")]
    {
        interrupted_sp.saturating_sub(128)
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        interrupted_sp.saturating_sub(16)
    }
}

#[cfg(test)]
#[path = "entry_ut.rs"]
mod entry_ut;
