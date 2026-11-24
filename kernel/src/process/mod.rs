// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process creation, the first `/init` process, and user-mode fault handling.
//!
//! S08 has one thread per process. The process owns its address space and handle table; the
//! single thread owns the kernel stack used by syscall and trap entry.

pub mod duplex;
pub mod exec;
pub mod handle;
pub mod pipe;
pub mod priority;
pub mod robust;
pub mod status;
pub mod thread;
pub mod tty;

use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::{Handle, prot};
use raven_elf::{Elf, ElfError, LoadMode, Machine, MapStep, StepKind};
use raven_handles::HandleTable;
use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};
use raven_usercopy::UserSlice;
use raven_vma::Backing;
use spin::Mutex;

#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::ipc::{
    self, PendingRequest, SchemeServer, capture::CaptureSlots, model::Namespace,
};
#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::syscall as arch_syscall;
#[cfg(target_arch = "aarch64")]
use crate::arch::aarch64::timer;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::syscall as arch_syscall;
#[cfg(target_arch = "x86_64")]
use crate::arch::x86_64::tsc;
use crate::cmdline::CmdLine;
use crate::initfs;
#[cfg(target_arch = "x86_64")]
use crate::ipc::{self, PendingRequest, SchemeServer, capture::CaptureSlots, model::Namespace};
use crate::kprintln;
use crate::memory::aspace::AddressSpace;
use crate::memory::uspace::UserAddressSpace;
use crate::memory::{frame, paging};
use crate::sched::thread::Thread;
use crate::sched::wait::WaitQueue;
use crate::usercopy::copy_from_user;

/// `THREAD_KICK` bit 0: inspect the pending signal state (PG-7).
const KICK_CHECK: u32 = 1;

use self::handle::Object;
use self::status::{RLIMIT_COUNT, Rlimit, SIGCHLD, default_limits};

/// Page size used by the user loader and stack.
const PAGE: u64 = 4096;
/// Size of the user stack mapping.
///
/// PG-49/PG-50: `raven_rt::spawn` sizes a child stack for `ARG_MAX` (128 KiB) plus the C8 arrays
/// and headroom. The first process gets the same 256 KiB mapping.
const USER_STACK_SIZE: u64 = crate::memory::aslr::STACK_SIZE;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// S36 vfsd map/unmap baseline (`frame::stats().allocated_bytes` at the exit-88 marker).
static S36_MAP_BASE: AtomicU64 = AtomicU64::new(0);
/// S36 lock/time churn baseline (`frame::stats().allocated_bytes` at the exit-90 marker).
static S36_LOCK_BASE: AtomicU64 = AtomicU64::new(0);
/// Process objects currently alive (S36 diagnostic).
static LIVE_PROCESSES: AtomicU64 = AtomicU64::new(0);

/// Count a newly created process object (S36 diagnostic).
fn note_process_created() {
    LIVE_PROCESSES.fetch_add(1, Ordering::Relaxed);
}

/// Number of live `Process` allocations (S36 diagnostic).
pub fn live_processes() -> u64 {
    LIVE_PROCESSES.load(Ordering::Acquire)
}

/// Every process ever created, weakly (S36b leaked-reference diagnostic only).
static ALL_PROCESSES: Mutex<Vec<Weak<Process>>> = Mutex::new(Vec::new());

/// Remember a new process so the diagnostic can inspect leaked objects after the pid registry
/// dropped them (S36b only).
fn note_process_weak(process: &Arc<Process>) {
    ALL_PROCESSES.lock().push(Arc::downgrade(process));
}

/// Live scheme requests held by each process list, including leaked processes (S36b diagnostic).
///
/// Returns `(client_total, active_total, queued_total, top_client_pid, top_client_total,
/// top_queue_pid, top_queued_total)`.
pub fn request_holder_stats() -> [u64; 7] {
    let mut stats = [0u64; 7];
    for weak in ALL_PROCESSES.lock().iter() {
        let Some(process) = weak.upgrade() else {
            continue;
        };
        let client = process.client_requests.lock().len() as u64;
        let active = process.active_requests.lock().len() as u64;
        let queued = process.scheme_queue.lock().len() as u64;
        stats[0] = stats[0].saturating_add(client);
        stats[1] = stats[1].saturating_add(active);
        stats[2] = stats[2].saturating_add(queued);
        if client > stats[4] {
            stats[4] = client;
            stats[3] = process.id;
        }
        if queued > stats[6] {
            stats[6] = queued;
            stats[5] = process.id;
        }
    }
    stats
}

/// Opcode histogram of the largest server request queue (S36b diagnostic).
///
/// Returns `(close, dup, open, other, top_queue_pid)`. It names the request kind that is
/// accumulating on the server that does not drain its queue.
pub fn queue_opcode_stats() -> [u64; 5] {
    let mut top_pid = 0u64;
    let mut top_len = 0usize;
    for weak in ALL_PROCESSES.lock().iter() {
        let Some(process) = weak.upgrade() else {
            continue;
        };
        let len = process.scheme_queue.lock().len();
        if len > top_len {
            top_len = len;
            top_pid = process.id;
        }
    }
    let mut stats = [0u64; 5];
    stats[4] = top_pid;
    for weak in ALL_PROCESSES.lock().iter() {
        let Some(process) = weak.upgrade() else {
            continue;
        };
        if process.id != top_pid {
            continue;
        }
        for request in process.scheme_queue.lock().iter() {
            match request.opcode {
                crate::ipc::protocol::Opcode::Close => stats[0] = stats[0].saturating_add(1),
                crate::ipc::protocol::Opcode::Dup => stats[1] = stats[1].saturating_add(1),
                crate::ipc::protocol::Opcode::Open => stats[2] = stats[2].saturating_add(1),
                _ => stats[3] = stats[3].saturating_add(1),
            }
        }
    }
    stats
}

/// State of the largest server request queue's owner (S36b diagnostic).
pub fn queue_owner_state() -> [u64; 6] {
    let top_pid = queue_opcode_stats()[4];
    for weak in ALL_PROCESSES.lock().iter() {
        let Some(process) = weak.upgrade() else {
            continue;
        };
        if process.id != top_pid {
            continue;
        }
        let state = match *process.state.lock() {
            ProcessState::Created => 0,
            ProcessState::Running => 1,
            ProcessState::Exited => 2,
        };
        return [
            top_pid,
            state,
            process.last_syscall.load(Ordering::Acquire),
            u64::from(process.in_syscall.load(Ordering::Acquire)),
            process.wait_object.load(Ordering::Acquire),
            process.threads.load(Ordering::Acquire) as u64,
        ];
    }
    [0; 6]
}

/// Leaked-process reference histogram (S36b diagnostic).
///
/// Returns `(leaked, external_refs_1, external_refs_2, external_refs_3plus, self_handles,
/// dead_handles_in_all_processes, leaked_threads)`. The `external` count excludes the
/// diagnostic's temporary upgrade, so it is the number of real owners still holding the
/// torn-down process.
pub fn leaked_process_stats() -> [u64; 15] {
    let mut stats = [0u64; 15];
    let processes: alloc::vec::Vec<Arc<Process>> = ALL_PROCESSES
        .lock()
        .iter()
        .filter_map(Weak::upgrade)
        .collect();
    stats[10] = processes.len() as u64;
    for (index, process) in processes.iter().enumerate() {
        if !processes[..index]
            .iter()
            .any(|previous| Arc::ptr_eq(previous, process))
        {
            stats[11] = stats[11].saturating_add(1);
        }
    }
    for process in &processes {
        if !process.torn_down.load(Ordering::Acquire) {
            continue;
        }
        stats[0] = stats[0].saturating_add(1);
        stats[6] = stats[6].saturating_add(u64::from(process.threads.load(Ordering::Acquire)));
        let external = Arc::strong_count(process).saturating_sub(1);
        match external {
            0 => {}
            1 => stats[1] = stats[1].saturating_add(1),
            2 => stats[2] = stats[2].saturating_add(1),
            _ => stats[3] = stats[3].saturating_add(1),
        }
        process.handles.lock().for_each_live(|_, object, _, _| {
            if let crate::process::handle::Object::Process(target) = object {
                if Arc::ptr_eq(target, process) {
                    stats[4] = stats[4].saturating_add(1);
                }
                if target.torn_down.load(Ordering::Acquire) {
                    stats[5] = stats[5].saturating_add(1);
                }
            }
            if let crate::process::handle::Object::Thread(thread) = object
                && thread.process.torn_down.load(Ordering::Acquire)
            {
                stats[9] = stats[9].saturating_add(1);
            }
        });
        for holder in &processes {
            holder.handles.lock().for_each_live(|_, object, _, _| {
                if let crate::process::handle::Object::SchemeFile(file) = object
                    && Arc::ptr_eq(&file.server, process)
                {
                    stats[7] = stats[7].saturating_add(1);
                }
            });
            let mut check = |request: &Arc<crate::ipc::PendingRequest>| {
                if Arc::ptr_eq(&request.server, process)
                    || request
                        .file
                        .as_ref()
                        .is_some_and(|file| Arc::ptr_eq(&file.server, process))
                {
                    stats[8] = stats[8].saturating_add(1);
                }
                if Arc::ptr_eq(&request.client, process) {
                    stats[12] = stats[12].saturating_add(1);
                }
            };
            for request in holder.scheme_queue.lock().iter() {
                check(request);
            }
            for request in holder.active_requests.lock().iter() {
                check(request);
            }
            for request in holder.client_requests.lock().iter() {
                check(request);
            }
            if holder
                .ctty
                .lock()
                .as_ref()
                .is_some_and(|ctty| Arc::ptr_eq(&ctty.file().server, process))
            {
                stats[13] = stats[13].saturating_add(1);
            }
        }
        if ZOMBIES
            .lock()
            .iter()
            .any(|zombie| Arc::ptr_eq(zombie, process))
        {
            stats[14] = stats[14].saturating_add(1);
        }
    }
    stats
}

impl Drop for Process {
    fn drop(&mut self) {
        LIVE_PROCESSES.fetch_sub(1, Ordering::AcqRel);
    }
}

/// S36 timeout-retention baseline: live non-idle Threads at exit 90.
static S36_TIMEOUT_LIVE: AtomicU64 = AtomicU64::new(0);
/// S36 timeout-retention baseline: queued timeout sleepers at exit 90.
static S36_TIMEOUT_SLEEPERS: AtomicU64 = AtomicU64::new(0);
/// S36 timeout-retention gate, parsed once at boot so the exit path does not re-parse the
/// command line on a small kernel stack.
static S36_TIMEOUTCHECK: AtomicBool = AtomicBool::new(false);

/// Record the boot-time S36 gate flags before any process can exit.
pub fn configure_gates(cmdline: &CmdLine) {
    S36_TIMEOUTCHECK.store(cmdline.get("s36-timeoutcheck").is_some(), Ordering::Release);
}

/// Live process ids. The weak references never keep a process alive; teardown removes its entry,
/// and lookup removes an entry whose last strong reference has already gone.
static PID_REGISTRY: Mutex<BTreeMap<u64, Weak<Process>>> = Mutex::new(BTreeMap::new());
/// Exited processes retained until a wait consumes their status.
///
/// The pid registry is weak so a process that is done and has no handles disappears; a zombie
/// must remain visible to `PROC_WAIT_EX` by pid even when the parent closed its process handle
/// (and after its parent exits and it is reparented to init).
static ZOMBIES: Mutex<Vec<Arc<Process>>> = Mutex::new(Vec::new());

/// Process lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    /// `PROC_CREATE` made the process but it has no thread yet.
    Created,
    /// At least one thread has been created and the process has not exited.
    Running,
    /// All threads are gone; waiters have been woken and the address space is released.
    Exited,
}

/// Process identity and credentials shared by the P2a syscall slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIds {
    /// Parent process id.
    pub ppid: u64,
    /// Process group id.
    pub pgid: u64,
    /// Session id.
    pub sid: u64,
    /// Real user id.
    pub uid: u32,
    /// Effective user id.
    pub euid: u32,
    /// Real group id.
    pub gid: u32,
    /// Effective group id.
    pub egid: u32,
    /// Saved user id.
    pub suid: u32,
    /// Saved group id.
    pub sgid: u32,
    /// Supplementary group list, in kernel storage order.
    pub groups: [u32; MAX_GROUPS],
    /// Number of valid entries in `groups`.
    pub group_count: u32,
}

/// Maximum supplementary groups stored by this slice.
pub const MAX_GROUPS: usize = 16;

/// `si_code` of a child status change (`CLD_EXITED`).
pub const CLD_EXITED_CODE: u32 = 1;
/// `si_code` of a plain user signal (`SI_USER`).
pub const SI_USER_CODE: u32 = 0;
/// `si_code` of a queued signal (`SI_QUEUE`; the C value is `-1`).
pub const SI_QUEUE_CODE: u32 = u32::MAX;

/// One queued process signal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingSignal {
    /// Signal number.
    pub signo: u32,
    /// Sender pid.
    pub sender: u64,
    /// `si_code` (`SI_USER`, `SI_QUEUE` or `CLD_EXITED`).
    pub code: u32,
    /// Sender real uid (`si_uid`).
    pub uid: u32,
    /// Queued `sigval` payload (`si_value`).
    pub value: u64,
}

impl ProcessIds {
    /// The kernel process identity.
    pub const fn kernel() -> Self {
        Self {
            ppid: 0,
            pgid: 0,
            sid: 0,
            uid: 0,
            euid: 0,
            gid: 0,
            egid: 0,
            suid: 0,
            sgid: 0,
            groups: [0; MAX_GROUPS],
            group_count: 0,
        }
    }

    /// Identity inherited from a parent process.
    pub const fn inherited(parent: &Self, ppid: u64) -> Self {
        Self {
            ppid,
            pgid: parent.pgid,
            sid: parent.sid,
            uid: parent.uid,
            euid: parent.euid,
            gid: parent.gid,
            egid: parent.egid,
            suid: parent.suid,
            sgid: parent.sgid,
            groups: parent.groups,
            group_count: parent.group_count,
        }
    }
}

