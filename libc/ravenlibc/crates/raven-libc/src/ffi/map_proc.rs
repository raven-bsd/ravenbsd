// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The map tables of the process family (C64 4.6): the C argument/status words to C53's request
//! types, the `Struct tms`/`rusage` conversions, the single-policy scheduler table, the CPU
//! affinity fill and the two opaque spawn objects. Host tests drive every function with C53's
//! `FakeProcd`; the exports are pointer checks and one call into this module.
//!
//! `posix_spawnattr_t` is a 336-byte blob and is laid out inline; `posix_spawn_file_actions_t`
//! is an 80-byte blob that holds one owned pointer plus a magic word (the crate's
//! `FileActions` owns a `Vec` and cannot live in 80 bytes).

use core::ffi::c_int;

use raven_libc_abi::errno::{EINVAL, Errno};
use raven_libc_abi::structs::{PosixSpawnFileActionsT, PosixSpawnattrT, Rlimit, Rusage, SiginfoT};
use raven_libc_proc::backend::{ProcBackend, WaitResult};
use raven_libc_proc::consts::{P_ALL, SCHED_BATCH, SCHED_FIFO, SCHED_IDLE, SCHED_OTHER, SCHED_RR};
use raven_libc_proc::spawn_model::apply::SpawnEnv;
use raven_libc_proc::spawn_model::{FileActions, SpawnAttr, SpawnCtx};
use raven_libc_proc::wire::{ExecRequest, ForkRequest};

/// C53's positive error number.
pub(crate) type ProcErrno = raven_libc_proc::Errno;

/// Convert one process error to the shell's error type.
pub(crate) fn error(value: ProcErrno) -> Errno {
    Errno(value as u16)
}

/// The `EINVAL` value as the shell's error type.
pub(crate) fn invalid() -> Errno {
    Errno(EINVAL as u16)
}

/// `fork`/`vfork`/`_Fork` through the backend; the request is the pre-`procd` zero shape.
pub(crate) fn fork<B: ProcBackend>(backend: &B) -> Result<i32, ProcErrno> {
    backend.fork(&ForkRequest {
        entry: 0,
        sp: 0,
        arg: 0,
        fork_flags: 0,
    })
}

/// The `exec` request and its result (the call never returns on success).
pub(crate) fn exec<B: ProcBackend>(
    backend: &B,
    cwd: &[u8],
    path: &[u8],
    argv: &[alloc::vec::Vec<u8>],
    envp: &[alloc::vec::Vec<u8>],
    fd: Option<i32>,
) -> ProcErrno {
    backend.exec(&ExecRequest {
        exec_flags: 0,
        fd_handle: fd
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(u32::MAX),
        path: path.to_vec(),
        cwd: cwd.to_vec(),
        argv: argv.to_vec(),
        envp: envp.to_vec(),
        close_list: alloc::vec::Vec::new(),
        fdmap: alloc::vec::Vec::new(),
        sigmask: [0; 128],
        ignored: [0; 128],
    })
}

/// `posix_spawn`/`posix_spawnp` through C53's driver.
#[allow(clippy::too_many_arguments)] // mirrors the C entry point
pub(crate) fn spawn<B: ProcBackend, E: SpawnEnv>(
    backend: &B,
    env: &mut E,
    ctx: &SpawnCtx<'_>,
    path: &[u8],
    use_path: bool,
    path_env: Option<&[u8]>,
    actions: Option<&FileActions>,
    attr: Option<&SpawnAttr>,
    argv: &[alloc::vec::Vec<u8>],
    envp: &[alloc::vec::Vec<u8>],
) -> Result<i32, ProcErrno> {
    raven_libc_proc::spawn_model::spawn_model_pid(
        backend, env, ctx, path, use_path, path_env, actions, attr, argv, envp,
    )
}

/// The `wait`/`waitpid` common body: status and zeroed usage records.
pub(crate) fn wait_status<B: ProcBackend>(
    backend: &B,
    idtype: u32,
    id: i64,
    options: c_int,
    status: *mut c_int,
    rusage: *mut Rusage,
) -> Result<i32, ProcErrno> {
    let result = backend.wait(idtype, id, options as u32)?;
    if !status.is_null() {
        // SAFETY: the caller's optional `int` status slot.
        unsafe { status.write(result.status) };
    }
    if !rusage.is_null() {
        // SAFETY: the caller's optional `struct rusage`; accounting is zero until O-8.
        unsafe { rusage.write(Rusage::ZERO) };
    }
    Ok(result.pid)
}

