// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process creation, the first `/init` process, and user-mode fault handling.
//!
//! S08 has one thread per process. The process owns its address space and handle table; the
//! single thread owns the kernel stack used by syscall and trap entry.

pub mod handle;
pub mod thread;

use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};

#[cfg(target_arch = "x86_64")]
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;

use raven_abi::errno::Errno;
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{USER_END, USER_MIN};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::prot;
use raven_elf::{Elf, ElfError, LoadMode, Machine, MapStep, StepKind};
use raven_handles::HandleTable;
use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};
use raven_vma::Backing;
use spin::Mutex;

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
use crate::ipc;
#[cfg(target_arch = "x86_64")]
use crate::ipc::capture::CaptureSlots;
#[cfg(target_arch = "x86_64")]
use crate::ipc::model::Namespace;
#[cfg(target_arch = "x86_64")]
use crate::ipc::{PendingRequest, SchemeServer};
use crate::kprintln;
use crate::memory::aspace::AddressSpace;
use crate::memory::uspace::UserAddressSpace;
use crate::memory::{frame, paging};
use crate::sched::wait::WaitQueue;

use self::handle::Object;

/// Page size used by the user loader and stack.
const PAGE: u64 = 4096;
/// Size of the user stack mapping.
///
/// S11's controller uses `raven_rt::spawn`, which builds a child stack in a 64 KiB buffer;
/// 128 KiB leaves room for that buffer and the call frames.
const USER_STACK_SIZE: u64 = 128 * 1024;
/// Guard page at the top of the user range.
const USER_GUARD: u64 = 4096;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

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

/// A process.
pub struct Process {
    /// Process id (PID 1 for the first process).
    pub id: u64,
    /// Path used to start the process.
    pub path: String,
    /// User address space (`None` for the kernel process or an exited process).
    pub address_space: Mutex<Option<UserAddressSpace>>,
    /// Handle table.
    pub handles: Mutex<HandleTable<Object>>,
    /// Exit code once the process exits.
    pub exit_code: AtomicI32,
    /// Live thread count.
    pub threads: AtomicU32,
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
    #[cfg(target_arch = "x86_64")]
    pub namespace: Mutex<Namespace>,
    /// Scheme names registered by this process.
    #[cfg(target_arch = "x86_64")]
    pub schemes: Mutex<alloc::vec::Vec<Arc<SchemeServer>>>,
    /// Packets waiting for this process's `RECV`.
    #[cfg(target_arch = "x86_64")]
    pub scheme_queue: Mutex<VecDeque<Arc<PendingRequest>>>,
    /// Server threads blocked in `RECV`.
    #[cfg(target_arch = "x86_64")]
    pub scheme_waiters: WaitQueue,
    /// Requests received by this process and not yet replied to.
    #[cfg(target_arch = "x86_64")]
    pub active_requests: Mutex<alloc::vec::Vec<Arc<PendingRequest>>>,
    /// Requests this process is waiting for as a client.
    #[cfg(target_arch = "x86_64")]
    pub client_requests: Mutex<alloc::vec::Vec<Arc<PendingRequest>>>,
    /// Capture-window slots owned by this process as a server.
    #[cfg(target_arch = "x86_64")]
    pub capture_slots: Mutex<CaptureSlots>,
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
    /// The init path does not fit the stack string area.
    PathTooLong,
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
            ProcessError::PathTooLong => f.write_str("init path is too long"),
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

/// Create the kernel process that owns kernel threads.
pub fn new_kernel_process() -> Result<Arc<Process>, Errno> {
    Ok(Arc::new(Process {
        id: 0,
        path: String::from("/kernel"),
        address_space: Mutex::new(None),
        handles: Mutex::new(HandleTable::new()),
        exit_code: AtomicI32::new(0),
        threads: AtomicU32::new(0),
        exiting: AtomicBool::new(false),
        torn_down: AtomicBool::new(false),
        timer_count: Arc::new(AtomicU32::new(0)),
        dma_bytes: Arc::new(AtomicU64::new(0)),
        state: Mutex::new(ProcessState::Running),
        waiters: WaitQueue::new(),
        #[cfg(target_arch = "x86_64")]
        namespace: Mutex::new(Namespace::root()),
        #[cfg(target_arch = "x86_64")]
        schemes: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        scheme_queue: Mutex::new(VecDeque::new()),
        #[cfg(target_arch = "x86_64")]
        scheme_waiters: WaitQueue::new(),
        #[cfg(target_arch = "x86_64")]
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        capture_slots: Mutex::new(CaptureSlots::new()),
    }))
}

/// Create an empty process for `PROC_CREATE`, with the well-known handles installed.
pub fn new_empty_process() -> Result<Arc<Process>, Errno> {
    let address_space = AddressSpace::new().map_err(|_| Errno::ENOMEM)?;
    #[cfg(target_arch = "x86_64")]
    let namespace = if let Some(parent) = current() {
        let snapshot = parent.namespace.lock().clone();
        snapshot.child_snapshot(&ipc::registry::names())
    } else {
        Namespace::root()
    };
    let process = Arc::new(Process {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        path: String::from("child"),
        address_space: Mutex::new(Some(UserAddressSpace::new(address_space))),
        handles: Mutex::new(HandleTable::new()),
        exit_code: AtomicI32::new(0),
        threads: AtomicU32::new(0),
        exiting: AtomicBool::new(false),
        torn_down: AtomicBool::new(false),
        timer_count: Arc::new(AtomicU32::new(0)),
        dma_bytes: Arc::new(AtomicU64::new(0)),
        state: Mutex::new(ProcessState::Created),
        waiters: WaitQueue::new(),
        #[cfg(target_arch = "x86_64")]
        namespace: Mutex::new(namespace),
        #[cfg(target_arch = "x86_64")]
        schemes: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        scheme_queue: Mutex::new(VecDeque::new()),
        #[cfg(target_arch = "x86_64")]
        scheme_waiters: WaitQueue::new(),
        #[cfg(target_arch = "x86_64")]
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        capture_slots: Mutex::new(CaptureSlots::new()),
    });
    handle::install_initial(&mut process.handles.lock(), &process, false)?;
    Ok(process)
}

impl Process {
    /// Account for one new thread.
    pub fn thread_started(&self) {
        self.threads.fetch_add(1, Ordering::AcqRel);
    }

