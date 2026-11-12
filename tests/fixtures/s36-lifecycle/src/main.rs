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
//!
//! Roles: `marker` (exit 0), `twomain`, `sleep` (sleep forever), `server` (register scheme
//! `s36`, then sleep).

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::{USER_END, USER_MIN};
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, map, prot};
use raven_elf::{Elf, LoadMode, Machine, StepKind, plan_load};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const INITFS_BASE: usize = 0x1000_0000;
const STACK_SCRATCH: usize = 0x2000_0000;
const STACK_SIZE: usize = 64 * 1024;
const STACK_TOP: usize = (USER_END as usize) - PAGE;
const STACK_BASE: usize = STACK_TOP - STACK_SIZE;

/// Cycles in the thread create/exit churn.
const CYCLES: usize = 10_000;
/// User stacks for the fixture's extra threads (16 KiB each, one helper live at a time).
const HELPER_STACK_SIZE: usize = 16 * 1024;

static CYCLE_DONE: AtomicBool = AtomicBool::new(false);
static HELPER_FLAG: AtomicU32 = AtomicU32::new(0);

#[repr(align(16))]
struct HelperStack(#[allow(dead_code)] [u8; HELPER_STACK_SIZE]);

static mut CYCLE_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);
static mut TW_HELPER_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);
static mut TW_SLEEP_STACK: HelperStack = HelperStack([0; HELPER_STACK_SIZE]);

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
    let entry = initfs.get("/init").ok_or(Errno::ENOENT)?;
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
        "twomain" => {
            twomain_part();
            0
        }
        "sleep" => loop {
            nanosleep(60_000_000_000);
        },
        "server" => {
            server_part();
            0
        }
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

fn main(args: raven_rt::Args) -> i32 {
    if let Some(role) = args.get(1) {
        return child_main(role);
    }

    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => fail("initfs parse"),
    };

    // Scenario 4: unstarted process reaped by wait.
    if let Err(message) = unstarted_part() {
        fail(message);
    }
    // Scenario 2, baseline marker: the kernel prints the stack/heap counters on this exit.
    let marker = match spawn_role(&initfs, "marker") {
        Ok(marker) => marker,
        Err(_) => fail("marker spawn"),
    };
    if proc_wait(marker).unwrap_or(-1) != 0 {
        fail("marker wait");
    }
    handle_close(marker);
    if let Err(message) = churn_part() {
        fail(message);
    }
    // Scenario 2, closing marker: its exit prints the counters again; the report compares.
    let marker = match spawn_role(&initfs, "marker") {
        Ok(marker) => marker,
        Err(_) => fail("marker spawn"),
    };
    if proc_wait(marker).unwrap_or(-1) != 0 {
        fail("marker wait");
    }
    handle_close(marker);
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

    println!("s36: lifecycle ok");
    0
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
