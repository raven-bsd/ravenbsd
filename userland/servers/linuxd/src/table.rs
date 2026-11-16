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
use raven_proto_linux::LinuxFrame;

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

/// Page size of the guest's mappings.
pub const PAGE: u64 = 4096;

/// Fixed address of the clone coordination page.
///
/// linuxd maps one shared, zero-filled page here before the Linux program starts. A clone child
/// has no process identity in the trap packet, so the fixture children signal their parent through
/// this page instead of making a syscall. The address is below the loader and stack ranges and is
/// never handed out by the `mmap` arena.
pub const FLAG_ADDR: u64 = 0x0000_4800_0000_0000;

/// First fixed address used for clone trampoline pages (one page per `clone`).
pub const TRAMPOLINE_BASE: u64 = FLAG_ADDR + PAGE;

/// One queued `clone`: the trap frame plus the Linux arguments.
#[derive(Clone, Copy, Debug)]
pub struct CloneRequest {
    /// `clone` flag word (including the low signal byte).
    pub flags: u64,
    /// Child stack pointer from the syscall (thread clones).
    pub child_stack: u64,
    /// Parent tid output pointer (`CLONE_PARENT_SETTID`).
    pub ptid: u64,
    /// Child tid output pointer (`CLONE_CHILD_SETTID`/`CLONE_CHILD_CLEARTID`).
    pub ctid: u64,
    /// Child TLS base (`CLONE_SETTLS`).
    pub tls: u64,
    /// The parent's saved user frame.
    pub frame: LinuxFrame,
}

/// One native task created by `clone`.
#[derive(Debug)]
pub struct CloneChild {
    /// Linux pid (fork-like clone) or tid (thread clone) returned to the parent.
    pub task: u32,
    /// Native thread id the kernel assigned to the child thread.
    pub native_tid: u32,
    /// Native process handle for a fork-like child; `None` for a thread.
    pub process: Option<Handle>,
    /// Native thread handle for a thread child; `None` when the handle was closed.
    pub thread: Option<Handle>,
}

/// One Linux process the server is responsible for.
#[derive(Debug)]
pub struct LinuxProcess {
    /// Linux pid shown to the embedded program.
    pub pid: i32,
    /// Native thread id of the Linux main thread (`THREAD_SELF` value).
    pub tid: u32,
    /// Native process handle the loader created.
    pub proc: Handle,
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
    /// Clear-on-exit word (`set_tid_address`), zero when unset.
    pub clear_tid: u64,
    /// Robust-list head and its UAPI length (`set_robust_list`).
    pub robust_list: u64,
    /// Length accepted by `set_robust_list` (24 on 64-bit).
    pub robust_list_len: usize,
    /// Frame captured by the current trap; `clone` consumes it.
    pub frame: Option<LinuxFrame>,
    /// A `clone` the handler validated and the trap loop must perform.
    pub pending_clone: Option<CloneRequest>,
    /// Native tasks created by `clone`, reaped when the process exits.
    pub clones: Vec<CloneChild>,
    /// Next Linux pid/tid handed to a clone child.
    pub next_task: u32,
    /// Next fixed address used for a clone trampoline page.
    pub trampoline_next: u64,
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
    files: Vec<Option<OpenFile>>,
}

impl LinuxProcess {
    /// Create the context for a freshly loaded process.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pid: i32,
        tid: u32,
        proc: Handle,
        thread: Handle,
        brk: u64,
        brk_limit: u64,
        mmap_next: u64,
        mmap_limit: u64,
    ) -> Self {
        Self {
            pid,
            tid,
            proc,
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
            clear_tid: 0,
            robust_list: 0,
            robust_list_len: 0,
            frame: None,
            pending_clone: None,
            clones: Vec::new(),
            next_task: 2,
            trampoline_next: TRAMPOLINE_BASE,
            fs_base: 0,
            pending_tls: None,
            exe: Vec::new(),
            comm: Vec::new(),
            argv: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Allocate the next Linux pid/tid for a clone child.
    pub fn next_task_id(&mut self) -> u32 {
        let task = self.next_task.max(2);
        self.next_task = task.saturating_add(1);
        task
    }

    /// Hand out the next fixed trampoline page address.
    pub fn next_trampoline(&mut self) -> u64 {
        let address = self.trampoline_next.max(TRAMPOLINE_BASE);
        self.trampoline_next = address.saturating_add(PAGE);
        address
    }

    /// Set the program name used by `prctl` and `/proc/self/stat`.
    pub fn set_name(&mut self, bytes: &[u8]) {
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len())
            .min(15);
        self.comm = bytes.get(..end).unwrap_or(&[]).to_vec();
    }

    /// The 16-byte `prctl(PR_GET_NAME)` buffer: the name and a NUL terminator.
    #[must_use]
    pub fn name_bytes(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        let take = self.comm.len().min(15);
        out[..take].copy_from_slice(&self.comm[..take]);
        out
    }

    /// The NUL-terminated program name, as `prctl` stores it.
    #[must_use]
    pub fn comm(&self) -> &[u8] {
        &self.comm
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
