// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `THREAD_CREATE`, `THREAD_EXIT` and the PG-7 signal entry/return state.

use core::mem::size_of;
use core::sync::atomic::Ordering;

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::signal::{CONTEXT_AREA_LIMIT, ENTRY_SP, FaultRecord, MContext};
use raven_abi::types::{Handle, Rights, prot};
use raven_usercopy::{UserSlice, is_user_address};

use crate::process::{self, ProcessState, handle::Object};
use crate::sched;
use crate::sched::thread::ThreadState;
use crate::syscall::SyscallFrame;
use crate::usercopy::{copy_from_user, copy_to_user};

/// `thread_create(proc, entry, stack_top, arg, flags) -> Thread handle`.
///
/// `proc` is any process handle with `Rights::MAP`; the entry must be inside an executable VMA of
/// that process and the initial stack pointer must lie in a writable VMA. All flag bits are
/// reserved and must be zero.
pub fn create(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(frame.rdi as u32);
    let target = {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::MAP)? {
            Object::Process(target) => Arc::clone(target),
            _ => return Err(Errno::EBADF),
        }
    };

    if frame.r8 != 0 {
        return Err(Errno::EINVAL);
    }
    let entry = frame.rsi;
    let stack_top = frame.rdx;
    if !is_user_address(entry) {
        return Err(Errno::EFAULT);
    }
    if stack_top & 0xf != 0 || !is_user_address(stack_top.saturating_sub(1)) {
        return Err(Errno::EFAULT);
    }

    // Admission is atomic with respect to process exit: the state lock is held from the
    // terminal check until the thread is enqueued.  Exit takes the same state lock first, so a
    // thread is either admitted into a live process (and then terminated by the process-exit
    // filter) or rejected with ESRCH; a terminal process can never be resurrected.
    let mut state = target.state.lock();
    if *state == ProcessState::Exited {
        return Err(Errno::ESRCH);
    }
    {
        let address_space = target.address_space.lock();
        let address_space = address_space.as_ref().ok_or(Errno::ESRCH)?;
        let entry_vma = address_space.vmas().find(entry).ok_or(Errno::EFAULT)?;
        if entry_vma.prot() & (prot::EXEC as u8) == 0 {
            return Err(Errno::EACCES);
        }
        let stack_vma = address_space
            .vmas()
            .find(stack_top.saturating_sub(1))
            .ok_or(Errno::EFAULT)?;
        if stack_vma.prot() & (prot::WRITE as u8) == 0 {
            return Err(Errno::EACCES);
        }
    }

    let thread = sched::new_user_thread(&target, entry, stack_top, frame.r10)?;
    let rights = Rights::SIGNAL.union(Rights::DUP);
    let raw = match caller
        .handles
        .lock()
        .insert(Object::Thread(Arc::clone(&thread)), rights)
    {
        Ok(raw) => raw,
        Err(error) => {
            // Roll the thread back: its stack is freed by the deferred reaper.
            sched::reap_thread(thread);
            return Err(error);
        }
    };
    if !sched::enqueue(Arc::clone(&thread)) {
        let _ = crate::event::close_handle(&caller, raw);
        sched::reap_thread(thread);
        return Err(Errno::EAGAIN);
    }
    if *state == ProcessState::Created {
        *state = ProcessState::Running;
    }
    Ok(raw.0 as usize)
}

/// `thread_exit(code) -> !`.
pub fn exit(frame: &SyscallFrame) -> ! {
    sched::exit_thread(frame.rdi as i32)
}