    /// Account for one exiting thread; true when it was the last one.
    pub fn thread_exited(&self) -> bool {
        self.threads.fetch_sub(1, Ordering::AcqRel) <= 1
    }
}

/// True when the initfs contains the requested init path.
pub fn init_available(cmdline: &CmdLine) -> bool {
    let path = cmdline.get("init").unwrap_or("/init");
    initfs::lookup(path).is_some()
}

/// Create PID 1 from the initfs and enter ring 3. On success this never returns.
pub fn start_init(cmdline: &CmdLine) -> Result<(), ProcessError> {
    let path = cmdline.get("init").unwrap_or("/init");
    let entry = initfs::lookup(path).ok_or(ProcessError::NoInit)?;
    let data = entry.data;
    let elf = Elf::parse(data, TARGET_MACHINE, USER_MIN..USER_END).map_err(ProcessError::Elf)?;
    if elf.interp().is_some() {
        return Err(ProcessError::Interpreter);
    }

    let address_space = AddressSpace::new().map_err(ProcessError::Map)?;
    let mut user_address_space = UserAddressSpace::new(address_space);
    load_segments(&mut user_address_space, &elf, data)?;
    let user_stack = build_stack(&mut user_address_space, path, elf.entry(), &elf)?;

    let process = Arc::new(Process {
        id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
        path: String::from(path),
        address_space: Mutex::new(Some(user_address_space)),
        handles: Mutex::new(HandleTable::new()),
        exit_code: AtomicI32::new(0),
        threads: AtomicU32::new(0),
        exiting: AtomicBool::new(false),
        torn_down: AtomicBool::new(false),
        timer_count: Arc::new(AtomicU32::new(0)),
        dma_bytes: Arc::new(AtomicU64::new(0)),
        state: Mutex::new(ProcessState::Running),
        waiters: WaitQueue::new(),
        #[cfg(target_arch = "x86_64")]
        namespace: Mutex::new(Namespace::root()),
        #[cfg(target_arch = "x86_64")]
        schemes: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        scheme_queue: Mutex::new(VecDeque::new()),
        #[cfg(target_arch = "x86_64")]
        scheme_waiters: WaitQueue::new(),
        #[cfg(target_arch = "x86_64")]
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        #[cfg(target_arch = "x86_64")]
        capture_slots: Mutex::new(CaptureSlots::new()),
    });
    handle::install_initial(&mut process.handles.lock(), &process, true)
        .map_err(ProcessError::Handle)?;
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
) -> Result<(), ProcessError> {
    let plan = raven_elf::plan_load(elf, PAGE, LoadMode::Copy).map_err(ProcessError::Elf)?;
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

fn build_stack(
    address_space: &mut UserAddressSpace,
    path: &str,
    entry: u64,
    elf: &Elf<'_>,
) -> Result<u64, ProcessError> {
    let stack_base = USER_END
        .checked_sub(USER_STACK_SIZE + USER_GUARD)
        .ok_or(ProcessError::BadAddress)?;
    let stack_top = USER_END
        .checked_sub(USER_GUARD)
        .ok_or(ProcessError::BadAddress)?;
    let sp0 = stack_top
        .checked_sub(0x200)
        .ok_or(ProcessError::BadAddress)?;

    let mut buffer = vec![0u8; USER_STACK_SIZE as usize];
    let string_offset = USER_STACK_SIZE
        .checked_sub(0x100)
        .ok_or(ProcessError::BadAddress)?;
    let random_offset = USER_STACK_SIZE
        .checked_sub(0x80)
        .ok_or(ProcessError::BadAddress)?;
    let sp_offset = USER_STACK_SIZE
        .checked_sub(0x200)
        .ok_or(ProcessError::BadAddress)?;

    let path_bytes = path.as_bytes();
    if path_bytes.len() >= 0x100 {
        return Err(ProcessError::PathTooLong);
    }
    let string_start = usize::try_from(string_offset).map_err(|_| ProcessError::BadAddress)?;
    let string_end = string_start
        .checked_add(path_bytes.len())
        .and_then(|end| end.checked_add(1))
        .ok_or(ProcessError::BadAddress)?;
    let string_slice = buffer
        .get_mut(string_start..string_end)
        .ok_or(ProcessError::BadAddress)?;
    string_slice.fill(0);
    if let Some(destination) = string_slice.get_mut(..path_bytes.len()) {
        destination.copy_from_slice(path_bytes);
    }

    let random = random16();
    let random_start = usize::try_from(random_offset).map_err(|_| ProcessError::BadAddress)?;
    let random_end = random_start
        .checked_add(random.len())
        .ok_or(ProcessError::BadAddress)?;
    if let Some(slot) = buffer.get_mut(random_start..random_end) {
        slot.copy_from_slice(&random);
    }

    let mut cursor = usize::try_from(sp_offset).map_err(|_| ProcessError::BadAddress)?;
    write_word(&mut buffer, &mut cursor, 1)?;
    write_word(
        &mut buffer,
        &mut cursor,
        stack_base
            .checked_add(string_offset)
            .ok_or(ProcessError::BadAddress)?,
    )?;
    write_word(&mut buffer, &mut cursor, 0)?;
    write_word(&mut buffer, &mut cursor, 0)?;
    write_word(&mut buffer, &mut cursor, 6)?;
    write_word(&mut buffer, &mut cursor, PAGE)?;
    write_word(&mut buffer, &mut cursor, 9)?;
    write_word(&mut buffer, &mut cursor, entry)?;
    write_word(&mut buffer, &mut cursor, 3)?;
    write_word(&mut buffer, &mut cursor, elf.phdr_vaddr().unwrap_or(0))?;
    write_word(&mut buffer, &mut cursor, 5)?;
    write_word(&mut buffer, &mut cursor, elf.phnum() as u64)?;
    write_word(&mut buffer, &mut cursor, 25)?;
    write_word(
        &mut buffer,
        &mut cursor,
        stack_base
            .checked_add(random_offset)
            .ok_or(ProcessError::BadAddress)?,
    )?;
    write_word(&mut buffer, &mut cursor, 0)?;
    write_word(&mut buffer, &mut cursor, 0)?;

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

fn write_word(buffer: &mut [u8], cursor: &mut usize, value: u64) -> Result<(), ProcessError> {
    let end = (*cursor).checked_add(8).ok_or(ProcessError::BadAddress)?;
    let slot = buffer
        .get_mut(*cursor..end)
        .ok_or(ProcessError::BadAddress)?;
    slot.copy_from_slice(&value.to_le_bytes());
    *cursor = end;
    Ok(())
}

fn random16() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    #[cfg(target_arch = "x86_64")]
    {
        let features = core::arch::x86_64::__cpuid(1);
        if features.ecx & (1 << 30) != 0 {
            let mut chunk_index = 0usize;
            while chunk_index < 2 {
                let mut attempt: u32 = 0;
                let mut filled = false;
                while attempt < 10 {
                    let value: u64;
                    let ok: u8;
                    // SAFETY: `rdrand` is available per CPUID.1:ECX[30] and has no memory side
                    // effects. `setc` records the success flag.
                    unsafe {
                        core::arch::asm!(
                            "rdrand {value}",
                            "setc {ok}",
                            value = out(reg) value,
                            ok = out(reg_byte) ok,
                            options(nomem, nostack),
                        );
                    }
                    if ok != 0 {
                        let start = chunk_index.saturating_mul(8);
                        if let Some(slot) = bytes.get_mut(start..start.saturating_add(8)) {
                            slot.copy_from_slice(&value.to_le_bytes());
                        }
                        filled = true;
                        break;
                    }
                    attempt = attempt.saturating_add(1);
                }
                if !filled {
                    break;
                }
                chunk_index = chunk_index.saturating_add(1);
            }
            if chunk_index == 2 {
                return bytes;
            }
        }
    }

    let mut state = entropy_seed() ^ 0x9E37_79B9_7F4A_7C15;
    let mut index = 0usize;
    while index < bytes.len() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let word = state.to_le_bytes();
        let end = index.saturating_add(8).min(bytes.len());
        if let Some(slot) = bytes.get_mut(index..end) {
            let copy = end.saturating_sub(index);
            if let Some(source) = word.get(..copy) {
                slot.copy_from_slice(source);
            }
        }
        index = end;
    }
    bytes
}

/// Non-cryptographic fallback entropy: the architecture's free-running counter.
#[cfg(target_arch = "x86_64")]
fn entropy_seed() -> u64 {
    tsc::read()
}

/// Non-cryptographic fallback entropy: the generic-timer physical counter.
#[cfg(target_arch = "aarch64")]
fn entropy_seed() -> u64 {
    timer::read_counter()
}

/// The ELF machine of the running kernel.
#[cfg(target_arch = "x86_64")]
const TARGET_MACHINE: Machine = Machine::X86_64;
/// The ELF machine of the running kernel.
#[cfg(target_arch = "aarch64")]
const TARGET_MACHINE: Machine = Machine::Aarch64;

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
fn teardown(process: &Arc<Process>, code: i32, own_context: bool) {
    if process.torn_down.swap(true, Ordering::AcqRel) {
        return;
    }
    // Fail pending scheme requests and remove the registry entries while the server page tables
    // still exist, so capture mappings can be removed before the address space is released.
    #[cfg(target_arch = "x86_64")]
    ipc::on_process_exit(process);
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
    // survives elsewhere (for example on the abandoned stack of its dead thread).
    *process.handles.lock() = HandleTable::new();
    if process.id == 1 {
        kprintln!("init exited ({code})");
        if crate::log::test_exit() {
            crate::boot::exit_qemu(if code == 0 { 0x10 } else { 0x11 });
        }
        crate::arch::halt()
    }
    kprintln!("process: {} exited ({code})", process.path);
    // The per-exit evidence prints belong to the dying thread's own context; a foreign teardown
    // (reaping an unstarted process, terminating another) must return to its caller without
    // them.
    if !own_context {
        return;
    }
    // S24 leak evidence: the free-frame count after every child exit.
    if CmdLine::parse(paging::cmdline())
        .get("s24-framecheck")
        .is_some()
    {
        kprintln!("s24: frames free={}", frame::stats().free_bytes);
    }
    // S25 handle-transfer leak evidence: the dying process's live handle count. A transferred
    // handle that survived process death, or a slot leaked by the transfer path, drifts this
    // number across repeated spawn/exit cycles.
    if CmdLine::parse(paging::cmdline())
        .get("s25-handlecheck")
        .is_some()
    {
        kprintln!("s25: exit handles={}", process.handles.lock().len());
    }
    // S21b DMA leak evidence: the free-frame count after every child exit. Consecutive exits
    // print the same number when allocation and release are balanced.
    if CmdLine::parse(paging::cmdline())
        .get("s21-framecheck")
        .is_some()
    {
        kprintln!("s21: frames free={}", frame::stats().free_bytes);
    }
    // S36 stack-reuse evidence: the fresh-allocation and cached-stack counters must return to
    // baseline across repeated spawn/exit cycles.
    if CmdLine::parse(paging::cmdline())
        .get("s36-stackcheck")
        .is_some()
    {
        let (heap_used, _) = crate::memory::heap::stats();
        kprintln!(
            "s36: stacks fresh={} cached={} heap={}",
            crate::sched::thread::stack_fresh(),
            crate::sched::thread::stack_cached(),
            heap_used
        );
    }
}

/// Set the terminal state exactly once: exit code, wake every waiter, run the teardown.
///
/// Returns false when the process was already terminal (no resurrection, no second teardown).
fn terminate_once(process: &Arc<Process>, code: i32, own_context: bool) -> bool {
    let mut state = process.state.lock();
    if *state == ProcessState::Exited {
        return false;
    }
    *state = ProcessState::Exited;
    process.exit_code.store(code, Ordering::Release);
    process.exiting.store(true, Ordering::Release);
    let _ = process.waiters.wake_all();
    drop(state);
    crate::event::notify_all();
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
pub fn handle_page_fault(cr2: u64) -> bool {
    let Some(process) = current() else {
        return false;
    };
    let mut address_space = process.address_space.lock();
    let Some(address_space) = address_space.as_mut() else {
        return false;
    };
    address_space.handle_fault(cr2).is_ok()
}

/// Terminate the current process because of a user page fault.
pub fn page_fault(cr2: u64) -> ! {
    let path = current().map_or_else(String::new, |process| process.path.clone());
    kprintln!("process: {} killed by page fault at 0x{:x}", path, cr2);
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
    assert!(USER_STACK_SIZE == 128 * 1024);
};
