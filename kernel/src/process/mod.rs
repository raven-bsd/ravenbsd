// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process creation, the first `/init` process, and user-mode fault handling.
//!
//! S08 has one thread per process. The process owns its address space and handle table; the
//! single thread owns the kernel stack used by syscall and trap entry.

pub mod handle;
pub mod thread;

use core::fmt;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::types::prot;
use raven_elf::{Elf, ElfError, LoadMode, Machine, MapStep, StepKind};
use raven_handles::HandleTable;
use raven_rmm::{MapError, PageFlags, PageSize, PhysAddr, VirtAddr};
use raven_vma::Backing;
use spin::Mutex;

use crate::arch::x86_64::{syscall, tsc};
use crate::cmdline::CmdLine;
use crate::initfs;
use crate::ipc::capture::CaptureSlots;
use crate::ipc::model::Namespace;
use crate::ipc::{self, PendingRequest, SchemeServer};
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
    /// Lifecycle state, guarded so `PROC_WAIT` cannot lose an exit wakeup.
    pub state: Mutex<ProcessState>,
    /// Threads blocked in `PROC_WAIT` on this process.
    pub waiters: WaitQueue,
    /// Visible scheme names (per-process namespace).
    pub namespace: Mutex<Namespace>,
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
        state: Mutex::new(ProcessState::Running),
        waiters: WaitQueue::new(),
        namespace: Mutex::new(Namespace::root()),
        schemes: Mutex::new(alloc::vec::Vec::new()),
        scheme_queue: Mutex::new(VecDeque::new()),
        scheme_waiters: WaitQueue::new(),
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        capture_slots: Mutex::new(CaptureSlots::new()),
    }))
}

/// Create an empty process for `PROC_CREATE`, with the well-known handles installed.
pub fn new_empty_process() -> Result<Arc<Process>, Errno> {
    let address_space = AddressSpace::new().map_err(|_| Errno::ENOMEM)?;
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
        state: Mutex::new(ProcessState::Created),
        waiters: WaitQueue::new(),
        namespace: Mutex::new(namespace),
        schemes: Mutex::new(alloc::vec::Vec::new()),
        scheme_queue: Mutex::new(VecDeque::new()),
        scheme_waiters: WaitQueue::new(),
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        client_requests: Mutex::new(alloc::vec::Vec::new()),
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

    /// Mark the process exited, publish the code and wake every waiter exactly once.
    pub fn mark_exited(&self, code: i32) {
        self.exit_code.store(code, Ordering::Release);
        let mut state = self.state.lock();
        if *state == ProcessState::Exited {
            return;
        }
        *state = ProcessState::Exited;
        let _ = self.waiters.wake_all();
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
    let elf = Elf::parse(data, Machine::X86_64, USER_MIN..USER_END).map_err(ProcessError::Elf)?;
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
        state: Mutex::new(ProcessState::Running),
        waiters: WaitQueue::new(),
        namespace: Mutex::new(Namespace::root()),
        schemes: Mutex::new(alloc::vec::Vec::new()),
        scheme_queue: Mutex::new(VecDeque::new()),
        scheme_waiters: WaitQueue::new(),
        active_requests: Mutex::new(alloc::vec::Vec::new()),
        client_requests: Mutex::new(alloc::vec::Vec::new()),
        capture_slots: Mutex::new(CaptureSlots::new()),
    });
    handle::install_initial(&mut process.handles.lock(), &process, true)
        .map_err(ProcessError::Handle)?;
    crate::sched::init().map_err(ProcessError::Sched)?;
    let thread = crate::sched::new_user_thread(&process, elf.entry(), user_stack, user_stack)
        .map_err(ProcessError::Sched)?;
    syscall::init();

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
    Ok(())
}

fn map_step(
    mapper: &mut raven_rmm::Mapper<
        '_,
        paging::KernelFrameAllocator,
        paging::KernelPhysMemory,
        paging::KernelTlb,
    >,
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

    let mut state = tsc::read() ^ 0x9E37_79B9_7F4A_7C15;
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

/// Mark the current process exited, release its address space and wake waiters.
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
    process.mark_exited(code);
    // Fail pending scheme requests and remove the registry entries while the server page tables
    // still exist, so capture mappings can be removed before the address space is released.
    ipc::on_process_exit(&process);
    let address_space = process.address_space.lock().take();
    if let Some(address_space) = address_space {
        address_space.release();
    }
    if process.id == 1 {
        kprintln!("init exited ({code})");
        if crate::log::test_exit() {
            crate::boot::exit_qemu(if code == 0 { 0x10 } else { 0x11 });
        }
        crate::arch::halt()
    }
    kprintln!("process: {} exited ({code})", process.path);
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
}

/// Terminate the calling process (`EXIT`).
pub fn exit_current(code: i32) -> ! {
    crate::sched::exit_thread(code)
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
