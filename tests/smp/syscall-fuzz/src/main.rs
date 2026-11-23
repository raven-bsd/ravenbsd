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
/// `EVQ_CTL` readiness bit for a signalled user event.
const SIGNALED: usize = 8;
/// A valid rights mask for the handle vectors.
const RIGHTS_ALL: usize = u32::MAX as usize;
/// The rights a freshly created memory object carries (`READ|WRITE|MAP|DUP|TRANSFER`).
const MEMOBJ_RIGHTS: usize = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 4) | (1 << 5);

const PTR0: usize = 0;
const PTR1: usize = 1;
const PTRMAX: usize = usize::MAX;
const LENMAX: usize = usize::MAX;
const LENMAX7: usize = usize::MAX - 7;

// Raw pre-enum dispatch numbers: `kernel/src/syscall/mod.rs` tests these before
// `match Syscall::from_usize(number)`. The checked list `raw-dispatch.list` is compared with the
// kernel source by `raw-check.sh`; a drift fails the fixture before boot.
const RAW_GETRANDOM: usize = 0x08;
const RAW_CLOCK_SET: usize = 0x09;
const RAW_TRACE_SET: usize = 0x0b;
const RAW_RECORD_LOCK: usize = 0x0c;
const RAW_PROC_CHROOT: usize = 0x0d;
const RAW_THREAD_SET_TLS: usize = 0x23;
const RAW_THREAD_SELF: usize = 0x24;
const RAW_THREAD_EXIT_EX: usize = 0x25;
const RAW_ROBUST_LIST_SET: usize = 0x26;
const RAW_PROC_SET_PERSONALITY: usize = 0x34;
const RAW_PROC_SELF_INFO: usize = 0x35;
const RAW_PROC_SETPGID: usize = 0x36;
const RAW_PROC_SETSID: usize = 0x37;
const RAW_PROC_SETCRED: usize = 0x38;
const RAW_PROC_FORK: usize = 0x39;
const RAW_PROC_PID_INFO: usize = 0x3a;
const RAW_PROC_SETCRED_EX: usize = 0x3b;
const RAW_PROC_CRED_INFO: usize = 0x3c;
const RAW_PROC_GETGROUPS: usize = 0x3d;
const RAW_PROC_HANDLE_INFO: usize = 0x3e;
const RAW_PROC_EXEC_CRED: usize = 0x3f;
const RAW_MSYNC: usize = 0x44;
const RAW_PROC_SET_COMMAND: usize = 0x45;
const RAW_PROC_WAIT_EX: usize = 0x48;
const RAW_PROC_GETPGID: usize = 0x49;
const RAW_PROC_SIGNAL: usize = 0x4a;
const RAW_PROC_SIGRECV: usize = 0x4b;
const RAW_PROC_SIGRECV_EX: usize = 0x85;
const RAW_PROC_SIGQUEUE: usize = 0x86;
const RAW_PROC_TIMES: usize = 0x4c;
const RAW_PROC_GETRUSAGE: usize = 0x4d;
const RAW_PROC_GETRLIMIT: usize = 0x4e;
const RAW_PROC_SETRLIMIT: usize = 0x4f;
const RAW_HANDLE_COPY: usize = 0x54;
const RAW_HANDLE_TRANSFER_FILE: usize = 0x55;
const RAW_PROC_EXEC: usize = 0x6a;
const RAW_PREAD: usize = 0x6b;
const RAW_PWRITE: usize = 0x6c;
const RAW_SEEK: usize = 0x6d;
const RAW_PIPE: usize = 0x6e;
const RAW_PIPE2: usize = 0x6f;
const RAW_GETPRIORITY: usize = 0x78;
const RAW_SETPRIORITY: usize = 0x79;
const RAW_NICE: usize = 0x7a;
const RAW_SCHED_GET_PRIORITY_MAX: usize = 0x7b;
const RAW_SCHED_GET_PRIORITY_MIN: usize = 0x7c;
const RAW_SCHED_SETSCHEDULER: usize = 0x7d;
const RAW_SCHED_GETSCHEDULER: usize = 0x7e;
const RAW_SIG_SETUP: usize = 0x80;
const RAW_SIGRETURN: usize = 0x82;
const RAW_THREAD_KICK: usize = 0x81;
const RAW_SIGRESEND: usize = 0x84;
const RAW_DTRACE_PROBE_LIST: usize = 0x90;
const RAW_DTRACE_ENABLE: usize = 0x91;
const RAW_DTRACE_DISABLE: usize = 0x92;
const RAW_DTRACE_READ: usize = 0x93;
const RAW_DTRACE_STATS: usize = 0x94;
const RAW_DTRACE_CLEAR: usize = 0x95;
const RAW_DUPLEX_PAIR: usize = 0x110;
const RAW_DUPLEX_SHUTDOWN: usize = 0x111;
const RAW_DUPLEX_SENDMSG: usize = 0x112;
const RAW_DUPLEX_RECVMSG: usize = 0x113;
const RAW_DUPLEX_PEERCRED: usize = 0x114;
const RAW_PROC_CTTY_SET: usize = 0x1e0;
const RAW_PROC_CTTY_GET: usize = 0x1e1;
const RAW_PROC_CTTY_SET_FG: usize = 0x1e2;
const RAW_PROC_CTTY_HANDLE: usize = 0x1e3;
/// Marker for the Linux-personality branch; it has no single syscall number.
const RAW_LINUX_TRAP: usize = 0;

/// Entries before the enum that are deliberately not called. The reason is part of the audit and
/// `raw-check.sh` asserts that no `RAW_<name>` call appears in this source.
const RAW_EXCLUDED: &[(&str, &str)] = &[(
    "PROC_REBOOT",
    "exits the machine (poweroff/halt/reset); no bounded non-destructive variant",
)];

const EFAULT: &[Errno] = &[Errno::EFAULT];
const EBADF: &[Errno] = &[Errno::EBADF];
const EINVAL: &[Errno] = &[Errno::EINVAL];
const EPIPE: &[Errno] = &[Errno::EPIPE];
const EOPNOTSUPP: &[Errno] = &[Errno::EOPNOTSUPP];
const ENAMETOOLONG: &[Errno] = &[Errno::ENAMETOOLONG];
const EACCES: &[Errno] = &[Errno::EACCES];
const EAGAIN: &[Errno] = &[Errno::EAGAIN];
const EPERM: &[Errno] = &[Errno::EPERM];
const ESRCH: &[Errno] = &[Errno::ESRCH];
const ENOTSUP: &[Errno] = &[Errno::ENOTSUP];
const EFBIG: &[Errno] = &[Errno::EFBIG];
const ENOTTY: &[Errno] = &[Errno::ENOTTY];

/// The IRQ id namespace is platform-defined: x86-64 numbers ISA lines, aarch64 numbers GIC
/// interrupt ids (SPI + 32). The vector is the same on both; only the valid baseline id differs.
#[cfg(target_arch = "aarch64")]
const IRQ_BASELINE: usize = 64;
#[cfg(not(target_arch = "aarch64"))]
const IRQ_BASELINE: usize = 9;

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
    /// The result must be this exact non-negative value (a full getrandom length).
    Exact(usize),
    /// A forked child must exit with this code while the parent survives.
    ChildCode(i32),
}

