// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one `Platform` instance of the process and the shell's lock.
//!
//! `raven-libc-platform` is a library: the shell publishes the process-wide object (C55 module
//! documentation). The instance is initialised once on first use; until C57 lands, every libc
//! call is single-threaded (the kernel starts one thread per process), so the `RefCell` borrows
//! inside `Platform` cannot overlap. `ShellLock` is already the real seam for the fd table, so
//! the only assumption left is "one thread until C57".
//!
//! The inherited fdmap and the standard descriptors are installed here on the **first** platform
//! use. The start path only records the spawner's fdmap; building the fd table (and its
//! allocator) is lazy, which keeps the static start graph at the C69 boot budget.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use raven_libc_platform::fd::inherit::{ENTRY_LEN, FdMap, HEADER_LEN};
use raven_libc_platform::{Lock, OfdKind, Platform, RealSys, SysBackend};

/// The shell's process lock: one spin word.
#[derive(Debug, Default)]
pub(crate) struct ShellLock {
    held: AtomicBool,
}

impl Lock for ShellLock {
    fn acquire(&self) {
        while self.held.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.held.store(false, Ordering::Release);
    }
}

const UNINIT: u8 = 0;
const READY: u8 = 2;

/// The spawner's inherited fdmap, recorded by the start and consumed on the first use.
static STARTUP_FDMAP: AtomicUsize = AtomicUsize::new(0);
/// The number of entries in [`STARTUP_FDMAP`].
static STARTUP_FDMAP_N: AtomicUsize = AtomicUsize::new(0);
/// The spawner's `AT_RAVEN_CWD` pointer, recorded by the start.
static STARTUP_CWD: AtomicUsize = AtomicUsize::new(0);
/// Set once the start has recorded the fdmap.
static STARTUP_RECORDED: AtomicBool = AtomicBool::new(false);
/// Set before the startup runs, so a recursive `global` cannot apply it twice.
static STARTUP_APPLIED: AtomicBool = AtomicBool::new(false);
/// Set before the start cwd is applied, so a recursive `global` cannot apply it twice.
static STARTUP_CWD_APPLIED: AtomicBool = AtomicBool::new(false);
/// Set when `execve` supplied the `__RAVEN_FDMAP` entry: the exec caller's fd table is
/// authoritative, including deliberately closed descriptors 0/1/2. A spawned service with no
/// fdmap still adopts the console.
static EXEC_INHERITED: AtomicBool = AtomicBool::new(false);

/// Longest accepted child cwd (including the trailing NUL).
const MAX_CWD: usize = 4096;

/// Record the inherited fdmap for the lazy platform startup.
pub(crate) fn set_startup_fdmap(pointer: usize, count: usize) {
    STARTUP_FDMAP_N.store(count, Ordering::Release);
    STARTUP_FDMAP.store(pointer, Ordering::Release);
    STARTUP_RECORDED.store(true, Ordering::Release);
}

/// Record the loader's `AT_RAVEN_CWD` for the platform startup.
///
/// The string lives on the initial stack for the whole process. If the platform was already
/// initialised by an early `global` call, apply the cwd immediately.
pub(crate) fn set_startup_cwd(pointer: usize) {
    STARTUP_CWD.store(pointer, Ordering::Release);
    if pointer != 0 && STATE.load(Ordering::Acquire) == READY {
        // SAFETY: READY was published with Release after the one-shot write, and the value is
        // never moved again (`static` storage).
        let platform = unsafe { (&*PLATFORM.0.get()).assume_init_ref() };
        apply_startup_cwd(platform);
    }
}

struct Global(UnsafeCell<MaybeUninit<Platform<RealSys, ShellLock>>>);

// SAFETY: the object is written exactly once before `READY` is published with Release, and a
// reader only obtains `&Platform` after an Acquire load of `READY`. The interior `RefCell`s are
// used by one thread at a time until C57 supplies the thread support (module documentation).
unsafe impl Sync for Global {}

static STATE: AtomicU8 = AtomicU8::new(UNINIT);
static PLATFORM: Global = Global(UnsafeCell::new(MaybeUninit::uninit()));

/// The process-wide platform.
pub(crate) fn global() -> &'static Platform<RealSys, ShellLock> {
    if STATE
        .compare_exchange(UNINIT, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        // SAFETY: this caller won the one-shot initialisation and owns the slot until READY.
        unsafe {
            let slot = &mut *PLATFORM.0.get();
            slot.write(Platform::with_lock(RealSys, ShellLock::default()));
        }
        STATE.store(READY, Ordering::Release);
    }
    while STATE.load(Ordering::Acquire) != READY {
        core::hint::spin_loop();
    }
    // SAFETY: READY was published with Release after the write, and the value is never moved
    // again (`static` storage).
    let platform = unsafe { (&*PLATFORM.0.get()).assume_init_ref() };
    apply_startup_cwd(platform);
    // The start records the fdmap at step 9b, possibly after an early `global` from an IFUNC
    // resolver. Apply the startup on the first call after the record; `APPLIED` is set before
    // the work so a recursive `global` cannot run it twice.
    if STARTUP_RECORDED.load(Ordering::Acquire) && !STARTUP_APPLIED.swap(true, Ordering::AcqRel) {
        apply_startup(platform);
    }
    platform
}

