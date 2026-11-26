// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handles, rights, mapping flags, time.

/// A per-process handle: low 24 bits index into the handle table, high 8 bits generation.
/// A stale handle (generation mismatch) fails with `EBADF`.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Handle(pub u32);

impl Handle {
    /// "No handle" (for example anonymous memory in `MAP`).
    pub const NONE: Handle = Handle(u32::MAX);
    /// In the first process: handle to the process itself (all rights).
    pub const SELF_PROC: Handle = Handle(0);
    /// In the first process: initfs memory object (`READ | MAP`).
    pub const INITFS: Handle = Handle(1);
    /// In the first process: root namespace.
    pub const ROOT_NAMESPACE: Handle = Handle(2);
    /// In the first process only: the root device capability (`ADMIN`); the source of `MmioRegion`, `DmaBuffer`, `Irq` and boot blobs (ticket S21a).
    pub const ROOT_DEVICE: Handle = Handle(3);

    /// Build a handle from table index and generation.
    pub const fn new(index: u32, generation: u8) -> Handle {
        Handle((index & 0x00FF_FFFF) | ((generation as u32) << 24))
    }

    /// Table index (low 24 bits).
    pub const fn index(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }

    /// Generation (high 8 bits).
    pub const fn generation(self) -> u8 {
        (self.0 >> 24) as u8
    }
}

/// A rights bitmask attached to a handle. Rights never grow (`handle_dup` only intersects).
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub struct Rights(pub u32);

impl Rights {
    /// Read data / receive.
    pub const READ: Rights = Rights(1 << 0);
    /// Write data / send.
    pub const WRITE: Rights = Rights(1 << 1);
    /// Map into an address space.
    pub const MAP: Rights = Rights(1 << 2);
    /// Map executable.
    pub const EXEC: Rights = Rights(1 << 3);
    /// Duplicate the handle.
    pub const DUP: Rights = Rights(1 << 4);
    /// Transfer to another process.
    pub const TRANSFER: Rights = Rights(1 << 5);
    /// Signal / interrupt the object.
    pub const SIGNAL: Rights = Rights(1 << 6);
    /// Administrative operations (create children capabilities, register schemes, ...).
    pub const ADMIN: Rights = Rights(1 << 7);
    /// Every defined right.
    pub const ALL: Rights = Rights(0xFF);
    /// No rights.
    pub const NONE: Rights = Rights(0);

    /// Raw bits.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// True if every right in `other` is present.
    pub const fn contains(self, other: Rights) -> bool {
        self.0 & other.0 == other.0
    }

    /// Union of both sets.
    pub const fn union(self, other: Rights) -> Rights {
        Rights(self.0 | other.0)
    }

    /// Intersection of both sets (the only way rights change on duplication).
    pub const fn intersect(self, other: Rights) -> Rights {
        Rights(self.0 & other.0)
    }
}

/// A process personality.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Personality {
    /// The native RavenBSD syscall table.
    #[default]
    Native = 0,
    /// Linux syscall numbers are trapped to the process handler (ticket S41).
    Linux = 1,
}

impl Personality {
    /// Decode one raw personality value.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Native),
            1 => Some(Self::Linux),
            _ => None,
        }
    }
}

/// Personality-related creation and setup flags.
pub mod personality {
    /// `PROC_CREATE` flag bit 0: create the process with the Linux personality
    /// and no handler (the child sets or receives one before its first trap).
    pub const CREATE_LINUX: usize = 1;
}

/// Memory protection bits (low byte of the `MAP` flags argument).
pub mod prot {
    /// Readable.
    pub const READ: usize = 1;
    /// Writable.
    pub const WRITE: usize = 2;
    /// Executable. `WRITE | EXEC` together is rejected (W^X).
    pub const EXEC: usize = 4;
    /// Mask of valid protection bits.
    pub const MASK: usize = 7;
}

/// Memory-object creation flags (the `flags` argument of `MEMOBJ_CREATE`).
pub mod memobj {
    /// The caller becomes the pager; the returned handle is the pager key and pages are lazy.
    pub const PAGER: usize = 1;
}

