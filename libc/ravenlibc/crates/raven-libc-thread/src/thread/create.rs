// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/thread/pthread_create.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! `pthread_create` and `init_main_thread`.
//!
//! One mapping holds the guard, the stack, the static TLS block (with the `Pthread` and the DTV)
//! and the TSD array; `pthread_create` never calls `malloc` (A-thr-1, A-thr-8). The creator takes
//! the `ptc` reader lock, carves the mapping, copies the TLS image, fills the TCB, takes the
//! thread-list lock and only then calls the kernel. Linking happens before the kernel call; a
//! failure unlinks, undoes the count and unmaps.

use core::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_tls::consts::{MAX_TLS_MODULES, TARGET_ABI};
use raven_libc_tls::tcb::{pthread_to_tp, tp_to_pthread};
use raven_libc_tls::{Pthread, StaticLayout, TlsModuleIn, compute_static_layout, module_offsets};

use crate::kernel::{CreateArgs, ThreadKernel};
use crate::libc_state::LIBC;
use crate::limits::{
    EINVAL, ENOSYS, Errno, PTHREAD_CREATE_DETACHED, PTHREAD_KEYS_MAX, PTHREAD_STACK_MIN,
};
use crate::raw;
use crate::thread::attr::{self, Attr};
use crate::thread::{exit, list, stack};

/// The thread start function: takes the user argument and returns the join result.
pub type StartFn = fn(usize) -> usize;

/// The hand-over structure carved below the new stack top; word 0 is the trampoline the
/// assembly stub jumps through.
#[repr(C)]
pub struct StartArgs {
    /// `thread_start::<K>` monomorphised for the creating kernel.
    pub trampoline: usize,
    /// The user's `StartFn`.
    pub start: usize,
    /// The user's argument.
    pub arg: usize,
}

static MOD_IMAGE: [AtomicUsize; MAX_TLS_MODULES] = [const { AtomicUsize::new(0) }; MAX_TLS_MODULES];
static MOD_FILESZ: [AtomicUsize; MAX_TLS_MODULES] =
    [const { AtomicUsize::new(0) }; MAX_TLS_MODULES];
static MOD_MEMSZ: [AtomicUsize; MAX_TLS_MODULES] = [const { AtomicUsize::new(0) }; MAX_TLS_MODULES];
static MOD_ALIGN: [AtomicUsize; MAX_TLS_MODULES] = [const { AtomicUsize::new(0) }; MAX_TLS_MODULES];
static MOD_COUNT: AtomicUsize = AtomicUsize::new(0);
static TLS_TOTAL: AtomicUsize = AtomicUsize::new(0);
static TSD_SIZE: AtomicUsize = AtomicUsize::new(0);

/// Record the static modules so `pthread_create` can copy the same image into every stack.
fn record_modules(modules: &[TlsModuleIn]) -> Result<(), Errno> {
    if modules.len() > MAX_TLS_MODULES {
        return Err(EINVAL);
    }
    for (index, module) in modules.iter().enumerate() {
        if let (Some(image), Some(filesz), Some(memsz), Some(align)) = (
            MOD_IMAGE.get(index),
            MOD_FILESZ.get(index),
            MOD_MEMSZ.get(index),
            MOD_ALIGN.get(index),
        ) {
            image.store(module.image_addr, Ordering::Release);
            filesz.store(module.filesz, Ordering::Release);
            memsz.store(module.memsz, Ordering::Release);
            align.store(module.align, Ordering::Release);
        }
    }
    MOD_COUNT.store(modules.len(), Ordering::Release);
    Ok(())
}

/// The recorded module list.
#[must_use]
pub fn recorded_modules() -> [TlsModuleIn; MAX_TLS_MODULES] {
    let mut modules = [TlsModuleIn::new(0, 0, 0, 1); MAX_TLS_MODULES];
    let count = MOD_COUNT.load(Ordering::Acquire).min(MAX_TLS_MODULES);
    for index in 0..count {
        if let Some(slot) = modules.get_mut(index) {
            *slot = TlsModuleIn::new(
                MOD_IMAGE
                    .get(index)
                    .map_or(0, |slot| slot.load(Ordering::Acquire)),
                MOD_FILESZ
                    .get(index)
                    .map_or(0, |slot| slot.load(Ordering::Acquire)),
                MOD_MEMSZ
                    .get(index)
                    .map_or(0, |slot| slot.load(Ordering::Acquire)),
                MOD_ALIGN
                    .get(index)
                    .map_or(1, |slot| slot.load(Ordering::Acquire)),
            );
        }
    }
    modules
}

/// The recorded module count.
#[must_use]
pub fn module_count() -> usize {
    MOD_COUNT.load(Ordering::Acquire)
}