/// One measured syscall call. `nr` is the enum value or the raw pre-enum number; `name` is its
/// stable label (enum name or the raw constant name).
struct Row {
    nr: usize,
    name: alloc::string::String,
    vector: &'static str,
    ret: isize,
    expect: Expect,
    /// True for a success row whose value is an ASLR-randomized address. The verdict still
    /// asserts success, but the address is not part of the deterministic table hash (K502).
    normalized: bool,
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

/// Name the setup step that failed. The architecture parity report needs the exact syscall,
/// not just the errno; this is test instrumentation only.
fn setup_step(name: &'static str, result: Result<usize, Errno>) -> Result<usize, Errno> {
    result.map_err(|error| {
        println!("syscall-fuzz: setup-failed step={name} errno={error:?}");
        error
    })
}

/// A pager-backed object is optional for the non-pager rows. aarch64 currently returns ENOSYS
/// from the kernel cfg gate; record the gap and use handle 0. The PAGER_SUPPLY rows keep the
/// contract expectation, so that gap is reported as a deviation with the kernel finding.
fn optional_pager_step(name: &'static str, result: Result<usize, Errno>) -> Result<usize, Errno> {
    match result {
        Ok(handle) => Ok(handle),
        Err(Errno::ENOSYS) => {
            println!("syscall-fuzz: pager-gap step={name} errno=ENOSYS");
            Ok(0)
        }
        Err(error) => {
            println!("syscall-fuzz: setup-failed step={name} errno={error:?}");
            Err(error)
        }
    }
}

fn setup() -> Result<Ctx, Errno> {
    let buf = core::ptr::addr_of!(BUF) as usize;
    let out = core::ptr::addr_of!(OUT) as usize;
    let base = setup_step(
        "MAP",
        checked(
            Syscall::MAP as usize,
            [
                Handle::SELF_PROC.0 as usize,
                Handle::NONE.0 as usize,
                0,
                0,
                2 * PAGE,
                prot::READ | prot::WRITE,
            ],
        ),
    )?;
    setup_step(
        "PROTECT",
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
        ),
    )?;
    let mut fds = [0u32; 2];
    setup_step(
        "PIPE",
        checked(RAW_PIPE, [fds.as_mut_ptr() as usize, 0, 0, 0, 0, 0]),
    )?;
    let (pipe_r, pipe_w) = (fds[0] as usize, fds[1] as usize);
    let memobj = setup_step(
        "MEMOBJ_CREATE",
        checked(Syscall::MEMOBJ_CREATE as usize, [PAGE, 0, 0, 0, 0, 0]),
    )?;
    let paged = optional_pager_step(
        "MEMOBJ_CREATE_PAGER1",
        checked(
            Syscall::MEMOBJ_CREATE as usize,
            [PAGE, memobj::PAGER, 0, 0, 0, 0],
        ),
    )?;
    let paged2 = optional_pager_step(
        "MEMOBJ_CREATE_PAGER2",
        checked(
            Syscall::MEMOBJ_CREATE as usize,
            [PAGE, memobj::PAGER, 0, 0, 0, 0],
        ),
    )?;
    let event = setup_step(
        "EVENT_CREATE",
        checked(Syscall::EVENT_CREATE as usize, [0; 6]),
    )?;
    let timer = setup_step(
        "TIMER_CREATE",
        checked(
            Syscall::TIMER_CREATE as usize,
            [clock::MONOTONIC as usize, 0, 0, 0, 0, 0],
        ),
    )?;
    let evq = setup_step("EVQ_CREATE", checked(Syscall::EVQ_CREATE as usize, [0; 6]))?;
    let proc_created = setup_step(
        "PROC_CREATE",
        checked(Syscall::PROC_CREATE as usize, [0; 6]),
    )?;
    let dup = setup_step(
        "HANDLE_DUP",
        checked(
            Syscall::HANDLE_DUP as usize,
            [pipe_r, RIGHTS_ALL, 0, 0, 0, 0],
        ),
    )?;
    setup_step(
        "HANDLE_CLOSE",
        checked(Syscall::HANDLE_CLOSE as usize, [dup, 0, 0, 0, 0, 0]),
    )?;
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
    raw_calls: usize,
}

impl Runner {
    fn new(ctx: Ctx) -> Self {
        Self {
            ctx,
            rows: Vec::new(),
            gaps: Vec::new(),
            raw_calls: 0,
        }
    }

    fn case(&mut self, call: Syscall, vector: &'static str, args: [usize; 6], expect: Expect) {
        let ret = raw(call as usize, args);
        self.push(Row {
            nr: call as usize,
            name: format!("{call:?}"),
            vector,
            ret,
            expect,
            normalized: false,
        });
    }

