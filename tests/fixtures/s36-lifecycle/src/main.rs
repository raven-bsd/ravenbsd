// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S36 process-lifecycle fixture.
//!
//! The controller (no argv role) drives the six acceptance scenarios:
//!
//! 1. `twomain`: a two-thread child whose helper thread exits with `THREAD_EXIT` (process alive),
//!    then a long-sleeping second helper, then `EXIT` kills every thread and the parent reaps.
//! 2. 10,000 `THREAD_CREATE`/`THREAD_EXIT` cycles in the controller, bracketed by `marker`
//!    children whose exits print the kernel stack/heap counters (`s36-stackcheck`).
//! 3. `THREAD_CREATE` into a terminated child must fail with `ESRCH` (no resurrection).
//! 4. a `PROC_CREATE` child that is never started is reaped by `PROC_WAIT`.
//! 5. `PROC_TERMINATE` stops a running child; a handle without `SIGNAL` gets `EPERM`; a stale
//!    handle gets `EBADF`; a second terminate gets `ESRCH`.
//! 6. a terminated server's scheme name can be registered again immediately.
//! 7. a child exits while the parent is blocked in `PROC_WAIT_EX`/`PROC_WAIT` with a registered
//!    signal entry; both waits return the child status, not `EINTR`.
//! 8. a caught signal delivered while the parent is blocked still returns `EINTR` (no
//!    suppression); a child-exit wake alone does not interrupt.
//!
//! Roles: `marker` (exit 0), `twomain`, `sleep` (sleep forever), `delayed` (exit 42 after a
//! short sleep), `server` (register scheme `s36`, then sleep).

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::signal::{SIG_SETUP, SIG_SETUP_ABS, SIGRETURN};
use raven_abi::syscall::Syscall;
use raven_abi::types::futex::{WAIT as FUTEX_WAIT, WAKE as FUTEX_WAKE};
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;
use raven_proto_fs::open::{WIRE_OPEN_CREATE, WIRE_OPEN_READ, WIRE_OPEN_WRITE};
use raven_proto_fs::{Call, CallRequest, OpenFlags};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Client;

entry!(main);

const PAGE: usize = 4096;
/// Raw `proc_wait_ex(idtype, id, options, out_status) -> pid`.
const PROC_WAIT_EX: usize = 0x48;
/// Raw `proc_signal(target, signo) -> 0`.
const PROC_SIGNAL: usize = 0x4A;
/// Raw `proc_sigrecv_ex(out, flags, deliverable) -> signo`.
const PROC_SIGRECV_EX: usize = 0x85;
/// `PROC_SIGRECV_EX` flag: select only signals in the deliverable mask.
const SIGRECV_DELIVERABLE: usize = 2;
/// `SIGUSR1`.
const SIGUSR1: usize = 10;
/// Raw `proc_handle_info(handle) -> pid`.
const PROC_HANDLE_INFO: usize = 0x3E;
/// Raw `proc_pid_info(pid, out, count) -> 0` (K391 words at byte 328).
const PROC_PID_INFO: usize = 0x3A;
/// Raw `futex(addr, op, val, timeout_ns)`.
const FUTEX: usize = 0x22;
/// Terminate-while-waiting cycles in the timeout-retention row.
const TIMEOUT_CYCLES: usize = 40;
/// Raw `sig_setup(entry, ctx, flags)`.
/// Raw `sigreturn()`.
/// Kernel `P_PID` wait id type.
const P_PID: usize = 1;
/// The registered signal context area.
const SIGNAL_CONTEXT_SIZE: usize = 4096;
const INITFS_BASE: usize = 0x1000_0000;
const STACK_SCRATCH: usize = 0x2000_0000;
const STACK_SIZE: usize = 64 * 1024;
const STACK_TOP: usize = (USER_END as usize) - PAGE;
const STACK_BASE: usize = STACK_TOP - STACK_SIZE;

/// Cycles in the thread create/exit churn.
const CYCLES: usize = 10_000;
/// Full process/address-space/stack lifecycles in the bounded session churn.
const SESSIONS: usize = 32;
/// Futex wake iterations in the lock/time churn.
const LOCK_WAKES: usize = 5_000;
/// Futex timeout iterations in the lock/time churn.
const LOCK_TIMEOUTS: usize = 1_000;
/// One futex timeout.
const LOCK_TIMEOUT_NS: u64 = 1_000_000;
/// Delay between the worker arming and the wake.
const LOCK_DELAY_NS: u64 = 1_000_000;
/// Fork/exec/exit cycles with a known mapped address space in the VMA churn.
const VMA_CYCLES: usize = 100;
/// Four VMAs per child; the large variant maps one MiB total.
const VMA_REGIONS: usize = 4;
const VMA_BASE: usize = 0x3000_0000;
const VMA_REGION_GAP: usize = 0x10_0000;
const VMA_LARGE: usize = 256 * 1024;
/// vfsd pager map/unmap cycles in the red frame row.
const MAP_CYCLES: usize = 1000;
/// Fixed scratch address for the vfsd pager chunk.
const MAP_SCRATCH: usize = 0x3000_0000;
/// User stacks for the fixture's extra threads (16 KiB each, one helper live at a time).
const HELPER_STACK_SIZE: usize = 16 * 1024;

static CYCLE_DONE: AtomicBool = AtomicBool::new(false);
static HELPER_FLAG: AtomicU32 = AtomicU32::new(0);
static LOCK_FUTEX: AtomicU32 = AtomicU32::new(0);
static LOCK_TIMEOUT: AtomicU32 = AtomicU32::new(0);
static LOCK_READY: AtomicU32 = AtomicU32::new(0);
static LOCK_DONE: AtomicU32 = AtomicU32::new(0);
static LOCK_STOP: AtomicBool = AtomicBool::new(false);

#[repr(align(16))]
struct HelperStack(#[allow(dead_code)] [u8; HELPER_STACK_SIZE]);

static mut CYCLE_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);
static mut TW_HELPER_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);
static mut TW_SLEEP_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);
static mut WAIT_SIGNAL_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);
static mut LOCK_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);

