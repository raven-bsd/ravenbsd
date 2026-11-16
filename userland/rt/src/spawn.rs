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
//!
//! PG-49 adds `PT_INTERP`: an `ET_DYN` interpreter is loaded at [`INTERP_BASE`], the main image at
//! [`MAIN_PIE_BASE`] when it is `ET_DYN`, and the first thread starts at the interpreter entry with
//! `AT_BASE` pointing at its bias. PG-50 builds the extended auxiliary vector in [`build_auxv`].

#[cfg(all(target_os = "none", feature = "alloc"))]
use alloc_crate::vec::Vec;
use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;
#[cfg(any(target_os = "none", test))]
use raven_abi::types::{map, prot};
#[cfg(any(target_os = "none", test))]
use raven_elf::ElfError;
#[cfg(any(target_os = "none", test))]
use raven_elf::{Elf, LoadMode, Machine, MapStep, StepKind};
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

pub use crate::spawn_error::SpawnError;

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
    /// Apply the setuid/setgid policy to an empty child process before its first thread starts.
    ///
    /// `file` is the open scheme-file handle for the image. The target implementation is
    /// [`RtSys`]; test doubles may record or override it. The default fails closed so a caller
    /// never silently skips the credential transition.
    fn exec_cred(&mut self, proc: Handle, file: Handle, nosuid: bool) -> Result<(), Errno> {
        let _ = (proc, file, nosuid);
        Err(Errno::ENOSYS)
    }
    /// Close a handle.
    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno>;
    /// Copy bytes into a mapping owned by the parent.
    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]);
    /// Copy `len` bytes between two mappings owned by the parent.
    ///
    /// The default is correct for every implementation whose `map` creates process-local
    /// mappings; a recording test double overrides it.
    fn copy_mapping(&mut self, destination: usize, source: usize, len: usize) {
        // SAFETY: both addresses are mappings in this process: `destination` is writable and
        // `source` is readable, each at least `len` bytes, and they do not overlap.
        unsafe {
            core::ptr::copy_nonoverlapping(source as *const u8, destination as *mut u8, len);
        }
    }
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
/// This is the full sequence: parse and plan the main image (and its `PT_INTERP`, if any), create
/// the process, map both images, build a [`STACK_SIZE`] stack (at least `ARG_MAX`) through a shared
/// memory object, start the first thread, and return the process handle. On failure after
/// `proc_create`, the child handle and the temporary image objects are closed.
#[cfg(any(target_os = "none", test))]
pub fn spawn_with<S: Sys>(
    initfs: &Initfs<'_>,
    path: &str,
    options: &SpawnOptions<'_>,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    let entry = initfs.get(path).ok_or(SpawnError::NotFound)?;
    let (main, main_steps, main_bias) = plan_image(entry.data, false)?;
    let interp = match main.interp() {
        Some(interp_path) => {
            let interpreter = initfs.get(interp_path).ok_or(SpawnError::NotFound)?;
            let (elf, steps, bias) = plan_image(interpreter.data, true)?;
            Some((
                elf,
                steps,
                ImageSource::Initfs {
                    data_offset: interpreter.data_offset,
                },
                bias,
            ))
        }
        None => None,
    };
    spawn_elf(
        &main,
        main_steps,
        ImageSource::Initfs {
            data_offset: entry.data_offset,
        },
        main_bias,
        interp,
        path,
        options,
        None,
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
    let (main, main_steps, main_bias) = plan_image(image, false)?;
    let Some(interp_path) = main.interp() else {
        return spawn_elf(
            &main,
            main_steps,
            ImageSource::Bytes(image),
            main_bias,
            None,
            path,
            options,
            None,
            sys,
        );
    };
    #[cfg(all(target_os = "none", feature = "alloc"))]
    {
        let interpreter = read_file_image(interp_path)?;
        spawn_image_with_interp(image, Some(&interpreter), path, options, sys)
    }
    #[cfg(not(all(target_os = "none", feature = "alloc")))]
    {
        let _ = interp_path;
        let _ = main_steps;
        let _ = main_bias;
        Err(SpawnError::NotFound)
    }
}

/// Spawn an in-memory image with an explicit interpreter image.
///
/// File-image callers on a target with `alloc` use [`spawn_image_with`], which reads the
/// interpreter from the file server. Tests and callers that already have the bytes use this entry
/// point.
#[cfg(any(target_os = "none", test))]
pub fn spawn_image_with_interp<S: Sys>(
    image: &[u8],
    interpreter: Option<&[u8]>,
    path: &str,
    options: &SpawnOptions<'_>,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    if image.len() > MAX_FILE_IMAGE {
        return Err(SpawnError::TooLarge);
    }
    let (main, main_steps, main_bias) = plan_image(image, false)?;
    let interp = match (main.interp(), interpreter) {
        (Some(_), Some(data)) => {
            if data.len() > MAX_FILE_IMAGE {
                return Err(SpawnError::TooLarge);
            }
            let (elf, steps, bias) = plan_image(data, true)?;
            Some((elf, steps, ImageSource::Bytes(data), bias))
        }
        (Some(_), None) => return Err(SpawnError::NotFound),
        (None, Some(_)) => return Err(SpawnError::BadElf(ElfError::BadType)),
        (None, None) => None,
    };
    spawn_elf(
        &main,
        main_steps,
        ImageSource::Bytes(image),
        main_bias,
        interp,
        path,
        options,
        None,
        sys,
    )
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

/// Spawn an image with the setuid/setgid policy applied to its child process.
#[cfg(any(target_os = "none", test))]
pub fn spawn_image_with_exec<S: Sys>(
    image: &[u8],
    path: &str,
    options: &SpawnOptions<'_>,
    file: Handle,
    nosuid: bool,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    if image.len() > MAX_FILE_IMAGE {
        return Err(SpawnError::TooLarge);
    }
    let (main, main_steps, main_bias) = plan_image(image, false)?;
    let exec = Some((file, nosuid));
    let Some(interp_path) = main.interp() else {
        return spawn_elf(
            &main,
            main_steps,
            ImageSource::Bytes(image),
            main_bias,
            None,
            path,
            options,
            exec,
            sys,
        );
    };
    #[cfg(all(target_os = "none", feature = "alloc"))]
    {
        let interpreter = read_file_image(interp_path)?;
        let (elf, steps, bias) = plan_image(&interpreter, true)?;
        spawn_elf(
            &main,
            main_steps,
            ImageSource::Bytes(image),
            main_bias,
            Some((elf, steps, ImageSource::Bytes(&interpreter), bias)),
            path,
            options,
            exec,
            sys,
        )
    }
    #[cfg(not(all(target_os = "none", feature = "alloc")))]
    {
        let _ = interp_path;
        let _ = main_steps;
        let _ = main_bias;
        Err(SpawnError::NotFound)
    }
}

/// Read an open file handle from offset zero without moving its cursor.
#[cfg(all(target_os = "none", feature = "alloc"))]
pub fn read_file_handle_at(file: Handle) -> Result<Vec<u8>, SpawnError> {
    let client = Client::new();
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut offset = 0u64;
    loop {
        let read = client
            .pread(file, offset, &mut buffer)
            .map_err(SpawnError::Sys)?;
        if read == 0 {
            return Ok(data);
        }
        let next = data.len().checked_add(read).ok_or(SpawnError::TooLarge)?;
        if next > MAX_FILE_IMAGE {
            return Err(SpawnError::TooLarge);
        }
        let chunk = buffer.get(..read).ok_or(SpawnError::TooLarge)?;
        data.extend_from_slice(chunk);
        offset = offset.saturating_add(read as u64);
    }
}

/// Spawn an ELF from an open scheme-file handle.
///
/// The handle is stat'ed and read from offset zero; its open-file description keeps the inode
/// stable while the image is loaded. `nosuid` is the mount policy the caller obtained from the
/// mount table. On success the child process has no credential transition until the kernel's
/// `proc_exec_cred` has checked the execute bit and applied any setuid/setgid bits.
#[cfg(all(target_os = "none", feature = "alloc"))]
pub fn spawn_file(
    file: Handle,
    path: &str,
    options: &SpawnOptions<'_>,
    nosuid: bool,
) -> Result<Handle, SpawnError> {
    let image = read_file_handle(file)?;
    let mut sys = RtSys;
    spawn_image_with_exec(&image, path, options, file, nosuid, &mut sys)
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
#[derive(Clone, Copy)]
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
fn prepare_source<S: Sys>(
    source: ImageSource<'_>,
    sys: &mut S,
) -> Result<(MapSource, Option<Handle>), SpawnError> {
    match source {
        ImageSource::Initfs { data_offset } => Ok((MapSource::Initfs { data_offset }, None)),
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
            Ok((MapSource::Object(object), Some(object)))
        }
    }
}

#[cfg(any(target_os = "none", test))]
fn map_plan<S: Sys>(
    sys: &mut S,
    child: Handle,
    source: MapSource,
    steps: BiasedPlan,
) -> Result<(), SpawnError> {
    for step in steps {
        map_step(sys, child, source, step)?;
    }
    Ok(())
}

#[cfg(any(target_os = "none", test))]
#[allow(clippy::too_many_arguments)] // the image pair, entry state and syscall sink
fn spawn_elf<S: Sys>(
    main_elf: &Elf<'_>,
    main_steps: BiasedPlan,
    main_source: ImageSource<'_>,
    main_bias: u64,
    interp: Option<(Elf<'_>, BiasedPlan, ImageSource<'_>, u64)>,
    path: &str,
    options: &SpawnOptions<'_>,
    exec: Option<(Handle, bool)>,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    let (main_map, main_object) = prepare_source(main_source, sys)?;
    let mut interp_map: Option<(Elf<'_>, BiasedPlan, MapSource, u64, Option<Handle>)> = None;
    if let Some((elf, steps, source, bias)) = interp {
        match prepare_source(source, sys) {
            Ok((map, object)) => interp_map = Some((elf, steps, map, bias, object)),
            Err(error) => {
                if let Some(object) = main_object {
                    let _ = sys.handle_close(object);
                }
                return Err(error);
            }
        }
    }

    let interp_bias = interp_map.as_ref().map_or(0, |entry| entry.3);
    let entry_address = match &interp_map {
        Some((elf, _, _, bias, _)) => elf
            .entry()
            .checked_add(*bias)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(SpawnError::TooLarge)?,
        None => main_elf
            .entry()
            .checked_add(main_bias)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(SpawnError::TooLarge)?,
    };
    let (auxv, auxv_len) = build_auxv(main_elf, main_bias, interp_bias);
    let random = random_bytes(&auxv as *const _ as usize);

    let child = match sys.proc_create() {
        Ok(child) => child,
        Err(error) => {
            close_objects(
                sys,
                main_object,
                interp_map.as_ref().and_then(|entry| entry.4),
            );
            return Err(SpawnError::Sys(error));
        }
    };
    if let Err(error) = map_plan(sys, child, main_map, main_steps) {
        close_objects(
            sys,
            main_object,
            interp_map.as_ref().and_then(|entry| entry.4),
        );
        let _ = sys.handle_close(child);
        return Err(error);
    }
    if let Some((_, steps, map, _, object)) = interp_map {
        if let Err(error) = map_plan(sys, child, map, steps) {
            close_objects(sys, main_object, object);
            let _ = sys.handle_close(child);
            return Err(error);
        }
        if let Some(object) = object {
            let _ = sys.handle_close(object);
        }
    }
    if let Some(object) = main_object {
        let _ = sys.handle_close(object);
    }

    let stack_base = STACK_TOP
        .checked_sub(STACK_SIZE as u64)
        .ok_or(SpawnError::TooLarge)?;
    let stack_base_address = usize::try_from(stack_base).map_err(|_| SpawnError::TooLarge)?;
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

    #[cfg(feature = "alloc")]
    let mut stack_buffer = alloc_crate::vec![0u8; STACK_SIZE];
    #[cfg(not(feature = "alloc"))]
    let mut stack_buffer = [0u8; 64 * 1024];
    #[cfg(not(feature = "alloc"))]
    let stack_size = stack_buffer.len();
    #[cfg(feature = "alloc")]
    let stack_size = STACK_SIZE;

    let (image_offset, sp0) = match build_stack(
        &mut stack_buffer,
        STACK_TOP,
        path,
        options.args,
        options.env,
        &auxv[..auxv_len],
        &random,
        platform_name(),
    ) {
        Ok(image) => image,
        Err(error) => {
            let _ = sys.unmap(Handle::SELF_PROC, scratch, stack_size);
            let _ = sys.handle_close(memobj);
            let _ = sys.handle_close(child);
            return Err(error);
        }
    };
    let Some(write_address) = scratch.checked_add(image_offset) else {
        let _ = sys.unmap(Handle::SELF_PROC, scratch, stack_size);
        let _ = sys.handle_close(memobj);
        let _ = sys.handle_close(child);
        return Err(SpawnError::TooLarge);
    };
    let image = match stack_buffer.get(image_offset..) {
        Some(image) => image,
        None => {
            let _ = sys.unmap(Handle::SELF_PROC, scratch, stack_size);
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
        stack_size,
        child_flags,
    ) {
        let _ = sys.unmap(Handle::SELF_PROC, scratch, stack_size);
        let _ = sys.handle_close(memobj);
        let _ = sys.handle_close(child);
        return Err(SpawnError::Sys(error));
    }
    if let Err(error) = sys.unmap(Handle::SELF_PROC, scratch, stack_size) {
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
    if let Some((file, nosuid)) = exec
        && let Err(error) = sys.exec_cred(child, file, nosuid)
    {
        let _ = sys.handle_close(child);
        return Err(SpawnError::Sys(error));
    }
    match sys.thread_create(child, entry_address, sp0, sp0) {
        Ok(_thread) => Ok(child),
        Err(error) => {
            let _ = sys.handle_close(child);
            Err(SpawnError::Sys(error))
        }
    }
}

#[cfg(any(target_os = "none", test))]
fn close_objects<S: Sys>(sys: &mut S, first: Option<Handle>, second: Option<Handle>) {
    if let Some(object) = first {
        let _ = sys.handle_close(object);
    }
    if let Some(object) = second {
        let _ = sys.handle_close(object);
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
            let (handle, offset) = source_request(source, offset)?;
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
        StepKind::CopyTail { offset, len: tail } => {
            copy_tail(sys, child, source, step, offset, tail)
        }
    }
}

#[cfg(any(target_os = "none", test))]
fn source_request(source: MapSource, offset: u64) -> Result<(Handle, u64), SpawnError> {
    match source {
        MapSource::Initfs { data_offset } => {
            let offset = data_offset
                .checked_add(offset)
                .ok_or(SpawnError::TooLarge)?;
            Ok((Handle::INITFS, offset))
        }
        MapSource::Object(handle) => Ok((handle, offset)),
    }
}

/// Fill one page from a partial file tail, then map it into the child.
#[cfg(any(target_os = "none", test))]
fn copy_tail<S: Sys>(
    sys: &mut S,
    child: Handle,
    source: MapSource,
    step: MapStep,
    offset: u64,
    len: u64,
) -> Result<(), SpawnError> {
    let vaddr = usize::try_from(step.vaddr).map_err(|_| SpawnError::TooLarge)?;
    let len = usize::try_from(len).map_err(|_| SpawnError::TooLarge)?;
    let page = usize::try_from(PAGE).map_err(|_| SpawnError::TooLarge)?;
    let (handle, offset) = source_request(source, offset)?;
    let offset = usize::try_from(offset).map_err(|_| SpawnError::TooLarge)?;
    let object = sys.memobj_create(page).map_err(SpawnError::Sys)?;
    let scratch_flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
    let scratch = match sys.map(Handle::SELF_PROC, object, 0, 0, page, scratch_flags) {
        Ok(address) => address,
        Err(error) => {
            let _ = sys.handle_close(object);
            return Err(SpawnError::Sys(error));
        }
    };
    let source_flags = map::pack(prot::READ, map::SHARED);
    let source_map = match sys.map(Handle::SELF_PROC, handle, offset, 0, page, source_flags) {
        Ok(address) => address,
        Err(error) => {
            let _ = sys.unmap(Handle::SELF_PROC, scratch, page);
            let _ = sys.handle_close(object);
            return Err(SpawnError::Sys(error));
        }
    };
    sys.copy_mapping(scratch, source_map, len);
    let _ = sys.unmap(Handle::SELF_PROC, source_map, page);
    let child_flags = map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED);
    if let Err(error) = sys.map(child, object, 0, vaddr, page, child_flags) {
        let _ = sys.unmap(Handle::SELF_PROC, scratch, page);
        let _ = sys.handle_close(object);
        return Err(SpawnError::Sys(error));
    }
    let _ = sys.unmap(Handle::SELF_PROC, scratch, page);
    if let Err(error) = sys.protect(child, vaddr, page, step.prot) {
        let _ = sys.handle_close(object);
        return Err(SpawnError::Sys(error));
    }
    sys.handle_close(object).map_err(SpawnError::Sys)
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

    fn exec_cred(&mut self, proc: Handle, file: Handle, nosuid: bool) -> Result<(), Errno> {
        let flags = usize::from(nosuid);
        // SAFETY: `PROC_EXEC_CRED` takes a target process handle, an open scheme-file handle and
        // a scalar flags word; the kernel validates both handles, rights and the file's Stat.
        let result = unsafe { crate::sys::syscall3(0x3E, proc.0 as usize, file.0 as usize, flags) };
        Errno::from_ret(result).map(|_| ())
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

    fn copy_mapping(&mut self, destination: usize, source: usize, len: usize) {
        // SAFETY: both addresses are mappings in this process: `destination` is writable and
        // `source` is readable, each at least `len` bytes, and they do not overlap.
        unsafe {
            core::ptr::copy_nonoverlapping(source as *const u8, destination as *mut u8, len);
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
const WORD: usize = core::mem::size_of::<usize>();
/// Minimum child stack: `ARG_MAX` (CR-15) plus the C8 arrays and headroom.
#[cfg(any(target_os = "none", test))]
const STACK_SIZE: usize = 256 * 1024;
#[cfg(any(target_os = "none", test))]
const ARG_MAX: usize = 128 * 1024;
#[cfg(any(target_os = "none", test))]
const _: () = assert!(STACK_SIZE >= ARG_MAX);
#[cfg(any(target_os = "none", test))]
const MAX_AUXV: usize = 20;
#[cfg(any(target_os = "none", test))]
const AT_PHDR: usize = 3;
#[cfg(any(target_os = "none", test))]
const AT_PHENT: usize = 4;
#[cfg(any(target_os = "none", test))]
const AT_PHNUM: usize = 5;
#[cfg(any(target_os = "none", test))]
const AT_PAGESZ: usize = 6;
#[cfg(any(target_os = "none", test))]
const AT_BASE: usize = 7;
#[cfg(any(target_os = "none", test))]
const AT_FLAGS: usize = 8;
#[cfg(any(target_os = "none", test))]
const AT_ENTRY: usize = 9;
#[cfg(any(target_os = "none", test))]
const AT_UID: usize = 11;
#[cfg(any(target_os = "none", test))]
const AT_EUID: usize = 12;
#[cfg(any(target_os = "none", test))]
const AT_GID: usize = 13;
#[cfg(any(target_os = "none", test))]
const AT_EGID: usize = 14;
#[cfg(any(target_os = "none", test))]
const AT_PLATFORM: usize = 15;
#[cfg(any(target_os = "none", test))]
const AT_HWCAP: usize = 16;
#[cfg(any(target_os = "none", test))]
const AT_CLKTCK: usize = 17;
#[cfg(any(target_os = "none", test))]
const AT_SECURE: usize = 23;
#[cfg(any(target_os = "none", test))]
const AT_RANDOM: usize = 25;
#[cfg(any(target_os = "none", test))]
const AT_HWCAP2: usize = 26;
#[cfg(any(target_os = "none", test))]
const AT_EXECFN: usize = 31;
#[cfg(any(target_os = "none", test))]
const AT_MINSIGSTKSZ: usize = 51;
#[cfg(any(target_os = "none", test))]
const RANDOM_LEN: usize = 16;
/// Address where an `ET_DYN` main image is based.
#[cfg(any(target_os = "none", test))]
const MAIN_PIE_BASE: u64 = 0x0000_1000_0000_0000;
/// Address where a `PT_INTERP` image is based.
#[cfg(any(target_os = "none", test))]
const INTERP_BASE: u64 = 0x0000_2000_0000_0000;
/// Size of one ELF64 program header.
#[cfg(any(target_os = "none", test))]
const PHENT: usize = 56;

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
/// The stack top is the top of the user range; the stack occupies [`STACK_SIZE`] and `rsp == sp0`.
const STACK_TOP: u64 = USER_END;

/// `e_type` from an ELF64 header (`2 = ET_EXEC`, `3 = ET_DYN`).
#[cfg(any(target_os = "none", test))]
fn elf_type(data: &[u8]) -> u16 {
    match data.get(16..18) {
        Some(bytes) => u16::from_le_bytes([bytes[0], bytes[1]]),
        None => 0,
    }
}

/// The `AT_PLATFORM` spelling for the build architecture.
#[cfg(any(target_os = "none", test))]
fn platform_name() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(target_arch = "riscv64")]
    {
        "riscv64"
    }
}

/// One page-aligned mapping step of a biased image.
#[cfg(any(target_os = "none", test))]
#[derive(Clone, Copy)]
struct SegPlan {
    prot: u8,
    filesz: u64,
    start: u64,
    file_end: u64,
    file_whole_end: u64,
    zero_start: u64,
    end: u64,
    file_offset_base: u64,
}

#[cfg(any(target_os = "none", test))]
impl SegPlan {
    const EMPTY: Self = Self {
        prot: 0,
        filesz: 0,
        start: 0,
        file_end: 0,
        file_whole_end: 0,
        zero_start: 0,
        end: 0,
        file_offset_base: 0,
    };

    fn has_file_tail(&self) -> bool {
        self.filesz > 0 && self.file_end != self.file_whole_end
    }
}

#[cfg(any(target_os = "none", test))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Start,
    File,
    CopyTail,
    Zero,
    Done,
}

/// The page plan of one image with its load bias applied.
#[cfg(any(target_os = "none", test))]
struct BiasedPlan {
    plans: [SegPlan; MAX_SEGMENTS],
    count: usize,
    page: u64,
    segment_index: usize,
    phase: Phase,
    cursor: u64,
}

#[cfg(any(target_os = "none", test))]
const MAX_SEGMENTS: usize = 16;

#[cfg(any(target_os = "none", test))]
impl Iterator for BiasedPlan {
    type Item = MapStep;

    fn next(&mut self) -> Option<MapStep> {
        loop {
            if self.segment_index >= self.count {
                return None;
            }
            let plan = *self.plans.get(self.segment_index)?;
            match self.phase {
                Phase::Start => {
                    self.cursor = plan.start;
                    self.phase = Phase::File;
                }
                Phase::File => {
                    if self.cursor < plan.file_whole_end {
                        let delta = self.cursor.checked_sub(plan.start)?;
                        let offset = plan.file_offset_base.checked_add(delta)?;
                        let step = MapStep {
                            vaddr: self.cursor,
                            len: self.page,
                            prot: plan.prot,
                            kind: StepKind::File { offset },
                        };
                        self.cursor = self.cursor.checked_add(self.page)?;
                        return Some(step);
                    }
                    self.phase = if plan.has_file_tail() {
                        Phase::CopyTail
                    } else {
                        Phase::Zero
                    };
                    self.cursor = plan.zero_start;
                }
                Phase::CopyTail => {
                    self.phase = Phase::Zero;
                    if plan.has_file_tail() {
                        let offset = plan
                            .file_offset_base
                            .checked_add(plan.file_whole_end.checked_sub(plan.start)?)?;
                        let len = plan.file_end.checked_sub(plan.file_whole_end)?;
                        return Some(MapStep {
                            vaddr: plan.file_whole_end,
                            len: self.page,
                            prot: plan.prot,
                            kind: StepKind::CopyTail { offset, len },
                        });
                    }
                }
                Phase::Zero => {
                    if self.cursor < plan.end {
                        let step = MapStep {
                            vaddr: self.cursor,
                            len: self.page,
                            prot: plan.prot,
                            kind: StepKind::Zero,
                        };
                        self.cursor = self.cursor.checked_add(self.page)?;
                        return Some(step);
                    }
                    self.phase = Phase::Done;
                }
                Phase::Done => {
                    self.segment_index += 1;
                    self.phase = Phase::Start;
                }
            }
        }
    }
}

/// Plan `elf` with `bias` applied to every segment and entry.
///
/// [`raven_elf::plan_load`] validates the unbiased addresses against the user range, so an
/// `ET_DYN` image at address zero cannot use it. This is the same split into whole file pages,
/// one optional `CopyTail` page and zero pages, with the bias applied first.
#[cfg(any(target_os = "none", test))]
fn plan_biased(
    elf: &Elf<'_>,
    page: u64,
    bias: u64,
    mode: LoadMode,
) -> Result<BiasedPlan, ElfError> {
    if page == 0 || !bias.is_multiple_of(page) {
        return Err(ElfError::BadPhdr);
    }
    let mut plans = [SegPlan::EMPTY; MAX_SEGMENTS];
    let mut count = 0usize;
    let mut previous_end: Option<u64> = None;
    for segment in elf.segments() {
        if segment.memsz == 0 {
            continue;
        }
        let biased_vaddr = segment
            .vaddr
            .checked_add(bias)
            .ok_or(ElfError::SegmentOutsideUserRange)?;
        let start = page_floor(biased_vaddr);
        let file_end = biased_vaddr
            .checked_add(segment.filesz)
            .ok_or(ElfError::SegmentOutsideUserRange)?;
        let mem_end = biased_vaddr
            .checked_add(segment.memsz)
            .ok_or(ElfError::SegmentOutsideUserRange)?;
        let end = page_ceil(mem_end).ok_or(ElfError::SegmentOutsideUserRange)?;
        let file_whole_end = page_floor(file_end);
        let zero_start = if segment.filesz == 0 {
            start
        } else {
            page_ceil(file_end).ok_or(ElfError::SegmentOutsideUserRange)?
        };
        if mode == LoadMode::Strict && segment.filesz > 0 && zero_start != file_end {
            return Err(ElfError::FileTailNotPageAligned);
        }
        if start < USER_MIN || end > USER_END {
            return Err(ElfError::SegmentOutsideUserRange);
        }
        let delta = biased_vaddr.checked_sub(start).ok_or(ElfError::BadPhdr)?;
        let file_offset_base = segment.offset.checked_sub(delta).ok_or(ElfError::BadPhdr)?;
        if let Some(previous) = previous_end
            && start < previous
        {
            return Err(ElfError::SegmentOverlap);
        }
        previous_end = Some(end);
        let Some(slot) = plans.get_mut(count) else {
            return Err(ElfError::TooManySegments);
        };
        *slot = SegPlan {
            prot: segment.prot,
            filesz: segment.filesz,
            start,
            file_end,
            file_whole_end,
            zero_start,
            end,
            file_offset_base,
        };
        count += 1;
    }
    Ok(BiasedPlan {
        plans,
        count,
        page,
        segment_index: 0,
        phase: Phase::Start,
        cursor: 0,
    })
}

#[cfg(any(target_os = "none", test))]
fn page_floor(value: u64) -> u64 {
    value - (value % PAGE)
}

#[cfg(any(target_os = "none", test))]
fn page_ceil(value: u64) -> Option<u64> {
    let remainder = value % PAGE;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(PAGE - remainder)
    }
}

/// Parse and plan one image. `interpreter` selects the `PT_INTERP` rules: the file must be
/// `ET_DYN` and is based at [`INTERP_BASE`].
#[cfg(any(target_os = "none", test))]
fn plan_image<'a>(
    data: &'a [u8],
    interpreter: bool,
) -> Result<(Elf<'a>, BiasedPlan, u64), SpawnError> {
    let elf = Elf::parse(data, target_machine(), USER_MIN..USER_END).map_err(SpawnError::BadElf)?;
    if let Some(path) = elf.interp()
        && !path.starts_with('/')
    {
        return Err(SpawnError::BadElf(ElfError::BadPhdr));
    }
    let kind = elf_type(data);
    let bias = if interpreter {
        if kind != 3 {
            return Err(SpawnError::BadElf(ElfError::BadType));
        }
        INTERP_BASE
    } else {
        match kind {
            2 => 0,
            3 => MAIN_PIE_BASE,
            _ => return Err(SpawnError::BadElf(ElfError::BadType)),
        }
    };
    let mode = if kind == 3 {
        LoadMode::Copy
    } else {
        LoadMode::Strict
    };
    let steps = plan_biased(&elf, PAGE, bias, mode).map_err(SpawnError::BadElf)?;
    Ok((elf, steps, bias))
}

/// Build the PG-50 auxiliary vector for the main image and an optional interpreter.
///
/// `AT_PLATFORM`, `AT_RANDOM` and `AT_EXECFN` carry zero here; [`build_stack`] replaces them with
/// the stack addresses it creates.
#[cfg(any(target_os = "none", test))]
fn build_auxv(
    elf: &Elf<'_>,
    main_bias: u64,
    interp_bias: u64,
) -> ([(usize, usize); MAX_AUXV], usize) {
    let mut auxv = [(0usize, 0usize); MAX_AUXV];
    let mut len = 0usize;
    let mut push = |key: usize, value: usize| {
        if let Some(slot) = auxv.get_mut(len) {
            *slot = (key, value);
            len += 1;
        }
    };
    let phdr = elf
        .phdr_vaddr()
        .and_then(|value| value.checked_add(main_bias))
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    push(AT_PHDR, phdr);
    push(AT_PHENT, PHENT);
    push(AT_PHNUM, elf.phnum());
    push(AT_PAGESZ, PAGE as usize);
    push(AT_BASE, usize::try_from(interp_bias).unwrap_or_default());
    push(AT_FLAGS, 0);
    push(
        AT_ENTRY,
        usize::try_from(elf.entry().saturating_add(main_bias)).unwrap_or_default(),
    );
    push(AT_UID, 0);
    push(AT_EUID, 0);
    push(AT_GID, 0);
    push(AT_EGID, 0);
    push(AT_PLATFORM, 0);
    push(AT_HWCAP, 0);
    push(AT_CLKTCK, 100);
    push(AT_SECURE, 0);
    push(AT_RANDOM, 0);
    push(AT_HWCAP2, 0);
    push(AT_EXECFN, 0);
    push(AT_MINSIGSTKSZ, 8192);
    (auxv, len)
}

/// Build the C8 initial stack image in `buffer`.
///
/// `buffer` covers the top `buffer.len()` bytes of the child stack (`stack_top - buffer.len()` ..
/// `stack_top`). The image is written at the end of the buffer and the function returns the buffer
/// offset of `sp0` and the child virtual address of `sp0`. Strings are placed at the top in
/// argument/environment order; pointer arrays follow the C8 order. `AT_RANDOM` in `auxv` is
/// replaced with the address of `random`, which is placed immediately below the strings.
#[cfg(any(target_os = "none", test))]
#[allow(clippy::too_many_arguments)] // the C8 block is passed explicitly
fn build_stack(
    buffer: &mut [u8],
    stack_top: u64,
    path: &str,
    args: &[&str],
    env: &[&str],
    auxv: &[(usize, usize)],
    random: &[u8; RANDOM_LEN],
    platform: &str,
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
    strings_len = strings_len
        .checked_add(platform.len())
        .and_then(|value| value.checked_add(1))
        .ok_or(SpawnError::TooLarge)?;

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
    let mut argv_zero = 0usize;
    for index in 0..argc {
        let text = if args.is_empty() {
            path
        } else {
            args.get(index).copied().ok_or(SpawnError::TooLarge)?
        };
        let pointer = write_string(buffer, &mut cursor, text, stack_base)?;
        if index == 0 {
            argv_zero = pointer;
        }
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

    let platform_address = {
        let start = stack_base
            .checked_add(u64::try_from(cursor).map_err(|_| SpawnError::TooLarge)?)
            .ok_or(SpawnError::TooLarge)?;
        let _ = write_string(buffer, &mut cursor, platform, stack_base)?;
        start
    };
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
        } else if *key == AT_PLATFORM {
            usize::try_from(platform_address).map_err(|_| SpawnError::TooLarge)?
        } else if *key == AT_EXECFN {
            argv_zero
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
