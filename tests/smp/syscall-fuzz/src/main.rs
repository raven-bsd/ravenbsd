// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K456 syscall robustness measurement.
//!
//! The program walks every `raven-abi` syscall variant with a deterministic vector set: bad
//! handles, bad pointers, wrapped lengths, invalid flags and opcodes, and a `PROT_NONE` buffer.
//! It records the raw return, the errno and whether the process survived, compares each row with
//! the contract-derived expectation, and prints a stable table hash. It fixes nothing: a deviation
//! is a finding for the owning lane, not a failure of this measurement.
//!
//! Vector classes follow the ticket: `fdneg`, `fdhuge`, `fdclosed`, `ptr0`, `ptr1`, `ptrneg`,
//! `buf`, `straddle`, `len0`, `len1`, `lenmax`, `lenmax7`, `lenbig`, `flags0`, `flagsneg`,
//! `flagsrand`, `protnone`, and a valid baseline per call. A vector poisons exactly one argument
//! and leaves the rest valid, so the returned errno is attributable.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, clock, futex, map, memobj, namespace, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

/// Deterministic generator seed (recorded in the report and the docs).
const SEED: u64 = 0x5A17_C0DE_1234_9876;
/// Page size used by every mapping vector.
const PAGE: usize = 4096;
/// Raw `pipe(fds)` number (kernel-owned, not a `Syscall` variant).
const PIPE: usize = 0x6e;
/// Raw `proc_fork(src, flags)` number (kernel-owned, not a `Syscall` variant).
const PROC_FORK: usize = 0x39;
/// `EVQ_CTL` readiness bit for a signalled user event.
const SIGNALED: usize = 8;
/// A valid rights mask for the handle vectors.
const RIGHTS_ALL: usize = u32::MAX as usize;

const PTR0: usize = 0;
const PTR1: usize = 1;
const PTRMAX: usize = usize::MAX;
const LENMAX: usize = usize::MAX;
const LENMAX7: usize = usize::MAX - 7;

const EFAULT: &[Errno] = &[Errno::EFAULT];
const EBADF: &[Errno] = &[Errno::EBADF];
const EINVAL: &[Errno] = &[Errno::EINVAL];
const ENAMETOOLONG: &[Errno] = &[Errno::ENAMETOOLONG];
const EACCES: &[Errno] = &[Errno::EACCES];
const EAGAIN: &[Errno] = &[Errno::EAGAIN];
const EPERM: &[Errno] = &[Errno::EPERM];
const ESRCH: &[Errno] = &[Errno::ESRCH];

#[repr(align(4096))]
struct Page(#[allow(dead_code)] [u8; PAGE]);

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; 16 * 1024]);

static mut BUF: Page = Page([0; PAGE]);
static mut OUT: Page = Page([0; PAGE]);
static mut CHILD_STACK: Stack = Stack([0; 16 * 1024]);
static mut PROBE_STACK: Stack = Stack([0; 16 * 1024]);
static mut FUTEX_WORD: u32 = 1;
static PATH: [u8; 7] = *b"file:/x";
static NAME: [u8; 4] = *b"fuzz";
static PREFIX: [u8; 4] = *b"fuzz";

/// What the contract expects for one measured row.
#[derive(Clone, Copy)]
enum Expect {
    /// A successful result is the documented outcome.
    Success,
    /// Any result is recorded; the resource is device- or server-dependent.
    Any,
    /// The result must be an error in this set.
    Err(&'static [Errno]),
    /// A forked child must exit with this code while the parent survives.
    ChildCode(i32),
}

/// One measured syscall call.
struct Row {
    call: Syscall,
    vector: &'static str,
    ret: isize,
    expect: Expect,
}

/// Runtime resources used by the valid baseline of each call.
#[derive(Clone, Copy)]
struct Ctx {
    buf: usize,
    out: usize,
    guard: usize,
    straddle: usize,
    memobj: usize,
    paged: usize,
    paged2: usize,
    event: usize,
    timer: usize,
    evq: usize,
    pipe_r: usize,
    pipe_w: usize,
    closed: usize,
    proc_created: usize,
    dma: Option<usize>,
}

/// Raw syscall with six arguments.
fn raw(number: usize, args: [usize; 6]) -> isize {
    // SAFETY: the number is a `Syscall` variant or a documented raw kernel number. Every user
    // pointer is either a live static buffer/mapped page or a deliberately invalid value that the
    // kernel validates with `UserSlice`/fault-safe copy; no Rust reference is created here.
    unsafe { sys::syscall6(number, args[0], args[1], args[2], args[3], args[4], args[5]) }
}

fn checked(number: usize, args: [usize; 6]) -> Result<usize, Errno> {
    Errno::from_ret(raw(number, args))
}

fn stack_top(stack: *mut Stack) -> usize {
    (stack as usize).saturating_add(core::mem::size_of::<Stack>())
}

/// Readiness word used by the `FUTEX` baseline.
fn futex_word() -> usize {
    core::ptr::addr_of!(FUTEX_WORD) as usize
}

fn valid_dma() -> Option<usize> {
    let raw = raw(
        Syscall::DMA_ALLOC as usize,
        [Handle::ROOT_DEVICE.0 as usize, PAGE, 0, 32, 0, 0],
    );
    (raw >= 0).then_some(raw as usize)
}

fn setup() -> Result<Ctx, Errno> {
    let buf = core::ptr::addr_of!(BUF) as usize;
    let out = core::ptr::addr_of!(OUT) as usize;
    let base = checked(
        Syscall::MAP as usize,
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            2 * PAGE,
            prot::READ | prot::WRITE,
        ],
    )?;
    checked(
        Syscall::PROTECT as usize,
        [
            Handle::SELF_PROC.0 as usize,
            base.saturating_add(PAGE),
            PAGE,
            0,
            0,
            0,
        ],
    )?;
    let mut fds = [0u32; 2];
    checked(PIPE, [fds.as_mut_ptr() as usize, 0, 0, 0, 0, 0])?;
    let (pipe_r, pipe_w) = (fds[0] as usize, fds[1] as usize);
    let memobj = checked(Syscall::MEMOBJ_CREATE as usize, [PAGE, 0, 0, 0, 0, 0])?;
    let paged = checked(
        Syscall::MEMOBJ_CREATE as usize,
        [PAGE, memobj::PAGER, 0, 0, 0, 0],
    )?;
    let paged2 = checked(
        Syscall::MEMOBJ_CREATE as usize,
        [PAGE, memobj::PAGER, 0, 0, 0, 0],
    )?;
    let event = checked(Syscall::EVENT_CREATE as usize, [0; 6])?;
    let timer = checked(
        Syscall::TIMER_CREATE as usize,
        [clock::MONOTONIC as usize, 0, 0, 0, 0, 0],
    )?;
    let evq = checked(Syscall::EVQ_CREATE as usize, [0; 6])?;
    let proc_created = checked(Syscall::PROC_CREATE as usize, [0; 6])?;
    let dup = checked(
        Syscall::HANDLE_DUP as usize,
        [pipe_r, RIGHTS_ALL, 0, 0, 0, 0],
    )?;
    checked(Syscall::HANDLE_CLOSE as usize, [dup, 0, 0, 0, 0, 0])?;
    Ok(Ctx {
        buf,
        out,
        guard: base.saturating_add(PAGE),
        straddle: base.saturating_add(PAGE).saturating_sub(8),
        memobj,
        paged,
        paged2,
        event,
        timer,
        evq,
        pipe_r,
        pipe_w,
        closed: dup,
        proc_created,
        dma: valid_dma(),
    })
}