/// Resolve a client path against a per-process root.
///
/// Both `root` and `path` are lexical: `.` is dropped, `..` pops but clamps at the root, and
/// the result is absolute and free of a trailing slash. A root of `/` is the unrooted view.
pub(crate) fn resolve_root_path(root: &str, path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    let mut result = String::new();
    if root != "/" {
        result.push_str(root);
    }
    for part in parts {
        result.push('/');
        result.push_str(part);
    }
    if result.is_empty() {
        result.push('/');
    }
    result
}

/// Maximum command-name bytes stored in a process record.
pub const MAX_COMMAND: usize = 256;

/// Truncate a command name to [`MAX_COMMAND`] bytes without splitting UTF-8.
#[must_use]
pub fn truncate_command(text: &str) -> String {
    let mut end = 0usize;
    for (index, character) in text.char_indices() {
        let next = index.saturating_add(character.len_utf8());
        if next > MAX_COMMAND {
            break;
        }
        end = next;
    }
    String::from(&text[..end])
}

/// Pick the command name from `argv[0]`, falling back to the image path.
#[must_use]
pub fn command_from_argv(argv: &[alloc::vec::Vec<u8>], path: &[u8]) -> String {
    let bytes = argv.first().map_or(path, alloc::vec::Vec::as_slice);
    let text = core::str::from_utf8(bytes)
        .ok()
        .or_else(|| core::str::from_utf8(path).ok())
        .unwrap_or("?");
    truncate_command(text)
}

/// A process.
/// Signal numbers and the per-process disposition table (C19).
///
/// The kernel stores the System V amd64 `struct sigaction` layout used by the raw
/// `SIGACTION 0x87` ABI: handler, 128-bit mask, flags and restorer. `SIG_DFL`/`SIG_IGN` are not
/// kernel entry points; a real handler is delivered by the frame path in a later S44 increment.
pub const NSIG: usize = 65;
/// `SIG_DFL`: the default disposition.
pub const SIG_DFL: usize = 0;
/// `SIG_IGN`: ignore the signal.
pub const SIG_IGN: usize = 1;
/// Highest signal number carried by the ABI.
pub const SIG_MAX: u32 = 64;
/// `sigaltstack` flag: no alternate signal stack is installed.
pub const SS_DISABLE: u32 = 2;

/// One per-process signal action, matching the raw `SIGACTION` ABI layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SigAction {
    /// Handler address, `SIG_DFL` or `SIG_IGN`.
    pub handler: usize,
    /// Signals blocked while the handler runs (System V `sigset_t`, 128 bits).
    pub mask: [u64; 2],
    /// `SA_*` flags.
    pub flags: u32,
    /// Reserved restorer field (zero on Raven).
    pub restorer: usize,
}

impl SigAction {
    /// The default action for every signal.
    pub const DEFAULT: Self = Self {
        handler: SIG_DFL,
        mask: [0; 2],
        flags: 0,
        restorer: 0,
    };

    /// The ignore action.
    pub const IGNORE: Self = Self {
        handler: SIG_IGN,
        mask: [0; 2],
        flags: 0,
        restorer: 0,
    };
}

impl Default for SigAction {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Per-process disposition table. `SIGKILL` and `SIGSTOP` cannot be changed.
#[derive(Clone, Copy)]
pub struct SigActs {
    actions: [SigAction; NSIG],
}

impl SigActs {
    /// A table with every signal at `SIG_DFL`.
    pub const fn new() -> Self {
        Self {
            actions: [SigAction::DEFAULT; NSIG],
        }
    }

    /// The current action for `signo`.
    pub fn get(&self, signo: u32) -> Option<SigAction> {
        let index = usize::try_from(signo).ok()?;
        self.actions.get(index).copied()
    }

    /// Install `action` for `signo`; `EINVAL` for a bad signal or the uncatchable pair.
    pub fn install(&mut self, signo: u32, action: SigAction) -> Result<(), Errno> {
        let index = usize::try_from(signo).map_err(|_| Errno::EINVAL)?;
        if !(1..=SIG_MAX).contains(&signo)
            || signo == crate::process::status::SIGKILL
            || signo == crate::process::status::SIGSTOP
        {
            return Err(Errno::EINVAL);
        }
        let slot = self.actions.get_mut(index).ok_or(Errno::EINVAL)?;
        *slot = action;
        Ok(())
    }

    /// Reset every handler to `SIG_DFL` while retaining `SIG_IGN` (`execsigs`).
    pub fn exec_reset(&mut self) {
        for action in &mut self.actions {
            if action.handler != SIG_IGN {
                *action = SigAction::DEFAULT;
            }
        }
    }
}

impl Default for SigActs {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Process {
    /// Process id (PID 1 for the first process).
    pub id: u64,
    /// Parent, group, session and credentials.
    pub ids: Mutex<ProcessIds>,
    /// Path used to start the process.
    pub path: String,
    /// Command name shown to `ps` (argv[0], replaced by exec and copied by fork).
    pub command: Mutex<String>,
    /// POSIX nice value (`-20..19`); inherited by `PROC_FORK` and preserved by exec.
    pub nice: AtomicI32,
    /// Controlling terminal acquired by this process, if any.
    pub ctty: Mutex<Option<Arc<tty::Ctty>>>,
    /// User address space (`None` for the kernel process or an exited process).
    pub address_space: Mutex<Option<UserAddressSpace>>,
    /// Handle table.
    pub handles: Mutex<HandleTable<Object>>,
    /// Exit code once the process exits.
    pub exit_code: AtomicI32,
    /// Live thread count.
    pub threads: AtomicU32,
    /// Live threads created for this process (weak; the scheduler owns the strong reference).
    pub threads_live: Mutex<Vec<Weak<Thread>>>,
    /// True once the process is terminal: none of its threads may run user code again.
    pub exiting: AtomicBool,
    /// True once the one-shot teardown has released the address space and handle table.
    pub torn_down: AtomicBool,
    /// Number of timer objects created by this process and still alive.
    pub timer_count: Arc<AtomicU32>,
    /// Bytes of DMA buffers created by this process and still alive.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21b DMA syscalls are x86-64.
    pub dma_bytes: Arc<AtomicU64>,
    /// Lifecycle state, guarded so `PROC_WAIT` cannot lose an exit wakeup.
    pub state: Mutex<ProcessState>,
    /// Threads blocked in `PROC_WAIT` on this process.
    pub waiters: WaitQueue,
    /// Visible scheme names (per-process namespace).
    pub namespace: Mutex<Namespace>,
    /// Per-process root, normalized absolute and without a trailing slash (`"/"` is unrooted).
    pub root: Mutex<String>,
    /// Scheme names registered by this process.
    pub schemes: Mutex<alloc::vec::Vec<Arc<SchemeServer>>>,
    /// Packets waiting for this process's `RECV`.
    pub scheme_queue: Mutex<VecDeque<Arc<PendingRequest>>>,
    /// Server threads blocked in `RECV`.
    pub scheme_waiters: WaitQueue,
    /// Requests received by this process and not yet replied to.
    pub active_requests: Mutex<alloc::vec::Vec<Arc<PendingRequest>>>,
    /// Requests this process is waiting for as a client.
    pub client_requests: Mutex<alloc::vec::Vec<Arc<PendingRequest>>>,
    /// Capture-window slots owned by this process as a server.
    pub capture_slots: Mutex<CaptureSlots>,
    /// Syscall personality and optional Linux trap handler (S41).
    pub personality: Mutex<PersonalityState>,
    /// Signals queued for this process.
    pub pending_signals: Mutex<VecDeque<PendingSignal>>,
    /// Per-process C19 signal dispositions (handlers are per process; the mask is per thread).
    /// Boxed so the 2.6 KiB `SigActs` table stays off the bounded kernel stack in
    /// `new_empty_process` (the S185 `PROC_CREATE` double fault).
    pub sigacts: Mutex<Box<SigActs>>,
    /// A successful exec's new user context, applied on syscall return.
    pub pending_exec: Mutex<Option<exec::ExecContext>>,
    /// True once an exec has replaced this process image; a child then refuses `setpgid`.
    pub exec_done: AtomicBool,
    /// Threads blocked in `PROC_SIGRECV`.
    pub signal_waiters: WaitQueue,
    /// Parents blocked in `PROC_WAIT_EX` on this process's children.
    pub child_waiters: WaitQueue,
    /// Non-zero stop signal while the process is stopped.
    pub stop_signal: AtomicU32,
    /// True when the current stop was already reported to a waiter.
    pub stop_reported: AtomicBool,
    /// True when a stopped process continued and that event is not reported yet.
    pub continued: AtomicBool,
    /// True after `PROC_WAIT_EX` reaped the exit status.
    pub reaped: AtomicBool,
    /// Soft/hard resource limits.
    pub limits: Mutex<[Rlimit; RLIMIT_COUNT]>,
    /// Process start time in nanoseconds.
    pub start_ns: u64,
    /// Last syscall number entered by a thread of this process (K391 diagnostic).
    pub last_syscall: AtomicU64,
    /// User PC at the last syscall entry of this process (K391 diagnostic).
    pub last_pc: AtomicU64,
    /// Blocking token of the last blocked thread of this process (K391 diagnostic).
    pub wait_object: AtomicU64,
    /// Number of threads of this process currently inside a syscall (K391 diagnostic).
    pub in_syscall: AtomicU32,
    /// Last scheme request id this process submitted as a client (K391 diagnostic).
    pub last_submit_id: AtomicU64,
    /// Last scheme request id this process received as a server (K391 diagnostic).
    pub last_recv_id: AtomicU64,
    /// Last scheme request id this process replied to as a server (K391 diagnostic).
    pub last_send_id: AtomicU64,
    /// Wakeups that removed a waiter without enqueueing the thread (K391 diagnostic).
    pub wake_fail: AtomicU32,
    /// Packed `(reason, cpu, thread_state)` of the last failed wake (K391 diagnostic).
    pub wake_fail_detail: AtomicU64,
    /// Wakeups that found the thread still running and set `pending_wake` (K391 diagnostic).
    pub wake_pending: AtomicU32,
    /// CPU that received the last successful wakeup, or `u32::MAX` once it resumed (K391).
    pub wake_last_cpu: AtomicU32,
    /// Accumulated kernel/syscall time.
    pub system_ns: AtomicU64,
    /// Accumulated user time (not sampled yet; zero).
    pub user_ns: AtomicU64,
    /// Accumulated reaped-child kernel time.
    pub child_system_ns: AtomicU64,
    /// Accumulated reaped-child user time.
    pub child_user_ns: AtomicU64,
    /// Maximum resident user pages observed for this process, in bytes.
    pub maxrss_bytes: AtomicU64,
    /// Maximum resident user pages of any reaped child, in bytes.
    pub child_maxrss_bytes: AtomicU64,
}

impl Process {
    /// Resident user pages for this process, in bytes, from the page tables.
    ///
    /// `UserAddressSpace::mapped_pages` walks the VMA tree and counts present 4 KiB leaves; it
    /// includes the initfs and memory-object mappings, so this is resident user memory, not
    /// private RSS. Zero for the kernel process or after the address space is released.
    pub fn resident_rss_bytes(&self) -> u64 {
        let mut slot = self.address_space.lock();
        slot.as_mut()
            .map_or(0, |space| space.mapped_pages().saturating_mul(PAGE))
    }

    /// Fold the current resident bytes into the process's `maxrss` high-water mark.
    pub fn update_maxrss(&self) {
        let rss = self.resident_rss_bytes();
        self.maxrss_bytes.fetch_max(rss, Ordering::AcqRel);
    }
}

/// Per-process syscall personality and Linux handler handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersonalityState {
    /// Native or Linux dispatch.
    pub personality: raven_abi::types::Personality,
    /// Handler handle in this process's table, when personality is Linux.
    pub handler: Option<Handle>,
}

impl PersonalityState {
    /// Native process with no handler.
    pub const fn native() -> Self {
        Self {
            personality: raven_abi::types::Personality::Native,
            handler: None,
        }
    }
}

/// Why the first process could not be created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessError {
    /// No parsed initfs with the requested init path.
    NoInit,
    /// ELF validation failed.
    Elf(ElfError),
    /// Page mapping failed.
    Map(MapError),
    /// No frame was available.
    OutOfFrames,
    /// Handle insertion failed.
    Handle(Errno),
    /// The argv/envp strings exceed `ARG_MAX` or the stack block.
    ArgTooLong,
    /// A virtual address was not canonical or an arithmetic operation overflowed.
    BadAddress,
    /// The ELF declared an interpreter.
    Interpreter,
    /// Scheduler setup failed.
    Sched(Errno),
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessError::NoInit => f.write_str("no initfs /init image"),
            ProcessError::Elf(error) => write!(f, "invalid init ELF: {error}"),
            ProcessError::Map(error) => write!(f, "user mapping failed: {error}"),
            ProcessError::OutOfFrames => f.write_str("out of frames for the init process"),
            ProcessError::Handle(error) => write!(f, "handle setup failed: {error:?}"),
            ProcessError::ArgTooLong => f.write_str("argument list too long"),
            ProcessError::BadAddress => f.write_str("invalid user address"),
            ProcessError::Interpreter => f.write_str("init ELF has a PT_INTERP"),
            ProcessError::Sched(error) => write!(f, "scheduler setup failed: {error:?}"),
        }
    }
}

/// The current process, if one has been started.
pub fn current() -> Option<Arc<Process>> {
    crate::sched::current_process()
}

/// Register a live process in the pid registry.
pub fn register_pid(process: &Arc<Process>) {
    note_process_weak(process);
    PID_REGISTRY
        .lock()
        .insert(process.id, Arc::downgrade(process));
}

/// Remove a process from the pid registry at teardown.
pub fn unregister_pid(id: u64) {
    let _ = PID_REGISTRY.lock().remove(&id);
}

