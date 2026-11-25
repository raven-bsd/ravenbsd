// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `THREAD_CREATE`, `THREAD_EXIT` and the PG-7 signal entry/return state.

use core::mem::size_of;
use core::sync::atomic::Ordering;

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::signal::{CONTEXT_AREA_LIMIT, ENTRY_SP, FaultRecord, MContext, SIG_SETUP_ABS};
use raven_abi::syscall::{THREAD_CREATE_EX, ThreadCreateEx};
use raven_abi::types::{Handle, Rights, prot};
use raven_usercopy::{UserSlice, is_user_address};

use crate::cmdline::CmdLine;
use crate::kprintln;
use crate::memory::paging;
use crate::process::status;
use crate::process::{self, Process, ProcessState, handle::Object};
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

    let create_ex = THREAD_CREATE_EX as u64;
    if frame.r8 & !create_ex != 0 {
        return Err(Errno::EINVAL);
    }
    let extended = if frame.r8 & create_ex != 0 {
        if frame.r9 == 0 {
            return Err(Errno::EINVAL);
        }
        let mut extended = ThreadCreateEx {
            tp: 0,
            tid_out: 0,
            clear_tid_addr: 0,
        };
        let bytes = unsafe {
            // SAFETY: `extended` is a live, fully initialised `ThreadCreateEx`; the mutable byte
            // view covers exactly its size and is not aliased during the copy.
            core::slice::from_raw_parts_mut(
                &mut extended as *mut ThreadCreateEx as *mut u8,
                size_of::<ThreadCreateEx>(),
            )
        };
        copy_from_user(
            bytes,
            UserSlice::new(frame.r9, size_of::<ThreadCreateEx>() as u64)?,
        )?;
        if extended.tp != 0 && !is_user_address(extended.tp as u64) {
            return Err(Errno::EFAULT);
        }
        if !writable_word(&caller, extended.tid_out as u64) {
            return Err(Errno::EFAULT);
        }
        if !writable_word(&target, extended.clear_tid_addr as u64) {
            return Err(Errno::EFAULT);
        }
        Some(extended)
    } else {
        None
    };
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
    // POSIX `pthread_create` inherits the creating thread's signal mask (NetBSD
    // `pthread_create.3:131`; FreeBSD `kern_thread.c` `thread_alloc`). A first thread created in
    // a different process (`raven-spawn`) starts with an empty mask, so only the same-process
    // case inherits. The thread is not enqueued yet; `sched::enqueue` publishes the write under
    // the run-queue lock.
    if target.id == caller.id
        && let Some(creator) = sched::current_thread()
    {
        thread.inherit_signal_mask(&creator);
    } else if let Some(seed) = target.fork_signal.lock().take() {
        // The forked child's first thread: FreeBSD `fork1` copied these from the forking thread
        // (`td_sigmask`, `td_sigstk`). Consume the seed so later pthreads do not inherit it.
        let unblockable =
            process::signal_bit(status::SIGKILL) | process::signal_bit(status::SIGSTOP);
        thread
            .sig_blocked
            .store(seed.mask & !unblockable, Ordering::Release);
        if seed.alt_flags & crate::process::SS_DISABLE == 0
            && seed.alt_sp != 0
            && seed.alt_size != 0
        {
            thread.sig_alt_sp.store(seed.alt_sp, Ordering::Release);
            thread.sig_alt_size.store(seed.alt_size, Ordering::Release);
            thread
                .sig_alt_flags
                .store(seed.alt_flags, Ordering::Release);
        }
    }
    if let Some(extended) = extended {
        if extended.tp != 0 {
            thread.set_tls_base(extended.tp as u64);
        }
        thread
            .clear_tid
            .store(extended.clear_tid_addr as u64, Ordering::Release);
        if extended.tid_out != 0 {
            let bytes = thread.tid.to_ne_bytes();
            let destination = UserSlice::new(extended.tid_out as u64, 4)?;
            if copy_to_user(destination, &bytes).is_err() {
                sched::reap_thread(thread);
                return Err(Errno::EFAULT);
            }
        }
    }
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

