// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Create a child process from an ELF file in the initfs or on a file server.
//!
//! This crate is the P9a extraction of the loader previously in `userland/rt/src/spawn.rs`.
//! Both `raven-rt` and ravenlibc call the same planning, mapping and stack-image code.

#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)] // raw assembly is isolated in `sys`; every block names its invariant.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(any(feature = "alloc", test))]
extern crate alloc as alloc_crate;
#[cfg(test)]
extern crate std;

// The loader plans the image with `raven-elf`, maps its segments and a fresh stack into a new
// empty process, fills the stack through a shared memory object (CONTRACTS C13), and starts the
// first thread. The caller receives the child's process handle and may wait for it with
// `PROC_WAIT`.
//
// Two image sources are supported: the read-only initfs archive (zero-copy segment mapping) and
// bytes read from a `file:` path (bounded copy into a memory object, then segment mapping). The
// latter is how root services start from a UFS root.
//
// PG-49 adds `PT_INTERP`: an `ET_DYN` interpreter is loaded at INTERP_BASE, the main image at
// MAIN_PIE_BASE when it is `ET_DYN`, and the first thread starts at the interpreter entry with
// AT_BASE pointing at its bias. PG-50 builds the extended auxiliary vector in build_auxv.

mod spawn_error;
mod sys;

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub use crate::sys::{close_handle, pid_for_handle, wait_ex, wait_process};

#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
use alloc_crate::vec::Vec;
use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
use raven_abi::types::{map, prot};
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
use raven_elf::ElfError;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
use raven_elf::{Elf, LoadMode, Machine, MapStep, StepKind};
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
use raven_initfs::Initfs;
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
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
pub const MAX_FILE_IMAGE: usize = 16 * 1024 * 1024;

/// Chunk size for reading a `file:` image.
///
/// The file servers handle a multi-megabyte read in one request (the kernel capture window is
/// 4 MiB), so a page-sized loop cost one IPC round trip per 4 KiB and dominated the spawn time:
/// the 984 KiB `/bin/true` needed 240 round trips. One MiB keeps a 3.3 MiB tool at four reads
/// while staying well inside both the capture window and [`MAX_FILE_IMAGE`].
#[cfg(any(
    all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"),
    test
))]
const IMAGE_READ_CHUNK: usize = 1024 * 1024;

/// Raw `PROC_EXEC_CRED`: apply the setuid/setgid policy before the first thread starts.
///
/// P9b moved this call from `0x3E` to `0x3F` so `PROC_HANDLE_INFO` owns `0x3E`.
pub const PROC_EXEC_CRED: usize = 0x3F;

pub use crate::spawn_error::SpawnError;

/// The final credentials a spawn publishes in the auxiliary vector.
///
/// The values are read from the target child after the kernel's `PROC_EXEC_CRED`
/// transition (or from the caller on the no-credential path), so the vector never
/// carries a zero default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    /// Real user id.
    pub uid: u32,
    /// Effective user id.
    pub euid: u32,
    /// Real group id.
    pub gid: u32,
    /// Effective group id.
    pub egid: u32,
}

impl ProcessIdentity {
    /// The host-test placeholder. The target always queries the kernel; this value is
    /// compiled only for the host unit tests.
    #[cfg(all(not(any(target_os = "none", target_os = "ravenbsd")), test))]
    const fn host_test() -> Self {
        Self {
            uid: 0,
            euid: 0,
            gid: 0,
            egid: 0,
        }
    }
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
    /// Transfer `source` into `dest` with the source's rights, returning the child handle.
    fn handle_transfer(&mut self, dest: Handle, source: Handle) -> Result<Handle, Errno> {
        let _ = (dest, source);
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
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
    // K139: pass the libc archive offset so the loader maps it from the initfs zero-copy
    // instead of copying it through the file server. Only dynamic mains need it.
    let libc = if interp.is_some() {
        initfs.get("/lib/libc.so.1").map(|entry| entry.data_offset)
    } else {
        None
    };
    spawn_elf(
        &main,
        main_steps,
        ImageSource::Initfs {
            data_offset: entry.data_offset,
        },
        main_bias,
        interp,
        libc,
        b"/",
        path,
        options,
        &[],
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
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
            None,
            b"/",
            path,
            options,
            &[],
            None,
            sys,
        );
    };
    #[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
    {
        let interpreter = read_file_image(interp_path)?;
        spawn_image_with_interp(image, Some(&interpreter), path, options, sys)
    }
    #[cfg(not(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc")))]
    {
        let _ = interp_path;
        let _ = main_steps;
        let _ = main_bias;
        Err(SpawnError::NotFound)
    }
}

