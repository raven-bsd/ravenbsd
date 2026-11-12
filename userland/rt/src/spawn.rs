// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Create a child process from an ELF file in the initfs.
//!
//! The kernel has no `exec`. This module plans the image with `raven-elf`, maps its segments and a
//! fresh stack into a new empty process, fills the stack through a shared memory object (CONTRACTS
//! C13), and starts the first thread. The caller receives the child's process handle and may wait
//! for it with `PROC_WAIT`.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
#[cfg(any(target_os = "none", test))]
use raven_abi::types::{map, prot};
use raven_elf::ElfError;
#[cfg(any(target_os = "none", test))]
use raven_elf::{Elf, LoadMode, Machine, MapStep, StepKind, plan_load};
#[cfg(any(target_os = "none", test))]
use raven_initfs::Initfs;

/// Options for [`spawn`].
#[derive(Clone, Copy, Debug)]
pub struct SpawnOptions<'a> {
    /// Arguments for the child. `argv[0]` defaults to the path when `args` is empty.
    pub args: &'a [&'a str],
    /// Environment strings, each in `KEY=VALUE` form.
    pub env: &'a [&'a str],
}

/// Why a spawn failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnError {
    /// The path is not present in the initfs.
    NotFound,
    /// The ELF image is invalid or cannot be loaded strictly.
    BadElf(ElfError),
    /// The ELF requests an interpreter, which the M1 loader does not support.
    HasInterpreter,
    /// The stack image or an address is too large for the target.
    TooLarge,
    /// A kernel syscall failed.
    Sys(Errno),
}

/// The syscall surface used by [`spawn_with`].
///
/// The real implementation is [`RtSys`]; tests use a recording mock.
pub trait Sys {
    /// Create an empty process.
    fn proc_create(&mut self) -> Result<Handle, Errno>;
    /// Create an anonymous, zero-filled memory object.
    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno>;
    /// Map a memory object or the initfs into a process.
    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, Errno>;
    /// Unmap a range in a process.
    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno>;
    /// Create the first thread of a process.
    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno>;
    /// Close a handle.
    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno>;
    /// Copy bytes into a mapping owned by the parent.
    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]);
}

/// Spawn `/path` from `initfs` using the real syscall implementation.
#[cfg(any(target_os = "none", test))]
pub fn spawn(
    initfs: &Initfs<'_>,
    path: &str,
    options: &SpawnOptions<'_>,
) -> Result<Handle, SpawnError> {
    let mut sys = RtSys;
    spawn_with(initfs, path, options, &mut sys)
}

/// Spawn `/path` using a caller-provided [`Sys`] implementation.
///
/// This is the full sequence: parse and plan, create the process, map every segment, build a
/// 64 KiB stack through a shared memory object, start the first thread, and return the process
/// handle. On failure after `proc_create`, the child handle is closed.
#[cfg(any(target_os = "none", test))]
pub fn spawn_with<S: Sys>(
    initfs: &Initfs<'_>,
    path: &str,
    options: &SpawnOptions<'_>,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    let entry = initfs.get(path).ok_or(SpawnError::NotFound)?;
    let elf =
        Elf::parse(entry.data, target_machine(), USER_MIN..USER_END).map_err(SpawnError::BadElf)?;
    if elf.interp().is_some() {
        return Err(SpawnError::HasInterpreter);
    }

    let entry_address = usize::try_from(elf.entry()).map_err(|_| SpawnError::TooLarge)?;
    let stack_top = STACK_TOP;
    let stack_base = stack_top
        .checked_sub(STACK_SIZE as u64)
        .ok_or(SpawnError::TooLarge)?;
    let stack_base_address = usize::try_from(stack_base).map_err(|_| SpawnError::TooLarge)?;

    let mut auxv = [(0usize, 0usize); MAX_AUXV];
    let mut auxv_len = 0usize;
    auxv[auxv_len] = (AT_PAGESZ, PAGE as usize);
    auxv_len += 1;
    auxv[auxv_len] = (AT_ENTRY, entry_address);
    auxv_len += 1;
    if let Some(phdr) = elf.phdr_vaddr() {
        let phdr = usize::try_from(phdr).map_err(|_| SpawnError::TooLarge)?;
        auxv[auxv_len] = (AT_PHDR, phdr);
        auxv_len += 1;
        auxv[auxv_len] = (AT_PHNUM, elf.phnum());
        auxv_len += 1;
    }
    auxv[auxv_len] = (AT_RANDOM, 0);
    auxv_len += 1;
    let random = random_bytes(&auxv as *const _ as usize);

    let steps = plan_load(&elf, PAGE, LoadMode::Strict).map_err(SpawnError::BadElf)?;
    let child = sys.proc_create().map_err(SpawnError::Sys)?;
    for step in steps {
        if let Err(error) = map_step(sys, child, entry.data_offset, step) {
            let _ = sys.handle_close(child);
            return Err(error);
        }
    }

    let memobj = match sys.memobj_create(STACK_SIZE) {
        Ok(handle) => handle,
        Err(error) => {
            let _ = sys.handle_close(child);
            return Err(SpawnError::Sys(error));
        }
    };
    let scratch_flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
    let scratch = match sys.map(Handle::SELF_PROC, memobj, 0, 0, STACK_SIZE, scratch_flags) {
        Ok(address) => address,
        Err(error) => {
            let _ = sys.handle_close(memobj);
            let _ = sys.handle_close(child);
            return Err(SpawnError::Sys(error));
        }
    };

    // The image builder needs a whole temporary stack. Keeping it on the caller's stack would
    // overflow the fixed-size stack of a service that spawns a child (all RavenBSD processes have
    // 64 KiB); the heap allocation is the same size and lives only for this spawn.
    #[cfg(feature = "alloc")]
    let mut stack_buffer = alloc_crate::vec![0u8; STACK_SIZE];
    #[cfg(not(feature = "alloc"))]
    let mut stack_buffer = [0u8; STACK_SIZE];
    let (image_offset, sp0) = match build_stack(
        &mut stack_buffer,
        stack_top,
        path,
        options.args,
        options.env,
        &auxv[..auxv_len],
        &random,
    ) {
        Ok(image) => image,
        Err(error) => {
            let _ = sys.unmap(Handle::SELF_PROC, scratch, STACK_SIZE);
            let _ = sys.handle_close(memobj);
            let _ = sys.handle_close(child);
            return Err(error);
        }
    };
    let Some(write_address) = scratch.checked_add(image_offset) else {
        let _ = sys.unmap(Handle::SELF_PROC, scratch, STACK_SIZE);
        let _ = sys.handle_close(memobj);
        let _ = sys.handle_close(child);
        return Err(SpawnError::TooLarge);
    };
    let image = match stack_buffer.get(image_offset..) {
        Some(image) => image,
        None => {
            let _ = sys.unmap(Handle::SELF_PROC, scratch, STACK_SIZE);
            let _ = sys.handle_close(memobj);
            let _ = sys.handle_close(child);
            return Err(SpawnError::TooLarge);
        }
    };
    sys.write_parent_mem(write_address, image);

    let child_flags = map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED);
    if let Err(error) = sys.map(
        child,
        memobj,
        0,
        stack_base_address,
        STACK_SIZE,
        child_flags,
    ) {
        let _ = sys.unmap(Handle::SELF_PROC, scratch, STACK_SIZE);
        let _ = sys.handle_close(memobj);
        let _ = sys.handle_close(child);
        return Err(SpawnError::Sys(error));
    }
    if let Err(error) = sys.unmap(Handle::SELF_PROC, scratch, STACK_SIZE) {
        let _ = sys.handle_close(memobj);
        let _ = sys.handle_close(child);
        return Err(SpawnError::Sys(error));
    }
    if let Err(error) = sys.handle_close(memobj) {
        let _ = sys.handle_close(child);
        return Err(SpawnError::Sys(error));
    }
    let sp0 = match usize::try_from(sp0) {
        Ok(sp0) => sp0,
        Err(_) => {
            let _ = sys.handle_close(child);
            return Err(SpawnError::TooLarge);
        }
    };
    match sys.thread_create(child, entry_address, sp0, sp0) {
        Ok(_thread) => Ok(child),
        Err(error) => {
            let _ = sys.handle_close(child);
            Err(SpawnError::Sys(error))
        }
    }
}

