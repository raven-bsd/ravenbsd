// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S55 native syscall negative and boundary fixture.
//!
//! Every probe calls the native syscall surface directly (no libc) and compares the raw
//! `-errno` result with the ABI contract: EFAULT for an invalid user range, EINVAL for a bad
//! enum/flag/size, EPERM for a revoked right, EBADF for a stale/unknown handle, and the
//! documented size/overflow rejections. A mismatch prints `syscall-negative: fail` and exits
//! nonzero; the final line names the number of checks when every probe agrees.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::signal::{SIGRESEND, SIGRETURN, SIG_SETUP};
use raven_abi::syscall::{
    HANDLE_COPY, PROC_SET_PERSONALITY, ROBUST_LIST_SET, Syscall, THREAD_CREATE_EX, THREAD_KICK,
    THREAD_SET_TLS,
};
use raven_abi::types::{
    Handle, Rights, Timespec, clock, futex, map, namespace, prot,
};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const STACK_ADDR: usize = 0x6000_0000;
const STACK_TOP: usize = STACK_ADDR + PAGE;
const BAD_PTR: usize = 0x1;

static TESTS: AtomicU32 = AtomicU32::new(0);
static FAILS: AtomicU32 = AtomicU32::new(0);
static FUTEX_WORD: AtomicU32 = AtomicU32::new(0);

#[cfg(target_arch = "x86_64")]
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
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
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

/// Issue one native syscall; scalar arguments only.
#[cfg(target_arch = "x86_64")]
fn sys(number: usize, a0: usize, a1: usize, a2: usize, a3: usize, a4: usize, a5: usize) -> isize {
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
    unsafe { syscall6(number, a0, a1, a2, a3, a4, a5) }
}

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn handle_of(raw: isize) -> Option<Handle> {
    match result(raw) {
        Ok(value) if value != 0 => Some(Handle(value as u32)),
        _ => None,
    }
}

fn check(name: &str, expected: Result<usize, Errno>, raw: isize) {
    TESTS.fetch_add(1, Ordering::Relaxed);
    let actual = result(raw);
    if actual != expected {
        FAILS.fetch_add(1, Ordering::Relaxed);
        println!(
            "syscall-negative: fail {name}: expected {expected:?}, got {actual:?}"
        );
    }
}

fn check_ok(name: &str, raw: isize) {
    check(name, Ok(0), raw);
}