/// Look up a live process by pid. A stale weak entry is removed and reported as unknown.
pub fn lookup_pid(pid: u64) -> Option<Arc<Process>> {
    let mut registry = PID_REGISTRY.lock();
    let upgraded = registry.get(&pid).and_then(Weak::upgrade);
    if upgraded.is_none() {
        let _ = registry.remove(&pid);
    }
    upgraded
}

/// Snapshot every currently live process.
///
/// The wait/signal scans use this instead of holding the registry lock while
/// taking process locks. Exited zombies stay in the registry until reaped.
pub fn processes() -> alloc::vec::Vec<Arc<Process>> {
    PID_REGISTRY
        .lock()
        .values()
        .filter_map(Weak::upgrade)
        .collect()
}

/// Non-blocking [`processes`] for the metrics-thread census.
pub fn processes_try() -> Option<alloc::vec::Vec<Arc<Process>>> {
    let registry = PID_REGISTRY.try_lock()?;
    Some(registry.values().filter_map(Weak::upgrade).collect())
}

/// Count live processes and their live threads without allocating or taking process locks.
pub fn live_totals() -> (usize, u32) {
    let registry = PID_REGISTRY.lock();
    let mut procs = 0usize;
    let mut threads = 0u32;
    for weak in registry.values() {
        if let Some(process) = weak.upgrade() {
            procs += 1;
            threads = threads.saturating_add(process.threads.load(Ordering::Relaxed));
        }
    }
    (procs, threads)
}

/// Live sshd/sshd-session processes and their thread count (K449 frame-cliff diagnostic).
pub fn sshd_totals() -> (usize, u32) {
    let mut procs = 0usize;
    let mut threads = 0u32;
    for process in processes() {
        let command = process.command.lock().clone();
        if process.path.contains("sshd") || command.contains("sshd") {
            procs = procs.saturating_add(1);
            threads = threads.saturating_add(process.threads.load(Ordering::Relaxed));
        }
    }
    (procs, threads)
}

/// K449 diagnostic: exited processes still retained for `PROC_WAIT`.
pub fn zombie_count() -> usize {
    ZOMBIES.lock().len()
}

/// Live waiters on the process-owned queues (S36 timeout-retention diagnostic).
pub fn wait_queue_len() -> usize {
    let mut total = 0usize;
    for process in processes() {
        total = total.saturating_add(process.waiters.len());
        total = total.saturating_add(process.scheme_waiters.len());
        total = total.saturating_add(process.signal_waiters.len());
        total = total.saturating_add(process.child_waiters.len());
    }
    total
}

/// Dead Threads still reachable from a process's weak live-thread list (S36 diagnostic).
pub fn dead_live_threads() -> usize {
    let mut total = 0usize;
    for process in processes() {
        let threads = process.threads_live.lock();
        for weak in threads.iter() {
            if let Some(thread) = weak.upgrade() {
                if thread.state() == crate::sched::thread::ThreadState::Dead {
                    total = total.saturating_add(1);
                }
            }
        }
    }
    total
}

/// True when at least one live process is in `pgid` inside `sid`.
pub fn group_in_session(pgid: u64, sid: u64) -> bool {
    let registry = PID_REGISTRY.lock();
    registry.values().filter_map(Weak::upgrade).any(|process| {
        let ids = *process.ids.lock();
        ids.pgid == pgid && ids.sid == sid
    })
}

/// Create the kernel process that owns kernel threads.
pub fn new_kernel_process() -> Result<Arc<Process>, Errno> {
    note_process_created();
    Ok(Arc::new(Process {
        id: 0,
        ids: Mutex::new(ProcessIds::kernel()),
        path: String::from("/kernel"),
        command: Mutex::new(String::from("/kernel")),
        nice: AtomicI32::new(0),
        ctty: Mutex::new(None),
        address_space: Mutex::new(None),
        handles: Mutex::new(HandleTable::new()),
        exit_code: AtomicI32::new(0),
        threads: AtomicU32::new(0),
        threads_live: Mutex::new(Vec::new()),
        exiting: AtomicBool::new(false),
        torn_down: AtomicBool::new(false),
        timer_count: Arc::new(AtomicU32::new(0)),
        dma_bytes: Arc::new(AtomicU64::new(0)),
        state: Mutex::new(ProcessState::Running),
        waiters: WaitQueue::new(),
        namespace: Mutex::new(Namespace::root()),
        root: Mutex::new(String::from("/")),
        schemes: Mutex::new(alloc::vec::Vec::new()),
        scheme_queue: Mutex::new(VecDeque::new()),
        scheme_waiters: WaitQueue::new(),
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        capture_slots: Mutex::new(CaptureSlots::new()),
        personality: Mutex::new(PersonalityState::native()),
        pending_signals: Mutex::new(VecDeque::new()),
        sigacts: Mutex::new(Box::new(SigActs::new())),
        pending_exec: Mutex::new(None),
        exec_done: AtomicBool::new(false),
        signal_waiters: WaitQueue::new(),
        child_waiters: WaitQueue::new(),
        stop_signal: AtomicU32::new(0),
        stop_reported: AtomicBool::new(false),
        continued: AtomicBool::new(false),
        reaped: AtomicBool::new(false),
        limits: Mutex::new(default_limits()),
        start_ns: 0,
        last_syscall: AtomicU64::new(0),
        last_pc: AtomicU64::new(0),
        wait_object: AtomicU64::new(0),
        in_syscall: AtomicU32::new(0),
        last_submit_id: AtomicU64::new(0),
        last_recv_id: AtomicU64::new(0),
        last_send_id: AtomicU64::new(0),
        wake_fail: AtomicU32::new(0),
        wake_fail_detail: AtomicU64::new(0),
        wake_pending: AtomicU32::new(0),
        wake_last_cpu: AtomicU32::new(u32::MAX),
        system_ns: AtomicU64::new(0),
        user_ns: AtomicU64::new(0),
        child_system_ns: AtomicU64::new(0),
        child_user_ns: AtomicU64::new(0),
        maxrss_bytes: AtomicU64::new(0),
        child_maxrss_bytes: AtomicU64::new(0),
    }))
}

/// Create an empty process for `PROC_CREATE`, with the well-known handles installed.
pub fn new_empty_process() -> Result<Arc<Process>, Errno> {
    let address_space = AddressSpace::new().map_err(|_| Errno::ENOMEM)?;
    let (namespace, root, ids, limits) = if let Some(parent) = current() {
        let snapshot = parent.namespace.lock().clone();
        let root = parent.root.lock().clone();
        let ids = ProcessIds::inherited(&parent.ids.lock(), parent.id);
        let limits = *parent.limits.lock();
        (
            snapshot.child_snapshot(&ipc::registry::names()),
            root,
            ids,
            limits,
        )
    } else {
        (
            Namespace::root(),
            String::from("/"),
            ProcessIds::kernel(),
            default_limits(),
        )
    };
    note_process_created();
    let process = Arc::new(Process {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        ids: Mutex::new(ids),
        path: String::from("child"),
        command: Mutex::new(String::from("child")),
        nice: AtomicI32::new(0),
        ctty: Mutex::new(None),
        address_space: Mutex::new(Some(UserAddressSpace::new(address_space))),
        handles: Mutex::new(HandleTable::new()),
        exit_code: AtomicI32::new(0),
        threads: AtomicU32::new(0),
        threads_live: Mutex::new(Vec::new()),
        exiting: AtomicBool::new(false),
        torn_down: AtomicBool::new(false),
        timer_count: Arc::new(AtomicU32::new(0)),
        dma_bytes: Arc::new(AtomicU64::new(0)),
        state: Mutex::new(ProcessState::Created),
        waiters: WaitQueue::new(),
        namespace: Mutex::new(namespace),
        root: Mutex::new(root),
        schemes: Mutex::new(alloc::vec::Vec::new()),
        scheme_queue: Mutex::new(VecDeque::new()),
        scheme_waiters: WaitQueue::new(),
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        capture_slots: Mutex::new(CaptureSlots::new()),
        personality: Mutex::new(PersonalityState::native()),
        pending_signals: Mutex::new(VecDeque::new()),
        sigacts: Mutex::new(Box::new(SigActs::new())),
        pending_exec: Mutex::new(None),
        exec_done: AtomicBool::new(false),
        signal_waiters: WaitQueue::new(),
        child_waiters: WaitQueue::new(),
        stop_signal: AtomicU32::new(0),
        stop_reported: AtomicBool::new(false),
        continued: AtomicBool::new(false),
        reaped: AtomicBool::new(false),
        limits: Mutex::new(limits),
        start_ns: crate::time::now_ns(),
        last_syscall: AtomicU64::new(0),
        last_pc: AtomicU64::new(0),
        wait_object: AtomicU64::new(0),
        in_syscall: AtomicU32::new(0),
        last_submit_id: AtomicU64::new(0),
        last_recv_id: AtomicU64::new(0),
        last_send_id: AtomicU64::new(0),
        wake_fail: AtomicU32::new(0),
        wake_fail_detail: AtomicU64::new(0),
        wake_pending: AtomicU32::new(0),
        wake_last_cpu: AtomicU32::new(u32::MAX),
        system_ns: AtomicU64::new(0),
        user_ns: AtomicU64::new(0),
        child_system_ns: AtomicU64::new(0),
        child_user_ns: AtomicU64::new(0),
        maxrss_bytes: AtomicU64::new(0),
        child_maxrss_bytes: AtomicU64::new(0),
    });
    handle::install_initial(&mut process.handles.lock(), &process, false)?;
    register_pid(&process);
    Ok(process)
}

/// Create an unstarted child whose address space is a COW clone of `source`.
///
/// The handle table is forked with `HandleTable::fork_into`, so indices, generations, rights,
/// retired slots and lineage nodes are preserved; the source's self-process handle is rewritten
/// to the child. `NOFORK` mappings are absent and their verifiable object handles are closed in
/// the child. The namespace and personality are copied. No thread is created.
pub fn new_forked_process(source: &Arc<Process>) -> Result<Arc<Process>, Errno> {
    let address_space = AddressSpace::new().map_err(|_| Errno::ENOMEM)?;
    let personality = *source.personality.lock();
    let ids = ProcessIds::inherited(&source.ids.lock(), source.id);
    let limits = *source.limits.lock();
    let root = source.root.lock().clone();
    note_process_created();
    let child = Arc::new(Process {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        ids: Mutex::new(ids),
        path: source.path.clone(),
        command: Mutex::new(source.command.lock().clone()),
        nice: AtomicI32::new(source.nice()),
        ctty: Mutex::new(source.ctty.lock().clone()),
        address_space: Mutex::new(Some(UserAddressSpace::new(address_space))),
        handles: Mutex::new(HandleTable::new()),
        exit_code: AtomicI32::new(0),
        threads: AtomicU32::new(0),
        threads_live: Mutex::new(Vec::new()),
        exiting: AtomicBool::new(false),
        torn_down: AtomicBool::new(false),
        timer_count: Arc::new(AtomicU32::new(0)),
        dma_bytes: Arc::new(AtomicU64::new(0)),
        state: Mutex::new(ProcessState::Created),
        waiters: WaitQueue::new(),
        namespace: Mutex::new(Namespace::root()),
        root: Mutex::new(root),
        schemes: Mutex::new(alloc::vec::Vec::new()),
        scheme_queue: Mutex::new(VecDeque::new()),
        scheme_waiters: WaitQueue::new(),
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        capture_slots: Mutex::new(CaptureSlots::new()),
        personality: Mutex::new(personality),
        pending_signals: Mutex::new(VecDeque::new()),
        sigacts: Mutex::new(Box::new(**source.sigacts.lock())),
        pending_exec: Mutex::new(None),
        exec_done: AtomicBool::new(false),
        signal_waiters: WaitQueue::new(),
        child_waiters: WaitQueue::new(),
        stop_signal: AtomicU32::new(0),
        stop_reported: AtomicBool::new(false),
        continued: AtomicBool::new(false),
        reaped: AtomicBool::new(false),
        limits: Mutex::new(limits),
        start_ns: crate::time::now_ns(),
        last_syscall: AtomicU64::new(0),
        last_pc: AtomicU64::new(0),
        wait_object: AtomicU64::new(0),
        in_syscall: AtomicU32::new(0),
        last_submit_id: AtomicU64::new(0),
        last_recv_id: AtomicU64::new(0),
        last_send_id: AtomicU64::new(0),
        wake_fail: AtomicU32::new(0),
        wake_fail_detail: AtomicU64::new(0),
        wake_pending: AtomicU32::new(0),
        wake_last_cpu: AtomicU32::new(u32::MAX),
        system_ns: AtomicU64::new(0),
        user_ns: AtomicU64::new(0),
        child_system_ns: AtomicU64::new(0),
        child_user_ns: AtomicU64::new(0),
        maxrss_bytes: AtomicU64::new(0),
        child_maxrss_bytes: AtomicU64::new(0),
    });

    {
        let mut source_space = source.address_space.lock();
        let source_space = source_space.as_mut().ok_or(Errno::ESRCH)?;
        let mut child_space = child.address_space.lock();
        let child_space = child_space.as_mut().ok_or(Errno::ESRCH)?;
        child_space.fork_clone_from(source_space)?;
    }

    let (nofork, inherited) = {
        let source_space = source.address_space.lock();
        let source_space = source_space.as_ref().ok_or(Errno::ESRCH)?;
        (
            source_space.nofork_handles(),
            source_space.inherited_handles(),
        )
    };
    {
        let source_table = source.handles.lock();
        let mut child_table = child.handles.lock();
        child_table.fork_into(&source_table, |object| {
            Ok(match object {
                Object::Process(process) if Arc::ptr_eq(process, source) => {
                    Object::Process(Arc::clone(&child))
                }
                other => other.clone(),
            })
        })?;
        for entry in nofork {
            if inherited.contains(&entry.handle) {
                continue;
            }
            let matches = match entry.object {
                Some(id) => matches!(
                    source_table.get(Handle(entry.handle), raven_abi::types::Rights::NONE),
                    Ok(Object::Memory(object)) | Ok(Object::BootBlob(object)) if object.id() == id
                ),
                None => false,
            };
            if matches {
                let _ = child_table.close(Handle(entry.handle));
            }
        }
    }
    *child.namespace.lock() = source.namespace.lock().clone();
    register_pid(&child);
    Ok(child)
}