#[repr(align(16))]
struct SignalContext(#[allow(dead_code)] [u8; SIGNAL_CONTEXT_SIZE]);

/// Absolute context area for the signal-entry registration used by the wait race.
static mut SIGNAL_CONTEXT: SignalContext = SignalContext([0; SIGNAL_CONTEXT_SIZE]);

/// Top of a helper stack, from its static address (no reference is ever formed).
fn helper_stack_top(stack: *const HelperStack) -> usize {
    let base = stack as usize;
    base.saturating_add(HELPER_STACK_SIZE)
}

#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: the caller passes valid registers for the RavenBSD ABI; the kernel validates every
    // pointer and handle.  `r10` is caller-saved under the syscall convention.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) }
}

fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

fn syscall4(number: usize, a0: usize, a1: usize, a2: usize, a3: usize) -> isize {
    // SAFETY: scalar arguments follow the RavenBSD syscall ABI.
    unsafe { syscall6(number, a0, a1, a2, a3, 0, 0) }
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle(raw: isize) -> Result<Handle, Errno> {
    let value = result(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn thread_exit(code: i32) -> ! {
    let _ = syscall1(Syscall::THREAD_EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

fn fail(message: &str) -> ! {
    println!("s36: failed {message}");
    exit(1)
}

fn nanosleep(ns: u64) {
    let _ = syscall1(Syscall::NANOSLEEP as usize, ns as usize);
}

fn yield_now() {
    let _ = syscall1(Syscall::YIELD as usize, 0);
}

fn handle_close(handle: Handle) {
    let _ = syscall1(Syscall::HANDLE_CLOSE as usize, handle.0 as usize);
}

fn handle_dup(source: Handle, mask: u32) -> Result<Handle, Errno> {
    let raw = syscall2(
        Syscall::HANDLE_DUP as usize,
        source.0 as usize,
        mask as usize,
    );
    handle(raw)
}

fn proc_create() -> Result<Handle, Errno> {
    handle(syscall1(Syscall::PROC_CREATE as usize, 0))
}

fn proc_wait(process: Handle) -> Result<i32, Errno> {
    let mut code = -1i32;
    result(syscall2(
        Syscall::PROC_WAIT as usize,
        process.0 as usize,
        &mut code as *mut i32 as usize,
    ))?;
    Ok(code)
}

fn proc_handle_info(process: Handle) -> Result<i32, Errno> {
    let pid = result(syscall1(PROC_HANDLE_INFO, process.0 as usize))?;
    i32::try_from(pid).map_err(|_| Errno::EINVAL)
}

/// True while `pid` is inside the futex wait, using the K391 `PROC_PID_INFO` words.
fn child_in_futex(pid: i32) -> bool {
    let mut words = [0u64; 61];
    let raw = unsafe {
        syscall4(
            PROC_PID_INFO,
            pid as usize,
            words.as_mut_ptr() as usize,
            words.len(),
            0,
        )
    };
    if raw < 0 {
        return false;
    }
    words[43] == FUTEX as u64 && words[45] != 0
}

/// Block on a private futex with a long timeout; the parent terminates the process while blocked.
fn futex_wait_timeout(word: &AtomicU32, expected: u32, timeout_ns: u64) -> Result<(), Errno> {
    result(unsafe {
        syscall4(
            FUTEX,
            word.as_ptr() as usize,
            0,
            expected as usize,
            timeout_ns as usize,
        )
    })?;
    Ok(())
}

fn proc_wait_ex(idtype: usize, id: i32, status: &mut i32) -> Result<usize, Errno> {
    result(syscall4(
        PROC_WAIT_EX,
        idtype,
        id as isize as usize,
        0,
        status as *mut i32 as usize,
    ))
}

fn proc_terminate(process: Handle, code: i32) -> Result<(), Errno> {
    result(syscall2(
        Syscall::PROC_TERMINATE as usize,
        process.0 as usize,
        code as usize,
    ))
    .map(|_| ())
}

fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    handle(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack_top,
            arg,
            0,
            0,
        )
    })
}

fn scheme_register(name: &[u8]) -> Result<(), Errno> {
    result(syscall3(
        Syscall::SCHEME_REGISTER as usize,
        name.as_ptr() as usize,
        name.len(),
        Handle::SELF_PROC.0 as usize,
    ))
    .map(|_| ())
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    handle(syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0))
}

fn map_memory(
    process: Handle,
    object: Handle,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
) -> Result<usize, Errno> {
    result(unsafe {
        syscall6(
            Syscall::MAP as usize,
            process.0 as usize,
            object.0 as usize,
            offset,
            address,
            len,
            flags,
        )
    })
}

fn unmap(process: Handle, address: usize, len: usize) -> Result<(), Errno> {
    result(syscall3(
        Syscall::UNMAP as usize,
        process.0 as usize,
        address,
        len,
    ))
    .map(|_| ())
}

fn map_initfs_image() -> Result<&'static [u8], Errno> {
    let page = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        0,
        PAGE,
        prot::READ as usize,
    )?;
    // SAFETY: the mapping is one readable page; the initfs header stores total_size at byte 40.
    let total = unsafe { ((page + 40) as *const u64).read_unaligned() as usize };
    let _ = unmap(Handle::SELF_PROC, page, PAGE);
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = map_memory(
        Handle::SELF_PROC,
        Handle::INITFS,
        0,
        INITFS_BASE,
        total,
        map::pack(prot::READ, map::FIXED),
    )?;
    // SAFETY: the kernel mapped the validated initfs bytes at `base` for `total` bytes.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}