/// Spawn an in-memory image and inherit `fdmap` descriptors into the child.
///
/// Each `(child_fd, parent_handle)` is transferred with `HANDLE_TRANSFER`; the child handles are
/// packed into the `RFDM` block published as `AT_RAVEN_FDMAP`/`AT_RAVEN_FDMAP_N`. Only static
/// ET_EXEC images are supported on this path; an interpreter is a separate dynamic-loading
/// feature.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
pub fn spawn_image_with_fdmap<S: Sys>(
    image: &[u8],
    path: &str,
    options: &SpawnOptions<'_>,
    fdmap: &[(i32, u32)],
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    spawn_image_with_fdmap_cwd(image, path, options, b"/", fdmap, sys)
}

/// Spawn an in-memory image with an explicit child working directory and `fdmap`.
///
/// The cwd is written as a NUL-terminated stack string and published as `AT_RAVEN_CWD`; the libc
/// process start reads it before `main`. An empty, relative, NUL-containing or overlong cwd fails
/// the spawn instead of silently starting in `/`.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
pub fn spawn_image_with_fdmap_cwd<S: Sys>(
    image: &[u8],
    path: &str,
    options: &SpawnOptions<'_>,
    cwd: &[u8],
    fdmap: &[(i32, u32)],
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    if image.len() > MAX_FILE_IMAGE {
        return Err(SpawnError::TooLarge);
    }
    let (main, main_steps, main_bias) = plan_image(image, false)?;
    if main.interp().is_some() {
        return Err(SpawnError::NotFound);
    }
    spawn_elf(
        &main,
        main_steps,
        ImageSource::Bytes(image),
        main_bias,
        None,
        None,
        cwd,
        path,
        options,
        fdmap,
        None,
        sys,
    )
}

/// Spawn an in-memory image with an explicit interpreter image.
///
/// File-image callers on a target with `alloc` use [`spawn_image_with`], which reads the
/// interpreter from the file server. Tests and callers that already have the bytes use this entry
/// point.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
        None,
        b"/",
        path,
        options,
        &[],
        None,
        sys,
    )
}

/// Spawn `path` from an in-memory image with the real syscall implementation.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub fn spawn_image(
    image: &[u8],
    path: &str,
    options: &SpawnOptions<'_>,
) -> Result<Handle, SpawnError> {
    let mut sys = RtSys;
    spawn_image_with(image, path, options, &mut sys)
}

/// Spawn an image with the setuid/setgid policy applied to its child process.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
            None,
            b"/",
            path,
            options,
            &[],
            exec,
            sys,
        );
    };
    #[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
    {
        let interpreter = read_file_image(interp_path)?;
        let (elf, steps, bias) = plan_image(&interpreter, true)?;
        spawn_elf(
            &main,
            main_steps,
            ImageSource::Bytes(image),
            main_bias,
            Some((elf, steps, ImageSource::Bytes(&interpreter), bias)),
            None,
            b"/",
            path,
            options,
            &[],
            exec,
            sys,
        )
    }
    #[cfg(not(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc")))]
    {
        let _ = interp_path;
        let _ = main_steps;
        let _ = main_bias;
        Err(SpawnError::NotFound)
    }
}

/// Read an open file handle from offset zero without moving its cursor.
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
pub fn read_file_handle_at(file: Handle) -> Result<Vec<u8>, SpawnError> {
    let client = Client::new();
    let size = image_size(&client, file)?;
    let mut offset = 0u64;
    collect_exact(size, |buffer| {
        let read = client.pread(file, offset, buffer)?;
        offset = offset.saturating_add(read as u64);
        Ok(read)
    })
}