/// Deliver one signal with POSIX job-control semantics.
pub fn deliver_signal(target: &Arc<Process>, signo: u32, sender: u64) -> Result<(), Errno> {
    deliver_signal_info(target, signo, sender, SI_USER_CODE, 0, 0)
}

/// Deliver one signal with an explicit `siginfo_t` payload.
///
/// The stop/continue bookkeeping is identical to [`deliver_signal`]; only the queued record
/// differs. `PROC_SIGQUEUE` uses `SI_QUEUE` plus the sender uid and the `sigval` payload.
pub fn deliver_signal_info(
    target: &Arc<Process>,
    signo: u32,
    sender: u64,
    code: u32,
    uid: u32,
    value: u64,
) -> Result<(), Errno> {
    if status::is_stop_signal(signo) {
        if target.stop_signal.swap(signo, Ordering::AcqRel) == 0 {
            target.stop_reported.store(false, Ordering::Release);
        }
        notify_parent(target, false);
        return Ok(());
    }
    if signo == status::SIGCONT {
        if target.stop_signal.swap(0, Ordering::AcqRel) != 0 {
            target.continued.store(true, Ordering::Release);
            target.stop_reported.store(false, Ordering::Release);
        }
        target.queue_signal_info(status::SIGCONT, sender, code, uid, value);
        notify_parent(target, false);
        return Ok(());
    }
    target.queue_signal_info(signo, sender, code, uid, value);
    Ok(())
}

/// Deliver one signal to every live process in `pgid`.
pub fn signal_group(pgid: u64, signo: u32, sender: u64) -> usize {
    let mut count = 0usize;
    for process in processes() {
        if *process.state.lock() == ProcessState::Exited {
            continue;
        }
        if process.ids.lock().pgid == pgid {
            let _ = deliver_signal(&process, signo, sender);
            count = count.saturating_add(1);
        }
    }
    count
}

/// True when the process group has no parent in a different group in the same session.
pub fn group_orphaned(pgid: u64, sid: u64) -> bool {
    let members: Vec<Arc<Process>> = processes()
        .into_iter()
        .filter(|process| {
            let state = *process.state.lock();
            let ids = *process.ids.lock();
            state != ProcessState::Exited && ids.pgid == pgid && ids.sid == sid
        })
        .collect();
    if members.is_empty() {
        return true;
    }
    for member in members {
        let ppid = member.ids.lock().ppid;
        if let Some(parent) = lookup_pid(ppid) {
            let parent_ids = *parent.ids.lock();
            if parent_ids.sid == sid && parent_ids.pgid != pgid {
                return false;
            }
        }
    }
    true
}

impl Process {
    /// The POSIX nice value (`-20..19`).
    pub fn nice(&self) -> i32 {
        self.nice.load(Ordering::Acquire)
    }

    /// Store the POSIX nice value; callers clamp it with [`priority::clamp_nice`].
    pub fn set_nice(&self, nice: i32) {
        self.nice.store(nice, Ordering::Release);
    }

    /// Account for one new thread.
    pub fn thread_started(&self) {
        self.threads.fetch_add(1, Ordering::AcqRel);
    }

    /// Account for one exiting thread; true when it was the last one.
    pub fn thread_exited(&self) -> bool {
        self.threads.fetch_sub(1, Ordering::AcqRel) <= 1
    }

    /// Publish a live thread so a process-directed signal can kick it.
    pub fn register_thread(&self, thread: &Arc<Thread>) {
        self.threads_live.lock().push(Arc::downgrade(thread));
    }

    /// Remove an exiting thread from the live-kick list.
    ///
    /// The list mirrors FreeBSD `thread_unlink` in `kern_thread.c` and NetBSD `lwp_free` in
    /// `kern_lwp.c`: the thread is removed from its process's thread list before its stack is
    /// reclaimed. Without this, every `THREAD_CREATE`/`THREAD_EXIT` cycle leaves one `Weak`
    /// entry behind, growing the list by 16 bytes per cycle until the churn exhausts the heap.
    pub fn unregister_thread(&self, thread: &Arc<Thread>) {
        let pointer = Arc::as_ptr(thread);
        self.threads_live.lock().retain(|weak| {
            weak.upgrade()
                .is_some_and(|live| Arc::as_ptr(&live) != pointer)
        });
    }

    /// True when any live thread has a registered PG-7 signal entry.
    ///
    /// Such a process has a libc entry that will run the disposition/default action in user
    /// space; a process without one must have its default action applied by the kernel.
    pub fn has_signal_entry(&self) -> bool {
        self.threads_live
            .lock()
            .iter()
            .filter_map(Weak::upgrade)
            .any(|thread| thread.signal_entry.load(Ordering::Acquire) != 0)
    }

    /// Queue a signal, wake a `PROC_SIGRECV` waiter and kick every live thread.
    ///
    /// The kick is the PG-7 bridge from the kernel's per-process queue to libc's signal entry:
    /// `THREAD_KICK` bit 0 makes the return-to-user path resume at the registered entry, and an
    /// interruptible wait (`ppoll`, `read`, `waitpid`) returns `EINTR` so the handler can run.
    pub fn queue_signal(&self, signo: u32, sender: u64) {
        self.queue_signal_info(signo, sender, SI_USER_CODE, 0, 0);
    }

    /// Queue a signal carrying the `siginfo_t` fields the kernel owns.
    ///
    /// The kernel stores the sender pid, `si_code`, sender uid and the queued `sigval`; it never
    /// interprets the signal disposition. `PROC_SIGRECV` hands the fields back to libc, which
    /// owns the user pending set and constructs the public `siginfo_t`.
    pub fn queue_signal_info(&self, signo: u32, sender: u64, code: u32, uid: u32, value: u64) {
        self.pending_signals.lock().push_back(PendingSignal {
            signo,
            sender,
            code,
            uid,
            value,
        });
        self.signal_waiters.wake_all();
        self.kick_threads();
    }

    /// Kick every live thread of this process with the signal-check bit.
    fn kick_threads(&self) {
        let mut threads = self.threads_live.lock();
        threads.retain(|weak| {
            let Some(thread) = weak.upgrade() else {
                return false;
            };
            // A kick is only meaningful after libc has registered the PG-7 entry. Without one
            // the word would be consumed as a bare `EINTR` with no handler, so leave the signal
            // in the process queue for `PROC_SIGRECV` and do not disturb an unrelated wait.
            if thread.signal_entry.load(Ordering::Acquire) == 0 {
                return true;
            }
            thread.kick_bits.fetch_or(KICK_CHECK, Ordering::AcqRel);
            if thread.interruptible.load(Ordering::Acquire) {
                let token = thread.block_token.load(Ordering::SeqCst);
                let token = (token != 0).then_some(token);
                let _ = crate::sched::wake(thread, token, Errno::EINTR.to_ret());
            }
            true
        });
    }

    /// Pop the oldest queued signal.
    pub fn pop_signal(&self) -> Option<PendingSignal> {
        self.pending_signals.lock().pop_front()
    }

    /// Remove and return the oldest queued signal matching `predicate`, leaving the rest in
    /// order.
    ///
    /// This is the masked-signal contract: a signal the calling thread blocks is not selected,
    /// so it stays queued for a thread (or a later unmask) that accepts it (Illumos `fsig`,
    /// FreeBSD `sigtd`, NetBSD `sigismasked`). No pop/requeue window exists.
    pub fn pop_signal_where(
        &self,
        mut predicate: impl FnMut(&PendingSignal) -> bool,
    ) -> Option<PendingSignal> {
        let mut pending = self.pending_signals.lock();
        let index = pending.iter().position(|signal| predicate(signal))?;
        pending.remove(index)
    }

    /// True when at least one signal is queued.
    pub fn has_pending_signal(&self) -> bool {
        !self.pending_signals.lock().is_empty()
    }

    /// True when at least one queued signal matches `predicate`.
    pub fn has_pending_signal_where(
        &self,
        mut predicate: impl FnMut(&PendingSignal) -> bool,
    ) -> bool {
        self.pending_signals
            .lock()
            .iter()
            .any(|signal| predicate(signal))
    }
}

/// Reparent every live child of `parent` to PID 1 (init).
///
/// Called once from the dying parent's exit path; the child keeps its own pid, group and
/// session, but its next `PROC_WAIT_EX` notification goes to init. A dying PID 1 does not
/// reparent (the system is ending).
pub fn reparent_children(parent: &Process) -> usize {
    if parent.id == 1 {
        return 0;
    }
    let mut count = 0usize;
    for child in processes() {
        if child.id == parent.id {
            continue;
        }
        let mut ids = child.ids.lock();
        if ids.ppid == parent.id {
            ids.ppid = 1;
            count = count.saturating_add(1);
        }
    }
    count
}

/// Wake a parent waiting for a child event and optionally queue `SIGCHLD`.
///
/// The caller must not hold `process.state`; exit accounting reads the child's
/// accumulated time and the parent's signal queue.
pub fn notify_parent(process: &Process, sigchld: bool) {
    let ppid = process.ids.lock().ppid;
    let Some(parent) = lookup_pid(ppid) else {
        return;
    };
    if sigchld {
        parent
            .child_system_ns
            .fetch_add(process.system_ns.load(Ordering::Acquire), Ordering::AcqRel);
        parent
            .child_user_ns
            .fetch_add(process.user_ns.load(Ordering::Acquire), Ordering::AcqRel);
        parent.child_maxrss_bytes.fetch_max(
            process.maxrss_bytes.load(Ordering::Acquire),
            Ordering::AcqRel,
        );
        let uid = process.ids.lock().uid;
        parent.queue_signal_info(SIGCHLD, process.id, CLD_EXITED_CODE, uid, 0);
    }
    parent.child_waiters.wake_all();
}

/// Mark an exited child reaped and remove its pid from the registry.
///
/// Returns `ECHILD` when the exit status was already consumed by another
/// `PROC_WAIT_EX`; PID 1 and the old handle wait keep their existing behavior.
pub fn reap_pid(process: &Process) -> Result<(), Errno> {
    if process.reaped.swap(true, Ordering::AcqRel) {
        return Err(Errno::ECHILD);
    }
    ZOMBIES.lock().retain(|zombie| zombie.id != process.id);
    unregister_pid(process.id);
    Ok(())
}

/// True when the initfs contains the requested init path.
pub fn init_available(cmdline: &CmdLine) -> bool {
    let path = cmdline.get("init").unwrap_or("/init");
    initfs::lookup(path).is_some()
}

/// S44 first-increment kernel test: the per-process disposition table, install/read/clear and
/// the exec reset (`SIG_DFL` for handlers, `SIG_IGN` retained). Runs before PID 1 and exits QEMU.
fn sigacts_selftest() -> ! {
    fn fail(message: &str) -> ! {
        kprintln!("s44: sigacts failed {message}");
        crate::boot::exit_qemu(0x11);
    }
    let mut table = SigActs::new();
    let handler = SigAction {
        handler: 0x1234,
        mask: [0; 2],
        flags: 0,
        restorer: 0,
    };
    if table.install(status::SIGUSR1, handler).is_err() {
        fail("install");
    }
    match table.get(status::SIGUSR1) {
        Some(action) if action.handler == 0x1234 => {}
        _ => fail("readback"),
    }
    if table.install(status::SIGKILL, handler).is_ok() {
        fail("SIGKILL accepted");
    }
    if table.install(status::SIGUSR1, SigAction::DEFAULT).is_err() {
        fail("clear");
    }
    match table.get(status::SIGUSR1) {
        Some(action) if action.handler == SIG_DFL => {}
        _ => fail("clear readback"),
    }
    if table.install(status::SIGTERM, SigAction::IGNORE).is_err()
        || table.install(status::SIGUSR1, handler).is_err()
    {
        fail("exec setup");
    }
    table.exec_reset();
    match (table.get(status::SIGUSR1), table.get(status::SIGTERM)) {
        (Some(handler_action), Some(ignore_action))
            if handler_action.handler == SIG_DFL && ignore_action.handler == SIG_IGN => {}
        _ => fail("exec reset"),
    }
    kprintln!("s44: sigacts table ok");
    crate::boot::exit_qemu(0x10);
}

