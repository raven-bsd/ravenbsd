// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S48 shared-futex fixture.
//!
//! Two child processes map the same shared memory object at different virtual
//! addresses. A shared FUTEX WAIT resolves the `(object id, offset)` key; the
//! parent wakes through its own VA. A private wait at the same parent VA stays
//! asleep, the child's kill exercises process-exit queue cleanup, and unmapping
//! the parent mapping wakes the private waiter with `EFAULT`.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, futex, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::RtSys;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const PARENT_VA: usize = 0x4000_0000;
const CHILD_DATA_VA: usize = 0x4100_0000;
const CHILD_CODE_VA: usize = 0x4200_0000;
const CHILD1_PARAMS: usize = 0x100;
const CHILD2_PARAMS: usize = 0x200;
const WAIT_TIMEOUT_NS: u64 = 2_000_000_000;

static PRIVATE_READY: AtomicBool = AtomicBool::new(false);
static PRIVATE_RESULT: AtomicI32 = AtomicI32::new(0);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s48_child_stub
    .type s48_child_stub, @function
s48_child_stub:
    mov dword ptr [rdi + 8], 1
    mov eax, 0x22
    xor esi, esi
    xor edx, edx
    mov r10, [rdi + 16]
    syscall
    mov [rdi + 24], rax
    mov eax, 1
    xor edi, edi
    syscall
s48_child_stub_end:
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s48_child_stub
    .type s48_child_stub, %function
s48_child_stub:
    mov x9, x0
    mov w10, #1
    str w10, [x9, #8]
    mov x8, #0x22
    mov x1, xzr
    mov x2, xzr
    ldr x3, [x9, #16]
    svc #0
    str x0, [x9, #24]
    mov x8, #1
    mov x0, xzr
    svc #0
s48_child_stub_end:
"#
);

unsafe extern "C" {
    fn s48_child_stub();
    fn s48_child_stub_end();
}

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

#[cfg(target_arch = "aarch64")]
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
            "svc #0",
            in("x8") number,
            inlateout("x0") a0 => result,
            in("x1") a1,
            in("x2") a2,
            in("x3") a3,
            in("x4") a4,
            in("x5") a5,
            options(nostack),
        );
    }
    result
}