/// `wait` (any child).
pub(crate) fn wait<B: ProcBackend>(
    backend: &B,
    status: *mut c_int,
    rusage: *mut Rusage,
) -> Result<i32, ProcErrno> {
    wait_status(backend, P_ALL, 0, 0, status, rusage)
}

/// `waitid`: fill the caller's `siginfo_t` or zero it for a `WNOHANG` miss.
pub(crate) fn waitid<B: ProcBackend>(
    backend: &B,
    idtype: u32,
    id: i64,
    info: *mut SiginfoT,
    options: c_int,
) -> Result<(), ProcErrno> {
    let result = backend.wait(idtype, id, options as u32)?;
    if info.is_null() {
        return Ok(());
    }
    if result.pid == 0 {
        // SAFETY: the caller's writable `siginfo_t`.
        unsafe { info.write(SiginfoT::ZERO) };
        return Ok(());
    }
    // SAFETY: the backend returned the opaque 128-byte `siginfo_t`; both are 128 bytes.
    unsafe { core::ptr::copy_nonoverlapping(result.siginfo.as_ptr(), info.cast::<u8>(), 128) };
    Ok(())
}

/// Read or write one resource limit through the backend.
pub(crate) fn rlimit<B: ProcBackend>(
    backend: &B,
    pid: c_int,
    resource: c_int,
    new: *const Rlimit,
) -> Result<(u64, u64), ProcErrno> {
    let new = if new.is_null() {
        None
    } else {
        // SAFETY: the caller supplied a readable `struct rlimit`.
        let value = unsafe { &*new };
        Some((value.rlim_cur, value.rlim_max))
    };
    let resource = u32::try_from(resource).map_err(|_| EINVAL)?;
    backend.rlimit(pid, resource, new)
}

/// The `C` view of one limit pair.
pub(crate) fn limit(pair: (u64, u64)) -> Rlimit {
    Rlimit {
        rlim_cur: pair.0,
        rlim_max: pair.1,
    }
}

/// Whether `pid` exists, through the backend's process-group probe.
pub(crate) fn pid_exists<B: ProcBackend>(backend: &B, pid: c_int) -> Result<(), ProcErrno> {
    backend.getpgid(pid).map(|_| ())
}

/// The single-policy priority range: `(max, min)`.
pub(crate) fn sched_bounds(policy: c_int) -> Result<(c_int, c_int), ProcErrno> {
    match policy {
        SCHED_OTHER => Ok((0, 0)),
        SCHED_BATCH => Ok((0, 0)),
        SCHED_IDLE => Ok((0, 0)),
        SCHED_FIFO => Ok((99, 1)),
        SCHED_RR => Ok((99, 1)),
        _ => Err(EINVAL),
    }
}

/// Report every CPU of the single-CPU system: CPU 0.
pub(crate) fn fill_affinity(set: &mut [u8]) -> Result<(), ProcErrno> {
    if set.len() < core::mem::size_of::<u64>() {
        return Err(EINVAL);
    }
    set.fill(0);
    if let Some(first) = set.first_mut() {
        *first = 1;
    }
    Ok(())
}

/// `getgroups`: a zero size is the count query, a NULL list with a positive size is `EFAULT`.
pub(crate) fn getgroups<B: ProcBackend>(
    backend: &B,
    size: c_int,
    list: *mut u32,
) -> Result<c_int, ProcErrno> {
    if size == 0 {
        return backend.getgroups(&mut []).map(|count| count as c_int);
    }
    if size < 0 {
        return Err(EINVAL);
    }
    if list.is_null() {
        return Err(raven_libc_proc::errors::EFAULT);
    }
    let count = usize::try_from(size).map_err(|_| EINVAL)?;
    // SAFETY: the caller's `size` writable `gid_t` slots.
    let slots = unsafe { core::slice::from_raw_parts_mut(list, count) };
    backend.getgroups(slots).map(|filled| filled as c_int)
}