/// Spawn this binary with `argv[1] = role` (a child with the ELF mapped and one running thread).
fn spawn_role(initfs: &Initfs<'_>, role: &str) -> Result<Handle, Errno> {
    let entry = initfs.get("/bin/s36-lifecycle").ok_or(Errno::ENOENT)?;
    let elf =
        Elf::parse(entry.data, Machine::X86_64, USER_MIN..USER_END).map_err(|_| Errno::EINVAL)?;
    let child = proc_create()?;
    let steps = plan_load(&elf, PAGE as u64, LoadMode::Strict).map_err(|_| Errno::EINVAL)?;
    for step in steps {
        let flags = map::pack(step.prot as usize, map::FIXED);
        match step.kind {
            StepKind::File { offset } => {
                let offset = entry.data_offset.checked_add(offset).ok_or(Errno::EINVAL)?;
                let offset = usize::try_from(offset).map_err(|_| Errno::EINVAL)?;
                map_memory(
                    child,
                    Handle::INITFS,
                    offset,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::Zero => {
                map_memory(
                    child,
                    Handle::NONE,
                    0,
                    step.vaddr as usize,
                    step.len as usize,
                    flags,
                )?;
            }
            StepKind::CopyTail { .. } => return Err(Errno::EINVAL),
        }
    }
    let stack = memobj_create(STACK_SIZE)?;
    let scratch = map_memory(
        Handle::SELF_PROC,
        stack,
        0,
        STACK_SCRATCH,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    map_memory(
        child,
        stack,
        0,
        STACK_BASE,
        STACK_SIZE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )?;
    let stack_pointer = build_stack(scratch, role.as_bytes());
    let thread = thread_create(child, elf.entry() as usize, stack_pointer, 0)?;
    handle_close(thread);
    unmap(Handle::SELF_PROC, STACK_SCRATCH, STACK_SIZE)?;
    handle_close(stack);
    Ok(child)
}

fn build_stack(base: usize, role: &[u8]) -> usize {
    let stack = STACK_TOP - 0x100;
    let argv0 = STACK_TOP - 0x40;
    let argv1 = argv0 + 2;
    let mut offset = stack - STACK_BASE;
    let words = [
        2u64,
        argv0 as u64,
        argv1 as u64,
        0u64,
        0u64,
        6u64,
        PAGE as u64,
        0u64,
        0u64,
    ];
    for value in words {
        // SAFETY: the shared stack mapping is writable and `offset` stays inside its fixed size.
        unsafe { ((base + offset) as *mut u64).write_unaligned(value.to_le()) };
        offset += 8;
    }
    // SAFETY: the strings are inside the writable shared stack mapping.
    unsafe {
        ((base + STACK_TOP - STACK_BASE - 0x40) as *mut u8).write(b'x');
        ((base + STACK_TOP - STACK_BASE - 0x3f) as *mut u8).write(0);
        let role_start = base + STACK_TOP - STACK_BASE - 0x40 + 2;
        let mut index = 0usize;
        while index < role.len() {
            ((role_start + index) as *mut u8).write(*role.get(index).unwrap_or(&0));
            index = index.saturating_add(1);
        }
        ((role_start + role.len()) as *mut u8).write(0);
    }
    stack
}

/// Cycle helper: signal completion and exit this thread only.
extern "C" fn cycle_thread(_arg: usize) -> ! {
    CYCLE_DONE.store(true, Ordering::Release);
    thread_exit(0)
}

/// Two-thread helper: publish a sequence number and exit the thread.
extern "C" fn tw_helper(arg: usize) -> ! {
    HELPER_FLAG.store(arg as u32, Ordering::Release);
    thread_exit(0)
}

/// Long-sleeping helper: proves the process-wide `EXIT` kills a blocked thread too.
extern "C" fn tw_sleeper(_arg: usize) -> ! {
    loop {
        nanosleep(60_000_000_000);
    }
}

/// Registered signal entry: the wait-race probe only needs a registration, not a disposition.
///
/// The kernel kicks a thread whose `SIG_SETUP` entry is non-zero for every queued signal,
/// including the `SIGCHLD` that announces the child exit the wait is collecting.
extern "C" fn signal_entry(_bits: usize, _context: usize, _mask: usize) -> ! {
    let _ = syscall1(SIGRETURN, 0);
    loop {
        core::hint::spin_loop();
    }
}

/// Deliver `SIGUSR1` to the controller process after it has blocked in the wait.
///
/// The target is this process's pid, not the process group: the sleeping child is in the same
/// group and has no signal entry, so a group signal would correctly take its default terminate
/// action before the parent's wait returns.
extern "C" fn wait_signal_thread(_arg: usize) -> ! {
    nanosleep(50_000_000);
    let pid = proc_handle_info(Handle::SELF_PROC).unwrap_or(0);
    let _ = syscall2(PROC_SIGNAL, pid as usize, SIGUSR1);
    thread_exit(0)
}

/// Pop the queued `SIGUSR1` so it cannot interrupt a later wait.
fn drain_sigusr1() {
    let mut words = [0u64; 6];
    let deliverable = 1u64 << (SIGUSR1 - 1);
    let _ = syscall3(
        PROC_SIGRECV_EX,
        words.as_mut_ptr() as usize,
        SIGRECV_DELIVERABLE,
        deliverable as usize,
    );
}

/// The 10,000 thread create/exit cycles (scenario 2).
fn churn_part() -> Result<(), &'static str> {
    let entry: extern "C" fn(usize) -> ! = cycle_thread;
    // SAFETY: only the static's address is taken; the helper thread runs on it while the
    // controller waits on the completion flag (one helper at a time).
    let stack_top = helper_stack_top(core::ptr::addr_of_mut!(CYCLE_STACK));
    let mut cycle = 0usize;
    while cycle < CYCLES {
        CYCLE_DONE.store(false, Ordering::Release);
        let thread = thread_create(Handle::SELF_PROC, entry as usize, stack_top, 0)
            .map_err(|_| "cycle create")?;
        // Wait for the helper so only one helper stack is live at a time: the kernel stack
        // cycles through the cache instead of piling up.
        let mut spins = 0usize;
        while !CYCLE_DONE.load(Ordering::Acquire) {
            yield_now();
            spins = spins.saturating_add(1);
            if spins > 1_000_000 {
                return Err("cycle helper did not finish");
            }
        }
        handle_close(thread);
        cycle = cycle.saturating_add(1);
    }
    println!("s36: cycles done={}", CYCLES);
    Ok(())
}

/// The bounded process/session churn: 32 full exec/address-space/stack lifecycles.
///
/// Each cycle creates a new process, maps its own address space and user stack, runs the
/// `marker` role to exit 0, reaps it, and closes the handle. The kernel `s36` counter lines on
/// the surrounding marker exits are compared by the qtest evidence; a per-cycle frame/stack/handle
/// leak would show as a closing delta, not a one-time stack-cache entry.
fn session_churn_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let mut session = 0usize;
    while session < SESSIONS {
        let child = spawn_role(initfs, "marker").map_err(|_| "session spawn")?;
        if proc_wait(child).unwrap_or(-1) != 0 {
            return Err("session wait");
        }
        handle_close(child);
        session = session.saturating_add(1);
    }
    println!("s36: sessions done={}", SESSIONS);
    Ok(())
}

/// The VMA churn child: map four shared memory objects, touch every page, then exit.
///
/// The `large` variant maps 1 MiB of data; the `small` variant maps only four pages. The
/// page-table cost is the same for both (four VMAs); the data-frame cost differs by almost 1 MiB.
/// Comparing the marker counters after the two churns separates page tables from VMA data.
fn vma_child(large: bool) {
    let size = if large { VMA_LARGE } else { PAGE };
    let mut region = 0usize;
    while region < VMA_REGIONS {
        let address = VMA_BASE.saturating_add(region.saturating_mul(VMA_REGION_GAP));
        let object = match memobj_create(size) {
            Ok(object) => object,
            Err(_) => {
                println!("s36: failed vma object");
                exit(1);
            }
        };
        if map_memory(
            Handle::SELF_PROC,
            object,
            0,
            address,
            size,
            map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
        )
        .is_err()
        {
            println!("s36: failed vma map");
            exit(1);
        }
        handle_close(object);
        let pages = size / PAGE;
        let mut page = 0usize;
        while page < pages {
            // SAFETY: the mapping is writable and `page` stays inside `size`.
            unsafe {
                ((address.saturating_add(page.saturating_mul(PAGE))) as *mut u8)
                    .write_volatile(0x5a);
            }
            page = page.saturating_add(1);
        }
        region = region.saturating_add(1);
    }
}

/// Spawn one marker and reap it; its exit prints the kernel counters.
fn marker_once(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let marker = spawn_role(initfs, "marker").map_err(|_| "marker spawn")?;
    if proc_wait(marker).unwrap_or(-1) != 0 {
        return Err("marker wait");
    }
    handle_close(marker);
    Ok(())
}

/// The large/small VMA churn: fork, map/touch four VMAs, exit, reap, repeat.
fn vma_churn_part(initfs: &Initfs<'_>, role: &str, cycles: usize) -> Result<(), &'static str> {
    let mut cycle = 0usize;
    while cycle < cycles {
        let child = spawn_role(initfs, role).map_err(|_| "vma spawn")?;
        if proc_wait(child).unwrap_or(-1) != 0 {
            return Err("vma wait");
        }
        handle_close(child);
        cycle = cycle.saturating_add(1);
    }
    println!("s36: vma {} done={}", role, cycles);
    Ok(())
}

/// Terminate-while-waiting timeout retention: spawn a child that blocks in a futex wait with a
/// 60 s timeout, wait until it is inside the futex, then terminate the process. The kernel exit
/// path must cancel the child's timeout sleeper; otherwise the dead Thread stays on the per-CPU
/// sleeper list and both the live Thread and queued-sleeper counters grow by one per cycle.
fn timeout_churn_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let base = spawn_role(initfs, "timebase").map_err(|_| "timebase spawn")?;
    if proc_wait(base).unwrap_or(-1) != 90 {
        return Err("timebase wait");
    }
    handle_close(base);

    let mut cycle = 0usize;
    while cycle < TIMEOUT_CYCLES {
        let child = spawn_role(initfs, "futex-wait").map_err(|_| "timeout spawn")?;
        let pid = proc_handle_info(child).map_err(|_| "timeout pid")?;
        let mut polls = 0usize;
        while !child_in_futex(pid) && polls < 200 {
            nanosleep(10_000_000);
            polls = polls.saturating_add(1);
        }
        proc_terminate(child, 9).map_err(|_| "timeout terminate")?;
        let _ = proc_wait(child);
        handle_close(child);
        cycle = cycle.saturating_add(1);
    }
    println!("s36: timeout churn done={}", TIMEOUT_CYCLES);

    let top = spawn_role(initfs, "timetop").map_err(|_| "timetop spawn")?;
    if proc_wait(top).unwrap_or(-1) != 91 {
        return Err("timetop wait");
    }
    handle_close(top);
    Ok(())
}

/// Open the fixture binary through vfsd, retrying until the file scheme is ready.
fn open_map_file(client: &Client) -> Result<Handle, Errno> {
    let paths = ["file:/bin/s36-lifecycle", "file:/bin/brush", "file:/init"];
    let mut attempt = 0usize;
    while attempt < 100 {
        for path in paths {
            if let Ok(file) = client.open_flags(path, OpenFlags::READ) {
                return Ok(file);
            }
        }
        nanosleep(10_000_000);
        attempt = attempt.saturating_add(1);
    }
    Err(Errno::ENOENT)
}

/// Map/unmap the same chunk through vfsd's file pager. The kernel gate on exit codes 88/89
/// prints `frame::stats().allocated_bytes` before and after; a leak keeps the delta above one
/// chunk and prints the forbidden `s36: mapcheck leak` line.
fn vfsd_map_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let base = spawn_role(initfs, "mapbase").map_err(|_| "mapbase spawn")?;
    if proc_wait(base).unwrap_or(-1) != 88 {
        return Err("mapbase wait");
    }
    handle_close(base);

    let client = Client::new();
    let file = open_map_file(&client).map_err(|_| "vfsd open")?;
    let request = CallRequest::new(Call::Mmap {
        offset: 0,
        len: PAGE as u64,
        shared: false,
    });
    let mut encoded = [0u8; 128];
    let length = request.encode(&mut encoded).map_err(|_| "mmap encode")?;
    let flags = map::pack((prot::READ | prot::WRITE) as usize, map::FIXED);
    let mut cycle = 0usize;
    while cycle < MAP_CYCLES {
        let mut response = [0u8; 16];
        let count = client
            .call(
                file,
                encoded.get(..length).ok_or("mmap request")?,
                &mut response,
            )
            .map_err(|_| "mmap call")?;
        if count < 4 {
            return Err("mmap reply");
        }
        let object = Handle(u32::from_le_bytes([
            response[0],
            response[1],
            response[2],
            response[3],
        ]));
        let _ = unmap(Handle::SELF_PROC, MAP_SCRATCH, PAGE);
        map_memory(Handle::SELF_PROC, object, 0, MAP_SCRATCH, PAGE, flags)
            .map_err(|_| "map object")?;
        // Fault the page through vfsd's pager.
        let _ = unsafe { core::ptr::read_volatile(MAP_SCRATCH as *const u8) };
        unmap(Handle::SELF_PROC, MAP_SCRATCH, PAGE).map_err(|_| "unmap object")?;
        let mut closed =
            Errno::from_ret(syscall1(Syscall::HANDLE_CLOSE as usize, object.0 as usize));
        if closed.is_err() {
            nanosleep(1_000_000);
            closed = Errno::from_ret(syscall1(Syscall::HANDLE_CLOSE as usize, object.0 as usize));
        }
        if let Err(error) = closed {
            println!(
                "s36: mapclose fail cycle={cycle} object={} errno={error:?}",
                object.0
            );
        }
        cycle = cycle.saturating_add(1);
    }
    client.close(file).map_err(|_| "vfsd close")?;
    // The scheme close is fire-and-forget: give vfsd a bounded window to retire the 1000 pager
    // keys and let the final in-flight page-outs drop their object references before the exit-89
    // frame sample, so the gate measures the steady state (pager=0) rather than a drained
    // backlog. Under the current loaded host the 1 s window still sampled 17-25 pages of
    // in-flight frames just above the gate; 2 s measures the drained steady state.
    nanosleep(2_000_000_000);
    println!("s36: vfsd map done={}", MAP_CYCLES);

    let top = spawn_role(initfs, "maptop").map_err(|_| "maptop spawn")?;
    if proc_wait(top).unwrap_or(-1) != 89 {
        return Err("maptop wait");
    }
    handle_close(top);
    Ok(())
}