/// True when `address` is zero or a 4-byte writable user range in `process`.
fn writable_word(process: &Arc<process::Process>, address: u64) -> bool {
    if address == 0 {
        return true;
    }
    if UserSlice::new(address, 4).is_err() {
        return false;
    }
    let space = process.address_space.lock();
    let Some(space) = space.as_ref() else {
        return false;
    };
    [address, address.saturating_add(3)].iter().all(|address| {
        space
            .vmas()
            .find(*address)
            .is_some_and(|vma| vma.prot() & (prot::WRITE as u8) != 0)
    })
}

/// Store zero to a clear-on-exit word and wake its futex waiters.
fn clear_word(address: u64) {
    if address == 0 {
        return;
    }
    let Some(process) = process::current() else {
        return;
    };
    let Ok(destination) = UserSlice::new(address, 4) else {
        return;
    };
    if copy_to_user(destination, &[0u8; 4]).is_ok() {
        crate::syscall::futex::wake_address(process.id, address);
    }
}

/// `thread_self() -> tid`.
pub fn self_tid() -> Result<usize, Errno> {
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    if !thread.user {
        return Err(Errno::EINVAL);
    }
    Ok(thread.tid as usize)
}

/// `robust_list_set(head, len) -> 0` (raw 0x26).
pub fn robust_list_set(frame: &SyscallFrame) -> Result<usize, Errno> {
    crate::process::robust::set(frame.rdi, frame.rsi)?;
    Ok(0)
}

/// `thread_exit(code) -> !`.
pub fn exit(frame: &SyscallFrame) -> ! {
    if let Err(error) = process::robust::walk_thread() {
        process::exit_process(error.to_ret() as i32);
    }
    if let Some(thread) = sched::current_thread() {
        clear_word(thread.clear_tid.load(Ordering::Acquire));
    }
    sched::exit_thread(frame.rdi as i32)
}

/// `thread_exit_ex(code, clear_addr, unmap_addr, unmap_len) -> !`.
pub fn exit_ex(frame: &SyscallFrame) -> ! {
    if let Err(error) = process::robust::walk_thread() {
        process::exit_process(error.to_ret() as i32);
    }
    clear_word(frame.rsi);
    if frame.r10 != 0 {
        crate::syscall::memory_ctl::unmap_current(frame.rdx, frame.r10);
    }
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
/// canonical user address; zero clears it. The value is per-thread, and for the
/// calling thread it is written to `IA32_FS_BASE` before the syscall returns;
/// another target receives it on its next switch-in.
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
    // The caller reads `%fs:` as soon as the syscall returns; install the base now instead of
    // waiting for the next context switch. A different target thread gets it on switch-in.
    #[cfg(target_arch = "x86_64")]
    if sched::current_thread().is_some_and(|current| Arc::ptr_eq(&current, &thread)) {
        crate::sched::context::install_tls_base(tp);
    }
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
    if flags & !(ENTRY_SP | SIG_SETUP_ABS) != 0 {
        return Err(Errno::EINVAL);
    }
    let context_size = size_of::<MContext>().saturating_add(size_of::<FaultRecord>());
    // An absolute area is a dedicated user page for a thread whose TLS mapping has no tail
    // (K443). The legacy relative form stays bound to the 64 KiB TLS window.
    let (context_offset, context_absolute) = if flags & SIG_SETUP_ABS != 0 {
        if offset == 0 || offset & 0xf != 0 || !is_user_address(offset as u64) {
            return Err(Errno::EFAULT);
        }
        if !writable_context(&caller, offset as u64, context_size) {
            return Err(Errno::EFAULT);
        }
        (0, offset as u64)
    } else {
        match offset.checked_add(context_size) {
            Some(end) if end <= CONTEXT_AREA_LIMIT => {}
            _ => return Err(Errno::EINVAL),
        }
        if offset & 0xf != 0 {
            return Err(Errno::EINVAL);
        }
        // The relative form resolves against this thread's TLS base. Reject an unset base or an
        // area outside a writable VMA now: the kernel can return an error here, so it must not
        // accept a context that would later force the delivery path to kill the process.
        let Some(context) = thread.tls_base().checked_add(offset as u64) else {
            return Err(Errno::EFAULT);
        };
        if !writable_context(&caller, context, context_size) {
            return Err(Errno::EFAULT);
        }
        (offset, 0)
    };
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
        .store(context_offset as u64, Ordering::Release);
    thread
        .signal_ctx_abs
        .store(context_absolute, Ordering::Release);
    thread.signal_flags.store(flags, Ordering::Release);
    thread.signal_entry.store(entry, Ordering::Release);
    if CmdLine::parse(paging::cmdline())
        .get("k495-debug")
        .is_some()
    {
        // K495 rev180: record the registered entry so a fault RIP can be compared with it. A
        // caller address here is the class Ken identified for the SIG_SETUP resume defect.
        kprintln!(
            "signal: SIG_SETUP pid={} entry=0x{:x} offset=0x{:x} abs=0x{:x} flags=0x{:x}",
            caller.id,
            entry,
            context_offset,
            context_absolute,
            flags
        );
    }
    Ok(0)
}

