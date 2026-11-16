// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-process Linux state and the regions the server can copy out of.
//!
//! A [`LinuxProcess`] is the dispatcher's `LinuxContext`. It is owned state only: the trap loop
//! pre-fetches user-memory arguments into [`LinuxProcess::user_buffer`] before dispatch, because
//! reading a Linux process's memory needs the memory-object handles the loader keeps and the
//! dispatcher must stay a pure function table.

use alloc::vec::Vec;

use raven_abi::types::Handle;
use raven_linux_dispatch::LinuxContext;

use crate::files::FileOp;

/// Largest single user buffer the trap loop copies in.
pub const USER_BUFFER_MAX: usize = 64 * 1024;

/// One mapping of a Linux process, as the server knows it.
///
/// `backing` is the memory object the loader mapped into the child; `backing_offset` is the byte
/// offset of `start` inside it. `Handle::NONE` marks a zero-filled kernel mapping with no object of
/// its own (the trap loop cannot copy out of it and reports `EFAULT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// First Linux virtual address of the mapping.
    pub start: u64,
    /// Length in bytes.
    pub len: u64,
    /// Memory object backing the mapping, or [`Handle::NONE`].
    pub backing: Handle,
    /// Byte offset of `start` inside `backing`.
    pub backing_offset: u64,
}

impl Region {
    /// Whether `address..address + len` is wholly inside this region.
    pub fn contains(&self, address: u64, len: u64) -> bool {
        let end = address.checked_add(len);
        let region_end = self.start.checked_add(self.len);
        match (end, region_end) {
            (Some(end), Some(region_end)) => address >= self.start && end <= region_end,
            _ => false,
        }
    }
}

/// One synthetic directory entry, in `getdents64` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// Entry name without a NUL.
    pub name: Vec<u8>,
    /// Linux `d_type` (`DT_REG` 8, `DT_DIR` 4, `DT_LNK` 10).
    pub kind: u8,
    /// Inode number shown to the program.
    pub ino: u64,
}

/// The content of one open synthetic or remote file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A regular file with bytes produced at `open` time.
    Regular(Vec<u8>),
    /// A directory with a snapshot of its entries.
    Directory(Vec<DirEntry>),
    /// A symlink target returned by `readlink`/`readlinkat`.
    Symlink(Vec<u8>),
    /// A live `file:`/`ufs:` handle served by the VFS stack.
    Remote(Handle),
}

/// One descriptor in a Linux process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenFile {
    /// Absolute path the program opened.
    pub path: Vec<u8>,
    /// Synthetic content or a remote scheme handle.
    pub kind: FileKind,
    /// Read/lseek/directory cursor for synthetic files; the scheme server owns the remote cursor.
    pub offset: u64,
    /// Linux `open` flags (`O_RDONLY`, `O_APPEND`, ...).
    pub flags: i32,
    /// Linux descriptor flags (`FD_CLOEXEC`).
    pub fd_flags: i32,
}

impl OpenFile {
    /// Length of a regular file or symlink target (zero for a directory or remote file).
    #[must_use]
    pub fn size(&self) -> u64 {
        match &self.kind {
            FileKind::Regular(bytes) | FileKind::Symlink(bytes) => bytes.len() as u64,
            FileKind::Directory(_) | FileKind::Remote(_) => 0,
        }
    }

    /// The scheme handle for a remote descriptor.
    #[must_use]
    pub fn remote(&self) -> Option<Handle> {
        match self.kind {
            FileKind::Remote(handle) => Some(handle),
            _ => None,
        }
    }
}