/// Create PID 1 from the initfs and enter ring 3. On success this never returns.
pub fn start_init(cmdline: &CmdLine) -> Result<(), ProcessError> {
    if cmdline.get("s44-sigactscheck").is_some() {
        sigacts_selftest();
    }
    let path = cmdline.get("init").unwrap_or("/init");
    let entry = initfs::lookup(path).ok_or(ProcessError::NoInit)?;
    let data = entry.data;
    let elf = Elf::parse(data, TARGET_MACHINE, USER_MIN..USER_END).map_err(ProcessError::Elf)?;
    if elf.interp().is_some() {
        return Err(ProcessError::Interpreter);
    }

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let ids = ProcessIds {
        ppid: 0,
        pgid: id,
        sid: id,
        uid: 0,
        euid: 0,
        gid: 0,
        egid: 0,
        suid: 0,
        sgid: 0,
        groups: [0; MAX_GROUPS],
        group_count: 0,
    };
    let address_space = AddressSpace::new().map_err(ProcessError::Map)?;
    let mut user_address_space = UserAddressSpace::new(address_space);
    load_segments(&mut user_address_space, &elf, data, 0)?;
    let path_bytes = path.as_bytes();
    let argv: [&[u8]; 1] = [path_bytes];
    let aux = StackAuxv {
        phdr: elf.phdr_vaddr().unwrap_or(0),
        base: 0,
        entry: elf.entry(),
        phnum: elf.phnum() as u64,
    };
    let user_stack = build_stack(&mut user_address_space, path_bytes, &argv, &[], &ids, aux)?;

    note_process_created();
    let process = Arc::new(Process {
        id,
        ids: Mutex::new(ids),
        path: String::from(path),
        command: Mutex::new(String::from(path)),
        nice: AtomicI32::new(0),
        ctty: Mutex::new(None),
        address_space: Mutex::new(Some(user_address_space)),
        handles: Mutex::new(HandleTable::new()),
        exit_code: AtomicI32::new(0),
        threads: AtomicU32::new(0),
        threads_live: Mutex::new(Vec::new()),
        exiting: AtomicBool::new(false),
        torn_down: AtomicBool::new(false),
        timer_count: Arc::new(AtomicU32::new(0)),
        dma_bytes: Arc::new(AtomicU64::new(0)),
        state: Mutex::new(ProcessState::Running),
        waiters: WaitQueue::new(),
        namespace: Mutex::new(Namespace::root()),
        root: Mutex::new(String::from("/")),
        schemes: Mutex::new(alloc::vec::Vec::new()),
        scheme_queue: Mutex::new(VecDeque::new()),
        scheme_waiters: WaitQueue::new(),
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        capture_slots: Mutex::new(CaptureSlots::new()),
        personality: Mutex::new(PersonalityState::native()),
        pending_signals: Mutex::new(VecDeque::new()),
        sigacts: Mutex::new(Box::new(SigActs::new())),
        pending_exec: Mutex::new(None),
        exec_done: AtomicBool::new(false),
        signal_waiters: WaitQueue::new(),
        child_waiters: WaitQueue::new(),
        stop_signal: AtomicU32::new(0),
        stop_reported: AtomicBool::new(false),
        continued: AtomicBool::new(false),
        reaped: AtomicBool::new(false),
        limits: Mutex::new(default_limits()),
        start_ns: crate::time::now_ns(),
        last_syscall: AtomicU64::new(0),
        last_pc: AtomicU64::new(0),
        wait_object: AtomicU64::new(0),
        in_syscall: AtomicU32::new(0),
        last_submit_id: AtomicU64::new(0),
        last_recv_id: AtomicU64::new(0),
        last_send_id: AtomicU64::new(0),
        wake_fail: AtomicU32::new(0),
        wake_fail_detail: AtomicU64::new(0),
        wake_pending: AtomicU32::new(0),
        wake_last_cpu: AtomicU32::new(u32::MAX),
        system_ns: AtomicU64::new(0),
        user_ns: AtomicU64::new(0),
        child_system_ns: AtomicU64::new(0),
        child_user_ns: AtomicU64::new(0),
        maxrss_bytes: AtomicU64::new(0),
        child_maxrss_bytes: AtomicU64::new(0),
    });
    handle::install_initial(&mut process.handles.lock(), &process, true)
        .map_err(ProcessError::Handle)?;
    register_pid(&process);
    crate::sched::init().map_err(ProcessError::Sched)?;
    let thread = crate::sched::new_user_thread(&process, elf.entry(), user_stack, user_stack)
        .map_err(ProcessError::Sched)?;
    arch_syscall::init();

    kprintln!("process: starting {path}");
    kprintln!(
        "process: {path} entry={:#x} sp={:#x}",
        elf.entry(),
        user_stack
    );

    crate::sched::start_user(thread)
}

fn load_segments(
    address_space: &mut UserAddressSpace,
    elf: &Elf<'_>,
    data: &[u8],
    bias: u64,
) -> Result<(), ProcessError> {
    let plan =
        raven_elf::plan_load_biased(elf, PAGE, bias, LoadMode::Copy).map_err(ProcessError::Elf)?;
    for step in plan {
        address_space
            .vmas_mut()
            .insert(step.vaddr, step.len, step.prot, Backing::Anonymous)
            .map_err(|_| ProcessError::BadAddress)?;
        let mut mapper = address_space.mapper().map_err(ProcessError::Map)?;
        map_step(&mut mapper, data, step)?;
    }
    // The just-written code pages must be visible to instruction fetch before EL0 runs. The
    // aarch64 loader path keeps the D-cache clean per page (`map_step`) and drops every stale
    // instruction-cache line here.
    #[cfg(target_arch = "aarch64")]
    crate::arch::mmu::icache_invalidate_all();
    Ok(())
}

fn map_step(
    mapper: &mut paging::KernelMapper<'_>,
    data: &[u8],
    step: MapStep,
) -> Result<(), ProcessError> {
    let flags = prot_flags(step.prot)?;
    let pages = step.len / PAGE;
    let mut page = 0u64;
    while page < pages {
        let vaddr = step
            .vaddr
            .checked_add(page.saturating_mul(PAGE))
            .ok_or(ProcessError::BadAddress)?;
        let virt = VirtAddr::new(vaddr).ok_or(ProcessError::BadAddress)?;
        let physical = frame::alloc_zeroed_frame().ok_or(ProcessError::OutOfFrames)?;
        let destination = frame::phys_to_virt(physical);
        if destination.is_null() {
            return Err(ProcessError::OutOfFrames);
        }
        match step.kind {
            StepKind::File { offset } => {
                let source = file_bytes(data, offset, page, PAGE)?;
                // SAFETY: `destination` is a fresh zeroed frame mapped through the HHDM and
                // `source` is a page inside the validated ELF image.
                unsafe {
                    core::ptr::copy_nonoverlapping(source.as_ptr(), destination, PAGE as usize);
                }
            }
            StepKind::CopyTail { offset, len } => {
                let source = file_bytes(data, offset, 0, len)?;
                // SAFETY: same fresh frame; `len` is bounded by one page and the source range is
                // checked by `file_bytes`.
                unsafe {
                    core::ptr::copy_nonoverlapping(source.as_ptr(), destination, len as usize);
                }
            }
            StepKind::Zero => {}
        }
        // Make the bytes written through the HHDM visible to instruction fetch.
        #[cfg(target_arch = "aarch64")]
        // SAFETY: `destination` is a whole 4 KiB frame mapped read/write through the HHDM.
        unsafe {
            crate::arch::mmu::dcache_clean_range(destination as u64, PAGE);
        }
        mapper
            .map(virt, PhysAddr(physical.0), flags, PageSize::Size4K)
            .map_err(ProcessError::Map)?;
        page = page.saturating_add(1);
    }
    Ok(())
}

/// Load an exec image directly from the caller's user range.
///
/// The ELF header and program-header table were copied into a small kernel buffer; this path
/// copies each `PT_LOAD` page from the still-live caller image into a fresh frame, so the 4 MiB
/// kernel heap never holds the multi-MiB image. The ranges were validated by
/// [`raven_elf::Elf::parse_with_len`] and are checked again here.
pub(crate) fn load_segments_from_user(
    address_space: &mut UserAddressSpace,
    elf: &Elf<'_>,
    image_base: u64,
    image_len: u64,
    bias: u64,
) -> Result<(), ProcessError> {
    let plan =
        raven_elf::plan_load_biased(elf, PAGE, bias, LoadMode::Copy).map_err(ProcessError::Elf)?;
    for step in plan {
        address_space
            .vmas_mut()
            .insert(step.vaddr, step.len, step.prot, Backing::Anonymous)
            .map_err(|_| ProcessError::BadAddress)?;
        let mut mapper = address_space.mapper().map_err(ProcessError::Map)?;
        map_step_from_user(&mut mapper, image_base, image_len, step)?;
    }
    // The just-written code pages must be visible to instruction fetch before EL0 runs.
    #[cfg(target_arch = "aarch64")]
    crate::arch::mmu::icache_invalidate_all();
    Ok(())
}

fn map_step_from_user(
    mapper: &mut paging::KernelMapper<'_>,
    image_base: u64,
    image_len: u64,
    step: MapStep,
) -> Result<(), ProcessError> {
    let flags = prot_flags(step.prot)?;
    let pages = step.len / PAGE;
    let mut page = 0u64;
    while page < pages {
        let vaddr = step
            .vaddr
            .checked_add(page.saturating_mul(PAGE))
            .ok_or(ProcessError::BadAddress)?;
        let virt = VirtAddr::new(vaddr).ok_or(ProcessError::BadAddress)?;
        let physical = frame::alloc_zeroed_frame().ok_or(ProcessError::OutOfFrames)?;
        let destination = frame::phys_to_virt(physical);
        if destination.is_null() {
            return Err(ProcessError::OutOfFrames);
        }
        match step.kind {
            StepKind::File { offset } => {
                let file_offset = offset
                    .checked_add(page.saturating_mul(PAGE))
                    .ok_or(ProcessError::BadAddress)?;
                copy_user_range(destination, image_base, image_len, file_offset, PAGE)?;
            }
            StepKind::CopyTail { offset, len } => {
                copy_user_range(destination, image_base, image_len, offset, len)?;
            }
            StepKind::Zero => {}
        }
        // Make the bytes written through the HHDM visible to instruction fetch.
        #[cfg(target_arch = "aarch64")]
        // SAFETY: `destination` is a whole 4 KiB frame mapped read/write through the HHDM.
        unsafe {
            crate::arch::mmu::dcache_clean_range(destination as u64, PAGE);
        }
        mapper
            .map(virt, PhysAddr(physical.0), flags, PageSize::Size4K)
            .map_err(ProcessError::Map)?;
        page = page.saturating_add(1);
    }
    Ok(())
}

fn copy_user_range(
    destination: *mut u8,
    image_base: u64,
    image_len: u64,
    offset: u64,
    length: u64,
) -> Result<(), ProcessError> {
    let end = offset.checked_add(length).ok_or(ProcessError::BadAddress)?;
    if end > image_len {
        return Err(ProcessError::BadAddress);
    }
    let address = image_base
        .checked_add(offset)
        .ok_or(ProcessError::BadAddress)?;
    let length = usize::try_from(length).map_err(|_| ProcessError::BadAddress)?;
    // SAFETY: `destination` is a freshly zeroed frame mapped read/write through the HHDM and
    // `length` is at most one page; the slice is live for the duration of the copy.
    let destination = unsafe { core::slice::from_raw_parts_mut(destination, length) };
    let source = UserSlice::new(address, length as u64).map_err(|_| ProcessError::BadAddress)?;
    copy_from_user(destination, source).map_err(|_| ProcessError::BadAddress)
}

fn file_bytes(data: &[u8], offset: u64, page: u64, length: u64) -> Result<&[u8], ProcessError> {
    let start = offset
        .checked_add(page.saturating_mul(PAGE))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ProcessError::BadAddress)?;
    let end = start
        .checked_add(usize::try_from(length).map_err(|_| ProcessError::BadAddress)?)
        .ok_or(ProcessError::BadAddress)?;
    data.get(start..end).ok_or(ProcessError::BadAddress)
}

fn prot_flags(bits: u8) -> Result<PageFlags, ProcessError> {
    let write = bits & (prot::WRITE as u8) != 0;
    let execute = bits & (prot::EXEC as u8) != 0;
    if write && execute {
        return Err(ProcessError::BadAddress);
    }
    Ok(if execute {
        PageFlags::user_rx()
    } else if write {
        PageFlags::user_rw()
    } else {
        PageFlags::user_ro()
    })
}

/// The `AT_*` values the exec path has already resolved for [`build_stack`].
///
/// The static path supplies the image's own values with `base == 0`; a dynamic exec supplies the
/// biased main-program header/entry and the interpreter load bias.
pub(crate) struct StackAuxv {
    /// `AT_PHDR`: the main image's program-header address.
    pub(crate) phdr: u64,
    /// `AT_BASE`: the interpreter load bias, or zero for a static image.
    pub(crate) base: u64,
    /// `AT_ENTRY`: the main image entry point.
    pub(crate) entry: u64,
    /// `AT_PHNUM`: the main image's program-header count.
    pub(crate) phnum: u64,
}