/// The Raven `SIGACTION` request layout (40 bytes), the one libc builds in
/// `raven-libc-signal/src/sigaction_abi.rs` and the only one the kernel may copy.
///
/// The C-visible `struct sigaction` (152 bytes, FreeBSD-shaped) stays inside libc, which
/// converts it to this request; the kernel never sees the C layout.
#[repr(C)]
#[derive(Clone, Copy)]
struct UserSigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

const _: () = assert!(size_of::<UserSigAction>() == 40);
const _: () = assert!(core::mem::offset_of!(UserSigAction, handler) == 0);
const _: () = assert!(core::mem::offset_of!(UserSigAction, mask) == 8);
const _: () = assert!(core::mem::offset_of!(UserSigAction, flags) == 24);
const _: () = assert!(core::mem::offset_of!(UserSigAction, _pad) == 28);
const _: () = assert!(core::mem::offset_of!(UserSigAction, restorer) == 32);

/// The System V amd64 `stack_t` (24 bytes).
#[repr(C)]
#[derive(Clone, Copy)]
struct UserStack {
    ss_sp: u64,
    ss_flags: i32,
    _pad: i32,
    ss_size: u64,
}

const _: () = assert!(size_of::<UserStack>() == 24);
const _: () = assert!(core::mem::offset_of!(UserStack, ss_flags) == 8);
const _: () = assert!(core::mem::offset_of!(UserStack, ss_size) == 16);

const ZERO_SIGACTION: UserSigAction = UserSigAction {
    handler: 0,
    mask: [0; 2],
    flags: 0,
    _pad: 0,
    restorer: 0,
};

/// `sigaction(signo: i32, act: *const SigAction, old: *mut SigAction) -> 0` (raw 0x87).
///
/// Installs or reads the per-process disposition. The user structure is the Raven private
/// [`UserSigAction`] wire (handler at 0, two mask words at 8, `flags` at 24, `restorer` at 32);
/// libc converts the public 152-byte C `struct sigaction` to this before the syscall.
/// `SIGKILL`/`SIGSTOP` accept only `SIG_DFL`; unknown handler flags are `EINVAL` when a handler
/// is caught, matching FreeBSD `kern_sigaction`. A failed call leaves the caller's `old` buffer
/// untouched: the old action is copied out only after the new one was validated and installed,
/// matching FreeBSD `sys_sigaction` (`kern_sig.c:952-958`).
pub fn sigaction(frame: &SyscallFrame) -> Result<usize, Errno> {
    let signo = (frame.rdi as u32) as i32;
    if signo < 1 || signo > status::SIG_MAX as i32 {
        return Err(Errno::EINVAL);
    }
    let signo = signo as u32;
    let caller = process::current().ok_or(Errno::ESRCH)?;
    let act = frame.rsi as *const UserSigAction;
    let old = frame.rdx as *mut UserSigAction;
    // Copy the new action in and validate it before the disposition table changes or the
    // caller's old buffer can be written. FreeBSD `sys_sigaction` copies the old action out only
    // when `kern_sigaction` succeeded (`sys/kern/kern_sig.c:952-958`).
    let mut input = ZERO_SIGACTION;
    if !act.is_null() {
        let source = UserSlice::new(act as u64, size_of::<UserSigAction>() as u64)?;
        let bytes = unsafe {
            // SAFETY: `input` is a live, uninitialised-by-value POD mirror; the mutable byte view
            // covers exactly its size and the copy fills it before it is read.
            core::slice::from_raw_parts_mut(
                &mut input as *mut UserSigAction as *mut u8,
                size_of::<UserSigAction>(),
            )
        };
        copy_from_user(bytes, source)?;
        let handler = input.handler;
        let flags = input.flags as u32;
        if handler != process::SIG_DFL
            && handler != process::SIG_IGN
            && flags & !process::SA_SUPPORTED != 0
        {
            return Err(Errno::EINVAL);
        }
        if handler != process::SIG_DFL && handler != process::SIG_IGN && !is_user_address(handler) {
            return Err(Errno::EFAULT);
        }
        if (signo == status::SIGKILL || signo == status::SIGSTOP) && handler != process::SIG_DFL {
            return Err(Errno::EINVAL);
        }
    }
    let mut sigacts = caller.sigacts.lock();
    let previous = sigacts.get(signo);
    if !act.is_null() {
        sigacts.set(
            signo,
            process::SigAction {
                handler: input.handler,
                mask: input.mask.first().copied().unwrap_or(0),
                flags: input.flags,
            },
        );
        if input.handler == process::SIG_IGN {
            // The donor drops a queued signal when it becomes ignored (`sigqueue_delete_proc`).
            caller
                .pending_signals
                .lock()
                .retain(|signal| signal.signo != signo);
        }
    }
    if !old.is_null() {
        let mut out = ZERO_SIGACTION;
        out.handler = previous.handler;
        out.mask[0] = previous.mask;
        out.flags = previous.flags;
        let destination = UserSlice::new(old as u64, size_of::<UserSigAction>() as u64)?;
        let bytes = unsafe {
            // SAFETY: `out` is a live, fully initialised user-structure mirror; the byte view
            // covers exactly its size and is only read for the copy.
            core::slice::from_raw_parts(
                &out as *const UserSigAction as *const u8,
                size_of::<UserSigAction>(),
            )
        };
        copy_to_user(destination, bytes)?;
    }
    Ok(0)
}

