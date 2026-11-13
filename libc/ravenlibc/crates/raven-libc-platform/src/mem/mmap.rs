// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mmap` glue over `MAP`, `UNMAP`, `PROTECT` and `MEMOBJ_CREATE`.
//!
//! Anonymous private mappings pass `Handle::NONE`; anonymous shared mappings create a memory
//! object first; file mappings need `OP_MMAP` (PG-36) and return `ENODEV` until then. `MAP_FIXED`
//! uses `map::REPLACE` when PG-13 exists; today it unmaps the target range and maps again, which
//! is not atomic (documented deviation). `mlock*` is an accepted no-op: pages are never pinned.

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, map, prot};
/// Page size from `raven-abi` (4096 on all three architectures).
pub const PAGE_SIZE: usize = raven_abi::PAGE_SIZE;
use raven_libc_abi::mman;

use crate::backend::{PendingCall, SysBackend};
use crate::lock::Lock;
use crate::platform::Platform;

/// `getpagesize`.
pub const fn getpagesize() -> i32 {
    PAGE_SIZE as i32
}

fn validate(len: usize, offset: i64, protection: i32, flags: i32) -> Result<(), Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    if len >= isize::MAX as usize {
        return Err(Errno::ENOMEM);
    }
    if offset < 0 || !(offset as u64).is_multiple_of(PAGE_SIZE as u64) {
        return Err(Errno::EINVAL);
    }
    if protection & !(prot::READ | prot::WRITE | prot::EXEC) as i32 != 0 {
        return Err(Errno::EINVAL);
    }
    if protection & prot::WRITE as i32 != 0 && protection & prot::EXEC as i32 != 0 {
        // W^X: the kernel rejects the combination (A-sys-19).
        return Err(Errno::EACCES);
    }
    if flags & mman::MAP_SHARED != 0 && flags & mman::MAP_PRIVATE != 0 {
        return Err(Errno::EINVAL);
    }
    Ok(())
}

/// `mmap`.
pub fn mmap<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
    protection: i32,
    flags: i32,
    fd: i32,
    offset: i64,
) -> Result<usize, Errno> {
    validate(len, offset, protection, flags)?;
    if flags & mman::MAP_FIXED != 0 && addr != 0 && !addr.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    let anonymous = flags & mman::MAP_ANONYMOUS != 0;
    if !anonymous && fd >= 0 {
        // PG-36: OP_MMAP returns an attached pager-backed MemObject.
        return Err(Errno::ENODEV);
    }
    let prot_bits = protection as u32;
    let map_bits = if flags & mman::MAP_SHARED != 0 {
        map::SHARED as u32
    } else {
        0
    };
    if flags & mman::MAP_FIXED != 0 && addr != 0 {
        // PG-13 adds map::REPLACE; until then unmap-then-map is observable and non-atomic.
        let _ = platform.backend().unmap(addr, len);
    }
    let fixed = if flags & mman::MAP_FIXED != 0 {
        map::FIXED as u32
    } else {
        0
    };
    if anonymous && flags & mman::MAP_SHARED != 0 {
        let object = platform.backend().memobj_create(len, 0)?;
        return platform
            .backend()
            .map(object, 0, addr, len, prot_bits, map_bits | fixed);
    }
    platform
        .backend()
        .map(Handle::NONE, 0, addr, len, prot_bits, map_bits | fixed)
}

/// `munmap`.
pub fn munmap<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
) -> Result<(), Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    platform.backend().unmap(addr, len)
}

/// `mprotect`.
pub fn mprotect<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
    protection: i32,
) -> Result<(), Errno> {
    if len == 0 || !addr.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    if protection & !(prot::READ | prot::WRITE | prot::EXEC) as i32 != 0 {
        return Err(Errno::EINVAL);
    }
    if protection & prot::WRITE as i32 != 0 && protection & prot::EXEC as i32 != 0 {
        return Err(Errno::EACCES);
    }
    platform.backend().protect(addr, len, protection as u32)
}

/// `msync`: `ENOSYS` until PG-22 adds `OP_MSYNC`.
pub fn msync<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
    _flags: i32,
) -> Result<(), Errno> {
    if len == 0 {
        return Err(Errno::EINVAL);
    }
    let _ = addr;
    let _ = platform;
    Err(Errno::ENOSYS)
}

/// `madvise`: documented advice is a success; `DONTNEED`/`FREE` need PG-15.
pub fn madvise<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
    advice: i32,
) -> Result<(), Errno> {
    if len == 0 || !addr.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    match advice {
        mman::MADV_DONTNEED | mman::MADV_FREE => {
            let _ = platform.backend().pending(PendingCall::Madvise)?;
            Ok(())
        }
        mman::MADV_NORMAL
        | mman::MADV_RANDOM
        | mman::MADV_SEQUENTIAL
        | mman::MADV_WILLNEED
        | mman::MADV_DONTFORK
        | mman::MADV_DOFORK
        | mman::MADV_MERGEABLE
        | mman::MADV_UNMERGEABLE
        | mman::MADV_HUGEPAGE
        | mman::MADV_NOHUGEPAGE
        | mman::MADV_DONTDUMP
        | mman::MADV_DODUMP
        | mman::MADV_WIPEONFORK
        | mman::MADV_KEEPONFORK
        | mman::MADV_COLD
        | mman::MADV_PAGEOUT => Ok(()),
        _ => Err(Errno::EINVAL),
    }
}

/// `mlock`: accepted, no-op (documented).
pub fn mlock<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    addr: usize,
    len: usize,
) -> Result<(), Errno> {
    if len == 0 || !addr.is_multiple_of(PAGE_SIZE) {
        return Err(Errno::EINVAL);
    }
    Ok(())
}

/// `munlock`: accepted, no-op.
pub fn munlock<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    addr: usize,
    len: usize,
) -> Result<(), Errno> {
    mlock(platform, addr, len)
}

/// `mlockall`: accepted, no-op.
pub fn mlockall<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _flags: i32,
) -> Result<(), Errno> {
    Ok(())
}

/// `munlockall`: accepted, no-op.
pub fn munlockall<B: SysBackend, L: Lock>(_platform: &Platform<B, L>) -> Result<(), Errno> {
    Ok(())
}

/// `brk`/`sbrk`: class B, `ENOSYS`; malloc uses `MAP`.
pub fn brk<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _addr: usize,
) -> Result<usize, Errno> {
    Err(Errno::ENOSYS)
}

/// `mremap`: only private anonymous growth can be attempted without PG-13; `ENOMEM` otherwise.
pub fn mremap<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _old_address: usize,
    _old_size: usize,
    _new_size: usize,
    _flags: i32,
) -> Result<usize, Errno> {
    Err(Errno::ENOMEM)
}

#[cfg(test)]
#[path = "mmap_ut.rs"]
mod tests;
