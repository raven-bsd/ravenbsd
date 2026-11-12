// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Create a child process from an ELF file in the initfs or on a file server.
//!
//! The kernel has no `exec`. This module plans the image with `raven-elf`, maps its segments and a
//! fresh stack into a new empty process, fills the stack through a shared memory object (CONTRACTS
//! C13), and starts the first thread. The caller receives the child's process handle and may wait
//! for it with `PROC_WAIT`.
//!
//! Two image sources are supported: the read-only initfs archive (zero-copy segment mapping) and
//! bytes read from a `file:` path (bounded copy into a memory object, then segment mapping). The
//! latter is how root services start from a UFS root.

#[cfg(all(target_os = "none", feature = "alloc"))]
use alloc_crate::vec::Vec;
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
#[cfg(all(target_os = "none", feature = "alloc"))]
use raven_scheme::Client;

/// Options for [`spawn`].
#[derive(Clone, Copy, Debug)]
pub struct SpawnOptions<'a> {
    /// Arguments for the child. `argv[0]` defaults to the path when `args` is empty.
    pub args: &'a [&'a str],
    /// Environment strings, each in `KEY=VALUE` form.
    pub env: &'a [&'a str],
}

/// Largest executable image accepted from a `file:` server.
///
/// Userland binaries are a few MiB even in a debug build; the bound keeps a hostile or corrupt
/// server from making PID 1 allocate without limit.
pub const MAX_FILE_IMAGE: usize = 8 * 1024 * 1024;

/// Why a spawn failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnError {
    /// The path is not present in the initfs.
    NotFound,
    /// The path is not present on the file server.
    FileNotFound,
    /// The ELF image is invalid or cannot be loaded strictly.
    BadElf(ElfError),
    /// The ELF requests an interpreter, which the M1 loader does not support.
    HasInterpreter,
    /// The stack image, an address or a file image exceeds its bound.
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
    /// Change the protection of a mapped range.
    ///
    /// The default keeps older test doubles compiling; the real runtime and svcd's grant wrapper
    /// implement it because file-image text is mapped writable first.
    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        let _ = (proc, addr, len, protection);
        Err(Errno::ENOSYS)
    }
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
    let steps = plan_load(&elf, PAGE, LoadMode::Strict).map_err(SpawnError::BadElf)?;
    spawn_elf(
        &elf,
        steps,
        ImageSource::Initfs {
            data_offset: entry.data_offset,
        },
        path,
        options,
        sys,
    )
}

/// Spawn `path` from an in-memory ELF image.
///
/// The image must be at most [`MAX_FILE_IMAGE`] bytes; callers normally obtain it from
/// [`read_file_image`]. Segments are copied into a temporary memory object, mapped into the child
/// and the object is closed before the first thread starts, so the child keeps only its own
/// mappings.
#[cfg(any(target_os = "none", test))]
pub fn spawn_image_with<S: Sys>(
    image: &[u8],
    path: &str,
    options: &SpawnOptions<'_>,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    if image.len() > MAX_FILE_IMAGE {
        return Err(SpawnError::TooLarge);
    }
    let elf =
        Elf::parse(image, target_machine(), USER_MIN..USER_END).map_err(SpawnError::BadElf)?;
    if elf.interp().is_some() {
        return Err(SpawnError::HasInterpreter);
    }
    let steps = plan_load(&elf, PAGE, LoadMode::Strict).map_err(SpawnError::BadElf)?;
    spawn_elf(&elf, steps, ImageSource::Bytes(image), path, options, sys)
}

/// Spawn `path` from an in-memory image with the real syscall implementation.
#[cfg(target_os = "none")]
pub fn spawn_image(
    image: &[u8],
    path: &str,
    options: &SpawnOptions<'_>,
) -> Result<Handle, SpawnError> {
    let mut sys = RtSys;
    spawn_image_with(image, path, options, &mut sys)
}

/// Read an open `file:` handle into a bounded image buffer.
#[cfg(all(target_os = "none", feature = "alloc"))]
pub fn read_file_handle(file: Handle) -> Result<Vec<u8>, SpawnError> {
    let client = Client::new();
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let read = client.read(file, 0, &mut buffer).map_err(SpawnError::Sys)?;
        if read == 0 {
            return Ok(data);
        }
        let next = data.len().checked_add(read).ok_or(SpawnError::TooLarge)?;
        if next > MAX_FILE_IMAGE {
            return Err(SpawnError::TooLarge);
        }
        let chunk = buffer.get(..read).ok_or(SpawnError::TooLarge)?;
        data.extend_from_slice(chunk);
    }
}

/// Serializes `file:` image reads in one process.
#[cfg(all(target_os = "none", feature = "alloc"))]
static FILE_READ_LOCK: crate::sync::Mutex<()> = crate::sync::Mutex::new(());