struct Runner {
    ctx: Ctx,
    rows: Vec<Row>,
    gaps: Vec<&'static str>,
}

impl Runner {
    fn new(ctx: Ctx) -> Self {
        Self {
            ctx,
            rows: Vec::new(),
            gaps: Vec::new(),
        }
    }

    fn case(&mut self, call: Syscall, vector: &'static str, args: [usize; 6], expect: Expect) {
        let ret = raw(call as usize, args);
        self.push(Row {
            call,
            vector,
            ret,
            expect,
        });
    }

    fn push(&mut self, row: Row) {
        emit(&row);
        self.rows.push(row);
    }

    fn ok(&mut self, call: Syscall, vector: &'static str, args: [usize; 6]) {
        self.case(call, vector, args, Expect::Success);
    }

    fn err(
        &mut self,
        call: Syscall,
        vector: &'static str,
        args: [usize; 6],
        set: &'static [Errno],
    ) {
        self.case(call, vector, args, Expect::Err(set));
    }

    fn any(&mut self, call: Syscall, vector: &'static str, args: [usize; 6]) {
        self.case(call, vector, args, Expect::Any);
    }

    fn gap(&mut self, text: &'static str) {
        self.gaps.push(text);
    }

    fn verdict(row: &Row) -> bool {
        match row.expect {
            Expect::Any => true,
            Expect::Success => row.ret >= 0,
            Expect::Err(set) => {
                row.ret < 0 && set.contains(&Errno(u16::try_from(-row.ret).unwrap_or(u16::MAX)))
            }
            Expect::ChildCode(code) => row.ret == code as isize,
        }
    }

    fn report(&self) {
        for gap in &self.gaps {
            println!("syscall-fuzz: coverage-gap {gap}");
        }
        for row in &self.rows {
            if Self::verdict(row) {
                continue;
            }
            let expected = match row.expect {
                Expect::Success => alloc::string::String::from("success"),
                Expect::Any => alloc::string::String::from("any"),
                Expect::Err(set) => format!("{set:?}"),
                Expect::ChildCode(code) => format!("child-code={code}"),
            };
            println!(
                "syscall-fuzz: deviation nr=0x{:02x} name={:?} vector={} expected={} actual={}",
                row.call as usize, row.call, row.vector, expected, row.ret
            );
        }
        let deviations = self.rows.iter().filter(|row| !Self::verdict(row)).count();
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for row in &self.rows {
            hash = fnv(hash, &(row.call as usize as u64).to_le_bytes());
            hash = fnv(hash, row.vector.as_bytes());
            hash = fnv(hash, &(row.ret as i64).to_le_bytes());
        }
        println!(
            "syscall-fuzz: summary rows={} deviations={} seed=0x{SEED:016x} table-hash=0x{hash:016x}",
            self.rows.len(),
            deviations
        );
    }
}

fn errno_name(error: Errno) -> &'static str {
    match error {
        Errno::EPERM => "EPERM",
        Errno::ENOENT => "ENOENT",
        Errno::ESRCH => "ESRCH",
        Errno::EINTR => "EINTR",
        Errno::EBADF => "EBADF",
        Errno::EAGAIN => "EAGAIN",
        Errno::ENOMEM => "ENOMEM",
        Errno::EACCES => "EACCES",
        Errno::EFAULT => "EFAULT",
        Errno::EBUSY => "EBUSY",
        Errno::EEXIST => "EEXIST",
        Errno::ENODEV => "ENODEV",
        Errno::EINVAL => "EINVAL",
        Errno::ENOSPC => "ENOSPC",
        Errno::ENAMETOOLONG => "ENAMETOOLONG",
        Errno::ENOSYS => "ENOSYS",
        Errno::EOVERFLOW => "EOVERFLOW",
        _ => "other",
    }
}

fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn emit(row: &Row) {
    let ok = Runner::verdict(row);
    let errno = if row.ret < 0 {
        errno_name(Errno(u16::try_from(-row.ret).unwrap_or(u16::MAX)))
    } else {
        "none"
    };
    println!(
        "syscall-fuzz: case nr=0x{:02x} name={:?} vector={} ret={} errno={} ok={}",
        row.call as usize,
        row.call,
        row.vector,
        row.ret,
        errno,
        if ok { "yes" } else { "no" }
    );
}

/// A live thread for the `THREAD_CREATE` baseline; exits without touching shared state.
extern "C" fn probe_thread(_arg: usize) -> ! {
    // SAFETY: `THREAD_EXIT` terminates only this probe thread.
    unsafe { sys::syscall1(Syscall::THREAD_EXIT as usize, 0) };
    loop {
        core::hint::spin_loop();
    }
}

/// `EXIT` with a caller-supplied code, run inside a forked child.
extern "C" fn child_exit(code: usize) -> ! {
    // SAFETY: `EXIT` terminates the calling (child) process.
    unsafe { sys::syscall1(Syscall::EXIT as usize, code) };
    loop {
        core::hint::spin_loop();
    }
}

/// `THREAD_EXIT` in a child whose only thread is this one; the process then exits.
extern "C" fn child_thread_exit(code: usize) -> ! {
    // SAFETY: `THREAD_EXIT` terminates this child thread; it is the child's only thread.
    unsafe { sys::syscall1(Syscall::THREAD_EXIT as usize, code) };
    loop {
        core::hint::spin_loop();
    }
}

/// `PROC_TERMINATE(SELF_PROC)`, run inside a forked child.
extern "C" fn child_self_terminate(code: usize) -> ! {
    // SAFETY: the child terminates only itself; the parent holds the child handle.
    unsafe {
        sys::syscall2(
            Syscall::PROC_TERMINATE as usize,
            Handle::SELF_PROC.0 as usize,
            code,
        )
    };
    loop {
        core::hint::spin_loop();
    }
}

