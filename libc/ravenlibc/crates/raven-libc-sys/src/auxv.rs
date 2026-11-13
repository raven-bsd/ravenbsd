// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C8 process-stack reader and auxv accessors.
//!
//! The kernel and `raven_rt::spawn` build `argc`, the `argv` pointer array, the `envp` pointer
//! array and the `(key, value)` auxv pairs terminated by `AT_NULL`. This module copies the auxv
//! into a fixed-size table so callers never hold a raw stack pointer. Unknown keys are kept but
//! ignored by the typed getters; the scan is bounded at [`MAX_ENTRIES`].

#![allow(clippy::arithmetic_side_effects)] // bounded stack offsets

/// `AT_NULL`: end of the auxiliary vector.
pub const AT_NULL: usize = 0;
/// `AT_PHDR`: program headers.
pub const AT_PHDR: usize = 3;
/// `AT_PHENT`: program-header entry size.
pub const AT_PHENT: usize = 4;
/// `AT_PHNUM`: number of program headers.
pub const AT_PHNUM: usize = 5;
/// `AT_PAGESZ`: page size.
pub const AT_PAGESZ: usize = 6;
/// `AT_BASE`: interpreter base.
pub const AT_BASE: usize = 7;
/// `AT_ENTRY`: program entry point.
pub const AT_ENTRY: usize = 9;
/// `AT_HWCAP`: CPU feature bits.
pub const AT_HWCAP: usize = 16;
/// `AT_CLKTCK`: clock ticks per second.
pub const AT_CLKTCK: usize = 17;
/// `AT_SECURE`: privileged-exec flag (always 0 in v1, H-7).
pub const AT_SECURE: usize = 23;
/// `AT_RANDOM`: 16 random bytes.
pub const AT_RANDOM: usize = 25;
/// `AT_HWCAP2`: second CPU feature word.
pub const AT_HWCAP2: usize = 26;
/// `AT_EXECFN`: executed file name pointer.
pub const AT_EXECFN: usize = 31;

/// Raven procd connection handle (`PG-50`).
pub const AT_RAVEN_PROCD: usize = 0x7001;
/// Raven procinfo page address (`PG-50`).
pub const AT_RAVEN_PROCINFO: usize = 0x7002;
/// Raven fdmap block address (`PG-50`).
pub const AT_RAVEN_FDMAP: usize = 0x7003;
/// Raven fdmap entry count (`PG-50`).
pub const AT_RAVEN_FDMAP_N: usize = 0x7004;
/// Raven current-directory string address (`PG-50`).
pub const AT_RAVEN_CWD: usize = 0x7005;
/// Raven ABI version (`PG-50`).
pub const AT_RAVEN_ABI: usize = 0x7006;
/// Raven time page address (`PG-50`, `PG-20`).
pub const AT_RAVEN_TIMEPAGE: usize = 0x7007;

/// Maximum number of auxv pairs read from the stack.
pub const MAX_ENTRIES: usize = 512;
/// Maximum accepted `argc`.
pub const MAX_ARGC: usize = 4096;
/// Maximum accepted environment entries.
pub const MAX_ENV: usize = 4096;

/// A parsed auxiliary vector with the first value winning for duplicate keys.
#[derive(Clone)]
pub struct AuxVec {
    keys: [usize; MAX_ENTRIES],
    values: [usize; MAX_ENTRIES],
    len: usize,
}

/// The ABI version in the stack does not match [`raven_abi::ABI_VERSION`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbiMismatch {
    /// Version this libc was built for.
    pub expected: u32,
    /// Version the spawner wrote.
    pub found: usize,
}

