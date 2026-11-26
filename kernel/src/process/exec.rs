// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Same-pid image replacement (`PROC_EXEC`).
//!
//! The new image is parsed and mapped into a fresh address space before the caller's live space
//! is touched. On success the process keeps its pid, ids, credentials, file-creation mask,
//! namespace and handle table; the old address space is freed, the current thread's cached root
//! is updated, and the syscall-return path installs the new entry/stack. Any failure drops the
//! temporary space and returns the error with the old image and registers intact.
//!
//! The loader uses the kernel's `raven-elf` planner and the C8 stack builder used by `/init`.
//! Only the ELF header, program headers and `PT_INTERP` string are copied into kernel memory;
//! each `PT_LOAD` page is copied directly from the caller's still-live image, so the 4 MiB kernel
//! heap never holds the multi-MiB image.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_elf::Elf;

use super::handle::Object;
use super::{AddressSpace, Process, ProcessError, ProcessState, UserAddressSpace};

/// Largest exec image accepted from the caller.
///
/// The image stays in the caller's address space; the kernel copies only the ELF header,
/// program-header table and `PT_INTERP` string (bounded separately) and streams each `PT_LOAD`
/// segment into fresh frames. The bound covers the staged 8.3 MiB OpenSSH helpers with headroom;
/// it is an input limit, not a kernel-heap allocation.
pub const MAX_EXEC_IMAGE: usize = 16 * 1024 * 1024;

/// `ET_EXEC`: a fixed-address executable.
const ET_EXEC: u16 = 2;
/// `ET_DYN`: a position-independent executable or shared object.
const ET_DYN: u16 = 3;
/// Largest `PT_INTERP` image read from the initfs for one exec.
const MAX_INTERP_IMAGE: usize = 2 * 1024 * 1024;

/// The user context a successful exec installs on syscall return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecContext {
    /// New user entry point.
    pub entry: u64,
    /// New user stack pointer (the C8 block).
    pub stack: u64,
}