#[cfg(any(target_os = "none", test))]
fn map_step<S: Sys>(
    sys: &mut S,
    child: Handle,
    initfs_offset: u64,
    step: MapStep,
) -> Result<(), SpawnError> {
    let vaddr = usize::try_from(step.vaddr).map_err(|_| SpawnError::TooLarge)?;
    let len = usize::try_from(step.len).map_err(|_| SpawnError::TooLarge)?;
    let flags = step.prot as usize;
    match step.kind {
        StepKind::File { offset } => {
            let offset = initfs_offset
                .checked_add(offset)
                .ok_or(SpawnError::TooLarge)?;
            let offset = usize::try_from(offset).map_err(|_| SpawnError::TooLarge)?;
            sys.map(child, Handle::INITFS, offset, vaddr, len, flags)
                .map(|_| ())
                .map_err(SpawnError::Sys)
        }
        StepKind::Zero => sys
            .map(child, Handle::NONE, 0, vaddr, len, flags)
            .map(|_| ())
            .map_err(SpawnError::Sys),
        StepKind::CopyTail { .. } => Err(SpawnError::BadElf(ElfError::FileTailNotPageAligned)),
    }
}

/// The real syscall implementation used by [`spawn`].
#[derive(Clone, Copy, Debug, Default)]
pub struct RtSys;

impl RtSys {
    /// Wait for `process` to exit and store its exit code in `code`.
    ///
    /// This is a small convenience for the `spawn_demo` example and callers that do not need their
    /// own `Sys` implementation.
    pub fn proc_wait(&mut self, process: Handle, code: &mut i32) -> Result<(), Errno> {
        // SAFETY: PROC_WAIT takes a process handle and a writable i32 pointer owned by the caller.
        let result = unsafe {
            crate::sys::syscall2(
                Syscall::PROC_WAIT as usize,
                process.0 as usize,
                code as *mut i32 as usize,
            )
        };
        Errno::from_ret(result).map(|_| ())
    }
}

impl Sys for RtSys {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        // SAFETY: PROC_CREATE takes only the flags argument (zero).
        let result = unsafe { crate::sys::syscall1(Syscall::PROC_CREATE as usize, 0) };
        handle_from_ret(result)
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
        // SAFETY: MEMOBJ_CREATE takes a page-aligned length and zero flags.
        let result = unsafe { crate::sys::syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0) };
        handle_from_ret(result)
    }

    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, Errno> {
        // SAFETY: MAP takes scalar arguments only; the kernel validates the handles and range.
        let result = unsafe {
            crate::sys::syscall6(
                Syscall::MAP as usize,
                proc.0 as usize,
                memobj.0 as usize,
                offset,
                addr,
                len,
                flags,
            )
        };
        Errno::from_ret(result)
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
        // SAFETY: UNMAP takes scalar arguments only; the kernel validates the range.
        let result =
            unsafe { crate::sys::syscall3(Syscall::UNMAP as usize, proc.0 as usize, addr, len) };
        Errno::from_ret(result).map(|_| ())
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        // SAFETY: THREAD_CREATE takes scalar arguments; flags are zero.
        let result = unsafe {
            crate::sys::syscall5(
                Syscall::THREAD_CREATE as usize,
                proc.0 as usize,
                entry,
                stack_top,
                arg,
                0,
            )
        };
        handle_from_ret(result)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        // SAFETY: HANDLE_CLOSE takes a scalar handle.
        let result =
            unsafe { crate::sys::syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize) };
        Errno::from_ret(result).map(|_| ())
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        // SAFETY: `addr` is a writable mapping created by MAP in this process and `bytes` is a
        // distinct source slice; the mapping is large enough for the stack image.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
        }
    }
}