/// Create a writable tmpfs file, map/fault one vfsd pager page and return the still-open file
/// and the client's object handle (the caller closes the object).
fn map_tmpfs_page(client: &Client, path: &str) -> Result<(Handle, Handle), &'static str> {
    let flags = OpenFlags::from_wire(WIRE_OPEN_READ | WIRE_OPEN_WRITE | WIRE_OPEN_CREATE)
        .map_err(|_| "tmpfs flags")?;
    let file = match client.open_flags(path, flags) {
        Ok(file) => file,
        Err(error) => {
            println!("s36: tmpfs open err {:?}", error);
            return Err("tmpfs open");
        }
    };
    let data = [0x5au8; PAGE];
    let written = match client.write(file, 0, &data) {
        Ok(written) => written,
        Err(error) => {
            println!("s36: tmpfs write err {:?}", error);
            return Err("tmpfs write");
        }
    };
    if written != PAGE {
        return Err("tmpfs short write");
    }
    let request = CallRequest::new(Call::Mmap {
        offset: 0,
        len: PAGE as u64,
        shared: false,
    });
    let mut encoded = [0u8; 128];
    let length = request.encode(&mut encoded).map_err(|_| "mmap encode")?;
    let mut response = [0u8; 16];
    let count = match client.call(
        file,
        encoded.get(..length).ok_or("mmap request")?,
        &mut response,
    ) {
        Ok(count) => count,
        Err(error) => {
            println!("s36: tmpfs mmap err {:?}", error);
            return Err("mmap call");
        }
    };
    if count < 4 {
        return Err("mmap reply");
    }
    let object = Handle(u32::from_le_bytes([
        response[0],
        response[1],
        response[2],
        response[3],
    ]));
    let flags = map::pack((prot::READ | prot::WRITE) as usize, map::FIXED);
    let _ = unmap(Handle::SELF_PROC, MAP_SCRATCH, PAGE);
    map_memory(Handle::SELF_PROC, object, 0, MAP_SCRATCH, PAGE, flags)
        .map_err(|_| "map object")?;
    // Fault the page through vfsd's pager.
    let _ = unsafe { core::ptr::read_volatile(MAP_SCRATCH as *const u8) };
    unmap(Handle::SELF_PROC, MAP_SCRATCH, PAGE).map_err(|_| "unmap object")?;
    Ok((file, object))
}