/// One Linux process the server is responsible for.
#[derive(Debug)]
pub struct LinuxProcess {
    /// Linux pid shown to the embedded program.
    pub pid: i32,
    /// Exit status once `exit_group` was served.
    pub exit: Option<i32>,
    /// Current program break.
    pub brk: u64,
    /// Initial program break (the first byte of the pre-mapped break arena).
    pub brk_start: u64,
    /// First address past the pre-mapped break arena.
    pub brk_limit: u64,
    /// Next address handed out by anonymous `mmap`.
    pub mmap_next: u64,
    /// First address past the pre-mapped anonymous arena.
    pub mmap_limit: u64,
    /// Every mapping the process owns.
    pub regions: Vec<Region>,
    /// Bytes `write` produced, drained by the server to its log sink.
    pub output: Vec<u8>,
    /// User-memory bytes the trap loop copied in for the current call.
    pub user_buffer: Vec<u8>,
    /// Bytes a handler wants the trap loop to copy into the process, with their address.
    pub pending_write: Option<(u64, Vec<u8>)>,
    /// A file operation the trap loop must perform (handlers are pure function tables).
    pub pending_file: Option<FileOp>,
    /// Thread handle of the Linux thread (`THREAD_CREATE`), used by `THREAD_SET_TLS`.
    pub thread: Handle,
    /// Current user TLS base, as `arch_prctl` `ARCH_GET_FS` reports it.
    pub fs_base: u64,
    /// A base the `arch_prctl` handler queued for the trap loop to install before it replies.
    pub pending_tls: Option<u64>,
    /// Absolute path of the loaded program (`/proc/self/exe`).
    pub exe: Vec<u8>,
    /// Program name without the directory (`/proc/self/stat` field 2).
    pub comm: Vec<u8>,
    /// Linux argv, including `argv[0]` (`/proc/self/cmdline`).
    pub argv: Vec<Vec<u8>>,
    /// Standard descriptors; `dup3` may replace them.
    stdio: [Option<OpenFile>; 3],
    /// Current working directory for relative `openat` paths.
    cwd: Vec<u8>,
    files: Vec<Option<OpenFile>>,
}

impl LinuxProcess {
    /// Create the context for a freshly loaded process.
    pub fn new(
        pid: i32,
        thread: Handle,
        brk: u64,
        brk_limit: u64,
        mmap_next: u64,
        mmap_limit: u64,
    ) -> Self {
        Self {
            pid,
            exit: None,
            brk,
            brk_start: brk,
            brk_limit,
            mmap_next,
            mmap_limit,
            regions: Vec::new(),
            output: Vec::new(),
            user_buffer: Vec::new(),
            pending_write: None,
            pending_file: None,
            thread,
            fs_base: 0,
            pending_tls: None,
            exe: Vec::new(),
            comm: Vec::new(),
            argv: Vec::new(),
            stdio: [None, None, None],
            cwd: b"/".to_vec(),
            files: Vec::new(),
        }
    }

    /// Set the program identity used by the synthetic procfs files.
    pub fn set_identity(&mut self, exe: &[u8], argv: Vec<Vec<u8>>) {
        self.exe = exe.to_vec();
        self.comm = exe
            .rsplit(|byte| *byte == b'/')
            .next()
            .unwrap_or(exe)
            .to_vec();
        self.argv = argv;
    }

    /// All descriptor slots; slot `n` is fd `3 + n` and `None` means closed.
    #[must_use]
    pub fn files(&self) -> &[Option<OpenFile>] {
        &self.files
    }

    /// The current working directory used for relative paths.
    #[must_use]
    pub fn cwd(&self) -> &[u8] {
        &self.cwd
    }

    /// Open a read-only synthetic file/directory/symlink and return its descriptor.
    pub fn open_file(&mut self, path: Vec<u8>, kind: FileKind) -> i32 {
        self.open_with_flags(path, kind, crate::files::O_RDONLY)
    }

    /// Open a synthetic file/directory/symlink with Linux `open` flags.
    pub fn open_with_flags(&mut self, path: Vec<u8>, kind: FileKind, flags: i32) -> i32 {
        let fd_flags = if flags & crate::files::O_CLOEXEC != 0 {
            crate::files::FD_CLOEXEC
        } else {
            0
        };
        let file = OpenFile {
            path,
            kind,
            offset: 0,
            flags: flags & !crate::files::O_CLOEXEC,
            fd_flags,
        };
        if let Some(index) = self.files.iter().position(Option::is_none) {
            if let Some(slot) = self.files.get_mut(index) {
                *slot = Some(file);
            }
            return 3 + index as i32;
        }
        self.files.push(Some(file));
        3 + self.files.len() as i32 - 1
    }