/// Mapping flags (`map_flags << 8` in the `MAP` flags argument).
pub mod map {
    /// Place the mapping exactly at `addr`; fail if it overlaps.
    pub const FIXED: usize = 1;
    /// Share the pages with other mappings of the memory object (default is private copy-on-write).
    pub const SHARED: usize = 2;
    /// Do not inherit this mapping (or its object handle) in a forked child (PG-14).
    pub const NOFORK: usize = 8;
    /// The forked child gets zero-filled pages at the same virtual range (PG-14).
    pub const ZERO_ON_FORK: usize = 16;
    /// Replace an existing anonymous `PROT_NONE` reservation atomically (PG-13, CR-24).
    pub const REPLACE: usize = 4;
    /// Shift of `map_flags` inside the `MAP` flags argument.
    pub const SHIFT: usize = 8;

    /// Pack protection and map flags into the sixth `MAP` argument.
    pub const fn pack(prot: usize, flags: usize) -> usize {
        prot | (flags << SHIFT)
    }
}

/// Futex operations.
pub mod futex {
    /// Sleep while the word at the address equals `val`.
    pub const WAIT: u32 = 0;
    /// Wake up to `val` waiters.
    pub const WAKE: u32 = 1;
    /// Move waiters from the source address to a destination address (PG-3).
    ///
    /// `val` is the number to wake at the source, the fourth `FUTEX` argument is the maximum
    /// number to move, and the fifth is the destination address. Returns the number of waiters
    /// woken plus requeued; see [`crate::syscall::Syscall::FUTEX`].
    pub const REQUEUE: u32 = 3;
    /// Operate on the private `(pid, virtual address)` key (PG-3).
    ///
    /// With this bit clear the address is resolved through its VMA and, for a
    /// shared mapping, keyed on `(memory object id, offset)`.
    pub const PRIVATE: u32 = 0x80;
    /// Robust futex word: the previous owner died while holding the lock.
    pub const OWNER_DIED: u32 = 0x4000_0000;
    /// Bits carrying the owner thread id in a robust futex word.
    pub const OWNER_TID: u32 = 0x3fff_ffff;
}

/// Clock identifiers (Linux numbering).
pub mod clock {
    /// Wall-clock time since the Unix epoch.
    pub const REALTIME: u32 = 0;
    /// Monotonic time since boot.
    pub const MONOTONIC: u32 = 1;
}

/// Kernel log ring record layout and `KLOG_READ` result flags.
pub mod klog {
    /// `KlogInfo.version` written by this kernel.
    pub const VERSION: u32 = 1;
    /// The reader cursor was older than the first retained record and was clamped.
    pub const LOST: u32 = 1;
    /// The next record did not fit in the supplied buffer; the cursor was not advanced past it.
    pub const MORE: u32 = 2;
    /// Fixed record header: `mono_ns: u64`, `level: u8`, `flags: u8`, `len: u16`,
    /// `reserved: u32`.
    pub const HEADER_LEN: usize = 16;
    /// Maximum record body in bytes.
    pub const MAX_BODY: usize = 1024;
    /// Record flag bit 0: written from the panic path.
    pub const RECORD_PANIC: u8 = 1;

    /// Result record for [`crate::syscall::KLOG_READ`].
    ///
    /// `next_cursor` is the absolute byte cursor the caller must pass on its next read.
    /// `first_cursor` and `last_cursor` bound the retained records; `last_cursor` is also the
    /// position where a caught-up reader stops.
    #[repr(C)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub struct KlogInfo {
        /// Record layout version; [`VERSION`].
        pub version: u32,
        /// [`LOST`] and/or [`MORE`].
        pub flags: u32,
        /// Cursor after the last complete record copied.
        pub next_cursor: u64,
        /// Cumulative count of records lost to overwrite or an oversized body.
        pub dropped: u64,
        /// Absolute cursor of the oldest retained record.
        pub first_cursor: u64,
        /// Absolute cursor of the next write / caught-up position.
        pub last_cursor: u64,
    }

    const _: () = assert!(core::mem::size_of::<KlogInfo>() == 40);
    const _: () = assert!(core::mem::align_of::<KlogInfo>() == 8);
    const _: () = assert!(core::mem::offset_of!(KlogInfo, version) == 0);
    const _: () = assert!(core::mem::offset_of!(KlogInfo, flags) == 4);
    const _: () = assert!(core::mem::offset_of!(KlogInfo, next_cursor) == 8);
    const _: () = assert!(core::mem::offset_of!(KlogInfo, dropped) == 16);
    const _: () = assert!(core::mem::offset_of!(KlogInfo, first_cursor) == 24);
    const _: () = assert!(core::mem::offset_of!(KlogInfo, last_cursor) == 32);
}