/// Spawn an ELF from an open scheme-file handle.
///
/// The handle is stat'ed and read from offset zero; its open-file description keeps the inode
/// stable while the image is loaded. `nosuid` is the mount policy the caller obtained from the
/// mount table. On success the child process has no credential transition until the kernel's
/// `proc_exec_cred` has checked the execute bit and applied any setuid/setgid bits.
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
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
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
pub fn read_file_handle(file: Handle) -> Result<Vec<u8>, SpawnError> {
    read_image_handle(&Client::new(), file)
}

/// Return the scheme path used to open an executable image.
///
/// A path that already names a scheme (`file:/bin/ls`, `tty:console`) is used unchanged. A bare
/// absolute path becomes `file:` followed by the path; a bare relative path is treated as
/// root-relative because the loader callers (init and libc's spawner) resolve `cwd` first.
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
fn image_scheme_path(path: &str) -> alloc_crate::string::String {
    use alloc_crate::format;
    match path.find(':') {
        Some(index) if !path[..index].contains('/') => path.into(),
        _ if path.starts_with('/') => format!("file:{path}"),
        _ => format!("file:/{path}"),
    }
}

/// The S11 wire flags for a read-only image open.
///
/// `O_RDONLY` is zero at the C level, but the kernel's `OPEN` wire encoding uses bit 0 for read
/// access; passing the raw zero flags is rejected.
#[cfg(any(
    test,
    all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc")
))]
fn image_open_flags() -> Result<raven_proto_fs::OpenFlags, SpawnError> {
    raven_proto_fs::OpenFlags::from_linux(raven_proto_fs::O_RDONLY)
        .map_err(|_| SpawnError::Sys(Errno::EINVAL))
}

/// Open and read a `file:` path into a bounded image buffer.
///
/// Every call opens its own handle, so concurrent readers do not share a file cursor.
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
pub fn read_file_image(path: &str) -> Result<Vec<u8>, SpawnError> {
    let scheme_path = image_scheme_path(path);
    let flags = image_open_flags()?;
    let client = Client::new();
    let handle = client.open_flags(&scheme_path, flags).map_err(|error| {
        if error == Errno::ENOENT {
            SpawnError::FileNotFound
        } else {
            SpawnError::Sys(error)
        }
    })?;
    let result = read_image_handle(&client, handle);
    let _ = client.close(handle);
    result
}

/// Read one open image handle to its end, bounded by [`MAX_FILE_IMAGE`].
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
fn read_image_handle(client: &Client, handle: Handle) -> Result<Vec<u8>, SpawnError> {
    let size = image_size(client, handle)?;
    collect_exact(size, |buffer| client.read(handle, 0, buffer))
}

/// Return the image length from `FSTAT`, bounded by [`MAX_FILE_IMAGE`].
///
/// Knowing the length lets the reader request exactly the image bytes: an EOF probe on a
/// multi-megabyte buffer otherwise costs the kernel a full capture mapping for no data.
#[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
fn image_size(client: &Client, handle: Handle) -> Result<usize, SpawnError> {
    let stat = client.fstat(handle).map_err(SpawnError::Sys)?;
    let size = usize::try_from(stat.size).map_err(|_| SpawnError::TooLarge)?;
    if size > MAX_FILE_IMAGE {
        return Err(SpawnError::TooLarge);
    }
    Ok(size)
}

/// Read exactly `size` image bytes through `read` in [`IMAGE_READ_CHUNK`] pieces.
#[cfg(any(
    all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"),
    test
))]
fn collect_exact<F>(size: usize, mut read: F) -> Result<Vec<u8>, SpawnError>
where
    F: FnMut(&mut [u8]) -> Result<usize, Errno>,
{
    if size > MAX_FILE_IMAGE {
        return Err(SpawnError::TooLarge);
    }
    let mut data = Vec::with_capacity(size);
    let mut buffer = alloc_crate::vec![0u8; IMAGE_READ_CHUNK.min(size)];
    while data.len() < size {
        let remaining = size - data.len();
        let want = remaining.min(buffer.len());
        let read_len =
            read(buffer.get_mut(..want).ok_or(SpawnError::TooLarge)?).map_err(SpawnError::Sys)?;
        if read_len == 0 || read_len > want {
            return Err(SpawnError::Sys(Errno::EIO));
        }
        let chunk = buffer.get(..read_len).ok_or(SpawnError::TooLarge)?;
        data.extend_from_slice(chunk);
    }
    Ok(data)
}