/// Fork a child and start `entry` in it; the parent keeps the child handle.
fn spawn_child(entry: extern "C" fn(usize) -> !, arg: usize) -> Result<usize, Errno> {
    let child = checked(PROC_FORK, [Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0])?;
    let stack = core::ptr::addr_of_mut!(CHILD_STACK);
    checked(
        Syscall::THREAD_CREATE as usize,
        [child, entry as usize, stack_top(stack), arg, 0, 0],
    )?;
    Ok(child)
}

/// Wait for a child and return its exit code.
fn wait_child(child: usize) -> Result<i32, Errno> {
    let mut code = 0i32;
    checked(
        Syscall::PROC_WAIT as usize,
        [child, core::ptr::addr_of_mut!(code) as usize, 0, 0, 0, 0],
    )
    .map(|_| code)
}

/// Spawn a child that runs `entry(arg)`, wait for it, and return the exit code.
fn child_code(_runner: &mut Runner, entry: extern "C" fn(usize) -> !, arg: usize) -> isize {
    match spawn_child(entry, arg) {
        Ok(child) => match wait_child(child) {
            Ok(code) => code as isize,
            Err(error) => -(error.0 as isize),
        },
        Err(error) => -(error.0 as isize),
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    println!(
        "syscall-fuzz: start seed=0x{SEED:016x} syscalls={}",
        Syscall::ALL.len()
    );
    let ctx = match setup() {
        Ok(ctx) => ctx,
        Err(error) => {
            println!("syscall-fuzz: setup-failed errno={:?}", error);
            return 1;
        }
    };
    let mut runner = Runner::new(ctx);
    plan(&mut runner);
    runner.report();
    println!("syscall-fuzz: alive");
    0
}

fn plan(runner: &mut Runner) {
    let ctx = runner.ctx;
    no_argument_and_scalar_calls(runner, ctx);
    pointer_calls(runner, ctx);
    thread_and_process_calls(runner, ctx);
    memory_calls(runner, ctx);
    handle_calls(runner, ctx);
    event_calls(runner, ctx);
    ipc_calls(runner, ctx);
    driver_calls(runner, ctx);
    terminating_calls(runner);
}

fn no_argument_and_scalar_calls(runner: &mut Runner, ctx: Ctx) {
    runner.ok(Syscall::YIELD, "noargs", [0; 6]);
    runner.ok(Syscall::NANOSLEEP, "zero", [0; 6]);
    // RECV has no resource until a scheme is registered: capacity 0 is an argument error, and a
    // full-size buffer with no registered scheme is the kernel's no-server outcome.
    runner.err(Syscall::RECV, "cap0", [0, 0, 0, 0, 0, 0], EINVAL);
    runner.any(Syscall::RECV, "noserver", [ctx.buf, PAGE, 0, 0, 0, 0]);

    runner.ok(
        Syscall::CLOCK_GET,
        "baseline",
        [clock::MONOTONIC as usize, ctx.out, 0, 0, 0, 0],
    );
    runner.err(
        Syscall::CLOCK_GET,
        "clockneg",
        [usize::MAX, ctx.out, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::CLOCK_GET,
        "ptr0",
        [clock::MONOTONIC as usize, 0, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::CLOCK_GET,
        "ptr1",
        [clock::MONOTONIC as usize, 1, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::CLOCK_GET,
        "ptrneg",
        [clock::MONOTONIC as usize, PTRMAX, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::CLOCK_GET,
        "protnone",
        [clock::MONOTONIC as usize, ctx.guard, 0, 0, 0, 0],
        EFAULT,
    );

    runner.ok(Syscall::UNAME, "baseline", [ctx.out, 0, 0, 0, 0, 0]);
    runner.err(Syscall::UNAME, "ptr0", [PTR0, 0, 0, 0, 0, 0], EFAULT);
    runner.err(Syscall::UNAME, "ptr1", [PTR1, 0, 0, 0, 0, 0], EFAULT);
    runner.err(Syscall::UNAME, "ptrneg", [PTRMAX, 0, 0, 0, 0, 0], EFAULT);
    runner.err(
        Syscall::UNAME,
        "protnone",
        [ctx.guard, 0, 0, 0, 0, 0],
        EFAULT,
    );

    runner.ok(Syscall::DEBUG_WRITE, "baseline", [ctx.buf, 1, 0, 0, 0, 0]);
    runner.ok(Syscall::DEBUG_WRITE, "len0", [ctx.buf, 0, 0, 0, 0, 0]);
    runner.err(Syscall::DEBUG_WRITE, "ptr0", [PTR0, 1, 0, 0, 0, 0], EFAULT);
    runner.err(Syscall::DEBUG_WRITE, "ptr1", [PTR1, 1, 0, 0, 0, 0], EFAULT);
    runner.err(
        Syscall::DEBUG_WRITE,
        "ptrneg",
        [PTRMAX, 1, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::DEBUG_WRITE,
        "straddle",
        [ctx.straddle, 16, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::DEBUG_WRITE,
        "protnone",
        [ctx.guard, 1, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::DEBUG_WRITE,
        "lenbig",
        [ctx.buf, PAGE + 1, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::DEBUG_WRITE,
        "lenmax",
        [ctx.buf, LENMAX, 0, 0, 0, 0],
        EINVAL,
    );

    runner.ok(
        Syscall::DEBUG_READ,
        "baseline",
        [ctx.out, 1, 1_000_000, 0, 0, 0],
    );
    runner.ok(Syscall::DEBUG_READ, "len0", [ctx.out, 0, 0, 0, 0, 0]);
    runner.err(
        Syscall::DEBUG_READ,
        "ptr0",
        [PTR0, 1, 1_000_000, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::DEBUG_READ,
        "ptr1",
        [PTR1, 1, 1_000_000, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::DEBUG_READ,
        "ptrneg",
        [PTRMAX, 1, 1_000_000, 0, 0, 0],
        EFAULT,
    );
    runner.any(
        Syscall::DEBUG_READ,
        "straddle",
        [ctx.straddle, 16, 1_000_000, 0, 0, 0],
    );
    runner.any(
        Syscall::DEBUG_READ,
        "protnone",
        [ctx.guard, 1, 1_000_000, 0, 0, 0],
    );
    runner.err(
        Syscall::DEBUG_READ,
        "lenmax",
        [ctx.out, LENMAX, 0, 0, 0, 0],
        EFAULT,
    );
    runner.gap(
        "DEBUG_READ copies only after console input arrives; no input is injected, so the \
         straddle/PROT_NONE destination copy is not reached",
    );

    let word = futex_word();
    runner.ok(
        Syscall::FUTEX,
        "wake",
        [word, futex::WAKE as usize, 1, 0, 0, 0],
    );
    runner.err(
        Syscall::FUTEX,
        "wait-other",
        [word, futex::WAIT as usize, 2, 1_000_000, 0, 0],
        EAGAIN,
    );
    runner.err(
        Syscall::FUTEX,
        "ptr0",
        [PTR0, futex::WAKE as usize, 1, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::FUTEX,
        "ptr1",
        [PTR1, futex::WAKE as usize, 1, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::FUTEX,
        "ptrneg",
        [PTRMAX & !3, futex::WAKE as usize, 1, 0, 0, 0],
        EFAULT,
    );
    runner.ok(
        Syscall::FUTEX,
        "wake-protnone",
        [ctx.guard, futex::WAKE as usize, 1, 0, 0, 0],
    );
    runner.err(
        Syscall::FUTEX,
        "wait-protnone",
        [ctx.guard, futex::WAIT as usize, 1, 1_000_000, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::FUTEX,
        "opneg",
        [word, usize::MAX, 1, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::FUTEX,
        "oprand",
        [word, rand_flags(), 1, 0, 0, 0],
        EINVAL,
    );
}

fn pointer_calls(runner: &mut Runner, ctx: Ctx) {
    let path = core::ptr::addr_of!(PATH) as usize;
    let name = core::ptr::addr_of!(NAME) as usize;

    runner.err(Syscall::OPEN, "ptr0", [PTR0, 7, 0, 0, 0, 0], EFAULT);
    runner.err(Syscall::OPEN, "ptr1", [PTR1, 7, 0, 0, 0, 0], EFAULT);
    runner.err(Syscall::OPEN, "ptrneg", [PTRMAX, 7, 0, 0, 0, 0], EFAULT);
    runner.err(
        Syscall::OPEN,
        "protnone",
        [ctx.guard, 7, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(Syscall::OPEN, "len0", [path, 0, 0, 0, 0, 0], EINVAL);
    runner.err(
        Syscall::OPEN,
        "lenmax",
        [path, LENMAX, 0, 0, 0, 0],
        ENAMETOOLONG,
    );
    runner.err(Syscall::OPEN, "noscheme", [path, 3, 0, 0, 0, 0], EINVAL);
    runner.err(
        Syscall::OPEN,
        "flagsneg",
        [path, 7, usize::MAX, 0, 0, 0],
        EINVAL,
    );
    runner.any(Syscall::OPEN, "baseline", [path, 7, 0, 0, 0, 0]);

    runner.err(
        Syscall::SCHEME_REGISTER,
        "ptr0",
        [PTR0, 4, Handle::SELF_PROC.0 as usize, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::SCHEME_REGISTER,
        "ptr1",
        [PTR1, 4, Handle::SELF_PROC.0 as usize, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::SCHEME_REGISTER,
        "protnone",
        [ctx.guard, 4, Handle::SELF_PROC.0 as usize, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::SCHEME_REGISTER,
        "len0",
        [name, 0, Handle::SELF_PROC.0 as usize, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::SCHEME_REGISTER,
        "lenmax",
        [name, LENMAX, Handle::SELF_PROC.0 as usize, 0, 0, 0],
        ENAMETOOLONG,
    );
    runner.err(
        Syscall::SCHEME_REGISTER,
        "fdneg",
        [name, 4, usize::MAX, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::SCHEME_REGISTER,
        "fdnotproc",
        [name, 4, ctx.memobj, 0, 0, 0],
        EACCES,
    );
    runner.ok(
        Syscall::SCHEME_REGISTER,
        "baseline",
        [name, 4, Handle::SELF_PROC.0 as usize, 0, 0, 0],
    );
}

fn thread_and_process_calls(runner: &mut Runner, ctx: Ctx) {
    let probe = core::ptr::addr_of_mut!(PROBE_STACK);
    let probe_top = stack_top(probe);
    let probe_entry = probe_thread as *const () as usize;
    runner.ok(
        Syscall::THREAD_CREATE,
        "baseline",
        [
            Handle::SELF_PROC.0 as usize,
            probe_entry,
            probe_top,
            0,
            0,
            0,
        ],
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "fdneg",
        [usize::MAX, probe_entry, probe_top, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "fdhuge",
        [0x7fff_ffff, probe_entry, probe_top, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "ptr0",
        [Handle::SELF_PROC.0 as usize, 0, probe_top, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "ptr1",
        [Handle::SELF_PROC.0 as usize, 1, probe_top, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "ptrneg",
        [Handle::SELF_PROC.0 as usize, PTRMAX, probe_top, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "stack0",
        [Handle::SELF_PROC.0 as usize, probe_entry, 0, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "stack1",
        [Handle::SELF_PROC.0 as usize, probe_entry, 1, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "flagsneg",
        [
            Handle::SELF_PROC.0 as usize,
            probe_entry,
            probe_top,
            0,
            usize::MAX,
            0,
        ],
        EINVAL,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "flags2",
        [
            Handle::SELF_PROC.0 as usize,
            probe_entry,
            probe_top,
            0,
            2,
            0,
        ],
        EINVAL,
    );
    runner.err(
        Syscall::THREAD_CREATE,
        "exnull",
        [
            Handle::SELF_PROC.0 as usize,
            probe_entry,
            probe_top,
            0,
            1,
            0,
        ],
        EINVAL,
    );

    runner.ok(Syscall::PROC_CREATE, "baseline", [0; 6]);
    runner.err(
        Syscall::PROC_CREATE,
        "flagsneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::PROC_CREATE,
        "flagsrand",
        [rand_flags(), 0, 0, 0, 0, 0],
        EINVAL,
    );

    runner.err(
        Syscall::PROC_TERMINATE,
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::PROC_TERMINATE,
        "fdhuge",
        [0x7fff_ffff, 0, 0, 0, 0, 0],
        EBADF,
    );
    runner.ok(
        Syscall::PROC_TERMINATE,
        "created",
        [ctx.proc_created, 3, 0, 0, 0, 0],
    );
    runner.err(
        Syscall::PROC_TERMINATE,
        "terminal",
        [ctx.proc_created, 3, 0, 0, 0, 0],
        ESRCH,
    );

    let prefix = core::ptr::addr_of!(PREFIX) as usize;
    runner.err(
        Syscall::PROC_NAMESPACE,
        "fdneg",
        [usize::MAX, namespace::ADD as usize, prefix, 4, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::PROC_NAMESPACE,
        "opneg",
        [Handle::SELF_PROC.0 as usize, usize::MAX, prefix, 4, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::PROC_NAMESPACE,
        "ptr0",
        [
            Handle::SELF_PROC.0 as usize,
            namespace::ADD as usize,
            PTR0,
            4,
            0,
            0,
        ],
        EFAULT,
    );
    runner.err(
        Syscall::PROC_NAMESPACE,
        "ptr1",
        [
            Handle::SELF_PROC.0 as usize,
            namespace::ADD as usize,
            PTR1,
            4,
            0,
            0,
        ],
        EFAULT,
    );
    runner.err(
        Syscall::PROC_NAMESPACE,
        "protnone",
        [
            Handle::SELF_PROC.0 as usize,
            namespace::ADD as usize,
            ctx.guard,
            4,
            0,
            0,
        ],
        EFAULT,
    );
    runner.err(
        Syscall::PROC_NAMESPACE,
        "len0",
        [
            Handle::SELF_PROC.0 as usize,
            namespace::ADD as usize,
            prefix,
            0,
            0,
            0,
        ],
        EINVAL,
    );
    runner.err(
        Syscall::PROC_NAMESPACE,
        "lenmax",
        [
            Handle::SELF_PROC.0 as usize,
            namespace::ADD as usize,
            prefix,
            LENMAX,
            0,
            0,
        ],
        ENAMETOOLONG,
    );
    runner.any(
        Syscall::PROC_NAMESPACE,
        "add",
        [
            Handle::SELF_PROC.0 as usize,
            namespace::ADD as usize,
            prefix,
            4,
            0,
            0,
        ],
    );
    runner.ok(
        Syscall::PROC_NAMESPACE,
        "clear",
        [
            Handle::SELF_PROC.0 as usize,
            namespace::CLEAR as usize,
            0,
            0,
            0,
            0,
        ],
    );
}

fn memory_calls(runner: &mut Runner, ctx: Ctx) {
    let map_base = checked(
        Syscall::MAP as usize,
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            PAGE,
            prot::READ | prot::WRITE,
        ],
    );
    if let Ok(base) = map_base {
        runner.ok(
            Syscall::PROTECT,
            "baseline",
            [Handle::SELF_PROC.0 as usize, base, PAGE, 0, 0, 0],
        );
        runner.err(
            Syscall::PROTECT,
            "protneg",
            [Handle::SELF_PROC.0 as usize, base, PAGE, usize::MAX, 0, 0],
            EINVAL,
        );
        runner.err(
            Syscall::PROTECT,
            "protflag",
            [Handle::SELF_PROC.0 as usize, base, PAGE, 8, 0, 0],
            EINVAL,
        );
        runner.err(
            Syscall::PROTECT,
            "fdneg",
            [usize::MAX, base, PAGE, 0, 0, 0],
            EBADF,
        );
        runner.err(
            Syscall::PROTECT,
            "addr1",
            [Handle::SELF_PROC.0 as usize, 1, PAGE, 0, 0, 0],
            EINVAL,
        );
        runner.err(
            Syscall::PROTECT,
            "len0",
            [Handle::SELF_PROC.0 as usize, base, 0, 0, 0, 0],
            EINVAL,
        );
        runner.err(
            Syscall::PROTECT,
            "lenmax",
            [Handle::SELF_PROC.0 as usize, base, LENMAX, 0, 0, 0],
            EINVAL,
        );
        runner.ok(
            Syscall::UNMAP,
            "baseline",
            [Handle::SELF_PROC.0 as usize, base, PAGE, 0, 0, 0],
        );
        runner.err(
            Syscall::UNMAP,
            "fdneg",
            [usize::MAX, base, PAGE, 0, 0, 0],
            EBADF,
        );
        runner.err(
            Syscall::UNMAP,
            "addr1",
            [Handle::SELF_PROC.0 as usize, 1, PAGE, 0, 0, 0],
            EINVAL,
        );
        runner.err(
            Syscall::UNMAP,
            "len0",
            [Handle::SELF_PROC.0 as usize, base, 0, 0, 0, 0],
            EINVAL,
        );
        runner.err(
            Syscall::UNMAP,
            "lenmax",
            [Handle::SELF_PROC.0 as usize, base, LENMAX, 0, 0, 0],
            EINVAL,
        );
    } else {
        runner.gap("MAP baseline failed; PROTECT/UNMAP baseline rows are not measured");
    }

    runner.ok(
        Syscall::MAP,
        "baseline",
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            PAGE,
            prot::READ | prot::WRITE,
        ],
    );
    runner.err(
        Syscall::MAP,
        "fdneg",
        [usize::MAX, Handle::NONE.0 as usize, 0, 0, PAGE, prot::READ],
        EBADF,
    );
    runner.err(
        Syscall::MAP,
        "memobjhuge",
        [
            Handle::SELF_PROC.0 as usize,
            0x7fff_ffff,
            0,
            0,
            PAGE,
            prot::READ,
        ],
        EBADF,
    );
    runner.err(
        Syscall::MAP,
        "len0",
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            0,
            prot::READ,
        ],
        EINVAL,
    );
    runner.err(
        Syscall::MAP,
        "lenbig",
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            PAGE + 1,
            prot::READ,
        ],
        EINVAL,
    );
    runner.err(
        Syscall::MAP,
        "lenmax",
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            LENMAX,
            prot::READ,
        ],
        EINVAL,
    );
    runner.err(
        Syscall::MAP,
        "flagsneg",
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            0,
            PAGE,
            usize::MAX,
        ],
        EINVAL,
    );
    runner.err(
        Syscall::MAP,
        "addr1",
        [
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            1,
            PAGE,
            prot::READ | (map::FIXED << map::SHIFT),
        ],
        EINVAL,
    );
    runner.err(
        Syscall::MAP,
        "offset1",
        [
            Handle::SELF_PROC.0 as usize,
            ctx.memobj,
            1,
            0,
            PAGE,
            prot::READ,
        ],
        EINVAL,
    );

    runner.ok(Syscall::MEMOBJ_CREATE, "baseline", [PAGE, 0, 0, 0, 0, 0]);
    runner.err(Syscall::MEMOBJ_CREATE, "len0", [0, 0, 0, 0, 0, 0], EINVAL);
    runner.err(
        Syscall::MEMOBJ_CREATE,
        "lenbig",
        [PAGE + 1, 0, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::MEMOBJ_CREATE,
        "lenmax",
        [LENMAX, 0, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::MEMOBJ_CREATE,
        "flagsneg",
        [PAGE, usize::MAX, 0, 0, 0, 0],
        EINVAL,
    );

    runner.ok(
        Syscall::PAGER_SUPPLY,
        "baseline",
        [ctx.paged, 0, ctx.buf, 1, 0, 0],
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "fdneg",
        [usize::MAX, 0, ctx.buf, 1, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "notpager",
        [ctx.memobj, 0, ctx.buf, 1, 0, 0],
        EPERM,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "offset1",
        [ctx.paged, 1, ctx.buf, 1, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "offsetpast",
        [ctx.paged, PAGE, ctx.buf, 1, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "len0",
        [ctx.paged, 0, ctx.buf, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "lenbig",
        [ctx.paged, 0, ctx.buf, PAGE + 1, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "ptr0",
        [ctx.paged, 0, PTR0, 1, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "ptr1",
        [ctx.paged, 0, PTR1, 1, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "ptrneg",
        [ctx.paged, 0, PTRMAX, 1, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::PAGER_SUPPLY,
        "protnone",
        [ctx.paged2, 0, ctx.guard, 1, 0, 0],
        EFAULT,
    );
}

fn handle_calls(runner: &mut Runner, ctx: Ctx) {
    let duplicate = checked(
        Syscall::HANDLE_DUP as usize,
        [ctx.pipe_r, RIGHTS_ALL, 0, 0, 0, 0],
    );
    if let Ok(dup) = duplicate {
        runner.ok(Syscall::HANDLE_CLOSE, "baseline", [dup, 0, 0, 0, 0, 0]);
    }
    runner.err(
        Syscall::HANDLE_CLOSE,
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::HANDLE_CLOSE,
        "fdhuge",
        [0x7fff_ffff, 0, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::HANDLE_CLOSE,
        "fdclosed",
        [ctx.closed, 0, 0, 0, 0, 0],
        EBADF,
    );

    runner.ok(
        Syscall::HANDLE_DUP,
        "baseline",
        [ctx.memobj, RIGHTS_ALL, 0, 0, 0, 0],
    );
    runner.err(
        Syscall::HANDLE_DUP,
        "fdneg",
        [usize::MAX, RIGHTS_ALL, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::HANDLE_DUP,
        "fdhuge",
        [0x7fff_ffff, RIGHTS_ALL, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::HANDLE_DUP,
        "fdclosed",
        [ctx.closed, RIGHTS_ALL, 0, 0, 0, 0],
        EBADF,
    );
    runner.ok(Syscall::HANDLE_DUP, "mask0", [ctx.memobj, 0, 0, 0, 0, 0]);

    runner.ok(
        Syscall::HANDLE_TRANSFER,
        "baseline",
        [ctx.proc_created, ctx.memobj, RIGHTS_ALL, 0, 0, 0],
    );
    runner.err(
        Syscall::HANDLE_TRANSFER,
        "fdneg",
        [usize::MAX, ctx.memobj, RIGHTS_ALL, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::HANDLE_TRANSFER,
        "destnotproc",
        [ctx.memobj, ctx.memobj, RIGHTS_ALL, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::HANDLE_TRANSFER,
        "srcneg",
        [ctx.proc_created, usize::MAX, RIGHTS_ALL, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::HANDLE_TRANSFER,
        "mask0",
        [ctx.proc_created, ctx.memobj, 0, 0, 0, 0],
        EINVAL,
    );

    let revoke = checked(
        Syscall::HANDLE_DUP as usize,
        [ctx.memobj, RIGHTS_ALL, 0, 0, 0, 0],
    );
    if let Ok(revoke) = revoke {
        runner.ok(
            Syscall::HANDLE_REVOKE,
            "baseline",
            [revoke, RIGHTS_ALL, 0, 0, 0, 0],
        );
    }
    runner.err(
        Syscall::HANDLE_REVOKE,
        "fdneg",
        [usize::MAX, RIGHTS_ALL, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::HANDLE_REVOKE,
        "fdclosed",
        [ctx.closed, RIGHTS_ALL, 0, 0, 0, 0],
        EBADF,
    );
}

fn event_calls(runner: &mut Runner, ctx: Ctx) {
    runner.ok(Syscall::EVQ_CREATE, "baseline", [0; 6]);
    runner.err(
        Syscall::EVQ_CREATE,
        "flagsneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(Syscall::EVQ_CREATE, "flags1", [1, 0, 0, 0, 0, 0], EINVAL);

    runner.ok(
        Syscall::EVQ_WAIT,
        "empty",
        [ctx.evq, ctx.out, 1, 1_000_000, 0, 0],
    );
    runner.err(
        Syscall::EVQ_WAIT,
        "fdneg",
        [usize::MAX, ctx.out, 1, 1_000_000, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::EVQ_WAIT,
        "ptr0",
        [ctx.evq, 0, 1, 1_000_000, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::EVQ_WAIT,
        "ptr1",
        [ctx.evq, 1, 1, 1_000_000, 0, 0],
        EFAULT,
    );
    runner.ok(Syscall::EVQ_WAIT, "max0", [ctx.evq, ctx.out, 0, 0, 0, 0]);
    runner.err(
        Syscall::EVQ_WAIT,
        "maxmax",
        [ctx.evq, ctx.out, LENMAX, 1_000_000, 0, 0],
        EINVAL,
    );

    runner.ok(
        Syscall::EVQ_CTL,
        "add",
        [ctx.evq, 1, ctx.event, SIGNALED, 0x1234, 0],
    );
    runner.err(
        Syscall::EVQ_CTL,
        "fdneg",
        [usize::MAX, 1, ctx.event, SIGNALED, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::EVQ_CTL,
        "opneg",
        [ctx.evq, usize::MAX, ctx.event, SIGNALED, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::EVQ_CTL,
        "srcneg",
        [ctx.evq, 1, usize::MAX, SIGNALED, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::EVQ_CTL,
        "eventsneg",
        [ctx.evq, 1, ctx.event, usize::MAX, 0, 0],
        EINVAL,
    );

    runner.ok(
        Syscall::EVENT_SIGNAL,
        "baseline",
        [ctx.event, 1, 0, 0, 0, 0],
    );
    runner.err(
        Syscall::EVENT_SIGNAL,
        "fdneg",
        [usize::MAX, 1, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::EVENT_SIGNAL,
        "fdclosed",
        [ctx.closed, 1, 0, 0, 0, 0],
        EBADF,
    );
    runner.ok(
        Syscall::EVENT_SIGNAL,
        "valuemax",
        [ctx.event, PTRMAX, 0, 0, 0, 0],
    );
    runner.ok(
        Syscall::EVQ_WAIT,
        "ready",
        [ctx.evq, ctx.out, 1, 1_000_000, 0, 0],
    );

    runner.ok(
        Syscall::TIMER_CREATE,
        "baseline",
        [clock::MONOTONIC as usize, 0, 0, 0, 0, 0],
    );
    runner.err(
        Syscall::TIMER_CREATE,
        "clockbad",
        [0, 0, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::TIMER_CREATE,
        "flagsneg",
        [clock::MONOTONIC as usize, usize::MAX, 0, 0, 0, 0],
        EINVAL,
    );

    runner.ok(Syscall::EVENT_CREATE, "baseline", [0; 6]);
    runner.err(
        Syscall::EVENT_CREATE,
        "flagsneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        EINVAL,
    );

    runner.ok(Syscall::TIMER_SET, "disarm", [ctx.timer, 0, 0, 0, 0, 0]);
    runner.err(
        Syscall::TIMER_SET,
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::TIMER_SET,
        "fdclosed",
        [ctx.closed, 0, 0, 0, 0, 0],
        EBADF,
    );
}

fn ipc_calls(runner: &mut Runner, ctx: Ctx) {
    // The read baseline needs one byte in the pipe; the invalid vectors follow it.
    let _ = raw(Syscall::WRITE as usize, [ctx.pipe_w, ctx.buf, 1, 0, 0, 0]);
    runner.ok(Syscall::READ, "baseline", [ctx.pipe_r, ctx.buf, 1, 0, 0, 0]);
    runner.err(
        Syscall::READ,
        "fdneg",
        [usize::MAX, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::READ,
        "fdhuge",
        [0x7fff_ffff, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::READ,
        "fdclosed",
        [ctx.closed, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::READ,
        "fdwrongdir",
        [ctx.pipe_w, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(Syscall::READ, "ptr0", [ctx.pipe_r, 0, 1, 0, 0, 0], EFAULT);
    runner.err(Syscall::READ, "ptr1", [ctx.pipe_r, 1, 1, 0, 0, 0], EFAULT);
    runner.err(
        Syscall::READ,
        "ptrneg",
        [ctx.pipe_r, PTRMAX, 1, 0, 0, 0],
        EFAULT,
    );
    let _ = raw(Syscall::WRITE as usize, [ctx.pipe_w, ctx.buf, 16, 0, 0, 0]);
    runner.err(
        Syscall::READ,
        "straddle",
        [ctx.pipe_r, ctx.straddle, 16, 0, 0, 0],
        EFAULT,
    );
    let _ = raw(Syscall::WRITE as usize, [ctx.pipe_w, ctx.buf, 16, 0, 0, 0]);
    runner.err(
        Syscall::READ,
        "protnone",
        [ctx.pipe_r, ctx.guard, 1, 0, 0, 0],
        EFAULT,
    );
    runner.ok(Syscall::READ, "len0", [ctx.pipe_r, ctx.buf, 0, 0, 0, 0]);
    runner.err(
        Syscall::READ,
        "lenmax",
        [ctx.pipe_r, ctx.buf, LENMAX, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::READ,
        "lenmax7",
        [ctx.pipe_r, ctx.buf, LENMAX7, 0, 0, 0],
        EFAULT,
    );

    runner.ok(
        Syscall::WRITE,
        "baseline",
        [ctx.pipe_w, ctx.buf, 1, 0, 0, 0],
    );
    runner.err(
        Syscall::WRITE,
        "fdneg",
        [usize::MAX, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::WRITE,
        "fdhuge",
        [0x7fff_ffff, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::WRITE,
        "fdclosed",
        [ctx.closed, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::WRITE,
        "fdwrongdir",
        [ctx.pipe_r, ctx.buf, 1, 0, 0, 0],
        EBADF,
    );
    runner.err(Syscall::WRITE, "ptr0", [ctx.pipe_w, 0, 1, 0, 0, 0], EFAULT);
    runner.err(Syscall::WRITE, "ptr1", [ctx.pipe_w, 1, 1, 0, 0, 0], EFAULT);
    runner.err(
        Syscall::WRITE,
        "ptrneg",
        [ctx.pipe_w, PTRMAX, 1, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::WRITE,
        "straddle",
        [ctx.pipe_w, ctx.straddle, 16, 0, 0, 0],
        EFAULT,
    );
    runner.err(
        Syscall::WRITE,
        "protnone",
        [ctx.pipe_w, ctx.guard, 1, 0, 0, 0],
        EFAULT,
    );
    runner.ok(Syscall::WRITE, "len0", [ctx.pipe_w, ctx.buf, 0, 0, 0, 0]);
    runner.err(
        Syscall::WRITE,
        "lenmax",
        [ctx.pipe_w, ctx.buf, LENMAX, 0, 0, 0],
        EFAULT,
    );

    let close = checked(
        Syscall::HANDLE_DUP as usize,
        [ctx.pipe_r, RIGHTS_ALL, 0, 0, 0, 0],
    );
    if let Ok(close) = close {
        runner.ok(Syscall::CLOSE, "baseline", [close, 0, 0, 0, 0, 0]);
    }
    runner.err(Syscall::CLOSE, "fdneg", [usize::MAX, 0, 0, 0, 0, 0], EBADF);
    runner.err(
        Syscall::CLOSE,
        "fdclosed",
        [ctx.closed, 0, 0, 0, 0, 0],
        EBADF,
    );

    runner.any(
        Syscall::DUP,
        "baseline",
        [ctx.pipe_r, RIGHTS_ALL, 0, 0, 0, 0],
    );
    runner.gap("DUP is the scheme-file variant; the initfs has no scheme file, so the baseline is not exercised");
    runner.err(
        Syscall::DUP,
        "fdneg",
        [usize::MAX, RIGHTS_ALL, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::DUP,
        "fdclosed",
        [ctx.closed, RIGHTS_ALL, 0, 0, 0, 0],
        EBADF,
    );

    // FSTAT/CALL/SEND need a registered scheme file. The initfs carries no vfsd, so the pipe is
    // the only file object: record the handle error and mark the pointer class as a coverage gap.
    runner.err(
        Syscall::FSTAT,
        "fdneg",
        [usize::MAX, ctx.out, 64, 0, 0, 0],
        EBADF,
    );
    runner.any(Syscall::FSTAT, "pipe", [ctx.pipe_r, ctx.out, 64, 0, 0, 0]);
    runner.err(
        Syscall::CALL,
        "fdneg",
        [usize::MAX, ctx.buf, 8, ctx.out, 64, 0],
        EBADF,
    );
    runner.err(
        Syscall::SEND,
        "fdneg",
        [usize::MAX, ctx.buf, 8, 0, 0, 0],
        EBADF,
    );
    runner.gap("FSTAT/CALL/SEND pointer vectors need a scheme server; the initfs has none");
}

fn driver_calls(runner: &mut Runner, ctx: Ctx) {
    runner.any(
        Syscall::MMIO_CREATE,
        "baseline",
        [Handle::ROOT_DEVICE.0 as usize, 0, PAGE, 0, 0, 0],
    );
    runner.err(
        Syscall::MMIO_CREATE,
        "fdneg",
        [usize::MAX, 0, PAGE, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::MMIO_CREATE,
        "len0",
        [Handle::ROOT_DEVICE.0 as usize, 0, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::MMIO_CREATE,
        "lenbig",
        [Handle::ROOT_DEVICE.0 as usize, 0, PAGE + 1, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::MMIO_CREATE,
        "attrneg",
        [Handle::ROOT_DEVICE.0 as usize, 0, PAGE, usize::MAX, 0, 0],
        EINVAL,
    );

    runner.any(
        Syscall::BOOT_BLOB,
        "baseline",
        [Handle::ROOT_DEVICE.0 as usize, 3, 0, 0, 0, 0],
    );
    runner.err(
        Syscall::BOOT_BLOB,
        "fdneg",
        [usize::MAX, 3, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::BOOT_BLOB,
        "kind0",
        [Handle::ROOT_DEVICE.0 as usize, 0, 0, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::BOOT_BLOB,
        "kindmax",
        [Handle::ROOT_DEVICE.0 as usize, usize::MAX, 0, 0, 0, 0],
        EINVAL,
    );

    runner.any(
        Syscall::DMA_ALLOC,
        "baseline",
        [Handle::ROOT_DEVICE.0 as usize, PAGE, 0, 32, 0, 0],
    );
    runner.err(
        Syscall::DMA_ALLOC,
        "fdneg",
        [usize::MAX, PAGE, 0, 32, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::DMA_ALLOC,
        "len0",
        [Handle::ROOT_DEVICE.0 as usize, 0, 0, 32, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::DMA_ALLOC,
        "lenbig",
        [Handle::ROOT_DEVICE.0 as usize, PAGE + 1, 0, 32, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::DMA_ALLOC,
        "alignbad",
        [Handle::ROOT_DEVICE.0 as usize, PAGE, 3 * PAGE, 32, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::DMA_ALLOC,
        "masklow",
        [Handle::ROOT_DEVICE.0 as usize, PAGE, 0, 19, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::DMA_ALLOC,
        "flagsneg",
        [Handle::ROOT_DEVICE.0 as usize, PAGE, 0, 32, usize::MAX, 0],
        EINVAL,
    );

    runner.err(
        Syscall::DMA_INFO,
        "fdneg",
        [usize::MAX, ctx.out, 0, 0, 0, 0],
        EBADF,
    );
    if let Some(dma) = ctx.dma {
        runner.err(Syscall::DMA_INFO, "ptr0", [dma, 0, 0, 0, 0, 0], EFAULT);
        runner.ok(Syscall::DMA_INFO, "baseline", [dma, ctx.out, 0, 0, 0, 0]);
    } else {
        runner.gap("DMA_ALLOC did not return a handle; DMA_INFO pointer vector is not measured");
    }

    let irq = raw(
        Syscall::IRQ_CREATE as usize,
        [Handle::ROOT_DEVICE.0 as usize, 1, 9, 0, 0, 0],
    );
    runner.push(Row {
        call: Syscall::IRQ_CREATE,
        vector: "baseline",
        ret: irq,
        expect: Expect::Any,
    });
    if irq >= 0 {
        let irq = irq as usize;
        runner.ok(Syscall::IRQ_WAIT, "timeout", [irq, 1_000_000, 0, 0, 0, 0]);
        runner.ok(Syscall::IRQ_ACK, "baseline", [irq, 0, 0, 0, 0, 0]);
    } else {
        runner.gap("IRQ_CREATE returned an error; IRQ_WAIT/ACK baselines are not measured");
    }
    runner.err(
        Syscall::IRQ_CREATE,
        "fdneg",
        [usize::MAX, 1, 9, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::IRQ_CREATE,
        "kind0",
        [Handle::ROOT_DEVICE.0 as usize, 0, 9, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::IRQ_CREATE,
        "kindmax",
        [Handle::ROOT_DEVICE.0 as usize, usize::MAX, 9, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::IRQ_CREATE,
        "numberbig",
        [Handle::ROOT_DEVICE.0 as usize, 1, 0x1_0000, 0, 0, 0],
        EINVAL,
    );
    runner.err(
        Syscall::IRQ_CREATE,
        "flags1",
        [Handle::ROOT_DEVICE.0 as usize, 1, 9, 1, 0, 0],
        EINVAL,
    );

    runner.err(
        Syscall::IRQ_WAIT,
        "fdneg",
        [usize::MAX, 1_000_000, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::IRQ_ACK,
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        EBADF,
    );
}

fn terminating_calls(runner: &mut Runner) {
    let ctx = runner.ctx;
    runner.err(
        Syscall::PROC_WAIT,
        "fdneg",
        [usize::MAX, ctx.out, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::PROC_WAIT,
        "fdhuge",
        [0x7fff_ffff, ctx.out, 0, 0, 0, 0],
        EBADF,
    );
    runner.err(
        Syscall::PROC_WAIT,
        "self",
        [Handle::SELF_PROC.0 as usize, ctx.out, 0, 0, 0, 0],
        EINVAL,
    );
    match spawn_child(child_exit, 5) {
        Ok(child) => {
            runner.err(Syscall::PROC_WAIT, "ptr0", [child, 0, 0, 0, 0, 0], EFAULT);
            runner.err(Syscall::PROC_WAIT, "ptr1", [child, 1, 0, 0, 0, 0], EFAULT);
            let code = match wait_child(child) {
                Ok(code) => code as isize,
                Err(error) => -(error.0 as isize),
            };
            runner.push(Row {
                call: Syscall::PROC_WAIT,
                vector: "child-code",
                ret: code,
                expect: Expect::ChildCode(5),
            });
        }
        Err(_) => runner.gap("PROC_WAIT child setup failed; pointer/code rows are not measured"),
    }

    let code = child_code(runner, child_exit, 7);
    runner.push(Row {
        call: Syscall::EXIT,
        vector: "child7",
        ret: code,
        expect: Expect::ChildCode(7),
    });
    let code = child_code(runner, child_exit, 0x7fff_ffff);
    runner.push(Row {
        call: Syscall::EXIT,
        vector: "childmax",
        ret: code,
        expect: Expect::Any,
    });
    let code = child_code(runner, child_thread_exit, 7);
    runner.push(Row {
        call: Syscall::THREAD_EXIT,
        vector: "child7",
        ret: code,
        expect: Expect::ChildCode(7),
    });
    let code = child_code(runner, child_self_terminate, 9);
    runner.push(Row {
        call: Syscall::PROC_TERMINATE,
        vector: "childself9",
        ret: code,
        expect: Expect::ChildCode(9),
    });
}

/// Deterministic 64-bit value used as the "seeded random" flags/opcode vector.
fn rand_flags() -> usize {
    let mut state = SEED;
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    let low = (state & 0xffff_ffff) as usize;
    let high = ((state >> 32) & 0xffff_ffff) as usize;
    low | (high << 32)
}