/// Apply the loader-provided `AT_RAVEN_CWD`, once.
fn apply_startup_cwd(platform: &Platform<RealSys, ShellLock>) {
    let pointer = STARTUP_CWD.load(Ordering::Acquire);
    if pointer == 0 || STARTUP_CWD_APPLIED.swap(true, Ordering::AcqRel) {
        return;
    }
    let bytes = pointer as *const u8;
    for length in 0..MAX_CWD {
        // SAFETY: the loader wrote a NUL-terminated cwd string on the initial stack; the scan
        // stays below MAX_CWD and stops at the first NUL.
        if unsafe { *bytes.add(length) } == 0 {
            // SAFETY: the bytes up to `length` were readable from the same stack string, which
            // lives for the whole process.
            let cwd = unsafe { core::slice::from_raw_parts(bytes, length) };
            platform.init_cwd(Some(cwd));
            return;
        }
    }
}

/// Install the spawner's descriptors and adopt standard descriptors on the first use.
fn apply_startup(platform: &Platform<RealSys, ShellLock>) {
    restore_inherited_fds(platform);
    if !EXEC_INHERITED.load(Ordering::Acquire) {
        ensure_standard_fds(platform);
    }
}

/// Install the `RFDM` block the spawner mapped into this process.
fn restore_inherited_fds(platform: &Platform<RealSys, ShellLock>) {
    let pointer = STARTUP_FDMAP.load(Ordering::Acquire);
    let count = STARTUP_FDMAP_N.load(Ordering::Acquire);
    if pointer != 0 && count != 0 && count <= 1024 {
        let Some(len) = ENTRY_LEN
            .checked_mul(count)
            .and_then(|entries| entries.checked_add(HEADER_LEN))
        else {
            return;
        };
        // SAFETY: the start recorded the spawner-mapped header+entry block; it is mapped for the
        // lifetime of the process and the bounds are the decoded count.
        let bytes = unsafe { core::slice::from_raw_parts(pointer as *const u8, len) };
        if let Ok(block) = FdMap::decode(bytes) {
            let _ = platform.restore_fdmap(&block);
        }
        return;
    }
    restore_exec_fdmap(platform);
}

/// Restore the fd table that `execve` serialized into [`ENV_NAME`].
///
/// The kernel keeps the handles across `exec`; the environment entry carries the identity
/// `fd -> handle` mapping. `FD_CLOEXEC` entries are restored with their flag and then closed so
/// the new image never observes them.
fn restore_exec_fdmap(platform: &Platform<RealSys, ShellLock>) {
    #[cfg(not(target_os = "none"))]
    {
        let _ = platform;
    }
    #[cfg(target_os = "none")]
    {
        let name = raven_libc_platform::fd::inherit::ENV_NAME;
        let Some(value) = crate::stdlib::env_support::get(name) else {
            return;
        };
        EXEC_INHERITED.store(true, Ordering::Release);
        if let Ok(block) = FdMap::decode_env(value)
            && platform.restore_fdmap(&block).is_ok()
        {
            let _ = platform.close_cloexec_fds();
        }
        let _ = crate::stdlib::env_support::unset(name);
    }
}

/// Ensure descriptors 0, 1 and 2 are open, preferring the console scheme.
#[cold]
#[inline(never)]
fn ensure_standard_fds(platform: &Platform<RealSys, ShellLock>) {
    // A spawned child starts with no descriptors; adopt the shell's `tty:console` handle for
    // stdin, stdout and stderr so child output reaches the serial console. A process that
    // already has a descriptor keeps it, and a process whose console is not available falls
    // back to `/dev/null` (K06c). This now runs on the first fd use, not in the start path.
    let wire = raven_libc_platform::xlat::open_flags(raven_libc_abi::fcntl::O_RDWR as u32)
        .map(|spec| spec.wire)
        .unwrap_or(0b11);
    let mut fd = 0i32;
    while fd < 3 {
        if platform
            .fcntl(fd, raven_libc_abi::fcntl::F_GETFD, 0)
            .is_ok()
        {
            fd = fd.wrapping_add(1);
            continue;
        }
        let installed =
            match raven_libc_platform::scheme::open_scheme_path(&RealSys, b"tty:console", wire) {
                Ok(handle) => {
                    match platform.install_at_fd(
                        fd as u32,
                        handle,
                        OfdKind::Tty,
                        raven_libc_abi::fcntl::O_RDWR as u32,
                        None,
                        false,
                        0,
                    ) {
                        Ok(()) => true,
                        Err(_) => {
                            let _ = RealSys.close(handle);
                            false
                        }
                    }
                }
                Err(_) => false,
            };
        if !installed {
            let _ = platform.open(b"/dev/null", raven_libc_abi::fcntl::O_RDWR as u32, 0);
        }
        fd = fd.wrapping_add(1);
    }
}
