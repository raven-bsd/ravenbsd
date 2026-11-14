// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getcontext`, `setcontext`, `makecontext` and `swapcontext` (CLEANROOM).
//!
//! musl has none of these; the NetBSD/FreeBSD versions call a kernel `getcontext(2)` or are tied
//! to the BSD `ucontext_t` (and two are BSD-4). This implementation is pure user space over the
//! Linux `ucontext_t` layout fixed by C36: `getcontext` saves callee-saved registers, `sp`, the
//! return address, the FP control word and the TCB mask; `setcontext` restores them and branches;
//! `makecontext` prepares a register image and the `__rl_start_context` stub; `swapcontext` saves
//! the old context and restores the new one in one asm sequence.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno;
use raven_libc_abi::structs::UcontextT;

use crate::errors::errno as make_errno;
use crate::frame;
use crate::raw;
use crate::sigset::APP_MASK;

/// The maximum number of integer arguments `makecontext_core` accepts.
pub const MAX_ARGS: usize = 6;

/// The calling thread's stack pointer.
#[must_use]
pub fn current_stack_pointer() -> usize {
    raw::current_stack_pointer()
}

/// Save the calling context; returns 0 now and after a later `setcontext`.
#[inline(always)]
pub fn getcontext(context: &mut UcontextT) -> Result<(), Errno> {
    *context = UcontextT::ZERO;
    let mask = crate::mask::current_mask(&crate::kernel::SysKernel);
    frame::set_uc_sigmask_word(context, mask);
    let result = raw::getcontext_save(context);
    if result == 0 {
        Ok(())
    } else {
        Err(make_errno(result))
    }
}

/// Restore a context; applies its signal mask and never returns.
#[inline(always)]
pub fn setcontext(context: &UcontextT) -> ! {
    let mask = frame::uc_sigmask_word(context) & APP_MASK;
    crate::mask::set_mask(&crate::kernel::SysKernel, mask);
    raw::setcontext_restore(context)
}

/// Save the current context in `old` and restore `new`; returns 0 when `old` is resumed.
#[inline(always)]
pub fn swapcontext(old: &mut UcontextT, new: &UcontextT) -> Result<(), Errno> {
    *old = UcontextT::ZERO;
    frame::set_uc_sigmask_word(old, crate::mask::current_mask(&crate::kernel::SysKernel));
    let result = raw::swapcontext_save(old, new);
    if result != 0 {
        return Err(make_errno(result));
    }
    let mask = frame::uc_sigmask_word(old) & APP_MASK;
    crate::mask::set_mask(&crate::kernel::SysKernel, mask);
    Ok(())
}

/// Prepare `context` to start `function(args...)` on its registered stack.
///
/// Up to six integer arguments go in the psABI argument registers. `argc > 6` returns `E2BIG`
/// (the C function is `void`, so the shell reports it as an abort, as ticket 4.8 requires).
/// When the function returns, `__rl_start_context` runs `uc_link` or calls `exit(0)`.
pub fn makecontext_core(
    context: &mut UcontextT,
    function: usize,
    args: &[usize],
) -> Result<(), Errno> {
    if args.len() > MAX_ARGS {
        return Err(make_errno(errno::E2BIG));
    }
    let stack = frame::uc_stack(context);
    let base = stack.ss_sp as usize;
    let size = stack.ss_size as usize;
    if base == 0 || size == 0 {
        return Err(make_errno(errno::EINVAL));
    }
    let top = base.wrapping_add(size) & !15usize;
    let mut machine = frame::mcontext_of(context);
    #[cfg(target_arch = "x86_64")]
    {
        // gregs: R8=0 R9=1 RDI=8 RSI=9 RDX=12 RCX=14 RSP=15 RIP=16; R12=4 R13=5.
        frame::set_ctx_greg(&mut machine, 15, top);
        frame::set_ctx_greg(&mut machine, 16, raw::start_context_address());
        frame::set_ctx_greg(&mut machine, 4, function);
        frame::set_ctx_greg(&mut machine, 5, frame::uc_link(context));
        for (index, arg) in args.iter().enumerate() {
            let register = match index {
                0 => 8,
                1 => 9,
                2 => 12,
                3 => 14,
                4 => 0,
                _ => 1,
            };
            frame::set_ctx_greg(&mut machine, register, *arg);
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // regs[0..30] at words 1..31; sp 32, pc 33; x19 = word 20, x20 = 21, x21 = 22.
        frame::set_ctx_greg(&mut machine, 32, top);
        frame::set_ctx_greg(&mut machine, 33, raw::start_context_address());
        frame::set_ctx_greg(&mut machine, 31, raw::start_context_address());
        frame::set_ctx_greg(&mut machine, 20, context as *mut UcontextT as usize);
        frame::set_ctx_greg(&mut machine, 21, function);
        frame::set_ctx_greg(&mut machine, 22, frame::uc_link(context));
        for (index, arg) in args.iter().enumerate() {
            frame::set_ctx_greg(&mut machine, index.wrapping_add(1), *arg);
        }
    }
    #[cfg(target_arch = "riscv64")]
    {
        // __gregs[0]=pc, [1]=ra, [2]=sp, [8..19]=s0..s11, [10..15]=a0..a5.
        frame::set_ctx_greg(&mut machine, 2, top);
        frame::set_ctx_greg(&mut machine, 0, raw::start_context_address());
        frame::set_ctx_greg(&mut machine, 1, raw::start_context_address());
        frame::set_ctx_greg(&mut machine, 8, context as *mut UcontextT as usize);
        frame::set_ctx_greg(&mut machine, 9, function);
        frame::set_ctx_greg(&mut machine, 18, frame::uc_link(context));
        for (index, arg) in args.iter().enumerate() {
            frame::set_ctx_greg(&mut machine, index.wrapping_add(10), *arg);
        }
    }
    raw::copy_mcontext_in(&machine, context);
    Ok(())
}

#[cfg(test)]
#[path = "context_ut.rs"]
mod context_ut;