/// Namespace prefix operations for `PROC_NAMESPACE`.
pub mod namespace {
    /// Grant every scheme name starting with the prefix.
    pub const ADD: u32 = 1;
    /// Remove the prefix; existing handles keep working.
    pub const REMOVE: u32 = 2;
    /// Remove all prefixes; exact names stay.
    pub const CLEAR: u32 = 3;
}

/// Seconds and nanoseconds, 64-bit each on every architecture.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Timespec {
    /// Seconds.
    pub tv_sec: i64,
    /// Nanoseconds, `0..1_000_000_000`.
    pub tv_nsec: i64,
}

/// POSIX record-lock wire types and commands for [`crate::syscall::RECORD_LOCK`].
pub mod fcntl {
    /// Shared (read) lock type.
    pub const RDLCK: i16 = 0;
    /// Exclusive (write) lock type.
    pub const WRLCK: i16 = 1;
    /// Unlock.
    pub const UNLCK: i16 = 2;

    /// `F_GETLK`: report the first conflicting lock.
    pub const GETLK: u32 = 5;
    /// `F_SETLK`: set or clear a lock without blocking.
    pub const SETLK: u32 = 6;
    /// `F_SETLKW`: set or clear a lock, blocking on a conflict.
    pub const SETLKW: u32 = 7;

    /// The 32-byte `struct flock` layout shared with libc.
    #[repr(C)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
    pub struct RecordLock {
        /// Lock type: [`RDLCK`], [`WRLCK`] or [`UNLCK`].
        pub l_type: i16,
        /// Origin: `SEEK_SET` (0), `SEEK_CUR` (1) or `SEEK_END` (2).
        pub l_whence: i16,
        /// Start offset relative to `l_whence`.
        pub l_start: i64,
        /// Length in bytes; zero means to EOF.
        pub l_len: i64,
        /// Conflicting owner pid for [`GETLK`]; ignored on input.
        pub l_pid: i32,
    }

    const _: () = assert!(core::mem::size_of::<RecordLock>() == 32);
    const _: () = assert!(core::mem::align_of::<RecordLock>() == 8);
    const _: () = assert!(core::mem::offset_of!(RecordLock, l_type) == 0);
    const _: () = assert!(core::mem::offset_of!(RecordLock, l_whence) == 2);
    const _: () = assert!(core::mem::offset_of!(RecordLock, l_start) == 8);
    const _: () = assert!(core::mem::offset_of!(RecordLock, l_len) == 16);
    const _: () = assert!(core::mem::offset_of!(RecordLock, l_pid) == 24);
}

/// POSIX file-creation-mask semantics (FreeBSD `p_pd->pd_cmask`).
///
/// FreeBSD stores the mask with [`ALLPERMS`] (`sys/sys/stat.h`) in `sys_umask`, and applies it as
/// `(mode & ~mask) & ALLPERMS` on create paths. The open path additionally strips the sticky bit
/// (`sys/kern/vfs_syscalls.c` `sys_open`), which [`open_create_mode`] mirrors.
pub mod umask {
    /// FreeBSD `CMASK` (`sys/sys/param.h`): the default mask of a fresh process.
    pub const CMASK: u32 = 0o022;
    /// FreeBSD `ALLPERMS` (`sys/sys/stat.h`): the bits `sys_umask` keeps from a new mask.
    pub const ALLPERMS: u32 = 0o7777;
    /// FreeBSD `ACCESSPERMS`: the owner/group/other rwx bits.
    pub const ACCESSPERMS: u32 = 0o777;
    /// `S_ISTXT`: the sticky bit, stripped from an open-created file.
    pub const S_ISTXT: u32 = 0o1000;