/// Child role: map/fault one pager page, then exit while the file is still open. The kernel
/// teardown closes the scheme file; vfsd must retire the pager key and release the frames.
fn pager_exit_child() -> i32 {
    let client = Client::new();
    match map_tmpfs_page(&client, "/tmp/pager-exit") {
        Ok((_file, object)) => {
            let _ = Errno::from_ret(syscall1(Syscall::HANDLE_CLOSE as usize, object.0 as usize));
            0
        }
        Err(message) => {
            println!("s36: pager exit map fail {}", message);
            3
        }
    }
}

/// C18 lifetime row: pager frames return to the base when (1) the owning process exits and
/// (2) the file is truncated while the controller still holds the open file. The kernel
/// `s36-mapcheck` gate prints the pager count and frame delta on the 88/89 markers.
fn pager_lifetime_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    // (1) owning process exits with the file still open.
    let base = spawn_role(initfs, "mapbase").map_err(|_| "exit base spawn")?;
    if proc_wait(base).unwrap_or(-1) != 88 {
        return Err("exit base wait");
    }
    handle_close(base);
    let child = spawn_role(initfs, "pager-exit").map_err(|_| "pager exit spawn")?;
    if proc_wait(child).unwrap_or(-1) != 0 {
        return Err("pager exit wait");
    }
    handle_close(child);
    // The scheme close is fire-and-forget; let vfsd process the exit close.
    nanosleep(1_000_000_000);
    let top = spawn_role(initfs, "maptop").map_err(|_| "exit top spawn")?;
    if proc_wait(top).unwrap_or(-1) != 89 {
        return Err("exit top wait");
    }
    handle_close(top);
    println!("s36: pager exit done");

    // (2) file truncation. The controller keeps the file open across the 89 marker, so only the
    // truncate retire can release the pager key and the object's frames.
    let base = spawn_role(initfs, "mapbase").map_err(|_| "trunc base spawn")?;
    if proc_wait(base).unwrap_or(-1) != 88 {
        return Err("trunc base wait");
    }
    handle_close(base);
    let client = Client::new();
    let (file, object) = map_tmpfs_page(&client, "/tmp/pager-trunc")?;
    let _ = Errno::from_ret(syscall1(Syscall::HANDLE_CLOSE as usize, object.0 as usize));
    let request = CallRequest::new(Call::Truncate { size: 0 });
    let mut encoded = [0u8; 64];
    let length = request.encode(&mut encoded).map_err(|_| "truncate encode")?;
    let mut response = [0u8; 16];
    client
        .call(
            file,
            encoded.get(..length).ok_or("truncate request")?,
            &mut response,
        )
        .map_err(|_| "truncate call")?;
    nanosleep(1_000_000_000);
    let top = spawn_role(initfs, "maptop").map_err(|_| "trunc top spawn")?;
    if proc_wait(top).unwrap_or(-1) != 89 {
        return Err("trunc top wait");
    }
    handle_close(top);
    println!("s36: pager truncate done");
    client.close(file).map_err(|_| "tmpfs close")?;
    Ok(())
}