/// Replace `caller`'s image from the still-live user range.
///
/// `header` contains the ELF header, the complete program-header table and any `PT_INTERP`
/// string; `image_base`/`image_len` name the caller's full image. Segments are copied directly
/// from that range. `argv`/`envp` are already copied into kernel storage; `path` is the
/// `AT_EXECFN` spelling. `close` is the caller's CLOEXEC handle list; pipe ends created with
/// `O_CLOEXEC` are added automatically.
// `PROC_EXEC` supplies eight distinct inputs (caller, header, image range, path, argv, envp and
// the close list) and has one caller in `kernel/src/syscall/exec.rs`. Grouping them into a context
// would change that cross-module API, which is outside K418's Owns; the ticket permits this
// narrowly scoped exception. No other lint is suppressed.
#[allow(clippy::too_many_arguments)]
pub fn replace_image_from_user(
    caller: &Arc<Process>,
    header: &[u8],
    image_base: u64,
    image_len: u64,
    path: &[u8],
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
    close: &[Handle],
) -> Result<(), Errno> {
    if header.is_empty() || image_len == 0 || image_len > MAX_EXEC_IMAGE as u64 {
        return Err(Errno::EFBIG);
    }
    // The current thread must be the only thread: this slice has no scheduler-side thread list
    // to suspend the siblings before the old address space is freed. Single-threaded exec is the
    // common libc path; a multi-threaded caller is refused without touching its image.
    if caller.threads.load(Ordering::Acquire) != 1 {
        return Err(Errno::EBUSY);
    }
    if *caller.state.lock() != ProcessState::Running {
        return Err(Errno::ESRCH);
    }
    let thread = crate::sched::current_thread().ok_or(Errno::ESRCH)?;

    let kind = elf_type(header).ok_or(Errno::ENOEXEC)?;
    let range = if kind == ET_DYN {
        0..super::USER_END
    } else {
        super::USER_MIN..super::USER_END
    };
    let elf = Elf::parse_with_len(header, image_len, super::TARGET_MACHINE, range)
        .map_err(|_| Errno::ENOEXEC)?;
    let main_bias = match kind {
        ET_EXEC => 0,
        ET_DYN => crate::memory::aslr::pie_bias(),
        _ => return Err(Errno::ENOEXEC),
    };
    let main_entry = elf.entry().checked_add(main_bias).ok_or(Errno::ENOEXEC)?;
    let (interpreter, interpreter_data) = match elf.interp() {
        Some(path) => {
            if !path.starts_with('/') {
                return Err(Errno::ENOEXEC);
            }
            let entry = crate::initfs::lookup(path).ok_or(Errno::ENOENT)?;
            if entry.data.len() > MAX_INTERP_IMAGE {
                return Err(Errno::EFBIG);
            }
            if elf_type(entry.data) != Some(ET_DYN) {
                return Err(Errno::ENOEXEC);
            }
            let parsed = Elf::parse(entry.data, super::TARGET_MACHINE, 0..super::USER_END)
                .map_err(|_| Errno::ENOEXEC)?;
            if parsed.interp().is_some() {
                return Err(Errno::ENOEXEC);
            }
            (Some(parsed), Some(entry.data))
        }
        None => (None, None),
    };
    let interpreter_bias = if interpreter.is_some() {
        crate::memory::aslr::interp_bias()
    } else {
        0
    };
    let initial_entry = match &interpreter {
        Some(interp) => interp
            .entry()
            .checked_add(interpreter_bias)
            .ok_or(Errno::ENOEXEC)?,
        None => main_entry,
    };
    let phdr = elf
        .phdr_vaddr()
        .and_then(|value| value.checked_add(main_bias));
    if interpreter.is_some() && phdr.is_none() {
        return Err(Errno::ENOEXEC);
    }
    let aux = super::StackAuxv {
        phdr: phdr.unwrap_or(0),
        base: interpreter_bias,
        entry: main_entry,
        phnum: elf.phnum() as u64,
    };
    let ids = *caller.ids.lock();
    let address_space = AddressSpace::new().map_err(|_| Errno::ENOMEM)?;
    let mut new_space = UserAddressSpace::new(address_space);
    if let (Some(interp), Some(data)) = (&interpreter, interpreter_data) {
        super::load_segments(&mut new_space, interp, data, interpreter_bias)
            .map_err(process_errno)?;
    }
    super::load_segments_from_user(&mut new_space, &elf, image_base, image_len, main_bias)
        .map_err(process_errno)?;
    let argv_refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let envp_refs: Vec<&[u8]> = envp.iter().map(Vec::as_slice).collect();
    let stack = super::build_stack(&mut new_space, path, &argv_refs, &envp_refs, &ids, aux)
        .map_err(process_errno)?;

    // Collect the handle exits before the switch: the image and stack are now fully built, so a
    // close failure can no longer abort the exec.
    let mut to_close: Vec<Handle> = Vec::new();
    to_close
        .try_reserve(close.len())
        .map_err(|_| Errno::ENOMEM)?;
    to_close.extend_from_slice(close);
    caller.handles.lock().for_each_live(|handle, object, _, _| {
        let cloexec = match object {
            Object::Pipe(end) => end.is_cloexec(),
            Object::Duplex(end) => end.is_cloexec(),
            _ => false,
        };
        if cloexec && !to_close.contains(&handle) {
            to_close.push(handle);
        }
    });

    let new_root = new_space.root();
    let old = {
        let mut slot = caller.address_space.lock();
        if slot.is_none() {
            return Err(Errno::ESRCH);
        }
        slot.replace(new_space)
    };
    let Some(mut old) = old else {
        return Err(Errno::ESRCH);
    };

    // From here on the operations are infallible. The kernel stack and code are present in the
    // new root's kernel half; only the user half changes.
    let asid = u16::try_from(caller.id & 0xff).unwrap_or(0);
    // SAFETY: `new_root` is the live root just installed in `caller.address_space`; the caller is
    // running on the shared kernel half, so switching CR3/TTBR0 cannot fault this code path.
    unsafe {
        crate::memory::uspace::activate_user_root(new_root.0, asid);
    }
    // SAFETY: this is the current thread, pinned to CPU 0 by the scheduler, and no other thread
    // can observe its `cr3` until the next scheduling point. The store keeps the cached root in
    // step with the active CR3/TTBR0.
    unsafe {
        let raw = Arc::as_ptr(&thread).cast_mut();
        (*raw).cr3 = new_root.0;
    }
    old.free_user_frames();
    drop(old);
    for handle in to_close {
        let _ = crate::event::close_handle(caller, handle);
    }
    caller.pending_signals.lock().clear();
    // C19/FreeBSD `execsigs`: caught handlers reset to SIG_DFL (ignored stay ignored), the
    // alternate stack is disabled, and no handler/entry from the old image survives. The signal
    // mask is preserved across exec (FreeBSD keeps `td_sigmask`).
    caller.sigacts.lock().exec_reset();
    if let Some(thread) = crate::sched::current_thread() {
        thread.signal_entry.store(0, Ordering::Release);
        thread.signal_ctx_offset.store(0, Ordering::Release);
        thread.signal_ctx_abs.store(0, Ordering::Release);
        thread.signal_flags.store(0, Ordering::Release);
        thread.signal_active_ctx.store(0, Ordering::Release);
        thread.signal_depth.store(0, Ordering::Release);
        thread.signal_mask.store(0, Ordering::Release);
        thread.resend.store(false, Ordering::Release);
        thread.kick_bits.store(0, Ordering::Release);
        thread.sig_alt_sp.store(0, Ordering::Release);
        thread.sig_alt_size.store(0, Ordering::Release);
        thread
            .sig_alt_flags
            .store(crate::process::SS_DISABLE, Ordering::Release);
        thread.sig_on_altstack.store(false, Ordering::Release);
    }
    caller.exec_done.store(true, Ordering::Release);
    let command = super::command_from_argv(argv, path);
    *caller.command.lock() = command;
    caller.pending_exec.lock().replace(ExecContext {
        entry: initial_entry,
        stack,
    });
    Ok(())
}

/// Read `e_type` from an ELF64 header.
fn elf_type(image: &[u8]) -> Option<u16> {
    image
        .get(16..18)
        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
        .map(u16::from_le_bytes)
}

/// Take the pending exec context for the current process, if any.
pub fn take_pending() -> Option<ExecContext> {
    super::current()?.pending_exec.lock().take()
}

fn process_errno(error: ProcessError) -> Errno {
    match error {
        ProcessError::Elf(_) | ProcessError::Interpreter => Errno::ENOEXEC,
        ProcessError::OutOfFrames
        | ProcessError::Map(_)
        | ProcessError::Handle(_)
        | ProcessError::Sched(_) => Errno::ENOMEM,
        ProcessError::ArgTooLong => Errno::E2BIG,
        ProcessError::BadAddress => Errno::EINVAL,
        ProcessError::NoInit => Errno::ENOENT,
    }
}