/// `sigprocmask(how: i32, set: *const u64, old: *mut u64) -> 0` (raw 0x88).
///
/// The mask is per thread; `SIGKILL` and `SIGSTOP` can never be blocked. A signal that becomes
/// unblocked is delivered on the return-to-user path by [`crate::process::sig_deliver_pending`].
pub fn sigprocmask(frame: &SyscallFrame) -> Result<usize, Errno> {
    let how = (frame.rdi as u32) as i32;
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    if !thread.user {
        return Err(Errno::EINVAL);
    }
    let current = thread.sig_blocked.load(Ordering::Acquire);
    let mut requested = 0u64;
    if frame.rsi != 0 {
        let source = UserSlice::new(frame.rsi, 8)?;
        let bytes = unsafe {
            // SAFETY: `requested` is a live u64; the mutable byte view covers exactly 8 bytes and
            // the copy fills it before it is read.
            core::slice::from_raw_parts_mut(&mut requested as *mut u64 as *mut u8, 8)
        };
        copy_from_user(bytes, source)?;
    }
    if frame.rdx != 0 {
        let destination = UserSlice::new(frame.rdx, 8)?;
        let bytes = unsafe {
            // SAFETY: `current` is a live u64; the byte view covers exactly 8 bytes and is only
            // read for the copy.
            core::slice::from_raw_parts(&current as *const u64 as *const u8, 8)
        };
        copy_to_user(destination, bytes)?;
    }
    if frame.rsi != 0 {
        let unblockable =
            process::signal_bit(status::SIGKILL) | process::signal_bit(status::SIGSTOP);
        let updated = match how {
            process::SIG_BLOCK => current | requested,
            process::SIG_UNBLOCK => current & !requested,
            process::SIG_SETMASK => requested,
            _ => return Err(Errno::EINVAL),
        } & !unblockable;
        thread.sig_blocked.store(updated, Ordering::Release);
        let caller = process::current().ok_or(Errno::ESRCH)?;
        // A held default-action signal runs now that the mask opened (FreeBSD `issignal`), and
        // an unblocked caught signal must re-check the kernel queue before the next wait.
        caller.apply_deferred_defaults(updated);
        if current & !updated != 0 {
            thread.signal_wake.store(true, Ordering::Release);
            thread.kick_bits.fetch_or(KICK_CHECK, Ordering::AcqRel);
        }
    }
    Ok(0)
}