fn futex_wait(addr: &AtomicU32, value: u32, timeout_ns: u64) -> Result<(), Errno> {
    result(syscall4(
        Syscall::FUTEX as usize,
        addr as *const AtomicU32 as usize,
        FUTEX_WAIT as usize,
        value as usize,
        timeout_ns as usize,
    ))
    .map(|_| ())
}

fn futex_wake(addr: &AtomicU32, count: usize) {
    let _ = syscall4(
        Syscall::FUTEX as usize,
        addr as *const AtomicU32 as usize,
        FUTEX_WAKE as usize,
        count,
        0,
    );
}

extern "C" fn lock_waiter(_: usize) -> ! {
    let address = core::ptr::addr_of!(LOCK_FUTEX) as usize;
    let mut seq = 0u32;
    while !LOCK_STOP.load(Ordering::Acquire) {
        seq = seq.wrapping_add(1);
        LOCK_READY.store(seq, Ordering::Release);
        let value = LOCK_FUTEX.load(Ordering::Acquire);
        let raw = syscall4(
            Syscall::FUTEX as usize,
            address,
            FUTEX_WAIT as usize,
            value as usize,
            0,
        );
        if raw != 0 && raw != Errno::EAGAIN.to_ret() && raw != Errno::EINTR.to_ret() {
            LOCK_READY.store(u32::MAX, Ordering::Release);
            thread_exit(1);
        }
        LOCK_DONE.store(seq, Ordering::Release);
    }
    thread_exit(0)
}

fn wait_lock_flag(flag: &AtomicU32, value: u32) -> Result<(), &'static str> {
    let mut spins = 0usize;
    while flag.load(Ordering::Acquire) < value {
        yield_now();
        spins = spins.saturating_add(1);
        if spins > 10_000_000 {
            return Err("lock flag");
        }
    }
    Ok(())
}

fn lock_churn_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let base = spawn_role(initfs, "lockbase").map_err(|_| "lockbase spawn")?;
    if proc_wait(base).unwrap_or(-1) != 90 {
        return Err("lockbase wait");
    }
    handle_close(base);

    let entry: extern "C" fn(usize) -> ! = lock_waiter;
    let stack_top = helper_stack_top(core::ptr::addr_of_mut!(LOCK_STACK));
    let worker = thread_create(Handle::SELF_PROC, entry as usize, stack_top, 0)
        .map_err(|_| "lock worker")?;
    let mut cycle = 0u32;
    while (cycle as usize) < LOCK_WAKES {
        cycle = cycle.wrapping_add(1);
        wait_lock_flag(&LOCK_READY, cycle)?;
        nanosleep(LOCK_DELAY_NS);
        LOCK_FUTEX.fetch_add(1, Ordering::AcqRel);
        futex_wake(&LOCK_FUTEX, 1);
        wait_lock_flag(&LOCK_DONE, cycle)?;
    }
    LOCK_STOP.store(true, Ordering::Release);
    LOCK_FUTEX.fetch_add(1, Ordering::AcqRel);
    futex_wake(&LOCK_FUTEX, 1);
    nanosleep(LOCK_DELAY_NS);
    handle_close(worker);

    let mut timeouts = 0usize;
    while timeouts < LOCK_TIMEOUTS {
        match futex_wait(&LOCK_TIMEOUT, 0, LOCK_TIMEOUT_NS) {
            Err(Errno::ETIMEDOUT) | Err(Errno::EAGAIN) => {}
            Err(_) => return Err("lock timeout errno"),
            Ok(()) => return Err("lock timeout wake"),
        }
        timeouts = timeouts.saturating_add(1);
    }
    println!(
        "s36: lock churn wake={} timeout={}",
        LOCK_WAKES, LOCK_TIMEOUTS
    );

    let top = spawn_role(initfs, "locktop").map_err(|_| "locktop spawn")?;
    if proc_wait(top).unwrap_or(-1) != 91 {
        return Err("locktop wait");
    }
    handle_close(top);
    Ok(())
}

/// The `twomain` child: one thread exits (process alive), a sleeper blocks, then EXIT.
fn twomain_part() {
    // Helper 1 exits the thread only; the process stays alive.
    HELPER_FLAG.store(0, Ordering::Release);
    let helper: extern "C" fn(usize) -> ! = tw_helper;
    // SAFETY: only the static's address is taken; the helper runs while the main thread waits.
    let stack_top = helper_stack_top(core::ptr::addr_of_mut!(TW_HELPER_STACK));
    match thread_create(Handle::SELF_PROC, helper as usize, stack_top, 1) {
        Ok(thread) => {
            let mut spins = 0usize;
            while HELPER_FLAG.load(Ordering::Acquire) != 1 {
                yield_now();
                spins = spins.saturating_add(1);
                if spins > 1_000_000 {
                    println!("s36: failed tw helper hang");
                    exit(1);
                }
            }
            handle_close(thread);
            println!("s36: tw thread exited");
        }
        Err(_) => {
            println!("s36: failed tw create");
            exit(1);
        }
    }
    // Helper 2 sleeps far beyond the test; the process-wide EXIT below must kill it.
    let sleeper: extern "C" fn(usize) -> ! = tw_sleeper;
    // SAFETY: only the static's address is taken; the sleeper runs until EXIT tears it down.
    let sleep_top = helper_stack_top(core::ptr::addr_of_mut!(TW_SLEEP_STACK));
    if thread_create(Handle::SELF_PROC, sleeper as usize, sleep_top, 0).is_err() {
        println!("s36: failed tw sleeper create");
        exit(1);
    }
    // Give the sleeper a chance to block, then exit the whole process.
    nanosleep(50_000_000);
    exit(0)
}

/// The `server` child: register the scheme name, announce readiness, then sleep forever.
fn server_part() {
    if scheme_register(b"s36").is_err() {
        println!("s36: failed server register");
        exit(1);
    }
    println!("s36: server ready");
    loop {
        nanosleep(60_000_000_000);
    }
}