/// One executable source.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[derive(Clone, Copy)]
enum MapSource {
    /// The initfs memory object, with the archive offset already added per step.
    Initfs { data_offset: u64 },
    /// A memory object holding the whole file image at offset zero.
    Object(Handle),
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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

/// Read the final child identity from the kernel after the credential transition.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn child_identity(child: Handle) -> Result<ProcessIdentity, SpawnError> {
    #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
    {
        crate::sys::child_ids(child).map_err(SpawnError::Sys)
    }
    #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
    {
        let _ = child;
        Ok(ProcessIdentity::host_test())
    }
}

/// `AT_SECURE` is set exactly when the file's setuid/setgid bit is applied and the
/// mount did not ask for `EXEC_NOSUID`. The open image handle's `Stat` is the same
/// inode the kernel checks.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn file_secure(file: Handle, nosuid: bool) -> Result<bool, SpawnError> {
    #[cfg(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc"))]
    {
        const S_ISUID: u32 = 0o4000;
        const S_ISGID: u32 = 0o2000;
        let stat = Client::new().fstat(file).map_err(SpawnError::Sys)?;
        Ok(!nosuid && stat.mode & (S_ISUID | S_ISGID) != 0)
    }
    #[cfg(not(all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc")))]
    {
        let _ = (file, nosuid);
        Ok(false)
    }
}