/// `sigaltstack(ss: *const Stack, old: *mut Stack) -> 0` (raw 0x89).
///
/// FreeBSD `kern_sigaltstack`: `SS_DISABLE` clears the stack, `SS_ONSTACK` makes a replace
/// `EPERM`, a live stack must be at least `MINSIGSTKSZ` and inside the user range.
pub fn sigaltstack(frame: &SyscallFrame) -> Result<usize, Errno> {
    let thread = sched::current_thread().ok_or(Errno::ESRCH)?;
    if !thread.user {
        return Err(Errno::EINVAL);
    }
    let old = frame.rsi as *mut UserStack;
    if !old.is_null() {
        let sp = thread.sig_alt_sp.load(Ordering::Acquire);
        let size = thread.sig_alt_size.load(Ordering::Acquire);
        let mut flags = thread.sig_alt_flags.load(Ordering::Acquire);
        if sp == 0 || size == 0 {
            flags = process::SS_DISABLE;
        } else if thread.sig_on_altstack.load(Ordering::Acquire) {
            flags |= process::SS_ONSTACK;
        }
        let out = UserStack {
            ss_sp: sp,
            ss_flags: flags as i32,
            _pad: 0,
            ss_size: size,
        };
        let destination = UserSlice::new(old as u64, size_of::<UserStack>() as u64)?;
        let bytes = unsafe {
            // SAFETY: `out` is a live POD mirror; the byte view covers exactly its size.
            core::slice::from_raw_parts(
                &out as *const UserStack as *const u8,
                size_of::<UserStack>(),
            )
        };
        copy_to_user(destination, bytes)?;
    }
    let ss = frame.rdi as *const UserStack;
    if !ss.is_null() {
        let mut input = UserStack {
            ss_sp: 0,
            ss_flags: 0,
            _pad: 0,
            ss_size: 0,
        };
        let source = UserSlice::new(ss as u64, size_of::<UserStack>() as u64)?;
        let bytes = unsafe {
            // SAFETY: `input` is a live POD mirror; the mutable byte view covers exactly its size.
            core::slice::from_raw_parts_mut(
                &mut input as *mut UserStack as *mut u8,
                size_of::<UserStack>(),
            )
        };
        copy_from_user(bytes, source)?;
        let flags = input.ss_flags as u32;
        if flags & !(process::SS_DISABLE | process::SS_AUTODISARM) != 0 {
            return Err(Errno::EINVAL);
        }
        if flags & process::SS_DISABLE != 0 {
            thread.sig_alt_sp.store(0, Ordering::Release);
            thread.sig_alt_size.store(0, Ordering::Release);
            thread
                .sig_alt_flags
                .store(process::SS_DISABLE, Ordering::Release);
            return Ok(0);
        }
        if thread.sig_on_altstack.load(Ordering::Acquire) {
            return Err(Errno::EPERM);
        }
        if input.ss_size < process::MINSIGSTKSZ {
            return Err(Errno::ENOMEM);
        }
        let Some(end) = input.ss_sp.checked_add(input.ss_size) else {
            return Err(Errno::EINVAL);
        };
        if !is_user_address(input.ss_sp) || !is_user_address(end.saturating_sub(1)) {
            return Err(Errno::EFAULT);
        }
        thread.sig_alt_sp.store(input.ss_sp, Ordering::Release);
        thread.sig_alt_size.store(input.ss_size, Ordering::Release);
        thread.sig_alt_flags.store(flags, Ordering::Release);
    }
    Ok(0)
}

/// True when `address..address + size` is a writable user range of `process`.
fn writable_context(process: &Arc<Process>, address: u64, size: usize) -> bool {
    if address == 0 || UserSlice::new(address, u64::try_from(size).unwrap_or(u64::MAX)).is_err() {
        return false;
    }
    let space = process.address_space.lock();
    let Some(space) = space.as_ref() else {
        return false;
    };
    let last = address
        .saturating_add(u64::try_from(size).unwrap_or(u64::MAX))
        .saturating_sub(1);
    [address, last].iter().all(|address| {
        space
            .vmas()
            .find(*address)
            .is_some_and(|vma| vma.prot() & (prot::WRITE as u8) != 0)
    })
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
    let absolute = thread.signal_ctx_abs.load(Ordering::Acquire);
    let offset = thread.signal_ctx_offset.load(Ordering::Acquire);
    let ctx = if absolute != 0 {
        absolute
    } else {
        let Some(ctx) = thread.tls_base().checked_add(offset) else {
            process::exit_process(Errno::EFAULT.to_ret() as i32);
        };
        ctx
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