fn child_main(role: &str) -> i32 {
    match role {
        "marker" => 0,
        "timebase" => 90,
        "timetop" => 91,
        "mapbase" => 88,
        "maptop" => 89,
        "lockbase" => 90,
        "locktop" => 91,
        "futex-wait" => {
            let word = AtomicU32::new(0);
            let _ = futex_wait_timeout(&word, 0, 60_000_000_000);
            0
        }
        "twomain" => {
            twomain_part();
            0
        }
        "sleep" => loop {
            nanosleep(60_000_000_000);
        },
        "delayed" => {
            nanosleep(100_000_000);
            42
        }
        "server" => {
            server_part();
            0
        }
        "vma-large" => {
            vma_child(true);
            0
        }
        "vma-small" => {
            vma_child(false);
            0
        }
        "pager-exit" => pager_exit_child(),
        _ => 2,
    }
}

/// Scenario 4: an unstarted process is reaped by `PROC_WAIT`.
fn unstarted_part() -> Result<(), &'static str> {
    let child = proc_create().map_err(|_| "unstarted create")?;
    match proc_wait(child) {
        Ok(0) => {}
        Ok(_) => return Err("unstarted wait code"),
        Err(_) => return Err("unstarted wait"),
    }
    handle_close(child);
    println!("s36: unstarted reaped");
    Ok(())
}

/// Scenario 1: the two-thread child; the parent observes one reap with code 0.
fn two_thread_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let child = spawn_role(initfs, "twomain").map_err(|_| "tw spawn")?;
    match proc_wait(child) {
        Ok(0) => {}
        Ok(_) => return Err("tw wait code"),
        Err(_) => return Err("tw wait"),
    }
    handle_close(child);
    println!("s36: twowait ok");
    Ok(())
}

/// Scenario 3: admission into a terminal process is refused, and stays refused.
fn admission_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let child = spawn_role(initfs, "sleep").map_err(|_| "admission spawn")?;
    proc_terminate(child, 0).map_err(|_| "admission terminate")?;
    match proc_wait(child) {
        Ok(0) => {}
        _ => return Err("admission wait"),
    }
    let helper: extern "C" fn(usize) -> ! = tw_helper;
    // SAFETY: only the static's address is taken; no thread is actually created (ESRCH first).
    let stack_top = helper_stack_top(core::ptr::addr_of_mut!(CYCLE_STACK));
    match thread_create(child, helper as usize, stack_top, 0) {
        Err(Errno::ESRCH) => {}
        Err(_) => return Err("admission wrong errno"),
        Ok(_) => return Err("terminal process admitted a thread"),
    }
    // Still terminal: a second attempt must fail the same way (no resurrection).
    if thread_create(child, helper as usize, stack_top, 0) != Err(Errno::ESRCH) {
        return Err("terminal process resurrected");
    }
    handle_close(child);
    println!("s36: admission ok");
    Ok(())
}

/// Scenario 5: authorized stop, `EPERM` without `SIGNAL`, `EBADF` on a stale handle,
/// `ESRCH` on a second terminate.
fn terminate_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    // Authorized stop of a blocked child.
    let child = spawn_role(initfs, "sleep").map_err(|_| "term spawn")?;
    proc_terminate(child, 7).map_err(|_| "term call")?;
    match proc_wait(child) {
        Ok(7) => {}
        Ok(_) => return Err("term wait code"),
        Err(_) => return Err("term wait"),
    }
    handle_close(child);
    println!("s36: terminate ok");

    // Rights and stale-handle errors, then the not-idempotent second terminate.
    let child = spawn_role(initfs, "sleep").map_err(|_| "eperm spawn")?;
    let weak = handle_dup(child, Rights::READ.0).map_err(|_| "eperm dup")?;
    match proc_terminate(weak, 7) {
        Err(Errno::EPERM) => {}
        Err(_) => return Err("eperm wrong errno"),
        Ok(_) => return Err("unprivileged terminate accepted"),
    }
    if proc_terminate(Handle::new(9999, 0), 7) != Err(Errno::EBADF) {
        return Err("stale terminate accepted");
    }
    // First terminate of this child succeeds; a second returns ESRCH.
    proc_terminate(child, 0).map_err(|_| "term second call")?;
    if proc_terminate(child, 0) != Err(Errno::ESRCH) {
        return Err("second terminate accepted");
    }
    match proc_wait(child) {
        Ok(0) => {}
        _ => return Err("term cleanup wait"),
    }
    handle_close(weak);
    handle_close(child);
    println!("s36: eperm ebadf esrch ok");
    Ok(())
}

/// Scenario 6: a terminated server's scheme name is immediately registrable again.
fn scheme_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let server = spawn_role(initfs, "server").map_err(|_| "scheme spawn")?;
    // The server registers at thread start and prints readiness; 200 ms of virtual time is
    // several scheduler slices, far beyond its register call.
    nanosleep(200_000_000);
    proc_terminate(server, 0).map_err(|_| "scheme terminate")?;
    match proc_wait(server) {
        Ok(0) => {}
        _ => return Err("scheme wait"),
    }
    handle_close(server);
    // The name must be free for the controller immediately.
    if scheme_register(b"s36").is_err() {
        return Err("scheme name not released");
    }
    println!("s36: scheme reregistered");
    Ok(())
}

/// Scenario 7: a child exits while the parent is blocked in a wait.
///
/// The parent has a registered signal entry, so the child's `SIGCHLD` kick and the wait wakeup
/// race. `PROC_WAIT_EX` must return the requested pid and exit code, and the handle `PROC_WAIT`
/// must return the code; neither may report `EINTR` while the child status is available.
fn wait_interrupt_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let entry: extern "C" fn(usize, usize, usize) -> ! = signal_entry;
    // SAFETY: only the static's address is taken; the kernel validates the executable entry and
    // the 4096-byte absolute context area before storing the per-thread registration.
    let context = core::ptr::addr_of_mut!(SIGNAL_CONTEXT) as usize;
    result(syscall3(
        SIG_SETUP,
        entry as usize,
        context,
        SIG_SETUP_ABS as usize,
    ))
    .map_err(|_| "wait signal setup")?;
    let child = spawn_role(initfs, "delayed").map_err(|_| "wait race spawn")?;
    let pid = proc_handle_info(child).map_err(|_| "wait race pid")?;
    let mut status = 0i32;
    match proc_wait_ex(P_PID, pid, &mut status) {
        Ok(got) if got == pid as usize && status == 42 << 8 => {}
        Ok(got) => {
            println!("s36: wait race got={} pid={} status={}", got, pid, status);
            return Err("wait race pid/status");
        }
        Err(error) => {
            println!("s36: wait race errno={}", error.0);
            return Err("wait race interrupted");
        }
    }
    handle_close(child);
    println!("s36: wait race ok");

    // The handle-based PROC_WAIT has the same interrupted-wakeup contract.
    let child = spawn_role(initfs, "delayed").map_err(|_| "wait race handle spawn")?;
    match proc_wait(child) {
        Ok(42) => {}
        Ok(code) => {
            println!("s36: wait race handle code={}", code);
            return Err("wait race handle code");
        }
        Err(error) => {
            println!("s36: wait race handle errno={}", error.0);
            return Err("wait race handle interrupted");
        }
    }
    handle_close(child);
    println!("s36: wait race handle ok");
    Ok(())
}

