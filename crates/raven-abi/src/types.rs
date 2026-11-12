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

/// Mapping flags (`map_flags << 8` in the `MAP` flags argument).
pub mod map {
    /// Place the mapping exactly at `addr`; fail if it overlaps.
    pub const FIXED: usize = 1;
    /// Share the pages with other mappings of the memory object (default is private copy-on-write).
    pub const SHARED: usize = 2;
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

const _: () = assert!(core::mem::size_of::<Timespec>() == 16);
const _: () = assert!(core::mem::size_of::<Handle>() == 4);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_packing() {
        let h = Handle::new(0x00AB_CDEF, 0x7F);
        assert_eq!(h.index(), 0x00AB_CDEF);
        assert_eq!(h.generation(), 0x7F);
        assert_eq!(
            Handle::new(0x0100_0000, 1).index(),
            0,
            "index is masked to 24 bits"
        );
    }

    #[test]
    fn rights_only_shrink_on_dup() {
        let r = Rights::READ.union(Rights::WRITE).union(Rights::DUP);
        let d = r.intersect(Rights::READ);
        assert!(d.contains(Rights::READ));
        assert!(!d.contains(Rights::WRITE));
        assert!(!r.intersect(Rights::MAP).contains(Rights::MAP));
    }

    #[test]
    fn map_flag_packing() {
        let f = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
        assert_eq!(f & prot::MASK, 3);
        assert_eq!(f >> map::SHIFT, 3);
    }
}