/// `setgroups`: build the `SETCRED` request and send it.
pub(crate) fn setgroups<B: ProcBackend>(
    backend: &B,
    size: c_int,
    list: *const u32,
) -> Result<(), ProcErrno> {
    if size < 0 {
        return Err(EINVAL);
    }
    let count = usize::try_from(size).map_err(|_| EINVAL)?;
    let groups = if count == 0 {
        &[][..]
    } else {
        if list.is_null() {
            return Err(raven_libc_proc::errors::EFAULT);
        }
        // SAFETY: the caller's `size` readable `gid_t` slots.
        unsafe { core::slice::from_raw_parts(list, count) }
    };
    let request = raven_libc_proc::ids::SetCred::setgroups(groups)?;
    backend.setcred(&request)
}

/// The `nice` priority operations over one backend.
struct NiceOps<'a, B: ProcBackend>(&'a B);

impl<B: ProcBackend> raven_libc_proc::nice::PrioOps for NiceOps<'_, B> {
    fn getpriority(&mut self, which: u32, who: i32) -> Result<i32, ProcErrno> {
        self.0.prio(which, who, None)
    }

    fn setpriority(&mut self, which: u32, who: i32, prio: i32) -> Result<(), ProcErrno> {
        self.0.prio(which, who, Some(prio)).map(|_| ())
    }
}

/// `nice`: the C53 increment model over the backend.
pub(crate) fn nice<B: ProcBackend>(backend: &B, increment: c_int) -> Result<c_int, ProcErrno> {
    raven_libc_proc::nice::nice(&mut NiceOps(backend), increment)
}

/// `getlogin`: the backend's login name.
pub(crate) fn login<B: ProcBackend>(backend: &B) -> Result<alloc::vec::Vec<u8>, ProcErrno> {
    backend.login(None)
}

/// The one `struct tms` shape (four `clock_t`, i.e. four `i64`).
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Tms {
    /// User time.
    pub tms_utime: i64,
    /// System time.
    pub tms_stime: i64,
    /// User time of children.
    pub tms_cutime: i64,
    /// System time of children.
    pub tms_cstime: i64,
}

/// Fill `tms` and return the monotonic tick count; accounting is zero until O-8.
pub(crate) fn times(
    tms: *mut Tms,
    monotonic_sec: i64,
    monotonic_usec: i64,
    clk_tck: u32,
) -> Result<i64, ProcErrno> {
    let zero = raven_libc_proc::times::Timeval { sec: 0, usec: 0 };
    let monotonic = raven_libc_proc::times::Timeval {
        sec: monotonic_sec,
        usec: monotonic_usec,
    };
    let mut record = raven_libc_proc::times::Tms::default();
    let ticks = raven_libc_proc::times::times(
        Some(&mut record),
        zero,
        zero,
        zero,
        zero,
        monotonic,
        clk_tck,
    )?;
    if !tms.is_null() {
        // SAFETY: the caller's optional `struct tms`.
        unsafe {
            tms.write(Tms {
                tms_utime: record.tms_utime,
                tms_stime: record.tms_stime,
                tms_cutime: record.tms_cutime,
                tms_cstime: record.tms_cstime,
            })
        };
    }
    Ok(ticks)
}

/// The `posix_spawnattr_t` view: 336 bytes, inline.
#[repr(C)]
pub(crate) struct SpawnAttrView {
    /// `POSIX_SPAWN_*` flags.
    pub flags: i16,
    /// `__pad`.
    pub pad: i16,
    /// Process group.
    pub pgroup: i32,
    /// Reset-to-default signal set.
    pub sigdefault: [u8; 128],
    /// Child signal mask.
    pub sigmask: [u8; 128],
    /// Scheduler policy.
    pub sched_policy: i32,
    /// Scheduler priority.
    pub sched_priority: i32,
    /// Reserved bytes up to the ABI size.
    pub reserved: [u8; 64],
}

/// The crate model of one attribute blob.
pub(crate) fn attr_model(view: &SpawnAttrView) -> SpawnAttr {
    let mut attr = SpawnAttr::new();
    attr.flags = view.flags;
    attr.pgroup = view.pgroup;
    attr.sigdefault = view.sigdefault;
    attr.sigmask = view.sigmask;
    attr.sched_policy = view.sched_policy;
    attr.sched_priority = view.sched_priority;
    attr
}