/// `init_main_thread`: build the main TCB, copy the TLS image and install the thread pointer.
///
/// Called once by the shell after the auxiliary vector is parsed (4.12). A failure here is fatal
/// for the caller; the error is returned instead of aborting so the shell can print a diagnostic.
///
/// # Errors
///
/// `ENOSYS` when the kernel has no settable thread pointer (PG-1), `ENOEXEC`/`ENOMEM` for a bad
/// `PT_TLS`, `EAGAIN` when the mapping fails.
pub fn init_main_thread<K: ThreadKernel>(
    kernel: &K,
    modules: &[TlsModuleIn],
    stack_base: usize,
    stack_size: usize,
) -> Result<usize, Errno> {
    if !kernel.caps().tls {
        return Err(ENOSYS);
    }
    let tcb_size = core::mem::size_of::<Pthread>();
    let layout = compute_static_layout(modules, tcb_size).map_err(|error| Errno(error.errno()))?;
    record_modules(modules)?;
    TLS_TOTAL.store(layout.total, Ordering::Release);
    TSD_SIZE.store(
        PTHREAD_KEYS_MAX.saturating_mul(core::mem::size_of::<usize>()),
        Ordering::Release,
    );
    let builtin = raw::builtin_tls();
    let mapped = if layout.total <= raven_libc_tls::consts::BUILTIN_TLS_SIZE {
        builtin
    } else {
        kernel.map(layout.total)?
    };
    let plan = raw::init_tls(mapped, &layout, modules)?;
    let tcb = plan.tcb;
    let tid = if kernel.caps().tid {
        kernel.tid_self().unwrap_or_else(|_| LIBC.next_tid())
    } else {
        LIBC.next_tid()
    };
    let pthread = raw::tcb_mut(tcb);
    pthread.self_ptr = tcb;
    pthread.tid.store(tid, Ordering::Release);
    pthread
        .detach_state
        .store(raven_libc_tls::tcb::DT_JOINABLE, Ordering::Release);
    pthread.errno_val = 0;
    pthread.sigmask.store(0, Ordering::Release);
    pthread
        .cancel
        .store(crate::limits::CANCEL_ENABLE, Ordering::Release);
    pthread.stack_base = stack_base;
    pthread.stack_size = stack_size;
    pthread.guard_size = 0;
    pthread.map_base = if mapped == builtin { 0 } else { mapped };
    pthread.map_size = if mapped == builtin { 0 } else { layout.total };
    pthread.robust_head = raw::robust_head_addr(pthread);
    raw::set_tsd_base(pthread, crate::thread::tsd::main_tsd_address());
    pthread.canary = 0;
    pthread.handle = 0;
    raw::set_links(pthread, tcb, tcb);
    kernel.set_thread_pointer(pthread_to_tp(tcb))?;
    LIBC.reset(tcb);
    stack::reset();
    list::reset_for_fork_child();
    crate::fork::reset_registry();
    Ok(tcb)
}