/// A thread entry used only by the invalid `THREAD_CREATE` probes; it never runs.
#[unsafe(no_mangle)]
pub extern "C" fn syscall_negative_thread_entry() -> ! {
    let _ = sys(Syscall::THREAD_EXIT as usize, 0, 0, 0, 0, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

/// A signal entry used by the invalid `SIG_SETUP` probes; it never runs.
#[unsafe(no_mangle)]
pub extern "C" fn syscall_negative_signal_entry() {}

fn main(_args: raven_rt::Args) -> i32 {
    let mut ts = Timespec::default();
    let mut stat = [0u8; 64];
    let mut dma = [0u8; 24];
    let mut evq_event = [0u8; 16];
    let mut code = 0i32;
    let prefix = b"x";
    let empty = [0u8; 8];

    // Valid kernel objects used to distinguish a rights failure from EBADF.
    let child = match handle_of(sys(Syscall::PROC_CREATE as usize, 0, 0, 0, 0, 0, 0)) {
        Some(handle) => handle,
        None => {
            println!("syscall-negative: fail setup proc");
            return 1;
        }
    };
    let child2 = match handle_of(sys(Syscall::PROC_CREATE as usize, 0, 0, 0, 0, 0, 0)) {
        Some(handle) => handle,
        None => {
            println!("syscall-negative: fail setup proc2");
            return 1;
        }
    };
    let memobj = match handle_of(sys(
        Syscall::MEMOBJ_CREATE as usize,
        PAGE,
        0,
        0,
        0,
        0,
        0,
    )) {
        Some(handle) => handle,
        None => {
            println!("syscall-negative: fail setup memobj");
            return 1;
        }
    };
    let evq = match handle_of(sys(Syscall::EVQ_CREATE as usize, 0, 0, 0, 0, 0, 0)) {
        Some(handle) => handle,
        None => {
            println!("syscall-negative: fail setup evq");
            return 1;
        }
    };
    let timer = match handle_of(sys(
        Syscall::TIMER_CREATE as usize,
        clock::MONOTONIC as usize,
        0,
        0,
        0,
        0,
        0,
    )) {
        Some(handle) => handle,
        None => {
            println!("syscall-negative: fail setup timer");
            return 1;
        }
    };
    let event = match handle_of(sys(Syscall::EVENT_CREATE as usize, 0, 0, 0, 0, 0, 0)) {
        Some(handle) => handle,
        None => {
            println!("syscall-negative: fail setup event");
            return 1;
        }
    };
    if result(sys(
        Syscall::MAP as usize,
        Handle::SELF_PROC.0 as usize,
        Handle::NONE.0 as usize,
        0,
        STACK_ADDR,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    ))
    .is_err()
    {
        println!("syscall-negative: fail setup stack");
        return 1;
    }

    // Basic syscalls: bad user ranges and bad enum values.
    check(
        "debug-write-efault",
        Err(Errno::EFAULT),
        sys(Syscall::DEBUG_WRITE as usize, BAD_PTR, 1, 0, 0, 0, 0),
    );
    check(
        "debug-write-overflow",
        Err(Errno::EINVAL),
        sys(
            Syscall::DEBUG_WRITE as usize,
            &empty as *const [u8; 8] as usize,
            usize::MAX,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "clock-get-einval",
        Err(Errno::EINVAL),
        sys(
            Syscall::CLOCK_GET as usize,
            0xFFFF,
            &mut ts as *mut Timespec as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "clock-get-efault",
        Err(Errno::EFAULT),
        sys(
            Syscall::CLOCK_GET as usize,
            clock::MONOTONIC as usize,
            BAD_PTR,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "uname-efault",
        Err(Errno::EFAULT),
        sys(Syscall::UNAME as usize, BAD_PTR, 0, 0, 0, 0, 0),
    );

    // Process control: bad flags, bad ops/pointers, stale handles.
    check(
        "proc-create-einval",
        Err(Errno::EINVAL),
        sys(Syscall::PROC_CREATE as usize, 2, 0, 0, 0, 0, 0),
    );
    check(
        "proc-wait-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::PROC_WAIT as usize,
            0xDEAD,
            &mut code as *mut i32 as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "proc-terminate-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::PROC_TERMINATE as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "proc-namespace-op",
        Err(Errno::EINVAL),
        sys(
            Syscall::PROC_NAMESPACE as usize,
            Handle::SELF_PROC.0 as usize,
            0,
            BAD_PTR,
            1,
            0,
            0,
        ),
    );
    check(
        "proc-namespace-efault",
        Err(Errno::EFAULT),
        sys(
            Syscall::PROC_NAMESPACE as usize,
            Handle::SELF_PROC.0 as usize,
            namespace::ADD as usize,
            BAD_PTR,
            1,
            0,
            0,
        ),
    );
    check(
        "personality-einval",
        Err(Errno::EINVAL),
        sys(
            PROC_SET_PERSONALITY,
            Handle::SELF_PROC.0 as usize,
            0xFF,
            Handle::NONE.0 as usize,
            0,
            0,
            0,
        ),
    );
    check(
        "personality-ebadf",
        Err(Errno::EBADF),
        sys(
            PROC_SET_PERSONALITY,
            0xDEAD,
            0,
            Handle::NONE.0 as usize,
            0,
            0,
            0,
        ),
    );

    // Memory objects and mapping boundaries.
    check(
        "memobj-create-zero",
        Err(Errno::EINVAL),
        sys(Syscall::MEMOBJ_CREATE as usize, 0, 0, 0, 0, 0, 0),
    );
    check(
        "memobj-create-flags",
        Err(Errno::EINVAL),
        sys(Syscall::MEMOBJ_CREATE as usize, PAGE, 2, 0, 0, 0, 0),
    );
    check(
        "memobj-create-max",
        Err(Errno::EINVAL),
        sys(Syscall::MEMOBJ_CREATE as usize, usize::MAX, 0, 0, 0, 0, 0),
    );
    check(
        "map-len-zero",
        Err(Errno::EINVAL),
        sys(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            STACK_ADDR,
            0,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        ),
    );
    check(
        "map-len-unaligned",
        Err(Errno::EINVAL),
        sys(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            STACK_ADDR,
            PAGE + 1,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        ),
    );
    check(
        "map-addr-unaligned",
        Err(Errno::EINVAL),
        sys(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            STACK_ADDR + 1,
            PAGE,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        ),
    );
    check(
        "map-wx",
        Err(Errno::EINVAL),
        sys(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            STACK_ADDR,
            PAGE,
            map::pack(prot::WRITE | prot::EXEC, map::FIXED),
        ),
    );
    check(
        "map-offset-unaligned",
        Err(Errno::EINVAL),
        sys(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            memobj.0 as usize,
            1,
            STACK_ADDR,
            PAGE,
            map::pack(prot::READ, map::FIXED),
        ),
    );
    check(
        "map-memobj-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            0xDEAD,
            0,
            STACK_ADDR,
            PAGE,
            map::pack(prot::READ, map::FIXED),
        ),
    );
    check(
        "map-proc-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::MAP as usize,
            0xDEAD,
            Handle::NONE.0 as usize,
            0,
            STACK_ADDR,
            PAGE,
            map::pack(prot::READ, map::FIXED),
        ),
    );
    check(
        "unmap-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::UNMAP as usize, 0xDEAD, STACK_ADDR, PAGE, 0, 0, 0),
    );
    check(
        "unmap-einval",
        Err(Errno::EINVAL),
        sys(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            BAD_PTR,
            PAGE,
            0,
            0,
            0,
        ),
    );
    check(
        "protect-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::PROTECT as usize,
            0xDEAD,
            STACK_ADDR,
            PAGE,
            prot::READ,
            0,
            0,
        ),
    );
    check(
        "protect-einval",
        Err(Errno::EINVAL),
        sys(
            Syscall::PROTECT as usize,
            Handle::SELF_PROC.0 as usize,
            BAD_PTR,
            PAGE,
            prot::READ,
            0,
            0,
        ),
    );
    check(
        "pager-supply-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::PAGER_SUPPLY as usize,
            0xDEAD,
            0,
            &empty as *const [u8; 8] as usize,
            8,
            0,
            0,
        ),
    );

    // Threads, futex, TLS and signal-entry boundaries.
    check(
        "thread-create-efault",
        Err(Errno::EFAULT),
        sys(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            0,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "thread-create-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::THREAD_CREATE as usize,
            0xDEAD,
            syscall_negative_thread_entry as *const () as usize,
            STACK_TOP,
            0,
            0,
            0,
        ),
    );
    check(
        "thread-create-flags",
        Err(Errno::EINVAL),
        sys(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            syscall_negative_thread_entry as *const () as usize,
            STACK_TOP,
            0,
            2,
            0,
        ),
    );
    check(
        "thread-create-arg-efault",
        Err(Errno::EFAULT),
        sys(
            Syscall::THREAD_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            syscall_negative_thread_entry as *const () as usize,
            STACK_TOP,
            0,
            THREAD_CREATE_EX,
            BAD_PTR,
        ),
    );
    check(
        "thread-set-tls-efault",
        Err(Errno::EFAULT),
        sys(THREAD_SET_TLS, Handle::NONE.0 as usize, BAD_PTR, 0, 0, 0, 0),
    );
    check(
        "thread-set-tls-max",
        Err(Errno::EFAULT),
        sys(
            THREAD_SET_TLS,
            Handle::NONE.0 as usize,
            usize::MAX,
            0,
            0,
            0,
            0,
        ),
    );
    check_ok(
        "thread-set-tls-clear",
        sys(THREAD_SET_TLS, Handle::NONE.0 as usize, 0, 0, 0, 0, 0),
    );
    check(
        "thread-kick-ebadf",
        Err(Errno::EBADF),
        sys(THREAD_KICK, 0xDEAD, 1, 0, 0, 0, 0),
    );
    check_ok(
        "thread-kick-zero",
        sys(THREAD_KICK, Handle::NONE.0 as usize, 0, 0, 0, 0, 0),
    );
    check(
        "futex-unmapped",
        Err(Errno::EINVAL),
        sys(
            Syscall::FUTEX as usize,
            0x4000_0000,
            futex::WAIT as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "futex-einval",
        Err(Errno::EINVAL),
        sys(
            Syscall::FUTEX as usize,
            &FUTEX_WORD as *const AtomicU32 as usize,
            0xFF,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "futex-eagain",
        Err(Errno::EAGAIN),
        sys(
            Syscall::FUTEX as usize,
            &FUTEX_WORD as *const AtomicU32 as usize,
            futex::WAIT as usize,
            1,
            0,
            0,
            0,
        ),
    );
    check(
        "robust-list-einval",
        Err(Errno::EINVAL),
        sys(ROBUST_LIST_SET, 0x0000_8000_0000_0000, 8, 0, 0, 0, 0),
    );
    check_ok("robust-list-clear", sys(ROBUST_LIST_SET, 0, 0, 0, 0, 0, 0));
    check(
        "sig-setup-offset",
        Err(Errno::EINVAL),
        sys(
            SIG_SETUP,
            syscall_negative_signal_entry as *const () as usize,
            1,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "sig-setup-flags",
        Err(Errno::EINVAL),
        sys(
            SIG_SETUP,
            syscall_negative_signal_entry as *const () as usize,
            0,
            2,
            0,
            0,
            0,
        ),
    );
    check(
        "sig-setup-entry",
        Err(Errno::EFAULT),
        sys(SIG_SETUP, BAD_PTR, 0, 0, 0, 0, 0),
    );
    check(
        "sig-setup-max-offset",
        Err(Errno::EINVAL),
        sys(
            SIG_SETUP,
            syscall_negative_signal_entry as *const () as usize,
            usize::MAX,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "sigresend-no-setup",
        Err(Errno::EINVAL),
        sys(SIGRESEND, 0, 0, 0, 0, 0, 0),
    );
    check(
        "sigreturn-no-signal",
        Err(Errno::EINVAL),
        sys(SIGRETURN, 0, 0, 0, 0, 0, 0),
    );

    // Handles: stale handles, rights revocation and transfer destinations.
    check(
        "handle-close-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::HANDLE_CLOSE as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "handle-dup-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::HANDLE_DUP as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check_ok(
        "revoke-signal",
        sys(
            Syscall::HANDLE_REVOKE as usize,
            child.0 as usize,
            Rights::SIGNAL.bits() as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "terminate-revoked",
        Err(Errno::EPERM),
        sys(Syscall::PROC_TERMINATE as usize, child.0 as usize, 0, 0, 0, 0, 0),
    );
    check_ok(
        "revoke-write",
        sys(
            Syscall::HANDLE_REVOKE as usize,
            child2.0 as usize,
            Rights::WRITE.bits() as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "transfer-no-write",
        Err(Errno::EPERM),
        sys(
            Syscall::HANDLE_TRANSFER as usize,
            child2.0 as usize,
            memobj.0 as usize,
            Rights::READ.bits() as usize,
            0,
            0,
            0,
        ),
    );
    check_ok(
        "revoke-dup",
        sys(
            Syscall::HANDLE_REVOKE as usize,
            memobj.0 as usize,
            Rights::DUP.bits() as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "dup-revoked",
        Err(Errno::EPERM),
        sys(Syscall::HANDLE_DUP as usize, memobj.0 as usize, 0, 0, 0, 0, 0),
    );
    check_ok(
        "revoke-admin",
        sys(
            Syscall::HANDLE_REVOKE as usize,
            child2.0 as usize,
            Rights::ADMIN.bits() as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "namespace-no-admin",
        Err(Errno::EPERM),
        sys(
            Syscall::PROC_NAMESPACE as usize,
            child2.0 as usize,
            namespace::ADD as usize,
            prefix.as_ptr() as usize,
            prefix.len(),
            0,
            0,
        ),
    );
    check(
        "handle-copy-ebadf",
        Err(Errno::EBADF),
        sys(
            HANDLE_COPY,
            Handle::SELF_PROC.0 as usize,
            0xDEAD,
            Rights::READ.bits() as usize,
            0,
            0,
            0,
        ),
    );

    // Scheme IPC: bad pointers and stale handles.
    check(
        "scheme-register-efault",
        Err(Errno::EFAULT),
        sys(
            Syscall::SCHEME_REGISTER as usize,
            BAD_PTR,
            1,
            memobj.0 as usize,
            0,
            0,
            0,
        ),
    );
    check(
        "open-efault",
        Err(Errno::EFAULT),
        sys(Syscall::OPEN as usize, BAD_PTR, 1, 0, 0, 0, 0),
    );
    check(
        "open-overflow",
        Err(Errno::ENAMETOOLONG),
        sys(
            Syscall::OPEN as usize,
            prefix.as_ptr() as usize,
            usize::MAX,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "read-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::READ as usize, 0xDEAD, &mut stat as *mut [u8; 64] as usize, 1, 0, 0, 0),
    );
    check(
        "write-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::WRITE as usize, 0xDEAD, &empty as *const [u8; 8] as usize, 1, 0, 0, 0),
    );
    check(
        "close-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::CLOSE as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "dup-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::DUP as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "fstat-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::FSTAT as usize,
            0xDEAD,
            &mut stat as *mut [u8; 64] as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "call-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::CALL as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "send-einval",
        Err(Errno::EINVAL),
        sys(
            Syscall::SEND as usize,
            0xDEAD,
            &empty as *const [u8; 8] as usize,
            1,
            0,
            0,
            0,
        ),
    );

    // Events, timers and driver capabilities.
    check(
        "evq-create-flags",
        Err(Errno::EINVAL),
        sys(Syscall::EVQ_CREATE as usize, 1, 0, 0, 0, 0, 0),
    );
    check(
        "evq-ctl-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::EVQ_CTL as usize, 0xDEAD, 1, 0, 0, 0, 0),
    );
    check(
        "evq-wait-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::EVQ_WAIT as usize,
            0xDEAD,
            &mut evq_event as *mut [u8; 16] as usize,
            1,
            0,
            0,
            0,
        ),
    );
    check(
        "timer-create-bad-clock",
        Err(Errno::EINVAL),
        sys(Syscall::TIMER_CREATE as usize, 0xFFFF, 0, 0, 0, 0, 0),
    );
    check(
        "timer-set-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::TIMER_SET as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "event-create-flags",
        Err(Errno::EINVAL),
        sys(Syscall::EVENT_CREATE as usize, 1, 0, 0, 0, 0, 0),
    );
    check(
        "event-signal-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::EVENT_SIGNAL as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "mmio-create-wrong-handle",
        Err(Errno::EPERM),
        sys(
            Syscall::MMIO_CREATE as usize,
            Handle::SELF_PROC.0 as usize,
            0,
            PAGE,
            0,
            0,
            0,
        ),
    );
    check(
        "boot-blob-kind",
        Err(Errno::EINVAL),
        sys(
            Syscall::BOOT_BLOB as usize,
            Handle::ROOT_DEVICE.0 as usize,
            0,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "dma-alloc-zero",
        Err(Errno::EINVAL),
        sys(
            Syscall::DMA_ALLOC as usize,
            Handle::ROOT_DEVICE.0 as usize,
            0,
            0,
            32,
            0,
            0,
        ),
    );
    check(
        "dma-info-ebadf",
        Err(Errno::EBADF),
        sys(
            Syscall::DMA_INFO as usize,
            0xDEAD,
            &mut dma as *mut [u8; 24] as usize,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "irq-create-kind",
        Err(Errno::EINVAL),
        sys(
            Syscall::IRQ_CREATE as usize,
            Handle::ROOT_DEVICE.0 as usize,
            0,
            0,
            0,
            0,
            0,
        ),
    );
    check(
        "irq-wait-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::IRQ_WAIT as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );
    check(
        "irq-ack-ebadf",
        Err(Errno::EBADF),
        sys(Syscall::IRQ_ACK as usize, 0xDEAD, 0, 0, 0, 0, 0),
    );

    // Valid objects are still open; the parent exit tears them down.
    let _ = (evq, timer, event);
    check_ok(
        "unmap-stack",
        sys(
            Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            STACK_ADDR,
            PAGE,
            0,
            0,
            0,
        ),
    );

    let tests = TESTS.load(Ordering::Acquire);
    let fails = FAILS.load(Ordering::Acquire);
    if fails == 0 {
        println!("syscall-negative: ok {tests} checks");
        0
    } else {
        println!("syscall-negative: fail {fails}");
        1
    }
}