/// Write a crate model back into the blob view.
pub(crate) fn attr_store(view: &mut SpawnAttrView, attr: &SpawnAttr) {
    view.flags = attr.flags;
    view.pgroup = attr.pgroup;
    view.sigdefault = attr.sigdefault;
    view.sigmask = attr.sigmask;
    view.sched_policy = attr.sched_policy;
    view.sched_priority = attr.sched_priority;
}

/// The magic word of an initialised file-action blob.
pub(crate) const FA_MAGIC: u64 = 0x5241_5645_4e46_4131;

/// The `posix_spawn_file_actions_t` view: one owned pointer plus the magic.
#[repr(C)]
pub(crate) struct FileActionsView {
    /// The boxed crate list (`NULL` until the first action).
    pub actions: *mut FileActions,
    /// [`FA_MAGIC`] when initialised.
    pub magic: u64,
    /// Reserved bytes up to the ABI size.
    pub reserved: [u64; 8],
}

impl FileActionsView {
    /// Initialise an all-zero blob.
    pub(crate) fn init(&mut self) {
        self.actions = core::ptr::null_mut();
        self.magic = FA_MAGIC;
        self.reserved = [0; 8];
    }

    /// Free the owned list, if any.
    pub(crate) fn destroy(&mut self) {
        if !self.actions.is_null() {
            // SAFETY: the pointer came from `Box::into_raw` in `load`.
            unsafe { drop(alloc::boxed::Box::from_raw(self.actions)) };
        }
        self.actions = core::ptr::null_mut();
        self.magic = 0;
        self.reserved = [0; 8];
    }

    /// The list, allocating it on the first action.
    pub(crate) fn load(&mut self) -> Result<&mut FileActions, ProcErrno> {
        if self.magic != FA_MAGIC {
            return Err(EINVAL);
        }
        if self.actions.is_null() {
            self.actions = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(FileActions::new()));
        }
        // SAFETY: the pointer is either the freshly boxed list or one this view owns.
        Ok(unsafe { &mut *self.actions })
    }

    /// The list for a spawn call; an uninitialised blob has no actions.
    pub(crate) fn get(&self) -> Option<&FileActions> {
        if self.magic != FA_MAGIC || self.actions.is_null() {
            return None;
        }
        // SAFETY: the pointer is owned by this view and lives at least as long as it.
        Some(unsafe { &*self.actions })
    }
}

/// The attribute blob as its view.
///
/// # Safety
/// `pointer` is a writable `posix_spawnattr_t`.
pub(crate) unsafe fn attr_view<'a>(pointer: *mut PosixSpawnattrT) -> &'a mut SpawnAttrView {
    // SAFETY: C36 asserts the blob is 336 bytes, align 8; the view is the same size.
    unsafe { &mut *pointer.cast::<SpawnAttrView>() }
}

/// The attribute blob as a read-only view.
///
/// # Safety
/// `pointer` is a readable `posix_spawnattr_t`.
pub(crate) unsafe fn attr_ref<'a>(pointer: *const PosixSpawnattrT) -> &'a SpawnAttrView {
    // SAFETY: C36 asserts the blob is 336 bytes, align 8; the view is the same size.
    unsafe { &*pointer.cast::<SpawnAttrView>() }
}

/// The file-action blob as a read-only view.
///
/// # Safety
/// `pointer` is a readable `posix_spawn_file_actions_t`.
pub(crate) unsafe fn actions_ref<'a>(
    pointer: *const PosixSpawnFileActionsT,
) -> &'a FileActionsView {
    // SAFETY: C36 asserts the blob is 80 bytes, align 8; the view is the same size.
    unsafe { &*pointer.cast::<FileActionsView>() }
}

/// The file-action blob as its view.
///
/// # Safety
/// `pointer` is a writable `posix_spawn_file_actions_t`.
pub(crate) unsafe fn actions_view<'a>(
    pointer: *mut PosixSpawnFileActionsT,
) -> &'a mut FileActionsView {
    // SAFETY: C36 asserts the blob is 80 bytes, align 8; the view is the same size.
    unsafe { &mut *pointer.cast::<FileActionsView>() }
}

/// One completed wait, for the host tests.
pub(crate) fn wait_result(pid: i32, status: i32) -> WaitResult {
    WaitResult {
        pid,
        status,
        siginfo: [0; 128],
        rusage: [0; 144],
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "map_proc_ut.rs"]
mod tests;
