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

const _: () = assert!(core::mem::size_of::<Timespec>() == 16);
const _: () = assert!(core::mem::size_of::<Handle>() == 4);

#[cfg(test)]
#[path = "types_ut.rs"]
mod tests;