fn build_stack(
    address_space: &mut UserAddressSpace,
    path: &[u8],
    argv: &[&[u8]],
    envp: &[&[u8]],
    ids: &ProcessIds,
    aux: StackAuxv,
) -> Result<u64, ProcessError> {
    // PG-50: `rsp == sp0`, 16-byte aligned, with the C8 argv/envp/auxv block at `sp0`.
    const AT_PHDR: u64 = 3;
    const AT_PHENT: u64 = 4;
    const AT_PHNUM: u64 = 5;
    const AT_PAGESZ: u64 = 6;
    const AT_BASE: u64 = 7;
    const AT_FLAGS: u64 = 8;
    const AT_ENTRY: u64 = 9;
    const AT_UID: u64 = 11;
    const AT_EUID: u64 = 12;
    const AT_GID: u64 = 13;
    const AT_EGID: u64 = 14;
    const AT_PLATFORM: u64 = 15;
    const AT_HWCAP: u64 = 16;
    const AT_CLKTCK: u64 = 17;
    const AT_SECURE: u64 = 23;
    const AT_RANDOM: u64 = 25;
    const AT_HWCAP2: u64 = 26;
    const AT_EXECFN: u64 = 31;
    const AT_MINSIGSTKSZ: u64 = 51;
    const PHENT: u64 = 56;
    const ARG_MAX: usize = 128 * 1024;
    const MAX_COUNT: usize = 4096;

    if argv.len() > MAX_COUNT || envp.len() > MAX_COUNT {
        return Err(ProcessError::ArgTooLong);
    }
    let platform = platform_name();
    let mut string_bytes = path
        .len()
        .checked_add(1)
        .and_then(|total| total.checked_add(platform.len().saturating_add(1)))
        .ok_or(ProcessError::ArgTooLong)?;
    for argument in argv {
        string_bytes = string_bytes
            .checked_add(argument.len().saturating_add(1))
            .ok_or(ProcessError::ArgTooLong)?;
    }
    for variable in envp {
        string_bytes = string_bytes
            .checked_add(variable.len().saturating_add(1))
            .ok_or(ProcessError::ArgTooLong)?;
    }
    if string_bytes > ARG_MAX {
        return Err(ProcessError::ArgTooLong);
    }

    let stack_base = address_space.stack_base();
    let mut buffer = vec![0u8; USER_STACK_SIZE as usize];
    let mut cursor = buffer.len();

    // AT_RANDOM comes from the same boot-seeded pool as GETRANDOM. On a platform with no
    // hardware source the pool is unseeded and the pair is published as zero rather than a
    // predictable fallback; GETRANDOM reports the documented EAGAIN.
    let random_address = match random16() {
        Some(random) => {
            let random_start = cursor
                .checked_sub(random.len())
                .ok_or(ProcessError::ArgTooLong)?;
            let random_slot = buffer
                .get_mut(random_start..cursor)
                .ok_or(ProcessError::BadAddress)?;
            random_slot.copy_from_slice(&random);
            cursor = random_start;
            stack_base
                .checked_add(u64::try_from(cursor).map_err(|_| ProcessError::BadAddress)?)
                .ok_or(ProcessError::BadAddress)?
        }
        None => 0,
    };

    let platform_address = place_bytes(&mut buffer, &mut cursor, stack_base, platform.as_bytes())?;
    let path_address = place_bytes(&mut buffer, &mut cursor, stack_base, path)?;
    let mut argv_addresses = Vec::new();
    argv_addresses
        .try_reserve(argv.len())
        .map_err(|_| ProcessError::OutOfFrames)?;
    for argument in argv {
        argv_addresses.push(place_bytes(&mut buffer, &mut cursor, stack_base, argument)?);
    }
    let mut env_addresses = Vec::new();
    env_addresses
        .try_reserve(envp.len())
        .map_err(|_| ProcessError::OutOfFrames)?;
    for variable in envp {
        env_addresses.push(place_bytes(&mut buffer, &mut cursor, stack_base, variable)?);
    }

    let auxv_pairs = 19usize;
    let words = 1usize
        .checked_add(argv.len())
        .and_then(|total| total.checked_add(1))
        .and_then(|total| total.checked_add(envp.len()))
        .and_then(|total| total.checked_add(1))
        .and_then(|total| total.checked_add(auxv_pairs.saturating_mul(2)))
        .and_then(|total| total.checked_add(2))
        .ok_or(ProcessError::ArgTooLong)?;
    let block_bytes = words.checked_mul(8).ok_or(ProcessError::ArgTooLong)?;
    let block_start = cursor
        .checked_sub(
            block_bytes
                .checked_add(16)
                .ok_or(ProcessError::ArgTooLong)?,
        )
        .ok_or(ProcessError::ArgTooLong)?
        & !0xf;
    if block_start
        .checked_add(block_bytes)
        .ok_or(ProcessError::ArgTooLong)?
        > cursor
    {
        return Err(ProcessError::ArgTooLong);
    }

    let mut block_cursor = block_start;
    write_word(
        &mut buffer,
        &mut block_cursor,
        u64::try_from(argv.len()).map_err(|_| ProcessError::ArgTooLong)?,
    )?;
    for address in &argv_addresses {
        write_word(&mut buffer, &mut block_cursor, *address)?;
    }
    write_word(&mut buffer, &mut block_cursor, 0)?;
    for address in &env_addresses {
        write_word(&mut buffer, &mut block_cursor, *address)?;
    }
    write_word(&mut buffer, &mut block_cursor, 0)?;
    for (key, value) in [
        (AT_PHDR, aux.phdr),
        (AT_PHENT, PHENT),
        (AT_PHNUM, aux.phnum),
        (AT_PAGESZ, PAGE),
        (AT_BASE, aux.base),
        (AT_FLAGS, 0),
        (AT_ENTRY, aux.entry),
        (AT_UID, u64::from(ids.uid)),
        (AT_EUID, u64::from(ids.euid)),
        (AT_GID, u64::from(ids.gid)),
        (AT_EGID, u64::from(ids.egid)),
        (AT_PLATFORM, platform_address),
        (AT_HWCAP, 0),
        (AT_CLKTCK, 100),
        (AT_SECURE, 0),
        (AT_RANDOM, random_address),
        (AT_HWCAP2, 0),
        (AT_EXECFN, path_address),
        (AT_MINSIGSTKSZ, 8192),
    ] {
        write_word(&mut buffer, &mut block_cursor, key)?;
        write_word(&mut buffer, &mut block_cursor, value)?;
    }
    write_word(&mut buffer, &mut block_cursor, 0)?;
    write_word(&mut buffer, &mut block_cursor, 0)?;
    if block_cursor > cursor {
        return Err(ProcessError::ArgTooLong);
    }
    let sp0 = stack_base
        .checked_add(u64::try_from(block_start).map_err(|_| ProcessError::BadAddress)?)
        .ok_or(ProcessError::BadAddress)?;

    address_space
        .vmas_mut()
        .insert(
            stack_base,
            USER_STACK_SIZE,
            (prot::READ | prot::WRITE) as u8,
            Backing::Anonymous,
        )
        .map_err(|_| ProcessError::BadAddress)?;
    let mut mapper = address_space.mapper().map_err(ProcessError::Map)?;
    let pages = USER_STACK_SIZE / PAGE;
    let mut page = 0u64;
    while page < pages {
        let vaddr = stack_base
            .checked_add(page.saturating_mul(PAGE))
            .ok_or(ProcessError::BadAddress)?;
        let virt = VirtAddr::new(vaddr).ok_or(ProcessError::BadAddress)?;
        let physical = frame::alloc_zeroed_frame().ok_or(ProcessError::OutOfFrames)?;
        let destination = frame::phys_to_virt(physical);
        if destination.is_null() {
            return Err(ProcessError::OutOfFrames);
        }
        let source_start =
            usize::try_from(page.saturating_mul(PAGE)).map_err(|_| ProcessError::BadAddress)?;
        let source_end = source_start
            .checked_add(PAGE as usize)
            .ok_or(ProcessError::BadAddress)?;
        let source = buffer
            .get(source_start..source_end)
            .ok_or(ProcessError::BadAddress)?;
        // SAFETY: `destination` is a fresh zeroed frame mapped through the HHDM; `source` is one
        // page of the kernel stack buffer.
        unsafe {
            core::ptr::copy_nonoverlapping(source.as_ptr(), destination, PAGE as usize);
        }
        mapper
            .map(
                virt,
                PhysAddr(physical.0),
                PageFlags::user_rw(),
                PageSize::Size4K,
            )
            .map_err(ProcessError::Map)?;
        page = page.saturating_add(1);
    }
    Ok(sp0)
}

/// Place one NUL-terminated string below `cursor` and return its user address.
fn place_bytes(
    buffer: &mut [u8],
    cursor: &mut usize,
    stack_base: u64,
    bytes: &[u8],
) -> Result<u64, ProcessError> {
    let need = bytes.len().checked_add(1).ok_or(ProcessError::ArgTooLong)?;
    let start = cursor.checked_sub(need).ok_or(ProcessError::ArgTooLong)?;
    let slot = buffer
        .get_mut(start..*cursor)
        .ok_or(ProcessError::BadAddress)?;
    slot.fill(0);
    if let Some(destination) = slot.get_mut(..bytes.len()) {
        destination.copy_from_slice(bytes);
    }
    *cursor = start;
    stack_base
        .checked_add(u64::try_from(start).map_err(|_| ProcessError::BadAddress)?)
        .ok_or(ProcessError::BadAddress)
}

/// `AT_PLATFORM` spelling for the build architecture.
fn platform_name() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(target_arch = "aarch64")]
    {
        "aarch64"
    }
    #[cfg(target_arch = "riscv64")]
    {
        "riscv64"
    }
}

fn write_word(buffer: &mut [u8], cursor: &mut usize, value: u64) -> Result<(), ProcessError> {
    let end = (*cursor).checked_add(8).ok_or(ProcessError::BadAddress)?;
    let slot = buffer
        .get_mut(*cursor..end)
        .ok_or(ProcessError::BadAddress)?;
    slot.copy_from_slice(&value.to_le_bytes());
    *cursor = end;
    Ok(())
}

/// 16 bytes for `AT_RANDOM` from the boot-seeded ChaCha20 pool.
///
/// `None` means the platform has no hardware entropy source; the auxv pair is then zero and
/// `GETRANDOM` returns the documented `EAGAIN` instead of serving a predictable fallback.
fn random16() -> Option<[u8; 16]> {
    let mut bytes = [0u8; 16];
    crate::time::entropy_fill(&mut bytes).ok()?;
    Some(bytes)
}

/// The ELF machine of the running kernel.
#[cfg(target_arch = "x86_64")]
const TARGET_MACHINE: Machine = Machine::X86_64;
/// The ELF machine of the running kernel.
#[cfg(target_arch = "aarch64")]
const TARGET_MACHINE: Machine = Machine::Aarch64;

fn cleanup_evidence(process: &Process) -> bool {
    if !paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s69-cleanupcheck")
    {
        return false;
    }
    let command = process.command.lock();
    command.starts_with("s69-") && !command.starts_with("s69-victim")
}

/// One-shot process teardown: scheme teardown, address space, handle table, exit reporting.
///
/// The first caller wins; later callers (the last-thread path racing an `EXIT`, a terminate
/// racing the target's own exit) observe `torn_down` and do nothing.
///
/// `own_context` is true when the teardown runs on a thread of the dying process itself (the
/// `EXIT` path).  Only then may the address space be released with
/// [`UserAddressSpace::release`], which switches CR3 to the kernel root: a foreign caller
/// (terminating another process, reaping an unstarted child) must keep its own CR3, because it
/// returns to user mode through `sysret`.  The cold path frees the target's frames and page
/// tables without switching, which is safe because user threads run only on CPU 0 in a
/// non-preemptible kernel: the caller is the only running user thread, so the target's root is
/// not active anywhere, and none of its blocked or runnable threads can be switched to (the
/// scheduler filter drops them).
///
/// The per-exit evidence prints (`s24`/`s25`/`s21`/`s36` counters) belong to the dying
/// thread's own exit path and run in that context only; a foreign caller skips them (an
/// unstarted process has no thread to print from, and a terminated process prints on its last
/// thread's exit).
/// Presence check for a boot command-line gate without building the 64-token `CmdLine` table.
///
/// `teardown` runs on the 28 KiB kernel stack. An inlined `CmdLine::parse` copy is about 1.5 KiB,
/// and the seven gate copies below exhausted the stack before the IPC/event close hooks ran (the
/// K434 init-exit double fault). This is the same token walk the s17 gate already uses.
fn cmdline_has(key: &str) -> bool {
    paging::cmdline().split_ascii_whitespace().any(|token| {
        token == key
            || token
                .strip_prefix(key)
                .is_some_and(|rest| rest.starts_with('='))
    })
}