/// Scenario 8: a caught signal delivered while the parent is blocked still returns `EINTR`.
///
/// This is the no-suppression control: a child-exit wake is not an interrupt, but a caught
/// signal is. A helper delivers `SIGUSR1` after the controller blocks on a sleeping child.
fn wait_eintr_part(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let child = spawn_role(initfs, "sleep").map_err(|_| "wait eintr spawn")?;
    let pid = proc_handle_info(child).map_err(|_| "wait eintr pid")?;
    let entry: extern "C" fn(usize) -> ! = wait_signal_thread;
    // SAFETY: only the static's address is taken; the helper runs on it until its thread exits.
    let stack_top = helper_stack_top(core::ptr::addr_of_mut!(WAIT_SIGNAL_STACK));
    let helper = thread_create(Handle::SELF_PROC, entry as usize, stack_top, pid as usize)
        .map_err(|_| "wait eintr helper")?;
    let mut status = 0i32;
    match proc_wait_ex(P_PID, pid, &mut status) {
        Err(Errno::EINTR) => {}
        Err(_) => return Err("wait eintr wrong errno"),
        Ok(_) => return Err("wait eintr not delivered"),
    }
    drain_sigusr1();
    proc_terminate(child, 0).map_err(|_| "wait eintr terminate")?;
    match proc_wait_ex(P_PID, pid, &mut status) {
        Ok(got) if got == pid as usize && status == 0 => {}
        _ => return Err("wait eintr reap"),
    }
    handle_close(helper);
    handle_close(child);
    println!("s36: wait eintr ok");
    Ok(())
}

/// Bounded churn row: baseline counters, 10,000 thread create/exit cycles, closing counters.
///
/// This mode skips the unrelated lifecycle scenarios so the thread teardown leak can be
/// reproduced and measured without the wait/ABI rows. The marker child's exit makes the kernel
/// print `s36: stacks fresh=... cached=... heap=...` before and after the churn.
#[cfg(feature = "s36-churn-only")]
fn churn_only(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let marker = spawn_role(initfs, "marker").map_err(|_| "marker spawn")?;
    if proc_wait(marker).unwrap_or(-1) != 0 {
        return Err("marker wait");
    }
    handle_close(marker);
    churn_part()?;
    let marker = spawn_role(initfs, "marker").map_err(|_| "marker spawn")?;
    if proc_wait(marker).unwrap_or(-1) != 0 {
        return Err("marker wait");
    }
    handle_close(marker);
    println!("s36: churn done");
    Ok(())
}

fn main(args: raven_rt::Args) -> i32 {
    if let Some(role) = args.get(1) {
        if role != "timeout-check" {
            return child_main(role);
        }
    }

    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => fail("initfs parse"),
    };

    if args.get(1).is_some_and(|role| role == "timeout-check") {
        if let Err(message) = timeout_churn_part(&initfs) {
            fail(message);
        }
        return 0;
    }

    #[cfg(feature = "s36-churn-only")]
    {
        if let Err(message) = churn_only(&initfs) {
            fail(message);
        }
        return 0;
    }

    // Scenario 4: unstarted process reaped by wait.
    if let Err(message) = unstarted_part() {
        fail(message);
    }
    // Scenario 7: the SIGCHLD kick must not replace an available child status with EINTR.
    if let Err(message) = wait_interrupt_part(&initfs) {
        fail(message);
    }
    // Scenario 8: a caught signal must still interrupt the wait (no EINTR suppression).
    if let Err(message) = wait_eintr_part(&initfs) {
        fail(message);
    }
    // Scenario 2, baseline marker: the kernel prints the stack/heap/frame counters on this exit.
    if let Err(message) = marker_once(&initfs) {
        fail(message);
    }
    if let Err(message) = churn_part() {
        fail(message);
    }
    if let Err(message) = session_churn_part(&initfs) {
        fail(message);
    }
    if let Err(message) = vma_churn_part(&initfs, "vma-large", VMA_CYCLES) {
        fail(message);
    }
    // Sample after the large churn; the next close is the small-VMA sample.
    if let Err(message) = marker_once(&initfs) {
        fail(message);
    }
    // Scenario 2, closing marker: its exit prints the counters again; the report compares.
    if let Err(message) = marker_once(&initfs) {
        fail(message);
    }
    // Scenario 1: two-thread child.
    if let Err(message) = two_thread_part(&initfs) {
        fail(message);
    }
    // Scenario 3: admission into a terminal process.
    if let Err(message) = admission_part(&initfs) {
        fail(message);
    }
    // Scenario 5: authorized termination and its errors.
    if let Err(message) = terminate_part(&initfs) {
        fail(message);
    }
    // Scenario 6: scheme name released on exit.
    if let Err(message) = scheme_part(&initfs) {
        fail(message);
    }
    // Lock/time churn: futex wake and timeout paths around allocated_bytes markers.
    if let Err(message) = lock_churn_part(&initfs) {
        fail(message);
    }
    // vfsd pager red row: map/unmap the same chunk and compare allocated frames.
    if let Err(message) = vfsd_map_part(&initfs) {
        fail(message);
    }
    // C18 lifetime row: pager frames release on owning-process exit and on file truncation.
    if let Err(message) = pager_lifetime_part(&initfs) {
        fail(message);
    }

    println!("s36: lifecycle ok");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