    /// Record one raw pre-enum dispatch call by number and stable name.
    fn raw(
        &mut self,
        number: usize,
        name: &'static str,
        vector: &'static str,
        args: [usize; 6],
        expect: Expect,
    ) {
        let ret = raw(number, args);
        self.raw_calls = self.raw_calls.saturating_add(1);
        self.push(Row {
            nr: number,
            name: alloc::string::String::from(name),
            vector,
            ret,
            expect,
            normalized: false,
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

    /// A success row whose return is an ASLR-randomized address: assert success, but normalize
    /// the address out of the deterministic table hash (K502).
    fn address(&mut self, call: Syscall, vector: &'static str, args: [usize; 6]) {
        let ret = raw(call as usize, args);
        self.push(Row {
            nr: call as usize,
            name: format!("{call:?}"),
            vector,
            ret,
            expect: Expect::Success,
            normalized: true,
        });
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
            Expect::Exact(value) => row.ret == value as isize,
            Expect::ChildCode(code) => row.ret == code as isize,
        }
    }

    fn report(&self) {
        for gap in &self.gaps {
            println!("syscall-fuzz: coverage-gap {gap}");
        }
        for (name, reason) in RAW_EXCLUDED {
            println!("syscall-fuzz: raw-excluded name={name} reason={reason}");
        }
        println!(
            "syscall-fuzz: raw-audit fuzzed={} excluded={}",
            self.raw_calls,
            RAW_EXCLUDED.len()
        );
        for row in &self.rows {
            if Self::verdict(row) {
                continue;
            }
            let expected = match row.expect {
                Expect::Success => alloc::string::String::from("success"),
                Expect::Any => alloc::string::String::from("any"),
                Expect::Err(set) => format!("{set:?}"),
                Expect::Exact(value) => format!("exact={value}"),
                Expect::ChildCode(code) => format!("child-code={code}"),
            };
            println!(
                "syscall-fuzz: deviation nr=0x{:02x} name={} vector={} expected={} actual={}",
                row.nr, row.name, row.vector, expected, row.ret
            );
        }
        let deviations = self.rows.iter().filter(|row| !Self::verdict(row)).count();
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for row in &self.rows {
            hash = fnv(hash, &(row.nr as u64).to_le_bytes());
            hash = fnv(hash, row.vector.as_bytes());
            // `Expect::Any` rows deliberately do not assert a value; K502 ASLR makes the MAP
            // baseline address differ per boot. Hash a fixed sentinel for those rows so the
            // determinism check still covers every asserted row without hashing an address.
            let ret = if row.normalized || matches!(row.expect, Expect::Any) {
                0
            } else {
                row.ret as i64
            };
            hash = fnv(hash, &ret.to_le_bytes());
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
        Errno::EIO => "EIO",
        Errno::E2BIG => "E2BIG",
        Errno::ENOEXEC => "ENOEXEC",
        Errno::ECHILD => "ECHILD",
        Errno::EBADF => "EBADF",
        Errno::EAGAIN => "EAGAIN",
        Errno::ENOMEM => "ENOMEM",
        Errno::EACCES => "EACCES",
        Errno::EFAULT => "EFAULT",
        Errno::EBUSY => "EBUSY",
        Errno::EEXIST => "EEXIST",
        Errno::ENODEV => "ENODEV",
        Errno::ENOTTY => "ENOTTY",
        Errno::EINVAL => "EINVAL",
        Errno::EFBIG => "EFBIG",
        Errno::ENOSPC => "ENOSPC",
        Errno::ENAMETOOLONG => "ENAMETOOLONG",
        Errno::ENOSYS => "ENOSYS",
        Errno::EILSEQ => "EILSEQ",
        Errno::EOVERFLOW => "EOVERFLOW",
        Errno::EOPNOTSUPP => "EOPNOTSUPP",
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
    let errno = match row.expect {
        Expect::ChildCode(_) => "exit-code",
        _ if row.ret < 0 => errno_name(Errno(u16::try_from(-row.ret).unwrap_or(u16::MAX))),
        _ => "none",
    };
    println!(
        "syscall-fuzz: case nr=0x{:02x} name={} vector={} ret={} errno={} ok={}",
        row.nr,
        row.name,
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

/// Signal entry probe: a delivered kick must return through `SIGRETURN`, not exit the thread.
extern "C" fn signal_return(_bits: usize, _fault: usize, _mask: u64) -> ! {
    // SAFETY: the kernel enters this function only after saving a context for the thread.
    unsafe { sys::syscall0(RAW_SIGRETURN) };
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

/// A child that waits to be signalled; used by the setpgid/signal coverage rows.
extern "C" fn child_sleep(_arg: usize) -> ! {
    loop {
        let _ = raw(Syscall::NANOSLEEP as usize, [10_000_000, 0, 0, 0, 0, 0]);
    }
}

/// A child that drops to uid 1000 and checks one authority rule. `arg` selects the rule:
/// 0 CLOCK_SET, 1 DTrace ENABLE, 2 SETRLIMIT hard raise. Exit 0 means the call was denied.
extern "C" fn child_nonroot(arg: usize) -> ! {
    let _ = raw(0x3b, [4, 1000, 1000, 1000, 0, 0]); // PROC_SETCRED_EX setresuid
    let _ = raw(0x3b, [8, 1000, 1000, 1000, 0, 0]); // PROC_SETCRED_EX setresgid
    let mut ts = [0u8; 16];
    ts[..8].copy_from_slice(&1_600_000_000i64.to_le_bytes());
    let denied = match arg {
        0 => {
            raw(
                RAW_CLOCK_SET,
                [clock::REALTIME as usize, ts.as_ptr() as usize, 0, 0, 0, 0],
            ) == -(Errno::EPERM.0 as isize)
        }
        1 => {
            raw(RAW_DTRACE_ENABLE, [1, 0, Handle::NONE.0 as usize, 0, 0, 0])
                == -(Errno::EPERM.0 as isize)
        }
        2 => raw(RAW_PROC_SETRLIMIT, [7, 10, 300, 0, 0, 0]) == -(Errno::EPERM.0 as isize),
        _ => false,
    };
    // SAFETY: `EXIT` terminates only this child; the parent reaps it.
    unsafe { sys::syscall1(Syscall::EXIT as usize, usize::from(!denied)) };
    loop {
        core::hint::spin_loop();
    }
}

/// `PROC_SETSID` in a forked child (the parent may already be a group leader).
extern "C" fn child_setsid(_arg: usize) -> ! {
    let result = raw(RAW_PROC_SETSID, [0; 6]);
    // SAFETY: `EXIT` terminates only this child; 0 means the syscall returned a session id.
    unsafe { sys::syscall1(Syscall::EXIT as usize, usize::from(result < 0)) };
    loop {
        core::hint::spin_loop();
    }
}

/// `THREAD_EXIT_EX` in a child, bounded to a clean clear word and no unmap.
extern "C" fn child_exit_ex(_arg: usize) -> ! {
    let _ = raw(RAW_THREAD_EXIT_EX, [7, 0, 0, 0, 0, 0]);
    // SAFETY: fallback exit if the terminating call returns.
    unsafe { sys::syscall1(Syscall::EXIT as usize, 1) };
    loop {
        core::hint::spin_loop();
    }
}

/// `THREAD_EXIT_EX` with a garbage clear address; the child terminates either way.
extern "C" fn child_exit_ex_garbage(_arg: usize) -> ! {
    let _ = raw(RAW_THREAD_EXIT_EX, [9, 1, 0, 0, 0, 0]);
    // SAFETY: fallback exit if the terminating call returns.
    unsafe { sys::syscall1(Syscall::EXIT as usize, 2) };
    loop {
        core::hint::spin_loop();
    }
}

/// Linux-personality `write(1, 0x1, 1)`: no handler is installed, so the trap ends the child.
extern "C" fn child_linux_write(_arg: usize) -> ! {
    let _ = raw(1, [1, 1, 1, 0, 0, 0]);
    // SAFETY: fallback exit if the trap returns.
    unsafe { sys::syscall1(Syscall::EXIT as usize, 0) };
    loop {
        core::hint::spin_loop();
    }
}

/// Linux-personality `read(-1, 0x1, 1)`.
extern "C" fn child_linux_read(_arg: usize) -> ! {
    let _ = raw(0, [usize::MAX, 1, 1, 0, 0, 0]);
    // SAFETY: fallback exit if the trap returns.
    unsafe { sys::syscall1(Syscall::EXIT as usize, 0) };
    loop {
        core::hint::spin_loop();
    }
}

/// Linux-personality `exit(1)`.
extern "C" fn child_linux_exit(_arg: usize) -> ! {
    let _ = raw(60, [1, 0, 0, 0, 0, 0]);
    // SAFETY: fallback exit if the trap returns.
    unsafe { sys::syscall1(Syscall::EXIT as usize, 0) };
    loop {
        core::hint::spin_loop();
    }
}

/// Fork a child and start `entry` in it; the parent keeps the child handle.
fn spawn_child(entry: extern "C" fn(usize) -> !, arg: usize) -> Result<usize, Errno> {
    let child = checked(RAW_PROC_FORK, [Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0])?;
    let stack = core::ptr::addr_of_mut!(CHILD_STACK);
    checked(
        Syscall::THREAD_CREATE as usize,
        [child, entry as usize, stack_top(stack), arg, 0, 0],
    )?;
    Ok(child)
}

/// Create a Linux-personality fork child and start `entry` in it (no handler installed).
fn spawn_linux_child(entry: extern "C" fn(usize) -> !) -> Result<usize, Errno> {
    let child = checked(RAW_PROC_FORK, [Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0])?;
    // `Personality::Linux` is 1. The child has no threads yet, so the change is allowed.
    checked(
        RAW_PROC_SET_PERSONALITY,
        [child, 1, Handle::NONE.0 as usize, 0, 0, 0],
    )?;
    let stack = core::ptr::addr_of_mut!(CHILD_STACK);
    checked(
        Syscall::THREAD_CREATE as usize,
        [child, entry as usize, stack_top(stack), 0, 0, 0],
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
    raw_plan(runner);
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

    runner.address(
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
        [Handle::ROOT_DEVICE.0 as usize, 1, IRQ_BASELINE, 0, 0, 0],
    );
    runner.push(Row {
        nr: Syscall::IRQ_CREATE as usize,
        name: alloc::string::String::from("IRQ_CREATE"),
        vector: "baseline",
        ret: irq,
        expect: Expect::Any,
        normalized: false,
    });
    if irq >= 0 {
        let irq = irq as usize;
        runner.ok(Syscall::IRQ_WAIT, "timeout", [irq, 1_000_000, 0, 0, 0, 0]);
        runner.ok(Syscall::IRQ_ACK, "baseline", [irq, 0, 0, 0, 0, 0]);
    } else {
        // The platform did not grant the baseline line. IRQ_WAIT/ACK cannot be attributed to a
        // valid handle, so record a gap instead of inventing an expectation.
        runner.gap("IRQ_CREATE baseline did not return a handle; IRQ_WAIT/ACK are not measured");
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
        [Handle::ROOT_DEVICE.0 as usize, 1, IRQ_BASELINE, 1, 0, 0],
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

/// Raw pre-enum dispatch coverage. Mirrors `raw-dispatch.list`; `raw-check.sh` fails if the kernel
/// source and the list disagree, or if a fuzzed entry loses its `RAW_*` reference in this file.
fn raw_plan(r: &mut Runner) {
    let ctx = r.ctx;
    let selfp = Handle::SELF_PROC.0 as usize;
    let none = Handle::NONE.0 as usize;
    let path = core::ptr::addr_of!(PATH) as usize;
    let name = core::ptr::addr_of!(NAME) as usize;
    let probe_entry = probe_thread as *const () as usize;

    // GETRANDOM / CLOCK_SET / TRACE_SET / RECORD_LOCK / PROC_CHROOT.
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "baseline",
        [ctx.out, 16, 0, 0, 0, 0],
        Expect::Exact(16),
    );
    // K545: a request the pool can fill returns the full length, not a short read.
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "len4096",
        [ctx.out, PAGE, 0, 0, 0, 0],
        Expect::Exact(PAGE),
    );
    // GRND_NONBLOCK is accepted and ignored by this pool (the boot seed is synchronous).
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "nonblock",
        [ctx.out, 16, 1, 0, 0, 0],
        Expect::Exact(16),
    );
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "len0",
        [ctx.out, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "ptr0",
        [PTR0, 16, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "protnone",
        [ctx.guard, 16, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "flagsneg",
        [ctx.out, 16, usize::MAX, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_GETRANDOM,
        "GETRANDOM",
        "lenmax",
        [ctx.out, LENMAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );

    let mut timespec = [0u8; 16];
    timespec[..8].copy_from_slice(&(-1i64).to_le_bytes());
    let ts = timespec.as_ptr() as usize;
    r.raw(
        RAW_CLOCK_SET,
        "CLOCK_SET",
        "clockbad",
        [2, ts, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_CLOCK_SET,
        "CLOCK_SET",
        "ptr0",
        [clock::REALTIME as usize, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_CLOCK_SET,
        "CLOCK_SET",
        "negts",
        [clock::REALTIME as usize, ts, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    // K529: root succeeds; a uid-1000 child is denied before the RTC write.
    let mut clock_ok = [0u8; 16];
    clock_ok[..8].copy_from_slice(&1_600_000_000i64.to_le_bytes());
    r.raw(
        RAW_CLOCK_SET,
        "CLOCK_SET",
        "baseline",
        [
            clock::REALTIME as usize,
            clock_ok.as_ptr() as usize,
            0,
            0,
            0,
            0,
        ],
        Expect::Success,
    );

    r.raw(RAW_TRACE_SET, "TRACE_SET", "zero", [0; 6], Expect::Success);
    r.raw(
        RAW_TRACE_SET,
        "TRACE_SET",
        "max",
        [PTRMAX, 0, 0, 0, 0, 0],
        Expect::Success,
    );

    let mut lock = [0u8; 32];
    lock[..2].copy_from_slice(&2i16.to_le_bytes());
    let lockp = lock.as_ptr() as usize;
    r.raw(
        RAW_RECORD_LOCK,
        "RECORD_LOCK",
        "cmdneg",
        [ctx.pipe_r, usize::MAX, lockp, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_RECORD_LOCK,
        "RECORD_LOCK",
        "cmd0",
        [ctx.pipe_r, 0, lockp, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_RECORD_LOCK,
        "RECORD_LOCK",
        "ptr0",
        [ctx.pipe_r, 5, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_RECORD_LOCK,
        "RECORD_LOCK",
        "handlebad",
        [usize::MAX, 6, lockp, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_RECORD_LOCK,
        "RECORD_LOCK",
        "pipe",
        [ctx.pipe_r, 6, lockp, 0, 0, 0],
        Expect::Err(EBADF),
    );

    r.raw(
        RAW_PROC_CHROOT,
        "PROC_CHROOT",
        "ptr0",
        [0, 7, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_CHROOT,
        "PROC_CHROOT",
        "ptr1",
        [1, 7, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_CHROOT,
        "PROC_CHROOT",
        "len0",
        [path, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_CHROOT,
        "PROC_CHROOT",
        "lenmax",
        [path, LENMAX, 0, 0, 0, 0],
        Expect::Err(ENAMETOOLONG),
    );

    // Thread and process identity/control.
    r.raw(
        RAW_THREAD_SET_TLS,
        "THREAD_SET_TLS",
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_THREAD_SET_TLS,
        "THREAD_SET_TLS",
        "zero",
        [none, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_THREAD_SET_TLS,
        "THREAD_SET_TLS",
        "tp1",
        [none, 1, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_THREAD_SET_TLS,
        "THREAD_SET_TLS",
        "tpmax",
        [none, PTRMAX, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_THREAD_SELF,
        "THREAD_SELF",
        "noargs",
        [0; 6],
        Expect::Success,
    );
    r.raw(
        RAW_ROBUST_LIST_SET,
        "ROBUST_LIST_SET",
        "clear",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_ROBUST_LIST_SET,
        "ROBUST_LIST_SET",
        "head1",
        [1, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_ROBUST_LIST_SET,
        "ROBUST_LIST_SET",
        "lenmax",
        [0, LENMAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );

    r.raw(
        RAW_PROC_SET_PERSONALITY,
        "PROC_SET_PERSONALITY",
        "persbad",
        [selfp, 2, none, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SET_PERSONALITY,
        "PROC_SET_PERSONALITY",
        "handlerbad",
        [selfp, 0, usize::MAX, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PROC_SET_PERSONALITY,
        "PROC_SET_PERSONALITY",
        "baseline",
        [selfp, 0, none, 0, 0, 0],
        Expect::Success,
    );

    r.raw(
        RAW_PROC_SELF_INFO,
        "PROC_SELF_INFO",
        "baseline",
        [ctx.out, 8, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_SELF_INFO,
        "PROC_SELF_INFO",
        "ptr0",
        [0, 8, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_SELF_INFO,
        "PROC_SELF_INFO",
        "count0",
        [ctx.out, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SETPGID,
        "PROC_SETPGID",
        "pidneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(ESRCH),
    );
    // POSIX.1-2017 setpgid(): a negative pgid is EINVAL. K488 (d88879078) implements it; the
    // fuzz table carried the pre-K488 EPERM expectation (K500).
    r.raw(
        RAW_PROC_SETPGID,
        "PROC_SETPGID",
        "pgidneg",
        [0, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SETPGID,
        "PROC_SETPGID",
        "baseline",
        [0, 0, 0, 0, 0, 0],
        Expect::Any,
    );
    // K488: a direct child can be moved to its own group and signalled; the pid comes from the
    // process handle. The child sleeps until killed.
    if let Ok(child) = spawn_child(child_sleep, 0) {
        let pid = raw(RAW_PROC_HANDLE_INFO, [child, 0, 0, 0, 0, 0]);
        if pid > 0 {
            let pid = pid as usize;
            r.raw(
                RAW_PROC_SETPGID,
                "PROC_SETPGID",
                "child",
                [pid, pid, 0, 0, 0, 0],
                Expect::Success,
            );
            r.raw(
                RAW_PROC_GETPGID,
                "PROC_GETPGID",
                "child",
                [pid, 0, 0, 0, 0, 0],
                Expect::Success,
            );
            r.raw(
                RAW_PROC_SIGNAL,
                "PROC_SIGNAL",
                "killchild",
                [pid, 9, 0, 0, 0, 0],
                Expect::Success,
            );
            let _ = wait_child(child);
        }
    }
    r.raw(
        RAW_PROC_SETCRED,
        "PROC_SETCRED",
        "whichneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SETCRED,
        "PROC_SETCRED",
        "valuechange",
        [0, 1, 0, 0, 0, 0],
        Expect::Err(EPERM),
    );
    r.raw(
        RAW_PROC_SETCRED,
        "PROC_SETCRED",
        "baseline",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_FORK,
        "PROC_FORK",
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PROC_FORK,
        "PROC_FORK",
        "flags1",
        [selfp, 1, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );

    r.raw(
        RAW_PROC_PID_INFO,
        "PROC_PID_INFO",
        "baseline",
        [0, ctx.out, 8, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_PID_INFO,
        "PROC_PID_INFO",
        "ptr0",
        [0, 0, 8, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_PID_INFO,
        "PROC_PID_INFO",
        "count0",
        [0, ctx.out, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_PID_INFO,
        "PROC_PID_INFO",
        "pidneg",
        [usize::MAX, ctx.out, 8, 0, 0, 0],
        Expect::Err(ESRCH),
    );

    r.raw(
        RAW_PROC_SETCRED_EX,
        "PROC_SETCRED_EX",
        "which0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SETCRED_EX,
        "PROC_SETCRED_EX",
        "groupsptr0",
        [9, 0, 0, 0, 0, 1],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_SETCRED_EX,
        "PROC_SETCRED_EX",
        "baseline",
        [1, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_CRED_INFO,
        "PROC_CRED_INFO",
        "baseline",
        [ctx.out, 6, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_CRED_INFO,
        "PROC_CRED_INFO",
        "ptr0",
        [0, 6, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_CRED_INFO,
        "PROC_CRED_INFO",
        "count5",
        [ctx.out, 5, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_GETGROUPS,
        "PROC_GETGROUPS",
        "size0",
        [ctx.out, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_GETGROUPS,
        "PROC_GETGROUPS",
        "ptr0",
        [0, 64, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_GETGROUPS,
        "PROC_GETGROUPS",
        "size1",
        [ctx.out, 1, 0, 0, 0, 0],
        Expect::Any,
    );
    r.raw(
        RAW_PROC_HANDLE_INFO,
        "PROC_HANDLE_INFO",
        "baseline",
        [selfp, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_HANDLE_INFO,
        "PROC_HANDLE_INFO",
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PROC_EXEC_CRED,
        "PROC_EXEC_CRED",
        "flags2",
        [selfp, 0, 2, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_EXEC_CRED,
        "PROC_EXEC_CRED",
        "self",
        [selfp, 0, 0, 0, 0, 0],
        Expect::Err(EPERM),
    );
    r.raw(
        RAW_PROC_EXEC_CRED,
        "PROC_EXEC_CRED",
        "filebad",
        [ctx.proc_created, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );

    r.raw(
        RAW_MSYNC,
        "MSYNC",
        "fdneg",
        [usize::MAX, 0, PAGE, 1, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_MSYNC,
        "MSYNC",
        "flags0",
        [selfp, 0, PAGE, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_MSYNC,
        "MSYNC",
        "flagsneg",
        [selfp, 0, PAGE, usize::MAX, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_MSYNC,
        "MSYNC",
        "addr1",
        [selfp, 1, PAGE, 1, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_MSYNC,
        "MSYNC",
        "len0",
        [selfp, 0, 0, 1, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_MSYNC,
        "MSYNC",
        "lenmax",
        [selfp, 0, LENMAX, 1, 0, 0],
        Expect::Err(EINVAL),
    );

    r.raw(
        RAW_PROC_SET_COMMAND,
        "PROC_SET_COMMAND",
        "baseline",
        [selfp, name, 4, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_SET_COMMAND,
        "PROC_SET_COMMAND",
        "ptr0",
        [selfp, 0, 4, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_SET_COMMAND,
        "PROC_SET_COMMAND",
        "len0",
        [selfp, name, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_SET_COMMAND,
        "PROC_SET_COMMAND",
        "lenmax",
        [selfp, name, LENMAX, 0, 0, 0],
        Expect::Err(ENAMETOOLONG),
    );
    r.raw(
        RAW_PROC_SET_COMMAND,
        "PROC_SET_COMMAND",
        "fdneg",
        [usize::MAX, name, 4, 0, 0, 0],
        Expect::Err(EBADF),
    );

    r.raw(
        RAW_PROC_GETPGID,
        "PROC_GETPGID",
        "self",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_GETPGID,
        "PROC_GETPGID",
        "pidneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(ESRCH),
    );
    // POSIX.1-2017 kill(): signal 0 is an existence/permission check and returns 0. K488
    // (d88879078) implements it; the fuzz table carried the pre-K488 EINVAL expectation (K500).
    r.raw(
        RAW_PROC_SIGNAL,
        "PROC_SIGNAL",
        "signo0",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    // The negative-pid group form is the same existence check on `-pgrp`; use the caller's own
    // group so the check is deterministic. NetBSD kill1() dispatches pid < 0 to killpg1().
    let pgrp = raw(RAW_PROC_GETPGID, [0, 0, 0, 0, 0, 0]);
    if pgrp > 0 {
        let group = 0usize.wrapping_sub(usize::try_from(pgrp).unwrap_or(0));
        r.raw(
            RAW_PROC_SIGNAL,
            "PROC_SIGNAL",
            "signo0pgrp",
            [group, 0, 0, 0, 0, 0],
            Expect::Success,
        );
    }
    r.raw(
        RAW_PROC_SIGNAL,
        "PROC_SIGNAL",
        "targethuge",
        [0x7fff_ffff, 9, 0, 0, 0, 0],
        Expect::Err(ESRCH),
    );
    // Drain any pending SIGCHLD left by the earlier process tests so the no-pending WNOHANG
    // vector is deterministic; the returned signal number itself is not a table row.
    while raw(RAW_PROC_SIGRECV, [ctx.out, 1, 0, 0, 0, 0]) >= 0 {}
    r.raw(
        RAW_PROC_SIGRECV,
        "PROC_SIGRECV",
        "ptr0",
        [0, 1, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_SIGRECV,
        "PROC_SIGRECV",
        "flagsneg",
        [ctx.out, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SIGRECV,
        "PROC_SIGRECV",
        "nohang",
        [ctx.out, 1, 0, 0, 0, 0],
        Expect::Err(EAGAIN),
    );
    r.raw(
        RAW_PROC_SIGRECV_EX,
        "PROC_SIGRECV_EX",
        "ptr0",
        [0, 1, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_SIGRECV_EX,
        "PROC_SIGRECV_EX",
        "flagsneg",
        [ctx.out, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SIGRECV_EX,
        "PROC_SIGRECV_EX",
        "nohang",
        [ctx.out, 1, 0, 0, 0, 0],
        Expect::Err(EAGAIN),
    );
    r.raw(
        RAW_PROC_SIGQUEUE,
        "PROC_SIGQUEUE",
        "signo0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SIGQUEUE,
        "PROC_SIGQUEUE",
        "signoneg",
        [0, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );

    r.raw(
        RAW_PROC_TIMES,
        "PROC_TIMES",
        "baseline",
        [ctx.out, 4, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_TIMES,
        "PROC_TIMES",
        "ptr0",
        [0, 4, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_TIMES,
        "PROC_TIMES",
        "count3",
        [ctx.out, 3, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_GETRUSAGE,
        "PROC_GETRUSAGE",
        "baseline",
        [0, ctx.out, 8, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_GETRUSAGE,
        "PROC_GETRUSAGE",
        "who1",
        [1, ctx.out, 8, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_GETRUSAGE,
        "PROC_GETRUSAGE",
        "ptr0",
        [0, 0, 8, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_GETRUSAGE,
        "PROC_GETRUSAGE",
        "count7",
        [0, ctx.out, 7, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_GETRLIMIT,
        "PROC_GETRLIMIT",
        "baseline",
        [0, ctx.out, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_GETRLIMIT,
        "PROC_GETRLIMIT",
        "res16",
        [16, ctx.out, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_GETRLIMIT,
        "PROC_GETRLIMIT",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    // K491: an unprivileged hard-limit raise is EPERM; root can set a finite pair and infinity.
    let rlimit_nonroot = child_code(r, child_nonroot, 2);
    r.push(Row {
        nr: RAW_PROC_SETRLIMIT,
        name: alloc::string::String::from("PROC_SETRLIMIT"),
        vector: "nonroot",
        ret: rlimit_nonroot,
        expect: Expect::ChildCode(0),
        normalized: false,
    });
    r.raw(
        RAW_PROC_SETRLIMIT,
        "PROC_SETRLIMIT",
        "baseline",
        [0, 1, 2, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_SETRLIMIT,
        "PROC_SETRLIMIT",
        "infinity",
        [7, LENMAX, LENMAX, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PROC_SETRLIMIT,
        "PROC_SETRLIMIT",
        "res16",
        [16, 1, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_SETRLIMIT,
        "PROC_SETRLIMIT",
        "invert",
        [0, 5, 1, 0, 0, 0],
        Expect::Err(EINVAL),
    );

    r.raw(
        RAW_HANDLE_COPY,
        "HANDLE_COPY",
        "baseline",
        [selfp, ctx.memobj, MEMOBJ_RIGHTS, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_HANDLE_COPY,
        "HANDLE_COPY",
        "fdneg",
        [usize::MAX, ctx.memobj, RIGHTS_ALL, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_HANDLE_COPY,
        "HANDLE_COPY",
        "mask0",
        [selfp, ctx.memobj, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_HANDLE_TRANSFER_FILE,
        "HANDLE_TRANSFER_FILE",
        "mask0",
        [ctx.pipe_r, ctx.memobj, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_HANDLE_TRANSFER_FILE,
        "HANDLE_TRANSFER_FILE",
        "filebad",
        [usize::MAX, ctx.memobj, RIGHTS_ALL, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_HANDLE_TRANSFER_FILE,
        "HANDLE_TRANSFER_FILE",
        "pipe",
        [ctx.pipe_r, ctx.memobj, RIGHTS_ALL, 0, 0, 0],
        Expect::Err(EBADF),
    );

    // PROC_EXEC: invalid requests only; a successful exec replaces the image, so no baseline.
    let exec_zero = [0u64; 11];
    let mut exec_flags = [0u64; 11];
    exec_flags[10] = 1;
    let mut exec_path = [0u64; 11];
    exec_path[1] = 1;
    exec_path[3] = usize::MAX as u64;
    let mut exec_image = [0u64; 11];
    exec_image[1] = usize::MAX as u64;
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "ptr1",
        [1, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "ptrneg",
        [PTRMAX, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "protnone",
        [ctx.guard, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "image0",
        [exec_zero.as_ptr() as usize, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "flags1",
        [exec_flags.as_ptr() as usize, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "pathmax",
        [exec_path.as_ptr() as usize, 0, 0, 0, 0, 0],
        Expect::Err(ENAMETOOLONG),
    );
    r.raw(
        RAW_PROC_EXEC,
        "PROC_EXEC",
        "imagmax",
        [exec_image.as_ptr() as usize, 0, 0, 0, 0, 0],
        Expect::Err(EFBIG),
    );

    // PREAD/PWRITE/SEEK need a scheme file; the pipe is not one.
    r.raw(
        RAW_PREAD,
        "PREAD",
        "fdneg",
        [usize::MAX, ctx.out, 1, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PREAD,
        "PREAD",
        "pipe",
        [ctx.pipe_r, ctx.out, 1, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PWRITE,
        "PWRITE",
        "fdneg",
        [usize::MAX, ctx.buf, 1, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PWRITE,
        "PWRITE",
        "pipe",
        [ctx.pipe_w, ctx.buf, 1, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_SEEK,
        "SEEK",
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_SEEK,
        "SEEK",
        "pipe",
        [ctx.pipe_r, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );

    // PIPE / PIPE2.
    let mut pipe_fds = [0u32; 2];
    r.raw(
        RAW_PIPE,
        "PIPE",
        "baseline",
        [pipe_fds.as_mut_ptr() as usize, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PIPE,
        "PIPE",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PIPE,
        "PIPE",
        "ptr1",
        [1, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PIPE,
        "PIPE",
        "protnone",
        [ctx.guard, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    let mut pipe2_fds = [0u32; 2];
    r.raw(
        RAW_PIPE2,
        "PIPE2",
        "baseline",
        [pipe2_fds.as_mut_ptr() as usize, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_PIPE2,
        "PIPE2",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PIPE2,
        "PIPE2",
        "flagsneg",
        [pipe2_fds.as_mut_ptr() as usize, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );

    // Duplex endpoints: pair, send/recv, credentials, shutdown.
    let mut pair_fds = [0u32; 2];
    let pair = raw(
        RAW_DUPLEX_PAIR,
        [pair_fds.as_mut_ptr() as usize, 0, 0, 0, 0, 0],
    );
    r.push(Row {
        nr: RAW_DUPLEX_PAIR,
        name: alloc::string::String::from("DUPLEX_PAIR"),
        vector: "baseline",
        ret: pair,
        expect: Expect::Success,
        normalized: false,
    });
    r.raw(
        RAW_DUPLEX_PAIR,
        "DUPLEX_PAIR",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_DUPLEX_PAIR,
        "DUPLEX_PAIR",
        "ptr1",
        [1, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_DUPLEX_PAIR,
        "DUPLEX_PAIR",
        "protnone",
        [ctx.guard, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_DUPLEX_PAIR,
        "DUPLEX_PAIR",
        "flagsneg",
        [pair_fds.as_mut_ptr() as usize, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    if pair >= 0 {
        let left = pair_fds[0] as usize;
        let right = pair_fds[1] as usize;
        let mut send = [0u64; 5];
        send[0] = ctx.buf as u64;
        send[1] = 1;
        let mut recv = [0u64; 5];
        recv[0] = ctx.out as u64;
        recv[1] = 8;
        let sent = raw(
            RAW_DUPLEX_SENDMSG,
            [left, send.as_ptr() as usize, 0, 0, 0, 0],
        );
        r.push(Row {
            nr: RAW_DUPLEX_SENDMSG,
            name: alloc::string::String::from("DUPLEX_SENDMSG"),
            vector: "baseline",
            ret: sent,
            expect: Expect::Success,
            normalized: false,
        });
        if sent >= 0 {
            r.raw(
                RAW_DUPLEX_RECVMSG,
                "DUPLEX_RECVMSG",
                "baseline",
                [right, recv.as_ptr() as usize, 0, 0, 0, 0],
                Expect::Success,
            );
        } else {
            r.gap("DUPLEX_SENDMSG baseline failed; the recvmsg baseline is not measured");
        }
        r.raw(
            RAW_DUPLEX_PEERCRED,
            "DUPLEX_PEERCRED",
            "baseline",
            [left, ctx.out, 0, 0, 0, 0],
            Expect::Success,
        );
        r.raw(
            RAW_DUPLEX_PEERCRED,
            "DUPLEX_PEERCRED",
            "ptr0",
            [left, 0, 0, 0, 0, 0],
            Expect::Err(EFAULT),
        );
        r.raw(
            RAW_DUPLEX_SENDMSG,
            "DUPLEX_SENDMSG",
            "fdneg",
            [usize::MAX, send.as_ptr() as usize, 0, 0, 0, 0],
            Expect::Err(EBADF),
        );
        r.raw(
            RAW_DUPLEX_SENDMSG,
            "DUPLEX_SENDMSG",
            "msgptr0",
            [left, 0, 0, 0, 0, 0],
            Expect::Err(EFAULT),
        );
        let mut flagsbad = send;
        flagsbad[4] = 0x8000_0000;
        r.raw(
            RAW_DUPLEX_SENDMSG,
            "DUPLEX_SENDMSG",
            "flagsbad",
            [left, flagsbad.as_ptr() as usize, 0, 0, 0, 0],
            Expect::Err(EINVAL),
        );
        let mut databad = send;
        databad[0] = 0;
        r.raw(
            RAW_DUPLEX_SENDMSG,
            "DUPLEX_SENDMSG",
            "databad",
            [left, databad.as_ptr() as usize, 0, 0, 0, 0],
            Expect::Err(EFAULT),
        );
        r.raw(
            RAW_DUPLEX_RECVMSG,
            "DUPLEX_RECVMSG",
            "fdneg",
            [usize::MAX, recv.as_ptr() as usize, 0, 0, 0, 0],
            Expect::Err(EBADF),
        );
        r.raw(
            RAW_DUPLEX_RECVMSG,
            "DUPLEX_RECVMSG",
            "msgptr0",
            [right, 0, 0, 0, 0, 0],
            Expect::Err(EFAULT),
        );
        r.raw(
            RAW_DUPLEX_SHUTDOWN,
            "DUPLEX_SHUTDOWN",
            "baseline",
            [left, 1, 0, 0, 0, 0],
            Expect::Success,
        );
        // K520/K521: after SHUT_WR the local send is EPIPE and the peer reads EOF.
        r.raw(
            RAW_DUPLEX_SENDMSG,
            "DUPLEX_SENDMSG",
            "after-shut-wr",
            [left, send.as_ptr() as usize, 0, 0, 0, 0],
            Expect::Err(EPIPE),
        );
        r.raw(
            RAW_DUPLEX_RECVMSG,
            "DUPLEX_RECVMSG",
            "after-shut-wr",
            [right, recv.as_ptr() as usize, 0, 0, 0, 0],
            Expect::Exact(0),
        );
        let mut waitall = recv;
        waitall[4] = 0x40; // MSG_WAITALL is refused: EOPNOTSUPP.
        r.raw(
            RAW_DUPLEX_RECVMSG,
            "DUPLEX_RECVMSG",
            "waitall",
            [right, waitall.as_ptr() as usize, 0, 0, 0, 0],
            Expect::Err(EOPNOTSUPP),
        );
        r.raw(
            RAW_DUPLEX_SHUTDOWN,
            "DUPLEX_SHUTDOWN",
            "howbad",
            [left, 5, 0, 0, 0, 0],
            Expect::Err(EINVAL),
        );
        r.raw(
            RAW_DUPLEX_SHUTDOWN,
            "DUPLEX_SHUTDOWN",
            "fdneg",
            [usize::MAX, 1, 0, 0, 0, 0],
            Expect::Err(EBADF),
        );
    } else {
        r.gap("DUPLEX_PAIR baseline failed; the duplex endpoint rows are not measured");
    }

    // Priority and scheduler.
    r.raw(
        RAW_GETPRIORITY,
        "GETPRIORITY",
        "baseline",
        [0, 0, ctx.out, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_GETPRIORITY,
        "GETPRIORITY",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_GETPRIORITY,
        "GETPRIORITY",
        "whichneg",
        [usize::MAX, 0, ctx.out, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_GETPRIORITY,
        "GETPRIORITY",
        "whohuge",
        [0, 0x7fff_ffff, ctx.out, 0, 0, 0],
        Expect::Err(ESRCH),
    );
    r.raw(
        RAW_SETPRIORITY,
        "SETPRIORITY",
        "baseline",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_SETPRIORITY,
        "SETPRIORITY",
        "whichneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_SETPRIORITY,
        "SETPRIORITY",
        "whohuge",
        [0, 0x7fff_ffff, 0, 0, 0, 0],
        Expect::Err(ESRCH),
    );
    r.raw(
        RAW_NICE,
        "NICE",
        "baseline",
        [0, ctx.out, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_NICE,
        "NICE",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_SCHED_GET_PRIORITY_MAX,
        "SCHED_GET_PRIORITY_MAX",
        "other",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_SCHED_GET_PRIORITY_MAX,
        "SCHED_GET_PRIORITY_MAX",
        "fifo",
        [1, 0, 0, 0, 0, 0],
        Expect::Err(ENOTSUP),
    );
    r.raw(
        RAW_SCHED_GET_PRIORITY_MAX,
        "SCHED_GET_PRIORITY_MAX",
        "policyneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_SCHED_GET_PRIORITY_MIN,
        "SCHED_GET_PRIORITY_MIN",
        "other",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_SCHED_GET_PRIORITY_MIN,
        "SCHED_GET_PRIORITY_MIN",
        "fifo",
        [1, 0, 0, 0, 0, 0],
        Expect::Err(ENOTSUP),
    );
    r.raw(
        RAW_SCHED_SETSCHEDULER,
        "SCHED_SETSCHEDULER",
        "baseline",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_SCHED_SETSCHEDULER,
        "SCHED_SETSCHEDULER",
        "policyfifo",
        [0, 1, 0, 0, 0, 0],
        Expect::Err(ENOTSUP),
    );
    r.raw(
        RAW_SCHED_SETSCHEDULER,
        "SCHED_SETSCHEDULER",
        "policyneg",
        [0, usize::MAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_SCHED_SETSCHEDULER,
        "SCHED_SETSCHEDULER",
        "priobad",
        [0, 0, 1, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_SCHED_SETSCHEDULER,
        "SCHED_SETSCHEDULER",
        "pidhuge",
        [0x7fff_ffff, 0, 0, 0, 0, 0],
        Expect::Err(ESRCH),
    );
    r.raw(
        RAW_SCHED_GETSCHEDULER,
        "SCHED_GETSCHEDULER",
        "self",
        [0, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_SCHED_GETSCHEDULER,
        "SCHED_GETSCHEDULER",
        "pidneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(ESRCH),
    );

    // TTY controls need a tty handle; a pipe is ENOTTY.
    r.raw(
        RAW_PROC_CTTY_SET,
        "PROC_CTTY_SET",
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PROC_CTTY_SET,
        "PROC_CTTY_SET",
        "pipe",
        [ctx.pipe_r, 0, 0, 0, 0, 0],
        Expect::Err(ENOTTY),
    );
    r.raw(
        RAW_PROC_CTTY_GET,
        "PROC_CTTY_GET",
        "fdneg",
        [usize::MAX, ctx.out, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PROC_CTTY_GET,
        "PROC_CTTY_GET",
        "pipe",
        [ctx.pipe_r, ctx.out, 0, 0, 0, 0],
        Expect::Err(ENOTTY),
    );
    r.raw(
        RAW_PROC_CTTY_SET_FG,
        "PROC_CTTY_SET_FG",
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_PROC_CTTY_SET_FG,
        "PROC_CTTY_SET_FG",
        "pipe",
        [ctx.pipe_r, 0, 0, 0, 0, 0],
        Expect::Err(ENOTTY),
    );
    r.raw(
        RAW_PROC_CTTY_HANDLE,
        "PROC_CTTY_HANDLE",
        "baseline",
        [0, 0, 0, 0, 0, 0],
        Expect::Any,
    );

    // Signals: SIGRESEND without a registration is the safe path; SIG_SETUP is installed last.
    r.raw(
        RAW_SIGRESEND,
        "SIGRESEND",
        "noregistration",
        [1, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_SIG_SETUP,
        "SIG_SETUP",
        "ptr0",
        [0, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_SIG_SETUP,
        "SIG_SETUP",
        "offset1",
        [probe_entry, 1, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_SIG_SETUP,
        "SIG_SETUP",
        "flagsneg",
        [probe_entry, 0, usize::MAX, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    // A relative offset with no TLS base has no writable context. SIG_SETUP can report this
    // now, so it must not register a context that would later kill the process on delivery.
    r.raw(
        RAW_SIG_SETUP,
        "SIG_SETUP",
        "relative0",
        [probe_entry, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    // The valid baseline uses the absolute context page (SIG_SETUP_ABS = 2).
    r.raw(
        RAW_SIG_SETUP,
        "SIG_SETUP",
        "baseline",
        [signal_return as *const () as usize, ctx.out, 2, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_THREAD_KICK,
        "THREAD_KICK",
        "fdneg",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_THREAD_KICK,
        "THREAD_KICK",
        "selfzero",
        [none, 0, 0, 0, 0, 0],
        Expect::Success,
    );

    // DTrace control calls.
    r.raw(
        RAW_DTRACE_PROBE_LIST,
        "DTRACE_PROBE_LIST",
        "count",
        [ctx.out, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_DTRACE_PROBE_LIST,
        "DTRACE_PROBE_LIST",
        "ptr0",
        [0, 1, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_DTRACE_PROBE_LIST,
        "DTRACE_PROBE_LIST",
        "maxmax",
        [ctx.out, LENMAX, 0, 0, 0, 0],
        Expect::Success,
    );
    // K529: root can enable/disable; a uid-1000 child is denied before provider state.
    r.raw(
        RAW_DTRACE_ENABLE,
        "DTRACE_ENABLE",
        "baseline",
        [1, 0, none, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_DTRACE_DISABLE,
        "DTRACE_DISABLE",
        "baseline",
        [1, 0, 0, 0, 0, 0],
        Expect::Success,
    );

    r.raw(
        RAW_DTRACE_ENABLE,
        "DTRACE_ENABLE",
        "flags1",
        [0, 1, none, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_DTRACE_ENABLE,
        "DTRACE_ENABLE",
        "handlebad",
        [0, 0, usize::MAX, 0, 0, 0],
        Expect::Err(EBADF),
    );
    r.raw(
        RAW_DTRACE_DISABLE,
        "DTRACE_DISABLE",
        "probe0",
        [0, 0, 0, 0, 0, 0],
        Expect::Any,
    );
    r.raw(
        RAW_DTRACE_DISABLE,
        "DTRACE_DISABLE",
        "probemax",
        [usize::MAX, 0, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_DTRACE_READ,
        "DTRACE_READ",
        "empty",
        [ctx.out, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_DTRACE_READ,
        "DTRACE_READ",
        "ptr0",
        [0, 1, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_DTRACE_READ,
        "DTRACE_READ",
        "maxmax",
        [ctx.out, LENMAX, 0, 0, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_DTRACE_STATS,
        "DTRACE_STATS",
        "count",
        [ctx.out, 0, 0, 0, 0, 0],
        Expect::Success,
    );
    r.raw(
        RAW_DTRACE_STATS,
        "DTRACE_STATS",
        "ptr0",
        [0, 1, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_DTRACE_CLEAR,
        "DTRACE_CLEAR",
        "noargs",
        [0; 6],
        Expect::Success,
    );

    // Children: setsid, THREAD_EXIT_EX, PROC_WAIT_EX, and the Linux personality trap.
    let code = child_code(r, child_setsid, 0);
    r.push(Row {
        nr: RAW_PROC_SETSID,
        name: alloc::string::String::from("PROC_SETSID"),
        vector: "child",
        ret: code,
        expect: Expect::ChildCode(0),
        normalized: false,
    });
    let code = child_code(r, child_exit_ex, 0);
    r.push(Row {
        nr: RAW_THREAD_EXIT_EX,
        name: alloc::string::String::from("THREAD_EXIT_EX"),
        vector: "child7",
        ret: code,
        expect: Expect::ChildCode(7),
        normalized: false,
    });
    let code = child_code(r, child_exit_ex_garbage, 0);
    r.push(Row {
        nr: RAW_THREAD_EXIT_EX,
        name: alloc::string::String::from("THREAD_EXIT_EX"),
        vector: "garbage",
        ret: code,
        expect: Expect::Any,
        normalized: false,
    });
    r.raw(
        RAW_PROC_WAIT_EX,
        "PROC_WAIT_EX",
        "optneg",
        [3, 0, usize::MAX, ctx.out, 0, 0],
        Expect::Err(EINVAL),
    );
    r.raw(
        RAW_PROC_WAIT_EX,
        "PROC_WAIT_EX",
        "ptr0",
        [3, 0, 0, 0, 0, 0],
        Expect::Err(EFAULT),
    );
    r.raw(
        RAW_PROC_WAIT_EX,
        "PROC_WAIT_EX",
        "idtypebad",
        [99, 0, 0, ctx.out, 0, 0],
        Expect::Err(EINVAL),
    );
    if let Ok(child) = spawn_child(child_exit, 6) {
        let _ = child;
        r.raw(
            RAW_PROC_WAIT_EX,
            "PROC_WAIT_EX",
            "baseline",
            [3, 0, 0, ctx.out, 0, 0],
            Expect::Success,
        );
    } else {
        r.gap("PROC_WAIT_EX child setup failed; the baseline is not measured");
    }

    for (vector, entry) in [
        ("write", child_linux_write as extern "C" fn(usize) -> !),
        ("read", child_linux_read),
        ("exit", child_linux_exit),
    ] {
        let code = match spawn_linux_child(entry) {
            Ok(child) => match wait_child(child) {
                Ok(code) => code as isize,
                Err(error) => -(error.0 as isize),
            },
            Err(error) => -(error.0 as isize),
        };
        r.push(Row {
            nr: RAW_LINUX_TRAP,
            name: alloc::string::String::from("LINUX_TRAP"),
            vector,
            ret: code,
            expect: Expect::ChildCode(-5),
            normalized: false,
        });
    }
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
                nr: Syscall::PROC_WAIT as usize,
                name: alloc::string::String::from("PROC_WAIT"),
                vector: "child-code",
                ret: code,
                expect: Expect::ChildCode(5),
                normalized: false,
            });
        }
        Err(_) => runner.gap("PROC_WAIT child setup failed; pointer/code rows are not measured"),
    }

    let code = child_code(runner, child_exit, 7);
    runner.push(Row {
        nr: Syscall::EXIT as usize,
        name: alloc::string::String::from("EXIT"),
        vector: "child7",
        ret: code,
        expect: Expect::ChildCode(7),
        normalized: false,
    });
    let code = child_code(runner, child_exit, 0x7fff_ffff);
    runner.push(Row {
        nr: Syscall::EXIT as usize,
        name: alloc::string::String::from("EXIT"),
        vector: "childmax",
        ret: code,
        expect: Expect::Any,
        normalized: false,
    });
    let code = child_code(runner, child_thread_exit, 7);
    runner.push(Row {
        nr: Syscall::THREAD_EXIT as usize,
        name: alloc::string::String::from("THREAD_EXIT"),
        vector: "child7",
        ret: code,
        expect: Expect::ChildCode(7),
        normalized: false,
    });
    let code = child_code(runner, child_self_terminate, 9);
    runner.push(Row {
        nr: Syscall::PROC_TERMINATE as usize,
        name: alloc::string::String::from("PROC_TERMINATE"),
        vector: "childself9",
        ret: code,
        expect: Expect::ChildCode(9),
        normalized: false,
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