/// `pthread_create`.
///
/// # Errors
///
/// `EINVAL` for a bad attribute, `EAGAIN` for a resource failure, `ENOSYS` before the main TCB
/// exists or when the kernel cannot start a thread.
pub fn create<K: ThreadKernel>(
    kernel: &K,
    attr: Option<&Attr>,
    start: StartFn,
    arg: usize,
) -> Result<usize, Errno> {
    if MOD_COUNT.load(Ordering::Acquire) == 0 {
        return Err(ENOSYS);
    }
    let default_attr;
    let attr = match attr {
        Some(attr) => attr,
        None => {
            default_attr = attr::init();
            &default_attr
        }
    };
    if attr.stacksize() < PTHREAD_STACK_MIN {
        return Err(EINVAL);
    }
    if attr.detach_state() != crate::limits::PTHREAD_CREATE_JOINABLE
        && attr.detach_state() != PTHREAD_CREATE_DETACHED
    {
        return Err(EINVAL);
    }
    let modules = recorded_modules();
    let count = module_count();
    let tls_size = TLS_TOTAL.load(Ordering::Acquire);
    let tsd_size = TSD_SIZE.load(Ordering::Acquire);
    crate::sync::ptc::acquire_ptc(kernel);
    stack::reap(kernel);
    let user_stack = if attr.stackaddr() != 0 {
        Some((
            attr.stackaddr().wrapping_sub(attr.stacksize()),
            attr.stacksize(),
        ))
    } else {
        None
    };
    let allocation = match stack::allocate(
        kernel,
        attr.stacksize(),
        attr.guardsize(),
        user_stack,
        tls_size,
        tsd_size,
    ) {
        Ok(allocation) => allocation,
        Err(error) => {
            crate::sync::ptc::release_ptc(kernel);
            return Err(error);
        }
    };
    let layout = match compute_static_layout(
        modules.get(..count).unwrap_or(&[]),
        core::mem::size_of::<Pthread>(),
    ) {
        Ok(layout) => layout,
        Err(error) => {
            stack::abort(kernel, allocation);
            crate::sync::ptc::release_ptc(kernel);
            return Err(Errno(error.errno()));
        }
    };
    let plan = match raw::init_tls(
        allocation.tls_base,
        &layout,
        modules.get(..count).unwrap_or(&[]),
    ) {
        Ok(plan) => plan,
        Err(error) => {
            stack::abort(kernel, allocation);
            crate::sync::ptc::release_ptc(kernel);
            return Err(error);
        }
    };
    let tcb = plan.tcb;
    let tid = LIBC.next_tid();
    let parent = crate::thread::current_pthread(kernel);
    let parent_sigmask = parent.sigmask.load(Ordering::Acquire);
    let parent_canary = parent.canary;
    let args_address = allocation
        .stack_top
        .wrapping_sub(core::mem::size_of::<StartArgs>())
        & !15;
    let pthread = raw::tcb_mut(tcb);
    pthread.self_ptr = tcb;
    pthread.tid.store(tid, Ordering::Release);
    pthread.detach_state.store(
        if attr.detach_state() == PTHREAD_CREATE_DETACHED {
            raven_libc_tls::tcb::DT_DETACHED
        } else {
            raven_libc_tls::tcb::DT_JOINABLE
        },
        Ordering::Release,
    );
    pthread.sigmask.store(parent_sigmask, Ordering::Release);
    pthread
        .cancel
        .store(crate::limits::CANCEL_ENABLE, Ordering::Release);
    pthread.canary = parent_canary;
    pthread.stack_base = allocation.stack_limit;
    pthread.stack_size = allocation.stack_top.wrapping_sub(allocation.stack_limit);
    pthread.guard_size = allocation.guard;
    pthread.map_base = allocation.map_base;
    pthread.map_size = allocation.map_size;
    pthread.startfn = start as usize;
    pthread.startarg = arg;
    pthread.result = 0;
    pthread.robust_head = raw::robust_head_addr(pthread);
    raw::set_tsd_base(pthread, allocation.tsd_base);
    raw::zero_bytes(allocation.tsd_base, tsd_size);
    pthread.handle = 0;
    raw::write_start_args(
        args_address,
        kernel.start_trampoline() as usize,
        start as usize,
        arg,
    );
    list::lock(kernel);
    let previous = crate::thread::current_pthread(kernel);
    let previous_address = previous as *const Pthread as usize;
    let previous_prev = raw::prev_thread(previous) as *const Pthread as usize;
    let next = raw::next_thread(previous);
    let next_address = next as *const Pthread as usize;
    let next_next = raw::next_thread(next) as *const Pthread as usize;
    raw::set_links(pthread, previous_address, next_address);
    raw::set_links(next, previous_address, next_next);
    raw::set_links(previous, previous_prev, tcb);
    LIBC.add_thread();
    sched_point_before_create(kernel);
    let create_args = CreateArgs {
        entry: raw::entry::entry_address(),
        stack_top: allocation.stack_top,
        arg: args_address,
        tp: pthread_to_tp(tcb),
        clear_tid_addr: tcb + core::mem::offset_of!(Pthread, tid),
    };
    let result = kernel.create(&create_args);
    match result {
        Ok(handle) => {
            raw::tcb_mut(tcb).handle = handle;
            list::unlock(kernel);
            crate::sync::ptc::release_ptc(kernel);
            Ok(tcb)
        }
        Err(error) => {
            raw::set_links(previous, previous_prev, next_address);
            raw::set_links(next, previous_address, next_next);
            LIBC.remove_thread();
            list::unlock(kernel);
            crate::sync::ptc::release_ptc(kernel);
            if allocation.map_base != 0 {
                let _ = kernel.unmap(allocation.map_base, allocation.map_size);
            }
            Err(error)
        }
    }
}

fn sched_point_before_create<K: ThreadKernel>(kernel: &K) {
    crate::sched_point!(kernel, "create_before_kernel");
}

/// `thread_start`: the Rust body behind the assembly entry stub.
pub fn thread_start<K: ThreadKernel>(kernel: &K, args_address: usize) -> ! {
    crate::sched_point!(kernel, "thread_start");
    let args = raw::start_args(args_address);
    let start = args.start;
    let argument = args.arg;
    let result = raw::call_start_safe(start, argument);
    exit::exit(kernel, result)
}

/// The thread pointer of a TCB (for tests).
#[must_use]
pub fn tp_of(tcb: usize) -> usize {
    pthread_to_tp(tcb)
}

/// The TCB of a thread pointer (for tests).
#[must_use]
pub fn tcb_of(tp: usize) -> usize {
    tp_to_pthread(tp)
}

/// The TLS layout used by new threads (tests).
#[must_use]
pub fn tls_layout() -> Option<StaticLayout> {
    let count = module_count();
    if count == 0 {
        return None;
    }
    compute_static_layout(
        recorded_modules().get(..count).unwrap_or(&[]),
        core::mem::size_of::<Pthread>(),
    )
    .ok()
}

/// The TPOFF bases of the recorded modules (the loader test uses them).
#[must_use]
pub fn tls_offsets() -> Option<raven_libc_tls::ModuleOffsets> {
    let count = module_count();
    if count == 0 {
        return None;
    }
    module_offsets(recorded_modules().get(..count).unwrap_or(&[])).ok()
}

const _: () = assert!(TARGET_ABI.dtp_offset() < 0x1000);

#[cfg(test)]
#[path = "create_ut.rs"]
mod tests;