impl AuxVec {
    /// Parse the C8 stack at `sp`.
    ///
    /// # Safety
    ///
    /// `sp` must point to the process stack the kernel or `raven_rt::spawn` built, or be null or
    /// misaligned. The parser bounds every read; a malformed stack returns `None`.
    pub unsafe fn from_stack(
        sp: *const usize,
        argc_out: &mut usize,
        argv_out: &mut *const *const usize,
        envp_out: &mut *const *const usize,
    ) -> Option<AuxVec> {
        *argc_out = 0;
        *argv_out = core::ptr::null();
        *envp_out = core::ptr::null();

        if sp.is_null() || !(sp as usize).is_multiple_of(core::mem::align_of::<usize>()) {
            return None;
        }
        // SAFETY: `sp` is non-null and aligned and points at the C8 stack.
        let argc = unsafe { *sp };
        if argc > MAX_ARGC {
            return None;
        }
        // SAFETY: the C8 layout places `argc` argv words after `argc`.
        let argv = unsafe { sp.add(1) } as *const *const usize;
        // SAFETY: the layout guarantees `argv[argc] == NULL`.
        if !unsafe { *argv.add(argc) }.is_null() {
            return None;
        }
        // SAFETY: the environment pointer array follows the argv terminator.
        let envp = unsafe { argv.add(argc + 1) };
        let mut env_count = 0usize;
        while env_count < MAX_ENV {
            // SAFETY: the environment array is NUL terminated; the scan stops at that
            // terminator or at the MAX_ENV bound.
            if unsafe { *envp.add(env_count) }.is_null() {
                break;
            }
            env_count += 1;
        }
        if env_count == MAX_ENV {
            return None;
        }
        // SAFETY: the auxv array follows the environment terminator.
        let auxv = unsafe { envp.add(env_count + 1) as *const usize };

        let mut vector = AuxVec {
            keys: [0; MAX_ENTRIES],
            values: [0; MAX_ENTRIES],
            len: 0,
        };
        let mut index = 0usize;
        while index < MAX_ENTRIES {
            // SAFETY: each pair is two readable words inside the bounded auxv array.
            let key = unsafe { *auxv.add(index * 2) };
            // SAFETY: see above.
            let value = unsafe { *auxv.add(index * 2 + 1) };
            if key == AT_NULL {
                break;
            }
            let slot = vector.keys.get_mut(index)?;
            *slot = key;
            let slot = vector.values.get_mut(index)?;
            *slot = value;
            vector.len += 1;
            index += 1;
        }

        *argc_out = argc;
        *argv_out = argv;
        *envp_out = envp;
        Some(vector)
    }

    /// Number of pairs stored (the terminator is not stored).
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the vector has no pairs.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// First value for `key`, or `None`.
    pub fn get(&self, key: usize) -> Option<usize> {
        let mut index = 0usize;
        while index < self.len {
            if self.keys.get(index).copied() == Some(key) {
                return self.values.get(index).copied();
            }
            index += 1;
        }
        None
    }

    /// `AT_PAGESZ`.
    pub fn pagesz(&self) -> Option<usize> {
        self.get(AT_PAGESZ)
    }

    /// `AT_PHDR`.
    pub fn phdr(&self) -> Option<usize> {
        self.get(AT_PHDR)
    }

    /// `AT_PHENT`.
    pub fn phent(&self) -> Option<usize> {
        self.get(AT_PHENT)
    }

    /// `AT_PHNUM`.
    pub fn phnum(&self) -> Option<usize> {
        self.get(AT_PHNUM)
    }

    /// `AT_BASE`.
    pub fn base(&self) -> Option<usize> {
        self.get(AT_BASE)
    }

    /// `AT_ENTRY`.
    pub fn entry(&self) -> Option<usize> {
        self.get(AT_ENTRY)
    }

    /// `AT_RANDOM`.
    pub fn random(&self) -> Option<usize> {
        self.get(AT_RANDOM)
    }

    /// `AT_SECURE`; always 0 in ABI v1 (H-7), even when a spawner writes the key.
    pub fn secure(&self) -> usize {
        0
    }

    /// `AT_HWCAP`.
    pub fn hwcap(&self) -> Option<usize> {
        self.get(AT_HWCAP)
    }

    /// `AT_HWCAP2`.
    pub fn hwcap2(&self) -> Option<usize> {
        self.get(AT_HWCAP2)
    }

    /// `AT_CLKTCK`.
    pub fn clktck(&self) -> Option<usize> {
        self.get(AT_CLKTCK)
    }

    /// `AT_EXECFN`.
    pub fn execfn(&self) -> Option<usize> {
        self.get(AT_EXECFN)
    }

    /// `AT_RAVEN_ABI`.
    pub fn raven_abi(&self) -> Option<usize> {
        self.get(AT_RAVEN_ABI)
    }

    /// `AT_RAVEN_FDMAP`.
    pub fn raven_fdmap(&self) -> Option<usize> {
        self.get(AT_RAVEN_FDMAP)
    }

    /// `AT_RAVEN_PROCINFO`.
    pub fn raven_procinfo(&self) -> Option<usize> {
        self.get(AT_RAVEN_PROCINFO)
    }

    /// `AT_RAVEN_PROCD`.
    pub fn raven_procd(&self) -> Option<usize> {
        self.get(AT_RAVEN_PROCD)
    }

    /// Compare `AT_RAVEN_ABI` with this libc's ABI version.
    ///
    /// An absent key is accepted for compatibility with the five-key spawner of ABI v1; the
    /// caller aborts on `Err` with a fixed diagnostic.
    pub fn check_abi(&self) -> Result<(), AbiMismatch> {
        match self.raven_abi() {
            None => Ok(()),
            Some(found) if found == raven_abi::ABI_VERSION as usize => Ok(()),
            Some(found) => Err(AbiMismatch {
                expected: raven_abi::ABI_VERSION,
                found,
            }),
        }
    }
}

const _: () = assert!(MAX_ENTRIES > 0);

#[cfg(test)]
#[path = "auxv_ut.rs"]
mod tests;