fn result(raw: isize) -> Result<usize, Errno> {
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    result(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn map_raw(
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

fn protect_raw(process: Handle, address: usize, len: usize, protection: u8) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::PROTECT as usize,
            process.0 as usize,
            address,
            len,
            protection as usize,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn unmap(process: Handle, address: usize, len: usize) -> Result<(), Errno> {
    result(unsafe {
        syscall6(
            Syscall::UNMAP as usize,
            process.0 as usize,
            address,
            len,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn thread_create(
    process: Handle,
    entry: usize,
    stack_top: usize,
    arg: usize,
) -> Result<Handle, Errno> {
    result(unsafe {
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
    .map(|value| Handle(value as u32))
}

fn proc_create() -> Result<Handle, Errno> {
    result(unsafe { syscall6(Syscall::PROC_CREATE as usize, 0, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn futex_raw(address: usize, op: u32, value: u32, timeout_ns: u64) -> isize {
    unsafe {
        syscall6(
            Syscall::FUTEX as usize,
            address,
            op as usize,
            value as usize,
            timeout_ns as usize,
            0,
            0,
        )
    }
}

fn write_u32(address: usize, value: u32) {
    // SAFETY: the fixture owns the mapping at `address` for the duration.
    unsafe {
        core::ptr::write_volatile(address as *mut u32, value);
    }
}

fn write_u64(address: usize, value: u64) {
    // SAFETY: the fixture owns the mapping at `address` for the duration.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn read_u32(address: usize) -> u32 {
    // SAFETY: the fixture owns the mapping at `address` for the duration.
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn read_i64(address: usize) -> i64 {
    // SAFETY: the fixture owns the mapping at `address` for the duration.
    unsafe { core::ptr::read_volatile(address as *const i64) }
}

fn wait_ready(offset: usize) -> bool {
    let mut attempts = 0u32;
    while attempts < 100 {
        if read_u32(PARENT_VA + offset + 8) == 1 {
            return true;
        }
        let _ = sys::nanosleep(10_000_000);
        attempts = attempts.saturating_add(1);
    }
    false
}

fn main(_args: raven_rt::Args) -> i32 {
    controller()
}

fn controller() -> i32 {
    let object = match memobj_create(2 * PAGE) {
        Ok(object) => object,
        Err(_) => return 1,
    };
    let flags = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
    if map_raw(Handle::SELF_PROC, object, 0, PARENT_VA, 2 * PAGE, flags).is_err() {
        println!("shared-futex: parent map failed");
        return 1;
    }
    // The code page is copied through the parent's writable mapping.
    let stub_start = s48_child_stub as *const () as usize;
    let stub_end = s48_child_stub_end as *const () as usize;
    let stub_len = stub_end.saturating_sub(stub_start);
    // SAFETY: the source is the fixture's own code blob and the destination is
    // the writable half of its shared object, with enough room for the stub.
    unsafe {
        core::ptr::copy_nonoverlapping(
            stub_start as *const u8,
            (PARENT_VA + PAGE) as *mut u8,
            stub_len,
        );
    }

    let child1 = match bare_child(object, CHILD1_PARAMS) {
        Ok(child) => child,
        Err(_) => {
            println!("shared-futex: child1 create failed");
            return 1;
        }
    };
    if !wait_ready(CHILD1_PARAMS) {
        println!("shared-futex: child1 not ready");
        return 1;
    }

    let private_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return 1,
    };
    let private = match sys::thread_create(
        Handle::SELF_PROC,
        private_waiter as *const () as usize,
        private_stack + 2 * PAGE,
        PARENT_VA + CHILD1_PARAMS,
    ) {
        Ok(thread) => thread,
        Err(_) => return 1,
    };
    let mut spins = 0u32;
    while !PRIVATE_READY.load(Ordering::SeqCst) && spins < 100 {
        let _ = sys::nanosleep(5_000_000);
        spins = spins.saturating_add(1);
    }
    let _ = sys::nanosleep(50_000_000);
    write_u32(PARENT_VA + CHILD1_PARAMS, 1);
    let wake = futex_raw(PARENT_VA + CHILD1_PARAMS, futex::WAKE, 1, 0);
    println!("shared-futex: shared wake={wake}");
    let mut code = 0i32;
    let mut rt = RtSys;
    if rt.proc_wait(child1, &mut code).is_err() || code != 0 {
        println!("shared-futex: child1 wait failed");
        return 1;
    }
    let child1_result = read_i64(PARENT_VA + CHILD1_PARAMS + 24);
    println!("shared-futex: child1 wake={child1_result}");
    println!("shared-futex: child1 status={code}");
    if child1_result != 0 {
        return 1;
    }

    let child2 = match bare_child(object, CHILD2_PARAMS) {
        Ok(child) => child,
        Err(_) => {
            println!("shared-futex: child2 create failed");
            return 1;
        }
    };
    if !wait_ready(CHILD2_PARAMS) {
        println!("shared-futex: child2 not ready");
        return 1;
    }
    let _ = sys::nanosleep(50_000_000);
    if sys::proc_terminate(child2, 0).is_err() {
        println!("shared-futex: child2 terminate failed");
        return 1;
    }
    let mut code2 = 0i32;
    if rt.proc_wait(child2, &mut code2).is_err() {
        println!("shared-futex: child2 wait failed");
        return 1;
    }
    let cleanup = futex_raw(PARENT_VA + CHILD2_PARAMS, futex::WAKE, 1, 0);
    println!("shared-futex: cleanup wake={cleanup}");
    println!("shared-futex: child2 status={code2}");
    if cleanup != 0 || code2 != 0 {
        return 1;
    }

    if unmap(Handle::SELF_PROC, PARENT_VA, 2 * PAGE).is_err() {
        println!("shared-futex: parent unmap failed");
        return 1;
    }
    let _ = sys::nanosleep(100_000_000);
    let private_result = PRIVATE_RESULT.load(Ordering::SeqCst);
    if private_result == Errno::EFAULT.to_ret() as i32 {
        println!("shared-futex: private=EFAULT");
    } else {
        println!("shared-futex: private unexpected={private_result}");
        return 1;
    }
    let _ = private;
    println!("shared-futex: done");
    0
}

/// Create a bare child process with the shared data/code mappings and one stub thread.
fn bare_child(object: Handle, params_offset: usize) -> Result<Handle, Errno> {
    let child = proc_create()?;
    let data_flags = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
    if map_raw(child, object, 0, CHILD_DATA_VA, PAGE, data_flags).is_err() {
        println!("shared-futex: child data map failed");
        return Err(Errno::ENOMEM);
    }
    // A memory-object handle has no EXEC right; map the code page writable,
    // then narrow it to RX (W^X) through PROTECT.
    let code_flags = map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED);
    if map_raw(child, object, PAGE, CHILD_CODE_VA, PAGE, code_flags).is_err() {
        println!("shared-futex: child code map failed");
        return Err(Errno::ENOMEM);
    }
    if protect_raw(child, CHILD_CODE_VA, PAGE, (prot::READ | prot::EXEC) as u8).is_err() {
        println!("shared-futex: child code protect failed");
        return Err(Errno::ENOMEM);
    }
    let stack = match map_raw(
        child,
        Handle::NONE,
        0,
        0,
        2 * PAGE,
        map::pack(prot::READ | prot::WRITE, 0),
    ) {
        Ok(stack) => stack,
        Err(_) => {
            println!("shared-futex: child stack map failed");
            return Err(Errno::ENOMEM);
        }
    };
    let params = CHILD_DATA_VA + params_offset;
    write_u64(PARENT_VA + params_offset + 16, WAIT_TIMEOUT_NS);
    write_u64(PARENT_VA + params_offset + 24, 0);
    write_u32(PARENT_VA + params_offset, 0);
    write_u32(PARENT_VA + params_offset + 8, 0);
    if thread_create(child, CHILD_CODE_VA, stack + 2 * PAGE, params).is_err() {
        println!("shared-futex: child thread create failed");
        return Err(Errno::ENOMEM);
    }
    Ok(child)
}

extern "C" fn private_waiter(arg: usize) -> ! {
    PRIVATE_READY.store(true, Ordering::SeqCst);
    let raw = futex_raw(arg, futex::WAIT | futex::PRIVATE, 0, WAIT_TIMEOUT_NS);
    PRIVATE_RESULT.store(raw as i32, Ordering::SeqCst);
    println!("shared-futex: private result={raw}");
    sys::thread_exit(0)
}