/// `thread_kick(thread, bits) -> 0` (raw 0x81).
///
/// Sets the sticky kick word and wakes an interruptible blocked target. A
/// running thread keeps the word for its next interruptible wait; `bits == 0`
/// is a successful no-op.
pub fn kick(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let bits = u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let thread = if handle == Handle::NONE {
        sched::current_thread().ok_or(Errno::ESRCH)?
    } else {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::SIGNAL) {
            Ok(Object::Thread(thread)) => Arc::clone(thread),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    if thread.state() == ThreadState::Dead {
        return Err(Errno::ESRCH);
    }
    if bits == 0 {
        return Ok(0);
    }
    thread
        .kick_bits
        .fetch_or(bits, core::sync::atomic::Ordering::AcqRel);
    if thread
        .interruptible
        .load(core::sync::atomic::Ordering::Acquire)
    {
        let token = thread
            .block_token
            .load(core::sync::atomic::Ordering::SeqCst);
        let token = (token != 0).then_some(token);
        let _ = sched::wake(thread, token, Errno::EINTR.to_ret());
    }
    Ok(0)
}

/// `thread_set_tls(thread, tp) -> 0`.
///
/// `Handle::NONE` targets the calling thread. A non-zero base must be a
/// canonical user address; zero clears it. The value is per-thread and is
/// installed by the next context switch to that thread.
pub fn set_tls(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let handle = Handle(u32::try_from(frame.rdi).map_err(|_| Errno::EBADF)?);
    let thread = if handle == Handle::NONE {
        sched::current_thread().ok_or(Errno::ESRCH)?
    } else {
        let handles = caller.handles.lock();
        match handles.get(handle, Rights::SIGNAL) {
            Ok(Object::Thread(thread)) => Arc::clone(thread),
            Ok(_) => return Err(Errno::EBADF),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        }
    };
    if !thread.user {
        return Err(Errno::EINVAL);
    }
    if thread.state() == ThreadState::Dead {
        return Err(Errno::ESRCH);
    }
    let tp = frame.rsi;
    if tp != 0 && !is_user_address(tp) {
        return Err(Errno::EFAULT);
    }
    thread.set_tls_base(tp);
    Ok(0)
}

/// One pending signal entry, resolved on the return-to-user path (PG-7).
pub struct SignalEntry {
    /// Kick bits delivered as the entry's first argument.
    pub bits: u32,
    /// Registered user entry point.
    pub entry: u64,
    /// Resolved user context area (TLS base + registered offset).
    pub ctx: u64,
    /// Registered `SIG_SETUP` flags.
    pub flags: u32,
    /// Current user signal mask, passed in the entry's third argument.
    pub mask: u64,
}

/// `THREAD_KICK` bit 0: inspect pending signal state.
const KICK_CHECK: u32 = 1;

/// `sig_setup(entry: usize, ctx_offset: usize, flags: u32) -> 0` (raw 0x80).
///
/// Self-only and per thread. The entry must be inside an executable VMA of the
/// caller, the context offset must be 16-byte aligned, and the context area
/// must fit in the 64 KiB window relative to the TLS base. Replacing an existing
/// registration is allowed, including inside a handler: the active delivery
/// keeps its own resolved area until `SIGRETURN`.
pub fn sig_setup(frame: &SyscallFrame) -> Result<usize, Errno> {
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    if !thread.user {
        return Err(Errno::EINVAL);
    }
    let entry = frame.rdi;
    let offset = usize::try_from(frame.rsi).map_err(|_| Errno::EINVAL)?;
    let flags = u32::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    if flags & !ENTRY_SP != 0 {
        return Err(Errno::EINVAL);
    }
    let context_size = size_of::<MContext>().saturating_add(size_of::<FaultRecord>());
    match offset.checked_add(context_size) {
        Some(end) if end <= CONTEXT_AREA_LIMIT => {}
        _ => return Err(Errno::EINVAL),
    }
    if offset & 0xf != 0 {
        return Err(Errno::EINVAL);
    }
    if !is_user_address(entry) {
        return Err(Errno::EFAULT);
    }
    {
        let address_space = caller.address_space.lock();
        let address_space = address_space.as_ref().ok_or(Errno::ESRCH)?;
        let entry_vma = address_space.vmas().find(entry).ok_or(Errno::EFAULT)?;
        if entry_vma.prot() & (prot::EXEC as u8) == 0 {
            return Err(Errno::EFAULT);
        }
    }
    thread
        .signal_ctx_offset
        .store(offset as u64, Ordering::Release);
    thread.signal_flags.store(flags, Ordering::Release);
    thread.signal_entry.store(entry, Ordering::Release);
    Ok(0)
}

/// `sigresend(sig: u32, info: usize, mask: u64) -> 0` (raw 0x84).
///
/// Stores the restored user mask, sets the `CHECK` kick bit and marks the
/// thread for a nested entry on the return-to-user path. `sig` and `info` are
/// opaque to the kernel; libc's pending set already owns the signal. Requires
/// a `SIG_SETUP` registration on the calling thread.
pub fn sigresend(frame: &SyscallFrame) -> Result<usize, Errno> {
    let _ = (frame.rdi, frame.rsi);
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    if !thread.user || thread.signal_entry.load(Ordering::Acquire) == 0 {
        return Err(Errno::EINVAL);
    }
    thread.signal_mask.store(frame.rdx, Ordering::Release);
    thread.resend.store(true, Ordering::Release);
    thread.kick_bits.fetch_or(KICK_CHECK, Ordering::AcqRel);
    Ok(0)
}

/// Resolve a registered entry and context area for the current thread.
///
/// `require_idle` rejects a thread already inside a signal; `SIGRESEND` is the
/// one operation allowed to nest. An invalid context area is a fatal delivery
/// error, per the PG-7 contract.
fn signal_target(
    thread: &crate::sched::thread::Thread,
    require_idle: bool,
) -> Option<(u64, u64, u32)> {
    if !thread.user {
        return None;
    }
    if require_idle && thread.signal_depth.load(Ordering::Acquire) != 0 {
        return None;
    }
    let entry = thread.signal_entry.load(Ordering::Acquire);
    if entry == 0 {
        return None;
    }
    let offset = thread.signal_ctx_offset.load(Ordering::Acquire);
    let Some(ctx) = thread.tls_base().checked_add(offset) else {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    };
    let size = size_of::<MContext>().saturating_add(size_of::<FaultRecord>());
    if UserSlice::new(ctx, size as u64).is_err() {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    Some((entry, ctx, thread.signal_flags.load(Ordering::Acquire)))
}

/// Resolve the current thread's pending kick, if it is deliverable now.
///
/// Returns `None` while inside a signal or without a registration (S43's
/// sticky `-EINTR` path then owns the word).
pub fn signal_due() -> Option<SignalEntry> {
    let thread = sched::current_thread()?;
    if thread.signal_entry.load(Ordering::Acquire) == 0 {
        return None;
    }
    let bits = thread.kick_bits.load(Ordering::Acquire);
    if bits == 0 {
        return None;
    }
    let (entry, ctx, flags) = signal_target(&thread, true)?;
    Some(SignalEntry {
        bits,
        entry,
        ctx,
        flags,
        mask: thread.signal_mask.load(Ordering::Acquire),
    })
}

/// Resolve a fault delivery target; the kick bits are not part of a fault.
pub fn signal_fault_due() -> Option<SignalEntry> {
    let thread = sched::current_thread()?;
    if thread.signal_entry.load(Ordering::Acquire) == 0 {
        return None;
    }
    let (entry, ctx, flags) = signal_target(&thread, true)?;
    Some(SignalEntry {
        bits: 0,
        entry,
        ctx,
        flags,
        mask: thread.signal_mask.load(Ordering::Acquire),
    })
}

/// Resolve a `SIGRESEND` re-entry, which is allowed while already in a signal.
pub fn signal_resend_due() -> Option<SignalEntry> {
    let thread = sched::current_thread()?;
    if !thread.resend.swap(false, Ordering::AcqRel) {
        return None;
    }
    let (entry, ctx, flags) = signal_target(&thread, false)?;
    Some(SignalEntry {
        bits: thread.kick_bits.load(Ordering::Acquire) | KICK_CHECK,
        entry,
        ctx,
        flags,
        mask: thread.signal_mask.load(Ordering::Acquire),
    })
}

/// Save `context` (and an optional fault record) to the entry's user area and
/// mark the thread in-signal.
///
/// Returns the user address of the fault record, or zero for a kick delivery.
/// The bits being delivered are removed from the sticky word; bits that arrived
/// during the copy stay pending for delivery after `SIGRETURN`.
pub fn signal_commit(due: &SignalEntry, context: &MContext, fault: Option<&FaultRecord>) -> u64 {
    let Some(thread) = sched::current_thread() else {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    };
    let size = size_of::<MContext>();
    let source = unsafe {
        // SAFETY: `context` is a live, fully initialised `MContext`; the byte view covers exactly
        // `size_of::<MContext>()` bytes and is only read for the duration of the copy.
        core::slice::from_raw_parts(context as *const MContext as *const u8, size)
    };
    let destination = match UserSlice::new(due.ctx, size as u64) {
        Ok(destination) => destination,
        Err(_) => process::exit_process(Errno::EFAULT.to_ret() as i32),
    };
    if copy_to_user(destination, source).is_err() {
        process::exit_process(Errno::EFAULT.to_ret() as i32);
    }
    let mut fault_address = 0u64;
    if let Some(record) = fault {
        let Some(address) = due.ctx.checked_add(size as u64) else {
            process::exit_process(Errno::EFAULT.to_ret() as i32);
        };
        let record_size = size_of::<FaultRecord>();
        let destination = match UserSlice::new(address, record_size as u64) {
            Ok(destination) => destination,
            Err(_) => process::exit_process(Errno::EFAULT.to_ret() as i32),
        };
        let bytes = unsafe {
            // SAFETY: `record` is a live `FaultRecord`; the byte view covers exactly
            // `size_of::<FaultRecord>()` bytes and is only read for the copy.
            core::slice::from_raw_parts(record as *const FaultRecord as *const u8, record_size)
        };
        if copy_to_user(destination, bytes).is_err() {
            process::exit_process(Errno::EFAULT.to_ret() as i32);
        }
        fault_address = address;
    }
    thread.signal_active_ctx.store(due.ctx, Ordering::Release);
    thread.signal_depth.fetch_add(1, Ordering::AcqRel);
    if due.bits != 0 {
        thread.kick_bits.fetch_and(!due.bits, Ordering::AcqRel);
    }
    fault_address
}

/// Restore the context saved by the active signal (PG-7 `SIGRETURN`, raw 0x82).
///
/// Decrements the nesting depth only after a complete copy-in; a nested
/// `SIGRETURN` returns to the previous handler, the last one clears the active
/// area. Not being in a signal is `EINVAL`.
pub fn sigreturn(context: &mut MContext) -> Result<(), Errno> {
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    if !thread.user || thread.signal_depth.load(Ordering::Acquire) == 0 {
        return Err(Errno::EINVAL);
    }
    let address = thread.signal_active_ctx.load(Ordering::Acquire);
    if address == 0 {
        return Err(Errno::EINVAL);
    }
    let size = size_of::<MContext>();
    let destination = unsafe {
        // SAFETY: `context` is a live caller-provided `MContext`; the mutable byte view covers
        // exactly `size_of::<MContext>()` bytes and is not aliased during the copy.
        core::slice::from_raw_parts_mut(context as *mut MContext as *mut u8, size)
    };
    let source = UserSlice::new(address, size as u64)?;
    copy_from_user(destination, source)?;
    if thread.signal_depth.fetch_sub(1, Ordering::AcqRel) == 1 {
        thread.signal_active_ctx.store(0, Ordering::Release);
    }
    Ok(())
}