fn teardown(process: &Arc<Process>, code: i32, own_context: bool) {
    if process.torn_down.swap(true, Ordering::AcqRel) {
        return;
    }
    // The pid registry keeps a weak zombie entry until `PROC_WAIT_EX` reaps the status; a weak
    // entry whose process is gone is removed by `lookup_pid`.
    // Fail pending scheme requests and remove the registry entries while the server page tables
    // still exist, so capture mappings can be removed before the address space is released.
    // The IPC/router/event teardown takes wait/scheduler locks; mask interrupts for it, but keep
    // the address-space release interruptible for the TLB shootdown.
    {
        let _irq = crate::sync::critical();
        ipc::on_process_exit(process);
    }
    // The RSS high-water mark is snapshotted in `terminate_once` before IPC teardown; do not
    // walk the address space again after that teardown has altered the mappings.
    let address_space = process.address_space.lock().take();
    if let Some(mut address_space) = address_space {
        if own_context {
            address_space.release();
        } else {
            // SAFETY: not unsafe, but context-dependent - see the function comment.  The target
            // root is inactive; free its frames and tables without switching the caller's CR3.
            address_space.free_user_frames();
            drop(address_space);
        }
    }
    // A dead process can no longer call syscalls: drop its handle table so DMA buffers and
    // every other object release their resources even while a last reference to the process
    // survives elsewhere (for example on the abandoned stack of its dead thread). Object drops
    // can wake waiters and take the scheduler lock, so mask interrupts for this section.
    {
        let _irq = crate::sync::critical();
        *process.handles.lock() = HandleTable::new();
    }
    // S69 cleanup evidence: every kernel-owned resource count plus the process-local waiter and
    // request queues after the table was replaced.  The line is gated to fixture command names so
    // the ten-thousand-exit soak does not flood the serial log; the fixture sets them with
    // PROC_SET_COMMAND.  It runs in both own and foreign teardown contexts because the interesting
    // `PROC_TERMINATE` path is foreign.
    if cleanup_evidence(process) {
        let command = process.command.lock().clone();
        let (heap_used, _) = crate::memory::heap::stats();
        kprintln!(
            "s69: exit {} processes={} zombies={} handles={} lineage={} timers={} timers_live={} dma={} dma_live={} irq={} mmio={} events={} captures={} pager={} wait={} childwait={} sigwait={} schemewait={} client={} active={} queued={} stack_fresh={} stack_cached={} heap={}",
            command,
            processes().len(),
            ZOMBIES.lock().len(),
            process.handles.lock().len(),
            crate::process::handle::lineage_nodes(),
            process.timer_count.load(Ordering::Acquire),
            crate::event::live_timers(),
            process.dma_bytes.load(Ordering::Acquire),
            crate::driver::dma_in_use(),
            crate::driver::irq::live_objects(),
            crate::process::handle::live_mmio(),
            crate::event::live_queues(),
            ipc::capture::frames_in_use(),
            crate::process::handle::pager_objects(),
            process.waiters.len(),
            process.child_waiters.len(),
            process.signal_waiters.len(),
            process.scheme_waiters.len(),
            process.client_requests.lock().len(),
            process.active_requests.lock().len(),
            process.scheme_queue.lock().len(),
            crate::sched::thread::stack_fresh(),
            crate::sched::thread::stack_cached(),
            heap_used
        );
    }
    // S16 churn evidence: after the handle table is replaced, the process must own no handles
    // and no lineage nodes that are not kept alive by a descendant in another process. The
    // controller terminates the fixture server first, so its own final line is the baseline.
    // Keep this own-context only: a foreign teardown (terminate/reap) must return through the
    // caller's path, the S40 rule for every teardown evidence print.
    if own_context && cmdline_has("s16-teardowncheck") {
        kprintln!(
            "s16: exit {} handles={} lineage={}",
            process.path,
            process.handles.lock().len(),
            crate::process::handle::lineage_nodes()
        );
    }
    // S17 pager evidence: once both the address space and the handle table are gone, no
    // pager-backed object may survive in this process. Own-context only, like every other
    // teardown print (S40). `split_ascii_whitespace` keeps this path off the deep `CmdLine`
    // parser: a pager fault already leaves the teardown close to the 20 KiB kernel stack.
    if own_context
        && paging::cmdline()
            .split_ascii_whitespace()
            .any(|token| token == "s17-pagecheck")
    {
        kprintln!(
            "s17: exit {} pager_objects={}",
            process.path,
            crate::process::handle::pager_objects()
        );
    }
    if process.id == 1 {
        kprintln!("init exited ({code})");
        if crate::log::test_exit() {
            crate::boot::exit_qemu(if code == 0 { 0x10 } else { 0x11 });
        }
        crate::arch::halt()
    }
    // Keep the interactive console quiet for normal child exits: `init exited` above is the
    // stable test marker. The per-process detail remains available with `log-level=debug`.
    crate::kdebug!("process: {} exited ({code})", process.path);
    // The per-exit evidence prints belong to the dying thread's own context; a foreign teardown
    // (reaping an unstarted process, terminating another) must return to its caller without
    // them.
    if !own_context {
        return;
    }
    // S24 leak evidence: the free-frame count after every child exit.
    if cmdline_has("s24-framecheck") {
        kprintln!("s24: frames free={}", frame::stats().free_bytes);
        // K570: the pty-churn frame line rides the same gate. A second top-level parse/print
        // block enlarged the teardown frame past the kernel stack (K536 rev187 double fault).
        if cmdline_has("s24-alloccheck") {
            kprintln!("s24: frames allocated={}", frame::stats().allocated_bytes);
        }
    }

    // S25 handle-transfer leak evidence: the dying process's live handle count. A transferred
    // handle that survived process death, or a slot leaked by the transfer path, drifts this
    // number across repeated spawn/exit cycles.
    if cmdline_has("s25-handlecheck") {
        kprintln!("s25: exit handles={}", process.handles.lock().len());
    }
    // S21b DMA leak evidence: the free-frame count after every child exit. Consecutive exits
    // print the same number when allocation and release are balanced.
    if cmdline_has("s21-framecheck") {
        kprintln!("s21: frames free={}", frame::stats().free_bytes);
    }
    // S36 stack-reuse evidence: the fresh-allocation and cached-stack counters must return to
    // baseline across repeated spawn/exit cycles.
    if cmdline_has("s36-stackcheck") {
        let (heap_used, _) = crate::memory::heap::stats();
        let stats = crate::memory::frame::stats();
        kprintln!(
            "s36: stacks fresh={} cached={} heap={} frames={} allocated={}",
            crate::sched::thread::stack_fresh(),
            crate::sched::thread::stack_cached(),
            heap_used,
            stats.free_bytes,
            stats.allocated_bytes
        );
    }
    // S36 vfsd map/unmap red row: exit 88 records the baseline, exit 89 compares the allocated
    // frame bytes. The markers run around the fixture's vfsd pager map/unmap cycles; a leak keeps
    // allocated_bytes above the one-chunk tolerance and prints the forbidden `leak` line.
    if cmdline_has("s36-mapcheck") {
        let stats = crate::memory::frame::stats();
        if code == 88 {
            S36_MAP_BASE.store(stats.allocated_bytes, Ordering::Release);
            kprintln!(
                "s36: mapcheck base allocated={} pager={}",
                stats.allocated_bytes,
                crate::process::handle::pager_objects()
            );
        } else if code == 89 {
            let base = S36_MAP_BASE.load(Ordering::Acquire);
            let delta = stats.allocated_bytes.saturating_sub(base);
            kprintln!(
                "s36: mapcheck delta={} allocated={} pager={}",
                delta,
                stats.allocated_bytes,
                crate::process::handle::pager_objects()
            );
            if delta <= 64 * 1024 {
                kprintln!("s36: mapcheck ok");
            } else {
                kprintln!("s36: mapcheck leak delta={}", delta);
            }
        }
    }
    // S36 timeout-retention row: exit 90 records the live non-idle Thread count and the queued
    // timeout sleepers; exit 91 compares them after the bounded terminate-while-waiting churn.
    // A dead Thread retained by a timeout entry keeps both counters above the tolerance.
    if S36_TIMEOUTCHECK.load(Ordering::Acquire) {
        let live = crate::sched::thread::live_threads() as u64;
        let sleepers = crate::sched::sleepers_len() as u64;
        let reap = crate::sched::reap_len() as u64;
        let handles = crate::process::handle::thread_handles();
        let rq = crate::sched::runqueue_len();
        let current = crate::sched::exiting_current();
        let futex = crate::syscall::futex::waiters_len();
        let procq = wait_queue_len();
        let deadlive = dead_live_threads();
        if code == 90 {
            S36_TIMEOUT_LIVE.store(live, Ordering::Release);
            S36_TIMEOUT_SLEEPERS.store(sleepers, Ordering::Release);
            kprintln!(
                "s36: timeoutcheck base live={} sleepers={} reap={} handles={} rq={} current={} futex={} procq={} deadlive={}",
                live,
                sleepers,
                reap,
                handles,
                rq,
                current,
                futex,
                procq,
                deadlive
            );
        } else if code == 91 {
            let live_delta = live.saturating_sub(S36_TIMEOUT_LIVE.load(Ordering::Acquire));
            let sleepers_delta =
                sleepers.saturating_sub(S36_TIMEOUT_SLEEPERS.load(Ordering::Acquire));
            kprintln!(
                "s36: timeoutcheck delta live={} sleepers={} reap={} handles={} rq={} current={} futex={} procq={} deadlive={} live_delta={} sleepers_delta={}",
                live,
                sleepers,
                reap,
                handles,
                rq,
                current,
                futex,
                procq,
                deadlive,
                live_delta,
                sleepers_delta
            );
            if live_delta <= 8 && sleepers_delta <= 8 {
                kprintln!("s36: timeoutcheck ok");
            } else {
                kprintln!(
                    "s36: timeoutcheck leak live_delta={} sleepers_delta={}",
                    live_delta,
                    sleepers_delta
                );
            }
        }
    }
    // S36 lock/time churn: exit 90 records the baseline, exit 91 compares the allocated frame
    // bytes after the futex wake/timeout churn.
    if cmdline_has("s36-lockcheck") {
        let stats = crate::memory::frame::stats();
        if code == 90 {
            S36_LOCK_BASE.store(stats.allocated_bytes, Ordering::Release);
            kprintln!("s36: lockcheck base allocated={}", stats.allocated_bytes);
        } else if code == 91 {
            let base = S36_LOCK_BASE.load(Ordering::Acquire);
            let delta = stats.allocated_bytes.saturating_sub(base);
            kprintln!(
                "s36: lockcheck delta={} allocated={}",
                delta,
                stats.allocated_bytes
            );
            if delta <= 64 * 1024 {
                kprintln!("s36: lockcheck ok");
            } else {
                kprintln!("s36: lockcheck leak delta={}", delta);
            }
        }
    }
    // S36b process-object diagnostic: separate line so the stable s36 regex is unchanged.
    if paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s36-proccheck")
    {
        let (dead_handles, top_pid, top_count) = crate::process::handle::dead_process_handles();
        let (notices, dead_clients, dead_servers) = crate::ipc::router::close_notice_stats();
        let leaked = leaked_process_stats();
        let leaked_threads = crate::sched::thread::leaked_thread_objects();
        kprintln!(
            "s36: procs={} threads={} zombies={} proc_size={} thread_size={} lineage={} dead_handles={} top_pid={} top_count={} notices={} dead_clients={} dead_servers={} requests={} reap={} leaked={} ext1={} ext2={} ext3={} self_handles={} all_dead_handles={} leaked_threads={} file_refs={} request_refs={} thread_handles={} all_entries={} unique={} client_refs={} ctty_refs={} zombie_refs={}",
            live_processes(),
            crate::sched::thread::live_threads(),
            ZOMBIES.lock().len(),
            core::mem::size_of::<Process>(),
            core::mem::size_of::<crate::sched::thread::Thread>(),
            crate::process::handle::lineage_nodes(),
            dead_handles,
            top_pid,
            top_count,
            notices,
            dead_clients,
            dead_servers,
            crate::ipc::live_requests(),
            crate::sched::reap_pending(),
            leaked[0],
            leaked[1],
            leaked[2],
            leaked[3],
            leaked[4],
            leaked[5],
            leaked[6],
            leaked[7],
            leaked[8],
            leaked[9],
            leaked[10],
            leaked[11],
            leaked[12],
            leaked[13],
            leaked[14]
        );
        kprintln!(
            "s36: leaked_tobj={} t_ext1={} t_ext2plus={} t_blocked={} t_ready={} t_dead={} t_token={}",
            leaked_threads[0],
            leaked_threads[1],
            leaked_threads[2],
            leaked_threads[3],
            leaked_threads[4],
            leaked_threads[5],
            leaked_threads[6]
        );
        let thread_holders = crate::sched::thread::thread_holder_stats();
        kprintln!(
            "s36: hold thr={} cur={} rq={} sleep={} reap={} live={} pwait={} futex={} handle={} multi={} unexp={} reqwait={}",
            thread_holders[0],
            thread_holders[1],
            thread_holders[2],
            thread_holders[3],
            thread_holders[4],
            thread_holders[5],
            thread_holders[6],
            thread_holders[7],
            thread_holders[8],
            thread_holders[9],
            thread_holders[10],
            thread_holders[11]
        );

        let holders = request_holder_stats();
        kprintln!(
            "s36: ipc client={} active={} queued={} top_pid={} top_client={} top_queue_pid={} top_queue={}",
            holders[0],
            holders[1],
            holders[2],
            holders[3],
            holders[4],
            holders[5],
            holders[6]
        );
        let (heap_used, heap_free) = crate::memory::heap::stats();
        let class = crate::memory::heap::class_stats(16);
        kprintln!(
            "s36: heap used={} free={} c3072_slabs={} c3072_used={} c3072_free={} req_size={} file_size={} cap_size={} evq_size={}",
            heap_used,
            heap_free,
            class.map_or(0, |stats| stats.slabs),
            class.map_or(0, |stats| stats.in_use),
            class.map_or(0, |stats| stats.free),
            core::mem::size_of::<crate::ipc::PendingRequest>(),
            core::mem::size_of::<crate::ipc::SchemeFile>(),
            core::mem::size_of::<crate::ipc::capture::Capture>(),
            core::mem::size_of::<crate::event::EventQueue>()
        );
        let opcodes = queue_opcode_stats();
        kprintln!(
            "s36: qop close={} dup={} open={} other={} top_queue_pid={}",
            opcodes[0],
            opcodes[1],
            opcodes[2],
            opcodes[3],
            opcodes[4]
        );
        let owner = queue_owner_state();
        kprintln!(
            "s36: qowner pid={} state={} last_syscall={} in_syscall={} wait_object={} threads={}",
            owner[0],
            owner[1],
            owner[2],
            owner[3],
            owner[4],
            owner[5]
        );
    }
}

/// Wake every wait-queue entry owned by a dying process before its objects are released.
///
/// A blocked thread holds the `Arc` of the object it waits on and a queue entry that keeps its
/// own `Arc<Thread>` alive.  Dropping the handle table alone therefore frees neither the object
/// nor the thread/stack: the queue still names the waiter and the waiter keeps the syscall frame
/// alive.  The sweep runs in the terminal process's state, before `ipc::on_process_exit` and the
/// handle-table replacement, and wakes the threads with `EIO`; `block_current` then observes
/// `exiting` and runs the normal thread-exit path.
///
/// The wait-queue lock is the innermost lock on every path here (`state` -> handles -> queue is
/// used only by this terminal path, which no syscall can race on the boot CPU), and every wake
/// releases the queue before taking the scheduler lock.
fn wake_process_waiters(process: &Arc<Process>) {
    // Process-owned queues.
    let _ = process.signal_waiters.wake_process(process.id);
    let _ = process.child_waiters.wake_process(process.id);
    let _ = process.scheme_waiters.wake_process(process.id);
    // Futex buckets are global.
    crate::syscall::futex::remove_process(process.id);
    // Per-CPU sleep lists hold an `Arc<Thread>` until the deadline.
    crate::sched::cancel_process_sleepers(process.id);

    // Resources reachable from the dying table.  A blocked syscall keeps its object `Arc`, so
    // the object is still live even after this table is replaced.  IRQ objects are collected and
    // retired after the table lock is released: deciding whether another live process shares one
    // needs the pid registry and another process's handle table.
    let mut irqs: Vec<Arc<crate::driver::irq::Irq>> = Vec::new();
    let mut pipes: Vec<Arc<crate::process::pipe::PipeEnd>> = Vec::new();
    let mut duplexes: Vec<Arc<crate::process::duplex::DuplexEnd>> = Vec::new();
    {
        let handles = process.handles.lock();
        handles.for_each_live(|_, object, _, _| match object {
            Object::Process(target) => {
                // The dying process had a thread blocked in PROC_WAIT on this target.
                let _ = target.waiters.wake_process(process.id);
            }
            Object::Thread(thread) => {
                // A helper thread may be in a pure scheduler sleep; a queue waiter is already
                // removed above.  The wake is harmless when the thread is ready or already gone.
                let _ = crate::sched::wake(Arc::clone(thread), None, Errno::EIO.to_ret());
            }
            Object::Irq(irq) => {
                irq.wake_process(process.id);
                irqs.push(Arc::clone(irq));
            }
            Object::Pipe(end) => {
                end.wake_process(process.id);
                pipes.push(Arc::clone(end));
            }
            Object::Duplex(end) => {
                end.wake_process(process.id);
                duplexes.push(Arc::clone(end));
            }
            Object::Memory(memory) => {
                let _ = memory.page_waiters().wake_process(process.id);
            }
            _ => {}
        });
    }
    // A thread of the dying process may wait on another live process's queue through a pid, not
    // a handle: `PROC_WAIT_EX`/`waitpid` keeps only the child pid, and libc closes the fork
    // handle. Sweep every live process's queues by waiter process id so that thread is woken and
    // its `Waiter` entry (which holds an `Arc<Thread>`) is removed. FreeBSD `kern_wait4` and
    // NetBSD `kern_lwp.c` abort a killed waiter's sleep queue at process exit.
    for other in processes() {
        if Arc::ptr_eq(&other, process) {
            continue;
        }
        let _ = other.waiters.wake_process(process.id);
        let _ = other.child_waiters.wake_process(process.id);
        let _ = other.signal_waiters.wake_process(process.id);
        let _ = other.scheme_waiters.wake_process(process.id);
    }
    // Object queues (pipe, duplex, memory page waiters, console, record locks) are not reachable
    // from the dying handle table; the global queue registry reaches every live queue.
    let _ = crate::sched::wait::WaitQueue::wake_all_queues_for_process(process.id);
    for irq in irqs {
        if !irq_held_elsewhere(process, &irq) {
            irq.retire();
        }
    }
    let mut retired_pipe: Option<Arc<crate::process::pipe::PipeEnd>> = None;
    for pipe in pipes {
        let duplicate = retired_pipe
            .as_ref()
            .is_some_and(|retired| retired.same_pipe(&pipe));
        if !duplicate && !pipe_held_elsewhere(process, &pipe) {
            pipe.retire();
            retired_pipe = Some(pipe);
        }
    }
    let mut retired_duplex: Option<Arc<crate::process::duplex::DuplexEnd>> = None;
    for duplex in duplexes {
        let duplicate = retired_duplex
            .as_ref()
            .is_some_and(|retired| retired.same_pair(&duplex));
        if !duplicate && !duplex_held_elsewhere(process, &duplex) {
            duplex.retire();
            retired_duplex = Some(duplex);
        }
    }
}