/// Read the caller identity for a spawn without a credential transition.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn caller_identity() -> Result<ProcessIdentity, SpawnError> {
    #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
    {
        crate::sys::self_ids().map_err(SpawnError::Sys)
    }
    #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
    {
        Ok(ProcessIdentity::host_test())
    }
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[allow(clippy::too_many_arguments)] // the image pair, entry state and syscall sink
fn spawn_elf<S: Sys>(
    main_elf: &Elf<'_>,
    main_steps: BiasedPlan,
    main_source: ImageSource<'_>,
    main_bias: u64,
    interp: Option<(Elf<'_>, BiasedPlan, ImageSource<'_>, u64)>,
    libc: Option<u64>,
    cwd: &[u8],
    path: &str,
    options: &SpawnOptions<'_>,
    fdmap: &[(i32, u32)],
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

    #[cfg(any(feature = "alloc", test))]
    let fdmap_info = if fdmap.is_empty() {
        None
    } else {
        let mut entries = alloc_crate::vec::Vec::with_capacity(fdmap.len());
        for (fd, source) in fdmap {
            match sys.handle_transfer(child, Handle(*source)) {
                Ok(handle) => entries.push((*fd, handle.0)),
                Err(error) => {
                    let _ = sys.handle_close(child);
                    return Err(SpawnError::Sys(error));
                }
            }
        }
        let block = match encode_fdmap(&entries) {
            Ok(block) => block,
            Err(error) => {
                let _ = sys.handle_close(child);
                return Err(error);
            }
        };
        let page_len = match block
            .len()
            .checked_add(PAGE as usize - 1)
            .map(|len| len / PAGE as usize * PAGE as usize)
        {
            Some(page_len) if page_len != 0 => page_len,
            _ => {
                let _ = sys.handle_close(child);
                return Err(SpawnError::TooLarge);
            }
        };
        let address = match stack_base.checked_sub(page_len as u64) {
            Some(address) => address,
            None => {
                let _ = sys.handle_close(child);
                return Err(SpawnError::TooLarge);
            }
        };
        let object = match sys.memobj_create(page_len) {
            Ok(object) => object,
            Err(error) => {
                let _ = sys.handle_close(child);
                return Err(SpawnError::Sys(error));
            }
        };
        let scratch_flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
        let scratch = match sys.map(Handle::SELF_PROC, object, 0, 0, page_len, scratch_flags) {
            Ok(scratch) => scratch,
            Err(error) => {
                let _ = sys.handle_close(object);
                let _ = sys.handle_close(child);
                return Err(SpawnError::Sys(error));
            }
        };
        sys.write_parent_mem(scratch, &block);
        if let Err(error) = sys.unmap(Handle::SELF_PROC, scratch, page_len) {
            let _ = sys.handle_close(object);
            let _ = sys.handle_close(child);
            return Err(SpawnError::Sys(error));
        }
        let child_flags = map::pack(prot::READ, map::SHARED | map::FIXED);
        if let Err(error) = sys.map(child, object, 0, address as usize, page_len, child_flags) {
            let _ = sys.handle_close(object);
            let _ = sys.handle_close(child);
            return Err(SpawnError::Sys(error));
        }
        if let Err(error) = sys.handle_close(object) {
            let _ = sys.handle_close(child);
            return Err(SpawnError::Sys(error));
        }
        Some((address as usize, entries.len()))
    };
    // The fd map is an alloc-backed encoding; a no-alloc image cannot publish it.  Reject a
    // non-empty map instead of silently dropping inherited descriptors (K463).
    #[cfg(not(any(feature = "alloc", test)))]
    let fdmap_info = match no_alloc_fdmap(fdmap) {
        Ok(()) => None,
        Err(error) => {
            let _ = sys.handle_close(child);
            return Err(error);
        }
    };
    let (identity, secure) = if let Some((file, nosuid)) = exec {
        if let Err(error) = sys.exec_cred(child, file, nosuid) {
            let _ = sys.handle_close(child);
            return Err(SpawnError::Sys(error));
        }
        let identity = match child_identity(child) {
            Ok(identity) => identity,
            Err(error) => {
                let _ = sys.handle_close(child);
                return Err(error);
            }
        };
        let secure = match file_secure(file, nosuid) {
            Ok(secure) => secure,
            Err(error) => {
                let _ = sys.handle_close(child);
                return Err(error);
            }
        };
        (identity, secure)
    } else {
        match caller_identity() {
            Ok(identity) => (identity, false),
            Err(error) => {
                let _ = sys.handle_close(child);
                return Err(error);
            }
        }
    };
    let (auxv, auxv_len) = build_auxv(
        main_elf,
        main_bias,
        interp_bias,
        libc,
        fdmap_info,
        identity,
        secure,
    );
    let random = match random_bytes() {
        Ok(random) => random,
        Err(error) => {
            let _ = sys.handle_close(child);
            return Err(error);
        }
    };

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

    #[cfg(any(feature = "alloc", test))]
    let mut stack_buffer = alloc_crate::vec![0u8; STACK_SIZE];
    #[cfg(not(any(feature = "alloc", test)))]
    let mut stack_buffer = [0u8; 64 * 1024];
    #[cfg(not(any(feature = "alloc", test)))]
    let stack_size = stack_buffer.len();
    #[cfg(any(feature = "alloc", test))]
    let stack_size = STACK_SIZE;

    let (image_offset, sp0) = match build_stack(
        &mut stack_buffer,
        STACK_TOP,
        path,
        options.args,
        options.env,
        cwd,
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

    // The parent's scratch mapping above is SHARED only to populate the memory object. The
    // child's stack must be private: `PROC_FORK` can only copy-on-write a private mapping, and a
    // shared stack would let a forked child scribble over the parent's frames.
    let child_flags = map::pack(prot::READ | prot::WRITE, map::FIXED);
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
    match sys.thread_create(child, entry_address, sp0, sp0) {
        Ok(thread) => {
            // The child keeps running; the caller only needs the process handle. Closing the
            // thread handle here ends the leak without unwinding a started child on failure.
            let _ = sys.handle_close(thread);
            Ok(child)
        }
        Err(error) => {
            let _ = sys.handle_close(child);
            Err(SpawnError::Sys(error))
        }
    }
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn close_objects<S: Sys>(sys: &mut S, first: Option<Handle>, second: Option<Handle>) {
    if let Some(object) = first {
        let _ = sys.handle_close(object);
    }
    if let Some(object) = second {
        let _ = sys.handle_close(object);
    }
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
    let source_flags = map::pack(prot::READ, 0);
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
    // Writable program data must be private so a fork's COW sees the parent's pages; the
    // parent's scratch mapping above still populates the shared object.
    let child_flags = map::pack(prot::READ | prot::WRITE, map::FIXED);
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

    fn handle_transfer(&mut self, dest: Handle, source: Handle) -> Result<Handle, Errno> {
        // SAFETY: HANDLE_TRANSFER takes scalar handles and a rights mask; the kernel validates the
        // destination's WRITE right and the source's TRANSFER right.
        let result = unsafe {
            crate::sys::syscall3(
                Syscall::HANDLE_TRANSFER as usize,
                dest.0 as usize,
                source.0 as usize,
                Rights::ALL.0 as usize,
            )
        };
        handle_from_ret(result)
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
        let result = unsafe {
            crate::sys::syscall3(PROC_EXEC_CRED, proc.0 as usize, file.0 as usize, flags)
        };
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const PAGE: u64 = 4096;
/// Longest accepted child cwd (including the trailing NUL).
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const MAX_CWD: usize = 4096;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const WORD: usize = core::mem::size_of::<usize>();
/// Minimum child stack: `ARG_MAX` (CR-15) plus the C8 arrays and headroom.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const STACK_SIZE: usize = 256 * 1024;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const ARG_MAX: usize = 128 * 1024;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const _: () = assert!(STACK_SIZE >= ARG_MAX);
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const MAX_AUXV: usize = 32;
/// Raven fd-inheritance block address.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_RAVEN_FDMAP: usize = 0x7003;
/// Raven fd-inheritance entry count.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_RAVEN_FDMAP_N: usize = 0x7004;
/// Initfs archive offset of the libc image (K139); the loader maps it zero-copy.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_RAVEN_LIBC: usize = 0x7007;
/// Child working-directory string pointer (K165).
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_RAVEN_CWD: usize = 0x7005;
/// Raven ABI revision.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_RAVEN_ABI: usize = 0x7006;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_PHDR: usize = 3;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_PHENT: usize = 4;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_PHNUM: usize = 5;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_PAGESZ: usize = 6;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_BASE: usize = 7;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_FLAGS: usize = 8;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_ENTRY: usize = 9;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_UID: usize = 11;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_EUID: usize = 12;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_GID: usize = 13;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_EGID: usize = 14;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_PLATFORM: usize = 15;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_HWCAP: usize = 16;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_CLKTCK: usize = 17;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_SECURE: usize = 23;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_RANDOM: usize = 25;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_HWCAP2: usize = 26;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_EXECFN: usize = 31;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const AT_MINSIGSTKSZ: usize = 51;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const RANDOM_LEN: usize = 16;
/// Address where an `ET_DYN` main image is based.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const MAIN_PIE_BASE: u64 = 0x0000_1000_0000_0000;
/// Address where a `PT_INTERP` image is based.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const INTERP_BASE: u64 = 0x0000_2000_0000_0000;
/// Size of one ELF64 program header.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const PHENT: usize = 56;

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[cfg(target_arch = "x86_64")]
const MACHINE: Machine = Machine::X86_64;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[cfg(target_arch = "aarch64")]
const MACHINE: Machine = Machine::Aarch64;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[cfg(target_arch = "riscv64")]
const MACHINE: Machine = Machine::Riscv64;

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn target_machine() -> Machine {
    MACHINE
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[cfg(target_arch = "x86_64")]
const USER_MIN: u64 = raven_abi::layout::x86_64::USER_MIN;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[cfg(target_arch = "x86_64")]
const USER_END: u64 = raven_abi::layout::x86_64::USER_END;
// ABI v1 publishes the x86-64 layout only; use the same canonical user window on the other
// architectures until S12-S15/S19 define theirs.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[cfg(not(target_arch = "x86_64"))]
const USER_MIN: u64 = 0x0000_0000_0000_1000;
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[cfg(not(target_arch = "x86_64"))]
const USER_END: u64 = 0x0000_7FFF_FFFF_F000;

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
/// The stack top is the top of the user range; the stack occupies [`STACK_SIZE`] and `rsp == sp0`.
const STACK_TOP: u64 = USER_END;

/// `e_type` from an ELF64 header (`2 = ET_EXEC`, `3 = ET_DYN`).
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn elf_type(data: &[u8]) -> u16 {
    match data.get(16..18) {
        Some(bytes) => u16::from_le_bytes([bytes[0], bytes[1]]),
        None => 0,
    }
}

/// The `AT_PLATFORM` spelling for the build architecture.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Start,
    File,
    CopyTail,
    Zero,
    Done,
}

/// The page plan of one image with its load bias applied.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
struct BiasedPlan {
    plans: [SegPlan; MAX_SEGMENTS],
    count: usize,
    page: u64,
    segment_index: usize,
    phase: Phase,
    cursor: u64,
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
const MAX_SEGMENTS: usize = 16;

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn page_floor(value: u64) -> u64 {
    value - (value % PAGE)
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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
///
/// Parse at `0..USER_END`, not `USER_MIN..USER_END`: an `ET_DYN` image (including the release
/// `ld-elf.so.1` interpreter) may have raw segments at `vaddr=0`. `plan_biased` applies the
/// non-zero bias and then enforces `USER_MIN..USER_END`, so page 0 is never mapped.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn plan_image<'a>(
    data: &'a [u8],
    interpreter: bool,
) -> Result<(Elf<'a>, BiasedPlan, u64), SpawnError> {
    let elf = Elf::parse(data, target_machine(), 0..USER_END).map_err(SpawnError::BadElf)?;
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

/// Encode the `RFDM` block consumed by `raven_libc_platform::fd::inherit::FdMap::decode`.
///
/// Entries carry no path or status; the child's `restore_fdmap` creates unknown-kind OFDs with
/// read/write status and the transferred handles. The loader only publishes this block for
/// inherited stdio and ordinary file descriptors.
/// Reject an inherited fd map when the crate is built without `alloc`.
///
/// The wire block is built in a heap `Vec`; a no-alloc image cannot publish it.  Empty is the
/// common case and stays supported (K463).
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[allow(dead_code)] // ravenbsd target builds compile this for the no-alloc path; the alloc build uses encode_fdmap
fn no_alloc_fdmap(fdmap: &[(i32, u32)]) -> Result<(), SpawnError> {
    if fdmap.is_empty() {
        Ok(())
    } else {
        Err(SpawnError::Sys(Errno::ENOTSUP))
    }
}

#[cfg(any(
    test,
    all(any(target_os = "none", target_os = "ravenbsd"), feature = "alloc")
))]
fn encode_fdmap(entries: &[(i32, u32)]) -> Result<alloc_crate::vec::Vec<u8>, SpawnError> {
    let count = u16::try_from(entries.len()).map_err(|_| SpawnError::TooLarge)?;
    let mut out = alloc_crate::vec::Vec::with_capacity(8usize.saturating_add(18 * entries.len()));
    out.extend_from_slice(b"RFDM");
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    for (fd, handle) in entries {
        out.extend_from_slice(&fd.to_le_bytes());
        out.extend_from_slice(&handle.to_le_bytes());
        out.push(255); // OfdKind::Unknown
        out.push(0); // FD_CLOEXEC already filtered by apply_actions
        out.extend_from_slice(&2u32.to_le_bytes()); // O_RDWR
        out.extend_from_slice(&u32::MAX.to_le_bytes()); // NO_PATH
    }
    Ok(out)
}

/// Build the PG-50 auxiliary vector for the main image and an optional interpreter.
///
/// `AT_PLATFORM`, `AT_RANDOM` and `AT_EXECFN` carry zero here; [`build_stack`] replaces them with
/// the stack addresses it creates.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn build_auxv(
    elf: &Elf<'_>,
    main_bias: u64,
    interp_bias: u64,
    libc: Option<u64>,
    fdmap: Option<(usize, usize)>,
    identity: ProcessIdentity,
    secure: bool,
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
    push(AT_UID, identity.uid as usize);
    push(AT_EUID, identity.euid as usize);
    push(AT_GID, identity.gid as usize);
    push(AT_EGID, identity.egid as usize);
    push(AT_PLATFORM, 0);
    push(AT_HWCAP, 0);
    push(AT_CLKTCK, 100);
    push(AT_SECURE, usize::from(secure));
    push(AT_RANDOM, 0);
    push(AT_HWCAP2, 0);
    push(AT_EXECFN, 0);
    push(AT_MINSIGSTKSZ, 8192);
    push(AT_RAVEN_CWD, 0);
    if let Some(offset) = libc {
        push(AT_RAVEN_LIBC, usize::try_from(offset).unwrap_or_default());
    }
    if let Some((address, count)) = fdmap {
        push(AT_RAVEN_FDMAP, address);
        push(AT_RAVEN_FDMAP_N, count);
        push(AT_RAVEN_ABI, raven_abi::ABI_VERSION as usize);
    }
    (auxv, len)
}

/// Build the C8 initial stack image in `buffer`.
///
/// `buffer` covers the top `buffer.len()` bytes of the child stack (`stack_top - buffer.len()` ..
/// `stack_top`). The image is written at the end of the buffer and the function returns the buffer
/// offset of `sp0` and the child virtual address of `sp0`. Strings are placed at the top in
/// argument/environment order; pointer arrays follow the C8 order. `AT_RANDOM` in `auxv` is
/// replaced with the address of `random`, which is placed immediately below the strings.
#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
#[allow(clippy::too_many_arguments)] // the C8 block is passed explicitly
fn build_stack(
    buffer: &mut [u8],
    stack_top: u64,
    path: &str,
    args: &[&str],
    env: &[&str],
    cwd: &[u8],
    auxv: &[(usize, usize)],
    random: &[u8; RANDOM_LEN],
    platform: &str,
) -> Result<(usize, u64), SpawnError> {
    if !stack_top.is_multiple_of(16) || buffer.is_empty() {
        return Err(SpawnError::TooLarge);
    }
    if cwd.is_empty() || !cwd.starts_with(b"/") || cwd.len() >= MAX_CWD || cwd.contains(&0) {
        return Err(SpawnError::BadCwd);
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
    strings_len = strings_len
        .checked_add(cwd.len())
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
    let cwd_text = core::str::from_utf8(cwd).map_err(|_| SpawnError::BadCwd)?;
    let cwd_address = {
        let start = stack_base
            .checked_add(u64::try_from(cursor).map_err(|_| SpawnError::TooLarge)?)
            .ok_or(SpawnError::TooLarge)?;
        let _ = write_string(buffer, &mut cursor, cwd_text, stack_base)?;
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
        } else if *key == AT_RAVEN_CWD {
            usize::try_from(cwd_address).map_err(|_| SpawnError::TooLarge)?
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn write_word(buffer: &mut [u8], offset: usize, value: usize) -> Result<(), SpawnError> {
    let end = offset.checked_add(WORD).ok_or(SpawnError::TooLarge)?;
    let Some(slot) = buffer.get_mut(offset..end) else {
        return Err(SpawnError::TooLarge);
    };
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
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

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
fn align_down(value: u64, align: u64) -> u64 {
    value - value % align
}

#[cfg(any(any(target_os = "none", target_os = "ravenbsd"), test))]
/// Fill 16 bytes for `AT_RANDOM` from the kernel's boot-seeded pool.
///
/// A pool failure aborts the exec with the kernel's error; there is no clock/address
/// fallback in this crate.
fn random_bytes() -> Result<[u8; RANDOM_LEN], SpawnError> {
    #[cfg(any(target_os = "none", target_os = "ravenbsd"))]
    {
        let mut bytes = [0u8; RANDOM_LEN];
        crate::sys::getrandom(&mut bytes).map_err(SpawnError::Sys)?;
        Ok(bytes)
    }
    #[cfg(not(any(target_os = "none", target_os = "ravenbsd")))]
    {
        Ok([0u8; RANDOM_LEN])
    }
}

#[cfg(test)]
#[path = "spawn_ut.rs"]
mod tests;