fn handle_from_ret(result: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(result)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

#[cfg(any(target_os = "none", test))]
const PAGE: u64 = 4096;
#[cfg(any(target_os = "none", test))]
const STACK_SIZE: usize = 64 * 1024;
#[cfg(any(target_os = "none", test))]
const WORD: usize = core::mem::size_of::<usize>();
#[cfg(any(target_os = "none", test))]
const MAX_AUXV: usize = 5;
#[cfg(any(target_os = "none", test))]
const AT_PAGESZ: usize = 6;
#[cfg(any(target_os = "none", test))]
const AT_PHDR: usize = 3;
#[cfg(any(target_os = "none", test))]
const AT_PHNUM: usize = 5;
#[cfg(any(target_os = "none", test))]
const AT_ENTRY: usize = 9;
#[cfg(any(target_os = "none", test))]
const AT_RANDOM: usize = 25;
#[cfg(any(target_os = "none", test))]
const RANDOM_LEN: usize = 16;

#[cfg(any(target_os = "none", test))]
#[cfg(target_arch = "x86_64")]
const MACHINE: Machine = Machine::X86_64;
#[cfg(any(target_os = "none", test))]
#[cfg(target_arch = "aarch64")]
const MACHINE: Machine = Machine::Aarch64;
#[cfg(any(target_os = "none", test))]
#[cfg(target_arch = "riscv64")]
const MACHINE: Machine = Machine::Riscv64;

#[cfg(any(target_os = "none", test))]
fn target_machine() -> Machine {
    MACHINE
}

#[cfg(any(target_os = "none", test))]
#[cfg(target_arch = "x86_64")]
const USER_MIN: u64 = raven_abi::layout::x86_64::USER_MIN;
#[cfg(any(target_os = "none", test))]
#[cfg(target_arch = "x86_64")]
const USER_END: u64 = raven_abi::layout::x86_64::USER_END;
// ABI v1 publishes the x86-64 layout only; use the same canonical user window on the other
// architectures until S12-S15/S19 define theirs.
#[cfg(any(target_os = "none", test))]
#[cfg(not(target_arch = "x86_64"))]
const USER_MIN: u64 = 0x0000_0000_0000_1000;
#[cfg(any(target_os = "none", test))]
#[cfg(not(target_arch = "x86_64"))]
const USER_END: u64 = 0x0000_7FFF_FFFF_F000;

#[cfg(any(target_os = "none", test))]
/// The stack top is the top of the user range; the stack occupies the 64 KiB below it.
const STACK_TOP: u64 = USER_END;

/// Build the C8 initial stack image in `buffer`.
///
/// `buffer` covers the top `buffer.len()` bytes of the child stack (`stack_top - buffer.len()` ..
/// `stack_top`). The image is written at the end of the buffer and the function returns the buffer
/// offset of `sp0` and the child virtual address of `sp0`. Strings are placed at the top in
/// argument/environment order; pointer arrays follow the C8 order. `AT_RANDOM` in `auxv` is
/// replaced with the address of `random`, which is placed immediately below the strings.
#[cfg(any(target_os = "none", test))]
fn build_stack(
    buffer: &mut [u8],
    stack_top: u64,
    path: &str,
    args: &[&str],
    env: &[&str],
    auxv: &[(usize, usize)],
    random: &[u8; RANDOM_LEN],
) -> Result<(usize, u64), SpawnError> {
    if !stack_top.is_multiple_of(16) || buffer.is_empty() {
        return Err(SpawnError::TooLarge);
    }
    let argc = if args.is_empty() { 1 } else { args.len() };
    let mut strings_len = 0usize;
    for index in 0..argc {
        let text = if args.is_empty() {
            path
        } else {
            args.get(index).copied().ok_or(SpawnError::TooLarge)?
        };
        strings_len = strings_len
            .checked_add(text.len())
            .and_then(|value| value.checked_add(1))
            .ok_or(SpawnError::TooLarge)?;
    }
    for text in env {
        strings_len = strings_len
            .checked_add(text.len())
            .and_then(|value| value.checked_add(1))
            .ok_or(SpawnError::TooLarge)?;
    }

    let auxv_words = auxv.len().checked_mul(2).ok_or(SpawnError::TooLarge)?;
    let words = 5usize
        .checked_add(argc)
        .and_then(|value| value.checked_add(env.len()))
        .and_then(|value| value.checked_add(auxv_words))
        .ok_or(SpawnError::TooLarge)?;
    let array_bytes = words.checked_mul(WORD).ok_or(SpawnError::TooLarge)?;

    let buffer_size = u64::try_from(buffer.len()).map_err(|_| SpawnError::TooLarge)?;
    let stack_base = stack_top
        .checked_sub(buffer_size)
        .ok_or(SpawnError::TooLarge)?;
    let strings_len = u64::try_from(strings_len).map_err(|_| SpawnError::TooLarge)?;
    let strings_start = stack_top
        .checked_sub(strings_len)
        .ok_or(SpawnError::TooLarge)?;
    let random_start = strings_start
        .checked_sub(RANDOM_LEN as u64)
        .ok_or(SpawnError::TooLarge)?;
    let array_bytes = u64::try_from(array_bytes).map_err(|_| SpawnError::TooLarge)?;
    let sp0 = align_down(
        random_start
            .checked_sub(array_bytes)
            .ok_or(SpawnError::TooLarge)?,
        16,
    );
    let offset = sp0.checked_sub(stack_base).ok_or(SpawnError::TooLarge)?;
    let offset = usize::try_from(offset).map_err(|_| SpawnError::TooLarge)?;
    let Some(image) = buffer.get_mut(offset..) else {
        return Err(SpawnError::TooLarge);
    };
    image.fill(0);

    write_word(buffer, offset, argc)?;
    let argv_offset = offset.checked_add(WORD).ok_or(SpawnError::TooLarge)?;
    let env_offset = argv_offset
        .checked_add(
            argc.checked_add(1)
                .and_then(|value| value.checked_mul(WORD))
                .ok_or(SpawnError::TooLarge)?,
        )
        .ok_or(SpawnError::TooLarge)?;
    let auxv_offset = env_offset
        .checked_add(
            env.len()
                .checked_add(1)
                .and_then(|value| value.checked_mul(WORD))
                .ok_or(SpawnError::TooLarge)?,
        )
        .ok_or(SpawnError::TooLarge)?;

    let mut cursor =
        usize::try_from(strings_start - stack_base).map_err(|_| SpawnError::TooLarge)?;
    for index in 0..argc {
        let text = if args.is_empty() {
            path
        } else {
            args.get(index).copied().ok_or(SpawnError::TooLarge)?
        };
        let pointer = write_string(buffer, &mut cursor, text, stack_base)?;
        write_word(
            buffer,
            argv_offset
                .checked_add(index.checked_mul(WORD).ok_or(SpawnError::TooLarge)?)
                .ok_or(SpawnError::TooLarge)?,
            pointer,
        )?;
    }
    write_word(
        buffer,
        argv_offset
            .checked_add(argc.checked_mul(WORD).ok_or(SpawnError::TooLarge)?)
            .ok_or(SpawnError::TooLarge)?,
        0,
    )?;

    for (index, text) in env.iter().enumerate() {
        let pointer = write_string(buffer, &mut cursor, text, stack_base)?;
        write_word(
            buffer,
            env_offset
                .checked_add(index.checked_mul(WORD).ok_or(SpawnError::TooLarge)?)
                .ok_or(SpawnError::TooLarge)?,
            pointer,
        )?;
    }
    write_word(
        buffer,
        env_offset
            .checked_add(env.len().checked_mul(WORD).ok_or(SpawnError::TooLarge)?)
            .ok_or(SpawnError::TooLarge)?,
        0,
    )?;

    let random_offset =
        usize::try_from(random_start - stack_base).map_err(|_| SpawnError::TooLarge)?;
    let random_end = random_offset
        .checked_add(RANDOM_LEN)
        .ok_or(SpawnError::TooLarge)?;
    let Some(slot) = buffer.get_mut(random_offset..random_end) else {
        return Err(SpawnError::TooLarge);
    };
    slot.copy_from_slice(random);

    for (index, (key, value)) in auxv.iter().enumerate() {
        let value = if *key == AT_RANDOM {
            usize::try_from(random_start).map_err(|_| SpawnError::TooLarge)?
        } else {
            *value
        };
        let pair_offset = auxv_offset
            .checked_add(
                index
                    .checked_mul(2)
                    .and_then(|value| value.checked_mul(WORD))
                    .ok_or(SpawnError::TooLarge)?,
            )
            .ok_or(SpawnError::TooLarge)?;
        write_word(buffer, pair_offset, *key)?;
        write_word(
            buffer,
            pair_offset.checked_add(WORD).ok_or(SpawnError::TooLarge)?,
            value,
        )?;
    }
    let terminator = auxv_offset
        .checked_add(
            auxv.len()
                .checked_mul(2)
                .and_then(|value| value.checked_mul(WORD))
                .ok_or(SpawnError::TooLarge)?,
        )
        .ok_or(SpawnError::TooLarge)?;
    write_word(buffer, terminator, 0)?;
    write_word(
        buffer,
        terminator.checked_add(WORD).ok_or(SpawnError::TooLarge)?,
        0,
    )?;

    Ok((offset, sp0))
}

#[cfg(any(target_os = "none", test))]
fn write_word(buffer: &mut [u8], offset: usize, value: usize) -> Result<(), SpawnError> {
    let end = offset.checked_add(WORD).ok_or(SpawnError::TooLarge)?;
    let Some(slot) = buffer.get_mut(offset..end) else {
        return Err(SpawnError::TooLarge);
    };
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(any(target_os = "none", test))]
fn write_string(
    buffer: &mut [u8],
    cursor: &mut usize,
    text: &str,
    stack_base: u64,
) -> Result<usize, SpawnError> {
    let start = stack_base
        .checked_add(u64::try_from(*cursor).map_err(|_| SpawnError::TooLarge)?)
        .ok_or(SpawnError::TooLarge)?;
    let end = cursor.checked_add(text.len()).ok_or(SpawnError::TooLarge)?;
    let Some(slot) = buffer.get_mut(*cursor..end) else {
        return Err(SpawnError::TooLarge);
    };
    slot.copy_from_slice(text.as_bytes());
    let Some(terminator) = buffer.get_mut(end) else {
        return Err(SpawnError::TooLarge);
    };
    *terminator = 0;
    *cursor = end.checked_add(1).ok_or(SpawnError::TooLarge)?;
    usize::try_from(start).map_err(|_| SpawnError::TooLarge)
}

#[cfg(any(target_os = "none", test))]
fn align_down(value: u64, align: u64) -> u64 {
    value - value % align
}

#[cfg(any(target_os = "none", test))]
/// Fill 16 bytes for `AT_RANDOM`.
///
/// The value mixes the monotonic clock with a parent address. It is deliberately not
/// cryptographic yet; the S24/M2 integration only needs distinct bytes per spawned child.
fn random_bytes(parent_address: usize) -> [u8; RANDOM_LEN] {
    #[cfg(target_os = "none")]
    let now = crate::sys::clock_get(raven_abi::types::clock::MONOTONIC).unwrap_or_default();
    #[cfg(not(target_os = "none"))]
    let now = raven_abi::types::Timespec::default();
    let mut state = (now.tv_sec as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (now.tv_nsec as u64)
        ^ (parent_address as u64);
    let mut bytes = [0u8; RANDOM_LEN];
    for chunk in bytes.chunks_mut(8) {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let word = state.to_le_bytes();
        chunk.copy_from_slice(&word[..chunk.len()]);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TOP: u64 = 0x8000_0000;
    const TEST_BUFFER: usize = 64 * 1024;

    fn build(
        path: &str,
        args: &[&str],
        env: &[&str],
        auxv: &[(usize, usize)],
    ) -> (Vec<u8>, usize, u64) {
        let mut buffer = vec![0u8; TEST_BUFFER];
        let (offset, sp0) = build_stack(
            &mut buffer,
            TEST_TOP,
            path,
            args,
            env,
            auxv,
            &[0xAB; RANDOM_LEN],
        )
        .expect("stack image should fit");
        (buffer, offset, sp0)
    }

    fn base() -> u64 {
        TEST_TOP - TEST_BUFFER as u64
    }

    fn word(buffer: &[u8], address: u64) -> usize {
        let offset = usize::try_from(address - base()).unwrap();
        usize::from_le_bytes(buffer[offset..offset + WORD].try_into().unwrap())
    }

    fn bytes_at(buffer: &[u8], address: usize) -> Vec<u8> {
        let address = address as u64;
        let offset = usize::try_from(address - base()).unwrap();
        let end = buffer[offset..]
            .iter()
            .position(|&byte| byte == 0)
            .map(|length| offset + length)
            .unwrap();
        buffer[offset..end].to_vec()
    }

    #[test]
    fn empty_args_use_the_path_as_argv_zero() {
        let (buffer, _, sp0) = build("/child", &[], &[], &[]);
        assert_eq!(word(&buffer, sp0), 1);
        assert_eq!(
            bytes_at(&buffer, word(&buffer, sp0 + WORD as u64)),
            b"/child"
        );
    }

    #[test]
    fn one_argument_layout() {
        let (buffer, _, sp0) = build("/child", &["/child"], &[], &[]);
        assert_eq!(word(&buffer, sp0), 1);
        assert_eq!(
            bytes_at(&buffer, word(&buffer, sp0 + WORD as u64)),
            b"/child"
        );
    }

    #[test]
    fn three_argument_layout() {
        let (buffer, _, sp0) = build("/child", &["a", "b", "c"], &[], &[]);
        assert_eq!(word(&buffer, sp0), 3);
        for (index, expected) in ["a", "b", "c"].iter().enumerate() {
            let pointer = word(&buffer, sp0 + WORD as u64 * (index as u64 + 1));
            assert_eq!(bytes_at(&buffer, pointer), expected.as_bytes());
        }
        assert_eq!(word(&buffer, sp0 + WORD as u64 * 4), 0);
    }

    #[test]
    fn empty_strings_are_preserved() {
        let (buffer, _, sp0) = build("/child", &["", "x", ""], &[], &[]);
        assert_eq!(word(&buffer, sp0), 3);
        assert!(bytes_at(&buffer, word(&buffer, sp0 + WORD as u64)).is_empty());
        assert_eq!(
            bytes_at(&buffer, word(&buffer, sp0 + WORD as u64 * 3)),
            b"".to_vec()
        );
    }

    #[test]
    fn env_pointers_follow_argv_null() {
        let (buffer, _, sp0) = build("/child", &["/child"], &["A=1", "B=2"], &[]);
        let argc = word(&buffer, sp0) as u64;
        let argv_null = sp0 + WORD as u64 * (1 + argc);
        assert_eq!(word(&buffer, argv_null), 0);
        let env0 = argv_null + WORD as u64;
        let env1 = env0 + WORD as u64;
        assert_eq!(bytes_at(&buffer, word(&buffer, env0)), b"A=1");
        assert_eq!(bytes_at(&buffer, word(&buffer, env1)), b"B=2");
        assert_eq!(word(&buffer, env1 + WORD as u64), 0);
    }

    #[test]
    fn auxv_pairs_and_terminator() {
        let (buffer, _, sp0) = build("/child", &["/child"], &[], &[(6, 4096), (9, 0x400000)]);
        let argc = word(&buffer, sp0) as u64;
        let argv_null = sp0 + WORD as u64 * (1 + argc);
        let env0 = argv_null + WORD as u64;
        let env_null = env0;
        assert_eq!(word(&buffer, env_null), 0);
        let auxv0 = env_null + WORD as u64;
        assert_eq!(word(&buffer, auxv0), 6);
        assert_eq!(word(&buffer, auxv0 + WORD as u64), 4096);
        assert_eq!(word(&buffer, auxv0 + 2 * WORD as u64), 9);
        assert_eq!(word(&buffer, auxv0 + 3 * WORD as u64), 0x400000);
        assert_eq!(word(&buffer, auxv0 + 4 * WORD as u64), 0);
        assert_eq!(word(&buffer, auxv0 + 5 * WORD as u64), 0);
    }

    #[test]
    fn at_random_points_at_sixteen_bytes() {
        let (buffer, _, sp0) = build("/child", &["/child"], &[], &[(25, 0)]);
        let argc = word(&buffer, sp0) as u64;
        let argv_null = sp0 + WORD as u64 * (1 + argc);
        let auxv0 = argv_null + 2 * WORD as u64;
        let pointer = word(&buffer, auxv0 + WORD as u64);
        let offset = usize::try_from(pointer as u64 - base()).unwrap();
        assert_eq!(buffer[offset..offset + RANDOM_LEN], [0xAB; RANDOM_LEN]);
    }

    #[test]
    fn sp0_is_sixteen_byte_aligned() {
        let (_, _, sp0) = build("/child", &["a", "b"], &["A=1"], &[]);
        assert_eq!(sp0 % 16, 0);
    }

    #[test]
    fn all_pointers_are_inside_the_stack_mapping() {
        let (buffer, _, sp0) = build("/child", &["a", "bc"], &["A=1"], &[(6, 4096)]);
        let argc = word(&buffer, sp0) as u64;
        for index in 0..argc {
            let pointer = word(&buffer, sp0 + WORD as u64 * (index + 1));
            assert!(pointer as u64 >= base() && (pointer as u64) < TEST_TOP);
        }
        let env0 = sp0 + WORD as u64 * (1 + argc + 1);
        let pointer = word(&buffer, env0);
        assert!(pointer as u64 >= base() && (pointer as u64) < TEST_TOP);
    }

    #[test]
    fn strings_are_in_argument_order() {
        let (buffer, _, sp0) = build("/child", &["first", "second", "third"], &[], &[]);
        let first = word(&buffer, sp0 + WORD as u64);
        let second = word(&buffer, sp0 + 2 * WORD as u64);
        let third = word(&buffer, sp0 + 3 * WORD as u64);
        assert!(first < second && second < third);
        assert_eq!(bytes_at(&buffer, first), b"first");
        assert_eq!(bytes_at(&buffer, second), b"second");
        assert_eq!(bytes_at(&buffer, third), b"third");
    }

    #[test]
    fn environment_strings_follow_argument_strings() {
        let (buffer, _, sp0) = build("/child", &["arg"], &["ENV=1"], &[]);
        let arg = word(&buffer, sp0 + WORD as u64);
        let env0 = sp0 + WORD as u64 * 3;
        let env = word(&buffer, env0);
        assert!(arg < env);
        assert_eq!(bytes_at(&buffer, env), b"ENV=1");
    }

    #[test]
    fn overlong_strings_are_too_large() {
        let long = "x".repeat(TEST_BUFFER + 1);
        let mut buffer = vec![0u8; TEST_BUFFER];
        assert!(matches!(
            build_stack(
                &mut buffer,
                TEST_TOP,
                "/child",
                &[&long],
                &[],
                &[],
                &[0; 16]
            ),
            Err(SpawnError::TooLarge)
        ));
    }

    #[test]
    fn too_many_arguments_are_too_large() {
        let names: Vec<&str> = vec![""; 9000];
        let mut buffer = vec![0u8; TEST_BUFFER];
        assert!(matches!(
            build_stack(&mut buffer, TEST_TOP, "/child", &names, &[], &[], &[0; 16]),
            Err(SpawnError::TooLarge)
        ));
    }

    #[test]
    fn misaligned_stack_top_is_rejected() {
        let mut buffer = vec![0u8; TEST_BUFFER];
        assert!(matches!(
            build_stack(&mut buffer, TEST_TOP + 1, "/child", &[], &[], &[], &[0; 16]),
            Err(SpawnError::TooLarge)
        ));
    }

    #[test]
    fn small_buffer_is_rejected() {
        let mut buffer = [0u8; 64];
        assert!(matches!(
            build_stack(&mut buffer, TEST_TOP, "/child", &["a"], &[], &[], &[0; 16]),
            Err(SpawnError::TooLarge)
        ));
    }

    #[test]
    fn image_only_touches_the_top_of_the_buffer() {
        let mut buffer = vec![0xFFu8; TEST_BUFFER];
        let (offset, _) =
            build_stack(&mut buffer, TEST_TOP, "/child", &["a"], &[], &[], &[0; 16]).unwrap();
        assert_eq!(&buffer[..offset], vec![0xFFu8; offset]);
        assert!(buffer[offset..].iter().any(|&byte| byte != 0xFF));
    }

    #[test]
    fn no_environment_has_one_null_word() {
        let (buffer, _, sp0) = build("/child", &["/child"], &[], &[]);
        let argc = word(&buffer, sp0) as u64;
        let env0 = sp0 + WORD as u64 * (1 + argc + 1);
        assert_eq!(word(&buffer, env0), 0);
    }

    #[test]
    fn auxv_without_random_keeps_the_value() {
        let (buffer, _, sp0) = build("/child", &["/child"], &[], &[(6, 4096)]);
        let argc = word(&buffer, sp0) as u64;
        let argv_null = sp0 + WORD as u64 * (1 + argc);
        let auxv0 = argv_null + 2 * WORD as u64;
        assert_eq!(word(&buffer, auxv0), 6);
        assert_eq!(word(&buffer, auxv0 + WORD as u64), 4096);
    }

    #[test]
    fn same_inputs_produce_the_same_image() {
        let first = build("/child", &["a", "b"], &["A=1"], &[(6, 4096)]);
        let second = build("/child", &["a", "b"], &["A=1"], &[(6, 4096)]);
        assert_eq!(first.1, second.1);
        assert_eq!(first.2, second.2);
        assert_eq!(first.0, second.0);
    }
}

#[cfg(test)]
mod mock_tests {
    use super::*;
    use raven_abi::layout::x86_64::USER_END;
    use std::cell::RefCell;

    const CHILD: Handle = Handle(0x10);
    const MEMOBJ: Handle = Handle(0x20);
    const THREAD: Handle = Handle(0x30);
    const SCRATCH: usize = 0x7000_0000;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Call {
        ProcCreate,
        MemobjCreate(usize),
        Map {
            proc: u32,
            memobj: u32,
            offset: usize,
            addr: usize,
            len: usize,
            flags: usize,
        },
        Unmap {
            proc: u32,
            addr: usize,
            len: usize,
        },
        ThreadCreate {
            proc: u32,
            entry: usize,
            stack_top: usize,
            arg: usize,
        },
        HandleClose(u32),
        WriteParentMem {
            addr: usize,
            len: usize,
        },
    }

    #[derive(Default)]
    struct Mock {
        calls: RefCell<Vec<Call>>,
        fail_proc: bool,
        fail_memobj: bool,
        fail_map: Option<usize>,
        fail_unmap: bool,
        fail_close: Option<u32>,
        fail_thread: bool,
        maps: usize,
        image: RefCell<Vec<u8>>,
    }

    impl Mock {
        fn calls(&self) -> Vec<Call> {
            self.calls.borrow().clone()
        }

        fn push(&self, call: Call) {
            self.calls.borrow_mut().push(call);
        }

        fn map_failure(&mut self) -> bool {
            self.maps += 1;
            self.fail_map == Some(self.maps)
        }
    }

    impl Sys for Mock {
        fn proc_create(&mut self) -> Result<Handle, Errno> {
            self.push(Call::ProcCreate);
            if self.fail_proc {
                Err(Errno::EIO)
            } else {
                Ok(CHILD)
            }
        }

        fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
            self.push(Call::MemobjCreate(len));
            if self.fail_memobj {
                Err(Errno::EIO)
            } else {
                Ok(MEMOBJ)
            }
        }

        fn map(
            &mut self,
            proc: Handle,
            memobj: Handle,
            offset: usize,
            addr: usize,
            len: usize,
            flags: usize,
        ) -> Result<usize, Errno> {
            self.push(Call::Map {
                proc: proc.0,
                memobj: memobj.0,
                offset,
                addr,
                len,
                flags,
            });
            if self.map_failure() {
                return Err(Errno::EIO);
            }
            if proc == Handle::SELF_PROC && addr == 0 {
                Ok(SCRATCH)
            } else {
                Ok(addr)
            }
        }

        fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
            self.push(Call::Unmap {
                proc: proc.0,
                addr,
                len,
            });
            if self.fail_unmap {
                Err(Errno::EIO)
            } else {
                Ok(())
            }
        }

        fn thread_create(
            &mut self,
            proc: Handle,
            entry: usize,
            stack_top: usize,
            arg: usize,
        ) -> Result<Handle, Errno> {
            self.push(Call::ThreadCreate {
                proc: proc.0,
                entry,
                stack_top,
                arg,
            });
            if self.fail_thread {
                Err(Errno::EIO)
            } else {
                Ok(THREAD)
            }
        }

        fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
            self.push(Call::HandleClose(handle.0));
            if self.fail_close == Some(handle.0) {
                Err(Errno::EIO)
            } else {
                Ok(())
            }
        }

        fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
            self.push(Call::WriteParentMem {
                addr,
                len: bytes.len(),
            });
            *self.image.borrow_mut() = bytes.to_vec();
        }
    }

    fn child_elf(interp: bool, partial_page: bool) -> Vec<u8> {
        const PAGE: usize = 4096;
        const SEGMENT: u64 = 0x0040_0000;
        let mut elf = vec![0u8; PAGE];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[6] = 1;
        put_u16(&mut elf, 16, 2);
        put_u16(&mut elf, 18, 62);
        put_u32(&mut elf, 20, 1);
        put_u64(&mut elf, 24, SEGMENT + 0x100);
        put_u64(&mut elf, 32, 64);
        put_u16(&mut elf, 52, 64);
        put_u16(&mut elf, 54, 56);
        put_u16(&mut elf, 56, if interp { 2 } else { 1 });

        put_u32(&mut elf, 64, 1);
        put_u32(&mut elf, 68, 5);
        put_u64(&mut elf, 72, 0);
        put_u64(&mut elf, 80, SEGMENT);
        put_u64(&mut elf, 88, 0);
        put_u64(&mut elf, 96, if partial_page { 0x800 } else { 0x1000 });
        put_u64(&mut elf, 104, 0x1000);
        put_u64(&mut elf, 112, 0x1000);

        if interp {
            put_u32(&mut elf, 120, 3);
            put_u32(&mut elf, 124, 4);
            put_u64(&mut elf, 128, 0x200);
            put_u64(&mut elf, 136, 0);
            put_u64(&mut elf, 144, 0);
            put_u64(&mut elf, 152, 11);
            put_u64(&mut elf, 160, 11);
            put_u64(&mut elf, 168, 1);
            elf[0x200..0x20b].copy_from_slice(b"/lib/ld.so\0");
        }

        elf[0x100..0x10b].copy_from_slice(&[
            0xb8, 0x01, 0x00, 0x00, 0x00, 0x31, 0xff, 0x0f, 0x05, 0x0f, 0x0b,
        ]);
        elf
    }

    fn initfs_with(elf: &[u8]) -> Vec<u8> {
        const IMAGE: usize = 8192;
        let mut image = vec![0u8; IMAGE];
        image[0..8].copy_from_slice(b"RAVENFS1");
        put_u32(&mut image, 8, 1);
        put_u32(&mut image, 12, 64);
        put_u32(&mut image, 16, 1);
        put_u32(&mut image, 20, 48);
        put_u64(&mut image, 24, 112);
        put_u64(&mut image, 32, 6);
        put_u64(&mut image, 40, IMAGE as u64);
        put_u32(&mut image, 64, 0);
        put_u32(&mut image, 68, 6);
        put_u64(&mut image, 72, 4096);
        put_u64(&mut image, 80, 4096);
        put_u32(&mut image, 88, 0o755);
        put_u32(&mut image, 92, raven_initfs::crc32(elf));
        image[112..118].copy_from_slice(b"/child");
        image[4096..8192].copy_from_slice(elf);
        image
    }

    fn valid_initfs() -> Vec<u8> {
        initfs_with(&child_elf(false, false))
    }

    fn put_u16(data: &mut [u8], offset: usize, value: u16) {
        data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(data: &mut [u8], offset: usize, value: u64) {
        data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn options<'a>(args: &'a [&'a str], env: &'a [&'a str]) -> SpawnOptions<'a> {
        SpawnOptions { args, env }
    }

    fn run(mock: &mut Mock) -> Result<Handle, SpawnError> {
        let image = valid_initfs();
        let initfs = Initfs::parse(&image).unwrap();
        spawn_with(&initfs, "/child", &options(&["/child", "world"], &[]), mock)
    }

    #[test]
    fn success_call_sequence() {
        let image = valid_initfs();
        let initfs = Initfs::parse(&image).unwrap();
        let data_offset = initfs.get("/child").unwrap().data_offset as usize;
        let mut mock = Mock::default();
        let child = spawn_with(
            &initfs,
            "/child",
            &options(&["/child", "world"], &["A=B"]),
            &mut mock,
        )
        .unwrap();
        assert_eq!(child, CHILD);
        let calls = mock.calls();
        assert_eq!(calls.len(), 9, "{calls:?}");
        assert_eq!(calls[0], Call::ProcCreate);
        assert_eq!(
            calls[1],
            Call::Map {
                proc: CHILD.0,
                memobj: Handle::INITFS.0,
                offset: data_offset,
                addr: 0x0040_0000,
                len: 0x1000,
                flags: prot::READ | prot::EXEC,
            }
        );
        assert_eq!(calls[2], Call::MemobjCreate(STACK_SIZE));
        assert_eq!(
            calls[3],
            Call::Map {
                proc: Handle::SELF_PROC.0,
                memobj: MEMOBJ.0,
                offset: 0,
                addr: 0,
                len: STACK_SIZE,
                flags: map::pack(prot::READ | prot::WRITE, map::SHARED),
            }
        );
        assert!(matches!(&calls[4], Call::WriteParentMem { len, .. } if *len > 0));
        let (stack_base, flags) = match &calls[5] {
            Call::Map { addr, flags, .. } => (*addr, *flags),
            other => panic!("expected child stack map, got {other:?}"),
        };
        assert_eq!(stack_base, usize::try_from(USER_END).unwrap() - STACK_SIZE);
        assert_eq!(
            flags,
            map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED)
        );
        assert_eq!(
            calls[6],
            Call::Unmap {
                proc: Handle::SELF_PROC.0,
                addr: SCRATCH,
                len: STACK_SIZE,
            }
        );
        assert_eq!(calls[7], Call::HandleClose(MEMOBJ.0));
        assert!(matches!(
            &calls[8],
            Call::ThreadCreate {
                proc,
                entry: 0x0040_0100,
                stack_top,
                arg
            } if *proc == CHILD.0 && stack_top == arg && *stack_top >= stack_base
        ));
    }

    #[test]
    fn not_found_does_nothing() {
        let image = valid_initfs();
        let initfs = Initfs::parse(&image).unwrap();
        let mut mock = Mock::default();
        let error = spawn_with(&initfs, "/missing", &options(&[], &[]), &mut mock).unwrap_err();
        assert_eq!(error, SpawnError::NotFound);
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn interpreter_is_rejected() {
        let image = initfs_with(&child_elf(true, false));
        let initfs = Initfs::parse(&image).unwrap();
        let mut mock = Mock::default();
        let error = spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap_err();
        assert_eq!(error, SpawnError::HasInterpreter);
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn strict_tail_is_rejected() {
        let image = initfs_with(&child_elf(false, true));
        let initfs = Initfs::parse(&image).unwrap();
        let mut mock = Mock::default();
        let error = spawn_with(&initfs, "/child", &options(&[], &[]), &mut mock).unwrap_err();
        assert_eq!(error, SpawnError::BadElf(ElfError::FileTailNotPageAligned));
        assert!(mock.calls().is_empty());
    }

    #[test]
    fn proc_create_failure_has_nothing_to_clean() {
        let mut mock = Mock {
            fail_proc: true,
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        assert_eq!(mock.calls(), vec![Call::ProcCreate]);
    }

    #[test]
    fn segment_map_failure_closes_the_child() {
        let mut mock = Mock {
            fail_map: Some(1),
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
        assert!(!mock.calls().contains(&Call::MemobjCreate(STACK_SIZE)));
    }

    #[test]
    fn memobj_failure_closes_the_child() {
        let mut mock = Mock {
            fail_memobj: true,
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
    }

    #[test]
    fn scratch_map_failure_closes_memobj_and_child() {
        let mut mock = Mock {
            fail_map: Some(2),
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        let calls = mock.calls();
        assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
        assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
    }

    #[test]
    fn child_stack_map_failure_unmaps_and_closes() {
        let mut mock = Mock {
            fail_map: Some(3),
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        let calls = mock.calls();
        assert!(calls.contains(&Call::Unmap {
            proc: Handle::SELF_PROC.0,
            addr: SCRATCH,
            len: STACK_SIZE,
        }));
        assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
        assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
    }

    #[test]
    fn unmap_failure_closes_memobj_and_child() {
        let mut mock = Mock {
            fail_unmap: true,
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        let calls = mock.calls();
        assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
        assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
    }

    #[test]
    fn memobj_close_failure_closes_the_child() {
        let mut mock = Mock {
            fail_close: Some(MEMOBJ.0),
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
    }

    #[test]
    fn thread_create_failure_closes_the_child() {
        let mut mock = Mock {
            fail_thread: true,
            ..Mock::default()
        };
        let error = run(&mut mock).unwrap_err();
        assert_eq!(error, SpawnError::Sys(Errno::EIO));
        assert_eq!(mock.calls().last(), Some(&Call::HandleClose(CHILD.0)));
    }

    #[test]
    fn oversized_stack_image_cleans_up() {
        let long = "x".repeat(70_000);
        let image = valid_initfs();
        let initfs = Initfs::parse(&image).unwrap();
        let mut mock = Mock::default();
        let error = spawn_with(&initfs, "/child", &options(&[&long], &[]), &mut mock).unwrap_err();
        assert_eq!(error, SpawnError::TooLarge);
        let calls = mock.calls();
        assert!(calls.contains(&Call::Unmap {
            proc: Handle::SELF_PROC.0,
            addr: SCRATCH,
            len: STACK_SIZE,
        }));
        assert!(calls.contains(&Call::HandleClose(MEMOBJ.0)));
        assert_eq!(calls.last(), Some(&Call::HandleClose(CHILD.0)));
        assert!(
            !calls
                .iter()
                .any(|call| matches!(call, Call::ThreadCreate { .. }))
        );
    }

    #[test]
    fn parent_memory_contains_the_c8_stack() {
        let mut mock = Mock::default();
        run(&mut mock).unwrap();
        let image = mock.image.borrow();
        assert!(!image.is_empty());
        assert!(image.len() <= STACK_SIZE);
    }
}