    /// Open a remote `file:`/`ufs:` handle and return its descriptor.
    pub fn open_remote(&mut self, path: Vec<u8>, handle: Handle, flags: i32) -> i32 {
        self.open_with_flags(path, FileKind::Remote(handle), flags)
    }

    /// Install `file` at an exact descriptor, replacing any previous file there.
    ///
    /// Returns false for a negative descriptor. Standard descriptors are stored in the
    /// three-slot table; higher descriptors extend the dynamic table.
    pub fn install_file(&mut self, fd: i32, file: OpenFile) -> bool {
        let Ok(index) = usize::try_from(fd) else {
            return false;
        };
        if index < 3 {
            self.stdio[index] = Some(file);
            return true;
        }
        let slot = index - 3;
        if self.files.len() <= slot {
            self.files.resize(slot + 1, None);
        }
        if let Some(entry) = self.files.get_mut(slot) {
            *entry = Some(file);
            return true;
        }
        false
    }

    /// Lowest free descriptor at or above `min`, or the next dynamic slot.
    #[must_use]
    pub fn lowest_free_fd(&self, min: i32) -> i32 {
        let start = min.max(0);
        let mut fd = start;
        while fd < 3 {
            if self.stdio[fd as usize].is_none() {
                return fd;
            }
            fd += 1;
        }
        let mut index = (start.max(3) - 3) as usize;
        while index < self.files.len() {
            if self.files[index].is_none() {
                return 3 + index as i32;
            }
            index += 1;
        }
        3 + self.files.len() as i32
    }

    /// One descriptor's file, if it is open.
    #[must_use]
    pub fn file(&self, fd: i32) -> Option<&OpenFile> {
        if (0..3).contains(&fd) {
            return self.stdio[fd as usize].as_ref();
        }
        let index = usize::try_from(fd.checked_sub(3)?).ok()?;
        self.files.get(index)?.as_ref()
    }

    /// Mutable access to one descriptor's file.
    pub fn file_mut(&mut self, fd: i32) -> Option<&mut OpenFile> {
        if (0..3).contains(&fd) {
            return self.stdio[fd as usize].as_mut();
        }
        let index = usize::try_from(fd.checked_sub(3)?).ok()?;
        self.files.get_mut(index)?.as_mut()
    }

    /// Close a descriptor. Standard descriptors without a replacement are a no-op.
    pub fn close_file(&mut self, fd: i32) -> bool {
        if (0..3).contains(&fd) {
            self.stdio[fd as usize] = None;
            return true;
        }
        let Some(index) = usize::try_from(fd.checked_sub(3).unwrap_or(-1)).ok() else {
            return false;
        };
        match self.files.get_mut(index) {
            Some(slot @ Some(_)) => {
                *slot = None;
                true
            }
            _ => false,
        }
    }

    /// The target of `/proc/self/fd/<fd>`.
    #[must_use]
    pub fn fd_target(&self, fd: i32) -> Option<&[u8]> {
        if let Some(file) = self.file(fd) {
            return Some(file.path.as_slice());
        }
        match fd {
            0 => Some(b"/dev/null"),
            1 | 2 => Some(b"tty:console"),
            _ => None,
        }
    }

    /// The region holding `address..address + len`, if any.
    pub fn region_for(&self, address: u64, len: u64) -> Option<Region> {
        self.regions
            .iter()
            .find(|region| region.contains(address, len))
            .copied()
    }

    /// Drain the bytes queued by `write`.
    pub fn take_output(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.output)
    }
}

impl LinuxContext for LinuxProcess {
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

#[cfg(test)]
#[path = "table_ut.rs"]
mod tests;
