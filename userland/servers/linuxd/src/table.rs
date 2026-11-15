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

use crate::rootfs::RootTree;

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

/// The content of one open synthetic file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// A regular file with bytes produced at `open` time.
    Regular(Vec<u8>),
    /// A directory with a snapshot of its entries.
    Directory(Vec<DirEntry>),
    /// A symlink target returned by `readlink`/`readlinkat`.
    Symlink(Vec<u8>),
}

/// One descriptor in a Linux process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenFile {
    /// Absolute path the program opened.
    pub path: Vec<u8>,
    /// Synthetic content.
    pub kind: FileKind,
    /// Read/lseek/directory cursor.
    pub offset: u64,
}

impl OpenFile {
    /// Length of a regular file or symlink target (zero for a directory).
    #[must_use]
    pub fn size(&self) -> u64 {
        match &self.kind {
            FileKind::Regular(bytes) | FileKind::Symlink(bytes) => bytes.len() as u64,
            FileKind::Directory(_) => 0,
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
    /// Metadata view of the staged Linux root (`/compat/linux`).
    root: RootTree,
    /// Current working directory; relative paths resolve here.
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
            thread,
            fs_base: 0,
            pending_tls: None,
            exe: Vec::new(),
            comm: Vec::new(),
            argv: Vec::new(),
            root: RootTree::default(),
            cwd: b"/compat/linux".to_vec(),
            files: Vec::new(),
        }
    }

    /// Install the staged-root metadata view.
    pub fn set_root(&mut self, root: RootTree) {
        self.root = root;
    }

    /// The staged-root metadata view.
    #[must_use]
    pub fn root(&self) -> &RootTree {
        &self.root
    }

    /// The current working directory used for relative path resolution.
    #[must_use]
    pub fn cwd(&self) -> &[u8] {
        &self.cwd
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

    /// Open a synthetic file/directory/symlink and return its descriptor.
    pub fn open_file(&mut self, path: Vec<u8>, kind: FileKind) -> i32 {
        let file = OpenFile {
            path,
            kind,
            offset: 0,
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

    /// One descriptor's file, if it is open.
    #[must_use]
    pub fn file(&self, fd: i32) -> Option<&OpenFile> {
        let index = usize::try_from(fd.checked_sub(3)?).ok()?;
        self.files.get(index)?.as_ref()
    }

    /// Mutable access to one descriptor's file.
    pub fn file_mut(&mut self, fd: i32) -> Option<&mut OpenFile> {
        let index = usize::try_from(fd.checked_sub(3)?).ok()?;
        self.files.get_mut(index)?.as_mut()
    }

    /// Close a synthetic descriptor. Standard descriptors are accepted as a no-op.
    pub fn close_file(&mut self, fd: i32) -> bool {
        if (0..=2).contains(&fd) {
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
        match fd {
            0 => Some(b"/dev/null"),
            1 | 2 => Some(b"tty:console"),
            _ => self.file(fd).map(|file| file.path.as_slice()),
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