/// True when a live process other than `owner` still has a handle to the same duplex pair.
fn duplex_held_elsewhere(
    owner: &Arc<Process>,
    duplex: &Arc<crate::process::duplex::DuplexEnd>,
) -> bool {
    let registry = PID_REGISTRY.lock();
    for (id, weak) in registry.iter() {
        if *id == owner.id {
            continue;
        }
        let Some(other) = weak.upgrade() else {
            continue;
        };
        let table = other.handles.lock();
        let mut found = false;
        table.for_each_live(|_, object, _, _| {
            if let Object::Duplex(candidate) = object
                && candidate.same_pair(duplex)
            {
                found = true;
            }
        });
        if found {
            return true;
        }
    }
    false
}

/// True when a live process other than `owner` still has a handle to the same pipe.
fn pipe_held_elsewhere(owner: &Arc<Process>, pipe: &Arc<crate::process::pipe::PipeEnd>) -> bool {
    let registry = PID_REGISTRY.lock();
    for (id, weak) in registry.iter() {
        if *id == owner.id {
            continue;
        }
        let Some(other) = weak.upgrade() else {
            continue;
        };
        let table = other.handles.lock();
        let mut found = false;
        table.for_each_live(|_, object, _, _| {
            if let Object::Pipe(candidate) = object
                && candidate.same_pipe(pipe)
            {
                found = true;
            }
        });
        if found {
            return true;
        }
    }
    false
}

/// True when a live process other than `owner` still has a handle to `irq`.
///
/// An `Arc` retained only by an abandoned syscall frame is not a live owner: the thread can never
/// run again, so the hardware claim must be released even though that reference still exists.
fn irq_held_elsewhere(owner: &Arc<Process>, irq: &Arc<crate::driver::irq::Irq>) -> bool {
    let registry = PID_REGISTRY.lock();
    for (id, weak) in registry.iter() {
        if *id == owner.id {
            continue;
        }
        let Some(other) = weak.upgrade() else {
            continue;
        };
        let table = other.handles.lock();
        let mut found = false;
        table.for_each_live(|_, object, _, _| {
            if let Object::Irq(candidate) = object
                && Arc::ptr_eq(candidate, irq)
            {
                found = true;
            }
        });
        if found {
            return true;
        }
    }
    false
}

/// Set the terminal state exactly once: exit code, wake every waiter, run the teardown.
///
/// Returns false when the process was already terminal (no resurrection, no second teardown).
fn terminate_once(process: &Arc<Process>, code: i32, own_context: bool) -> bool {
    // A process can die without the EXIT syscall (SIGKILL, a fault or OOM). POSIX record locks
    // are owned by the process, so release them on the single teardown path before any waiter
    // can observe a stale owner.
    crate::syscall::record_lock::release_process(process.id);
    // Snapshot the final resident footprint while the address space still exists. This runs
    // before the critical section (the VMA walk allocates) and before `notify_parent`, so the
    // reaped-child maxrss fold sees the completed footprint. The stack fault that forced the
    // earlier removal was the inlined CmdLine parser in `teardown`, now gone.
    process.update_maxrss();
    // The state/wake graph takes the per-CPU scheduler lock through `sched::wake`. The syscall
    // path now runs with IF enabled, so mask interrupts for this lock/wake section; otherwise a
    // timer tick on the same CPU can spin on the scheduler lock held here. The long address-space
    // release below stays interruptible so a target CPU can take a TLB IPI.
    let irq = crate::sync::critical();
    let mut state = process.state.lock();
    if *state == ProcessState::Exited {
        return false;
    }
    // Publish the strong zombie reference before `Exited` becomes visible, so a wait that sees
    // the state can always find the status even if the parent already closed its handle.
    ZOMBIES.lock().push(Arc::clone(process));
    *state = ProcessState::Exited;
    process.exit_code.store(code, Ordering::Release);
    process.exiting.store(true, Ordering::Release);
    process.stop_signal.store(0, Ordering::Release);
    process.continued.store(false, Ordering::Release);
    // A user thread of this process may still be running on another CPU. Ask every other CPU to
    // reschedule so it observes `exiting` at its next scheduling point instead of returning to
    // user mode. The local CPU handles its own threads through the normal exit path.
    crate::sched::request_reschedule_all(crate::sched::smp::percpu::this_cpu());
    reparent_children(process);
    robust::walk_process(process);
    wake_process_waiters(process);
    let _ = process.waiters.wake_all();
    drop(state);
    crate::process::tty::hangup_session_leader(process);
    notify_parent(process, true);
    crate::event::notify_source(Arc::as_ptr(process) as usize as u64);
    drop(irq);
    teardown(process, code, own_context);
    true
}

/// Process-wide `EXIT`: terminate every thread, tear down once, then exit the caller.
///
/// Other threads of the process never run user code again: the scheduler drops any runnable
/// thread of a terminal process, and a woken thread re-checks the flag before returning to user
/// mode.  Since user threads run only on the boot CPU inside a non-preemptible kernel, no other
/// thread is executing while this syscall runs, so the address space can be released here.
pub fn exit_process(code: i32) -> ! {
    if let Some(process) = current() {
        let _ = robust::walk_thread();
        let _ = terminate_once(&process, code, true);
    }
    crate::sched::exit_thread(code)
}

/// Authorized termination of `target` by a caller holding a `SIGNAL` process handle.
///
/// A terminal process returns `ESRCH` (already exiting); the caller's rights check happens in
/// the syscall layer.
pub fn terminate(target: &Arc<Process>, code: i32) -> Result<(), Errno> {
    if terminate_once(target, code, false) {
        Ok(())
    } else {
        Err(Errno::ESRCH)
    }
}

/// Reap a process that was created but never started.
///
/// Such a process has no threads and can never run, so `PROC_WAIT` on it completes at once; the
/// terminal state and teardown are the same as any other exit (`Created` -> `Exited`).
pub fn reap_unstarted(target: &Arc<Process>) -> Result<i32, Errno> {
    let code = {
        let state = target.state.lock();
        if *state != ProcessState::Created {
            return Err(Errno::EBUSY);
        }
        target.exit_code.load(Ordering::Acquire)
    };
    let _ = terminate_once(target, code, false);
    Ok(code)
}

/// Mark the current process exited and release its resources (last-thread path).
///
/// PID 1 follows the C5 test-exit protocol and never returns; a child process returns to the
/// scheduler after its address space has been switched away from.
pub fn notify_exit(code: i32) {
    let Some(process) = current() else {
        kprintln!("init exited ({code})");
        if crate::log::test_exit() {
            crate::boot::exit_qemu(if code == 0 { 0x10 } else { 0x11 });
        }
        crate::arch::halt()
    };
    let _ = terminate_once(&process, code, true);
}

/// Terminate the calling process (`EXIT`).
pub fn exit_current(code: i32) -> ! {
    exit_process(code)
}

/// Try to resolve a user page fault. Returns true when the faulting instruction can be retried.
///
/// This is the legacy entry point for the usercopy fixup path and the aarch64 vectors; both
/// reach it for a write or for the only arch-specific COW resolution they support. The x86-64
/// exception entry uses [`handle_page_fault_access`] so a read-only VMA write is rejected
/// instead of paged in.
pub fn handle_page_fault(cr2: u64) -> bool {
    resolve_page_fault(cr2, true, false)
}

/// Resolve a user page fault with the faulting access kind.
///
/// The address-space lock is released before a pager PageIn wait (C18); the pager service
/// re-acquires it after the supply to map the page.
#[cfg(target_arch = "x86_64")]
pub fn handle_page_fault_access(cr2: u64, write: bool) -> bool {
    resolve_page_fault(cr2, write, true)
}

/// Page-fault handlers run through an interrupt gate, so the CPU clears IF on entry. The
/// address-space lock is a blocking mutex and may wait for another CPU; keep the handler
/// interruptible so a target CPU can still take an SMP TLB IPI. The original IF state is
/// restored when the handler returns, and the per-CPU diagnostic is cleared on every exit.
#[cfg(target_arch = "x86_64")]
struct PageFaultGuard(u64);

#[cfg(target_arch = "x86_64")]
impl PageFaultGuard {
    fn new() -> Self {
        let flags = crate::arch::x86_64::ap_start::save_and_disable_local_interrupts();
        crate::arch::x86_64::ap_start::enable_local_interrupts();
        crate::sched::smp::percpu::note_page_fault(true);
        Self(flags)
    }
}

#[cfg(target_arch = "x86_64")]
impl Drop for PageFaultGuard {
    fn drop(&mut self) {
        crate::sched::smp::percpu::note_page_fault(false);
        crate::arch::x86_64::ap_start::restore_local_interrupts(self.0);
    }
}

fn resolve_page_fault(cr2: u64, write: bool, interruptible: bool) -> bool {
    let _ = interruptible;
    #[cfg(target_arch = "x86_64")]
    let _fault = if interruptible {
        Some(PageFaultGuard::new())
    } else {
        // A usercopy fixup still holds the kernel-side critical state; keep it non-preemptible.
        None
    };
    crate::sched::smp::percpu::note_page_fault_phase(1);
    let Some(process) = current() else {
        return false;
    };
    let outcome = {
        crate::sched::smp::percpu::note_page_fault_phase(2);
        let mut guard = process.address_space.lock();
        crate::sched::smp::percpu::note_page_fault_phase(3);
        // Only the lock wait needs to be interruptible. Keep the actual fault resolution
        // non-preemptible while the address-space state is locked, then restore IF after the
        // lock is released.
        let hold = crate::sync::critical();
        let outcome = match guard.as_mut() {
            Some(address_space) => {
                crate::sched::smp::percpu::note_page_fault_phase(4);
                address_space.handle_fault(cr2, write)
            }
            None => {
                drop(guard);
                drop(hold);
                return false;
            }
        };
        drop(guard);
        drop(hold);
        outcome
    };
    crate::sched::smp::percpu::note_page_fault_phase(5);
    match outcome {
        Ok(()) => true,
        #[cfg(target_arch = "x86_64")]
        Err(crate::memory::uspace::FaultError::Pager(fault)) => {
            crate::sched::smp::percpu::note_page_fault_phase(6);
            let resolved = crate::memory::pager::service(&process, fault);
            crate::sched::smp::percpu::note_page_fault_phase(7);
            resolved
        }
        #[cfg(not(target_arch = "x86_64"))]
        Err(crate::memory::uspace::FaultError::Pager(_)) => false,
        Err(crate::memory::uspace::FaultError::Errno(_)) => false,
    }
}

/// Terminate the current process because of a user page fault.
pub fn page_fault(cr2: u64, rip: u64) -> ! {
    let (path, pid, ppid, sig_entry) = match current() {
        Some(process) => {
            let ids = *process.ids.lock();
            let sig_entry = crate::sched::current_thread()
                .map_or(0, |thread| thread.signal_entry.load(Ordering::Acquire));
            (process.path.clone(), process.id, ids.ppid, sig_entry)
        }
        None => (String::new(), 0, 0, 0),
    };
    // K495 rev180: the fault address alone does not identify the process or the instruction that
    // caused it. The SIG_SETUP entry is the caller address the kernel would resume a delivered
    // signal at; printing it with the fault RIP distinguishes a bad child exit from a bad signal
    // resume (Ken's SIG_SETUP class).
    kprintln!(
        "process: {} pid={} ppid={} killed by page fault at 0x{:x} rip=0x{:x} sig_entry=0x{:x}",
        path,
        pid,
        ppid,
        cr2,
        rip,
        sig_entry
    );
    exit_current(-11)
}

/// Terminate the current process because of another user exception.
pub fn user_exception(vector: usize) -> ! {
    let path = current().map_or_else(String::new, |process| process.path.clone());
    kprintln!("process: {} killed by exception {}", path, vector);
    exit_current(-11)
}

/// A syscall returned to a non-canonical user instruction pointer.
#[cfg(target_arch = "x86_64")]
pub fn bad_sysret(rip: u64) -> ! {
    let path = current().map_or_else(String::new, |process| process.path.clone());
    kprintln!(
        "process: {} killed by non-canonical sysret rip 0x{:x}",
        path,
        rip
    );
    exit_current(-11)
}

const _: () = {
    assert!(PAGE == 4096);
    assert!(USER_STACK_SIZE == 256 * 1024);
};

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
