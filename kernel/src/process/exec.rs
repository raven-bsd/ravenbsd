// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Same-pid image replacement (`PROC_EXEC`).
//!
//! The new image is parsed and mapped into a fresh address space before the caller's live space
//! is touched. On success the process keeps its pid, ids, credentials, namespace and handle
//! table; the old address space is freed, the current thread's cached root is updated, and the
//! syscall-return path installs the new entry/stack. Any failure drops the temporary space and
//! returns the error with the old image and registers intact.
//!
//! The loader is the kernel's existing `raven-elf` plan/copy path (`load_segments`) and the C8
//! stack builder used by `/init`; the runtime reads the file with `raven-spawn`'s shared image
//! reader and passes the bytes here.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_elf::Elf;

use super::handle::Object;
use super::{AddressSpace, Process, ProcessError, ProcessState, UserAddressSpace};

/// Largest image copied into kernel memory for one exec.
///
/// The kernel heap is 4 MiB and the image is a temporary copy; the release user programs are
/// about 200 KiB. A larger image is rejected with `EFBIG` until the loader can map directly from
/// the caller's memory object.
pub const MAX_EXEC_IMAGE: usize = 1024 * 1024;

/// `ET_DYN`; dynamic executables and `PT_INTERP` are not loaded by this slice.
const ET_DYN: u16 = 3;

/// The user context a successful exec installs on syscall return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecContext {
    /// New user entry point.
    pub entry: u64,
    /// New user stack pointer (the C8 block).
    pub stack: u64,
}

/// Replace `caller`'s image with `image`.
///
/// `argv`/`envp` are already copied into kernel storage; `path` is the `AT_EXECFN` spelling.
/// `close` is the caller's CLOEXEC handle list; pipe ends created with `O_CLOEXEC` are added
/// automatically.
pub fn replace_image(
    caller: &Arc<Process>,
    image: &[u8],
    path: &[u8],
    argv: &[Vec<u8>],
    envp: &[Vec<u8>],
    close: &[Handle],
) -> Result<(), Errno> {
    if image.is_empty() || image.len() > MAX_EXEC_IMAGE {
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

    let elf = Elf::parse(
        image,
        super::TARGET_MACHINE,
        super::USER_MIN..super::USER_END,
    )
    .map_err(|_| Errno::ENOEXEC)?;
    if elf.interp().is_some() {
        return Err(Errno::ENOTSUP);
    }
    let dynamic = image
        .get(16..18)
        .and_then(|bytes| <[u8; 2]>::try_from(bytes).ok())
        .map(u16::from_le_bytes)
        == Some(ET_DYN);
    if dynamic {
        return Err(Errno::ENOTSUP);
    }

    let ids = *caller.ids.lock();
    let address_space = AddressSpace::new().map_err(|_| Errno::ENOMEM)?;
    let mut new_space = UserAddressSpace::new(address_space);
    super::load_segments(&mut new_space, &elf, image).map_err(process_errno)?;
    let argv_refs: Vec<&[u8]> = argv.iter().map(Vec::as_slice).collect();
    let envp_refs: Vec<&[u8]> = envp.iter().map(Vec::as_slice).collect();
    let stack = super::build_stack(
        &mut new_space,
        path,
        &argv_refs,
        &envp_refs,
        &ids,
        elf.entry(),
        &elf,
    )
    .map_err(process_errno)?;

    // Collect the handle exits before the switch: the image and stack are now fully built, so a
    // close failure can no longer abort the exec.
    let mut to_close: Vec<Handle> = Vec::new();
    to_close
        .try_reserve(close.len())
        .map_err(|_| Errno::ENOMEM)?;
    to_close.extend_from_slice(close);
    caller.handles.lock().for_each_live(|handle, object, _, _| {
        if let Object::Pipe(end) = object
            && end.is_cloexec()
            && !to_close.contains(&handle)
        {
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
    caller.pending_exec.lock().replace(ExecContext {
        entry: elf.entry(),
        stack,
    });
    Ok(())
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