    /// The value `sys_umask` stores: the new mask limited to [`ALLPERMS`].
    #[must_use]
    pub const fn normalize(mask: u32) -> u32 {
        mask & ALLPERMS
    }

    /// The mode of a file created by `open(..., O_CREAT, mode)`.
    ///
    /// FreeBSD `sys_open` computes `((mode & ~pd_cmask) & ALLPERMS) & ~S_ISTXT`; the sticky bit is
    /// never inherited from the requested mode.
    #[must_use]
    pub const fn open_create_mode(mode: u32, mask: u32) -> u32 {
        (mode & !normalize(mask) & ALLPERMS) & !S_ISTXT
    }
}

/// The kernel-authenticated creation context of a scheme request.
///
/// A filesystem server asks the kernel for the context of the request it is serving; the kernel
/// fills the caller's credentials and file-creation mask from its own process state. The record is
/// fixed-size and versioned so a consumer can reject an unknown layout instead of misreading it.
/// The kernel masks [`Context::mode`] before writing it; a server must use it verbatim.
pub mod creation {
    /// Record version.
    pub const VERSION: u32 = 1;
    /// Maximum supplementary groups carried by the record.
    pub const MAX_GROUPS: usize = 16;
    /// Encoded record size in bytes.
    pub const SIZE: usize = 120;

    /// One authenticated creation context.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct Context {
        /// [`VERSION`].
        pub version: u32,
        /// [`SIZE`]; a consumer rejects a mismatch.
        pub size: u32,
        /// Kernel request id the context is bound to.
        pub request_id: u64,
        /// Client process id.
        pub pid: u64,
        /// Client real user id.
        pub uid: u32,
        /// Client effective user id.
        pub euid: u32,
        /// Client real group id.
        pub gid: u32,
        /// Client effective group id.
        pub egid: u32,
        /// Client file-creation mask (`p_pd->pd_cmask` equivalent).
        pub umask: u32,
        /// Requested creation mode after the mask and `~S_ISTXT` were applied.
        pub mode: u32,
        /// Number of valid entries in [`Context::groups`].
        pub group_count: u32,
        /// Reserved; zero.
        pub reserved: u32,
        /// Client supplementary groups.
        pub groups: [u32; MAX_GROUPS],
    }

    const _: () = assert!(core::mem::size_of::<Context>() == SIZE);
    const _: () = assert!(core::mem::align_of::<Context>() == 8);
    const _: () = assert!(core::mem::offset_of!(Context, version) == 0);
    const _: () = assert!(core::mem::offset_of!(Context, size) == 4);
    const _: () = assert!(core::mem::offset_of!(Context, request_id) == 8);
    const _: () = assert!(core::mem::offset_of!(Context, pid) == 16);
    const _: () = assert!(core::mem::offset_of!(Context, uid) == 24);
    const _: () = assert!(core::mem::offset_of!(Context, euid) == 28);
    const _: () = assert!(core::mem::offset_of!(Context, gid) == 32);
    const _: () = assert!(core::mem::offset_of!(Context, egid) == 36);
    const _: () = assert!(core::mem::offset_of!(Context, umask) == 40);
    const _: () = assert!(core::mem::offset_of!(Context, mode) == 44);
    const _: () = assert!(core::mem::offset_of!(Context, group_count) == 48);
    const _: () = assert!(core::mem::offset_of!(Context, groups) == 56);
}

const _: () = assert!(core::mem::size_of::<Timespec>() == 16);
const _: () = assert!(core::mem::size_of::<Handle>() == 4);

#[cfg(test)]
#[path = "types_ut.rs"]
mod tests;