/// Open and read a `file:` path into a bounded image buffer.
#[cfg(all(target_os = "none", feature = "alloc"))]
pub fn read_file_image(path: &str) -> Result<Vec<u8>, SpawnError> {
    let _guard = FILE_READ_LOCK.lock();
    let mut file = crate::fs::open(path, 0).map_err(|error| {
        if error == Errno::ENOENT {
            SpawnError::FileNotFound
        } else {
            SpawnError::Sys(error)
        }
    })?;
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let read = file.read(&mut buffer).map_err(SpawnError::Sys)?;
        if read == 0 {
            return Ok(data);
        }
        let next = data.len().checked_add(read).ok_or(SpawnError::TooLarge)?;
        if next > MAX_FILE_IMAGE {
            return Err(SpawnError::TooLarge);
        }
        let chunk = buffer.get(..read).ok_or(SpawnError::TooLarge)?;
        data.extend_from_slice(chunk);
    }
}

/// One executable source.
#[cfg(any(target_os = "none", test))]
enum ImageSource<'a> {
    /// A file inside the mapped initfs archive.
    Initfs {
        /// Offset of the ELF in the archive memory object.
        data_offset: u64,
    },
    /// An in-memory ELF image.
    Bytes(&'a [u8]),
}

/// Where mapped pages read their file data from.
#[cfg(any(target_os = "none", test))]
#[derive(Clone, Copy)]
enum MapSource {
    /// The initfs memory object, with the archive offset already added per step.
    Initfs { data_offset: u64 },
    /// A memory object holding the whole file image at offset zero.
    Object(Handle),
}

#[cfg(any(target_os = "none", test))]
fn spawn_elf<S: Sys, I: IntoIterator<Item = MapStep>>(
    elf: &Elf<'_>,
    steps: I,
    source: ImageSource<'_>,
    path: &str,
    options: &SpawnOptions<'_>,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    let (map_source, image_object) = match source {
        ImageSource::Initfs { data_offset, .. } => (MapSource::Initfs { data_offset }, None),
        ImageSource::Bytes(bytes) => {
            let page_len = bytes
                .len()
                .checked_add(PAGE as usize - 1)
                .ok_or(SpawnError::TooLarge)?
                / PAGE as usize
                * PAGE as usize;
            if page_len == 0 {
                return Err(SpawnError::BadElf(ElfError::TooShort));
            }
            let object = sys.memobj_create(page_len).map_err(SpawnError::Sys)?;
            let scratch_flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
            let scratch = match sys.map(Handle::SELF_PROC, object, 0, 0, page_len, scratch_flags) {
                Ok(address) => address,
                Err(error) => {
                    let _ = sys.handle_close(object);
                    return Err(SpawnError::Sys(error));
                }
            };
            sys.write_parent_mem(scratch, bytes);
            if let Err(error) = sys.unmap(Handle::SELF_PROC, scratch, page_len) {
                let _ = sys.handle_close(object);
                return Err(SpawnError::Sys(error));
            }
            (MapSource::Object(object), Some(object))
        }
    };

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

    let child = match sys.proc_create() {
        Ok(child) => child,
        Err(error) => {
            if let Some(object) = image_object {
                let _ = sys.handle_close(object);
            }
            return Err(SpawnError::Sys(error));
        }
    };
    for step in steps {
        if let Err(error) = map_step(sys, child, map_source, step) {
            if let Some(object) = image_object {
                let _ = sys.handle_close(object);
            }
            let _ = sys.handle_close(child);
            return Err(error);
        }
    }
    if let Some(object) = image_object
        && let Err(error) = sys.handle_close(object)
    {
        let _ = sys.handle_close(child);
        return Err(SpawnError::Sys(error));
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
    source: MapSource,
    step: MapStep,
) -> Result<(), SpawnError> {
    let vaddr = usize::try_from(step.vaddr).map_err(|_| SpawnError::TooLarge)?;
    let len = usize::try_from(step.len).map_err(|_| SpawnError::TooLarge)?;
    let flags = step.prot as usize;
    match step.kind {
        StepKind::File { offset } => {
            let (handle, offset) = match source {
                MapSource::Initfs { data_offset } => {
                    let offset = data_offset
                        .checked_add(offset)
                        .ok_or(SpawnError::TooLarge)?;
                    (Handle::INITFS, offset)
                }
                MapSource::Object(handle) => (handle, offset),
            };
            let offset = usize::try_from(offset).map_err(|_| SpawnError::TooLarge)?;
            // A generic memory object has no EXEC right in ABI v1. Text is therefore populated
            // read-write and protected to read-execute before the child's first thread exists;
            // no running thread ever observes the writable mapping.
            let executable_object =
                matches!(source, MapSource::Object(_)) && flags & prot::EXEC != 0;
            let map_flags = if executable_object {
                (flags & !prot::EXEC) | prot::WRITE
            } else {
                flags
            };
            sys.map(child, handle, offset, vaddr, len, map_flags)
                .map(|_| ())
                .map_err(SpawnError::Sys)?;
            if executable_object {
                sys.protect(child, vaddr, len, flags as u8)
                    .map_err(SpawnError::Sys)?;
            }
            Ok(())
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

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        // SAFETY: PROTECT takes scalar arguments; the kernel validates the process handle, range
        // and protection bits.
        let result = unsafe {
            crate::sys::syscall4(
                Syscall::PROTECT as usize,
                proc.0 as usize,
                addr,
                len,
                protection as usize,
            )
        };
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
#[path = "spawn_ut.rs"]
mod tests;
