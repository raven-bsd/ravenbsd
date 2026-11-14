// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S49 robust-futex fixture.
//!
//! A locker thread registers a Linux `robust_list_head` and exits without
//! unlocking; the kernel sets OWNER_DIED and wakes the waiter. The same is
//! proved for `PROC_TERMINATE` of a bare child holding a shared robust word.
//! Bad head/len and the 2048-entry walk bound are covered too.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{ROBUST_LIST_SET, Syscall};
use raven_abi::types::{Handle, futex, map, prot};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::RtSys;
use raven_rt::sys;

entry!(main);

const PAGE: usize = 4096;
const OWNER: u32 = 0x1234;
const OWNER_DIED: u32 = futex::OWNER_DIED;
const OWNER_TID: u32 = futex::OWNER_TID;
const BIG_ENTRIES: usize = 2050;
const PARENT_VA: usize = 0x4000_0000;
const CHILD_DATA_VA: usize = 0x4100_0000;
const CHILD_CODE_VA: usize = 0x4200_0000;
const HEAD_OFF: usize = 0x100;
const ENTRY_OFF: usize = 0x200;
const PARAMS_OFF: usize = 0x300;

#[repr(C)]
struct RobustHead {
    next: usize,
    futex_offset: i64,
    pending: usize,
}

#[repr(C)]
struct RobustEntry {
    next: usize,
    futex: u32,
    padding: u32,
}

impl RobustEntry {
    const fn new() -> Self {
        Self {
            next: 0,
            futex: 0,
            padding: 0,
        }
    }
}

static mut SIMPLE_HEAD: RobustHead = RobustHead {
    next: 0,
    futex_offset: 0,
    pending: 0,
};
static mut SIMPLE_ENTRY: RobustEntry = RobustEntry::new();
static mut BIG_HEAD: RobustHead = RobustHead {
    next: 0,
    futex_offset: 0,
    pending: 0,
};
static mut BIG_LIST: [RobustEntry; BIG_ENTRIES] = [const { RobustEntry::new() }; BIG_ENTRIES];

static SIMPLE_LOCKER_DONE: AtomicBool = AtomicBool::new(false);
static SIMPLE_WAITER_DONE: AtomicBool = AtomicBool::new(false);
static SIMPLE_WAITER_DIED: AtomicBool = AtomicBool::new(false);
static SIMPLE_WAITER_CLEARED: AtomicBool = AtomicBool::new(false);
static BIG_LOCKER_DONE: AtomicBool = AtomicBool::new(false);
static PROC_WAITER_DONE: AtomicBool = AtomicBool::new(false);
static PROC_WAITER_DIED: AtomicBool = AtomicBool::new(false);
static PROC_WAITER_CLEARED: AtomicBool = AtomicBool::new(false);
static PROC_WAITER_READY: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s49_child_stub
    .type s49_child_stub, @function
s49_child_stub:
    mov rbx, rdi
    mov rdi, [rbx]
    mov esi, 24
    mov eax, 0x26
    syscall
    mov dword ptr [rbx + 8], 1
    lea rdi, [rbx + 16]
    mov eax, 0x22
    xor esi, esi
    xor edx, edx
    xor r10d, r10d
    syscall
    mov eax, 1
    xor edi, edi
    syscall
s49_child_stub_end:
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global s49_child_stub
    .type s49_child_stub, %function
s49_child_stub:
    mov x9, x0
    ldr x0, [x9]
    mov x1, #24
    mov x8, #0x26
    svc #0
    mov w10, #1
    str w10, [x9, #8]
    add x0, x9, #16
    mov x8, #0x22
    mov x1, xzr
    mov x2, xzr
    mov x3, xzr
    svc #0
    mov x8, #1
    mov x0, xzr
    svc #0
s49_child_stub_end:
"#
);

unsafe extern "C" {
    fn s49_child_stub();
    fn s49_child_stub_end();
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

fn robust_set(head: usize, len: usize) -> Result<(), Errno> {
    result(unsafe { syscall6(ROBUST_LIST_SET, head, len, 0, 0, 0, 0) }).map(|_| ())
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

fn read_u32(address: usize) -> u32 {
    // SAFETY: the fixture owns the mapping/static for the duration.
    unsafe { core::ptr::read_volatile(address as *const u32) }
}

fn write_u32(address: usize, value: u32) {
    // SAFETY: the fixture owns the mapping/static for the duration.
    unsafe {
        core::ptr::write_volatile(address as *mut u32, value);
    }
}

fn write_u64(address: usize, value: u64) {
    // SAFETY: the fixture owns the mapping/static for the duration.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn write_i64(address: usize, value: i64) {
    // SAFETY: the fixture owns the mapping/static for the duration.
    unsafe {
        core::ptr::write_volatile(address as *mut i64, value);
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

fn main(_args: raven_rt::Args) -> i32 {
    controller()
}

fn controller() -> i32 {
    if !bad_arguments() {
        return 1;
    }
    if !simple_thread_case() {
        return 1;
    }
    if !bound_case() {
        return 1;
    }
    if !terminate_case() {
        return 1;
    }
    println!("robust-futex: done");
    0
}

fn bad_arguments() -> bool {
    let head = core::ptr::addr_of_mut!(SIMPLE_HEAD) as usize;
    let bad_head = !matches!(robust_set(1, 24), Err(Errno::EINVAL));
    let bad_len = !matches!(robust_set(head, 8), Err(Errno::EINVAL));
    let bad_clear = !matches!(robust_set(0, 24), Err(Errno::EINVAL));
    let clear = robust_set(0, 0).is_ok();
    let ok = !bad_head && !bad_len && !bad_clear && clear;
    println!("robust-futex: bad={}", u8::from(ok));
    ok
}

fn simple_thread_case() -> bool {
    let head = core::ptr::addr_of_mut!(SIMPLE_HEAD) as usize;
    let entry = core::ptr::addr_of_mut!(SIMPLE_ENTRY) as usize;
    let futex_word = entry.saturating_add(8);
    // SAFETY: the fixture owns these statics and no other thread reads them yet.
    unsafe {
        let head_ref = &mut *core::ptr::addr_of_mut!(SIMPLE_HEAD);
        head_ref.next = entry;
        head_ref.futex_offset = 8;
        head_ref.pending = 0;
        let entry_ref = &mut *core::ptr::addr_of_mut!(SIMPLE_ENTRY);
        entry_ref.next = head;
        entry_ref.futex = OWNER;
        entry_ref.padding = 0;
    }
    let waiter_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return false,
    };
    let waiter = thread_create(
        Handle::SELF_PROC,
        simple_waiter as *const () as usize,
        waiter_stack + 2 * PAGE,
        futex_word,
    );
    if waiter.is_err() {
        return false;
    }
    let locker_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return false,
    };
    let locker = thread_create(
        Handle::SELF_PROC,
        simple_locker as *const () as usize,
        locker_stack + 2 * PAGE,
        head,
    );
    if locker.is_err() {
        return false;
    }
    let mut spins = 0u32;
    while !SIMPLE_WAITER_DONE.load(Ordering::SeqCst) && spins < 400 {
        let _ = sys::nanosleep(5_000_000);
        spins = spins.saturating_add(1);
    }
    let died = SIMPLE_WAITER_DIED.load(Ordering::SeqCst);
    let cleared = SIMPLE_WAITER_CLEARED.load(Ordering::SeqCst);
    let ok = SIMPLE_WAITER_DONE.load(Ordering::SeqCst)
        && SIMPLE_LOCKER_DONE.load(Ordering::SeqCst)
        && died
        && cleared;
    println!(
        "robust-futex: thread waiter owner-died={} cleared={}",
        u8::from(died),
        u8::from(cleared)
    );
    ok
}

fn bound_case() -> bool {
    let head = core::ptr::addr_of_mut!(BIG_HEAD) as usize;
    // SAFETY: the controller is the only thread filling the static list before
    // the locker is created.
    unsafe {
        let head_ref = &mut *core::ptr::addr_of_mut!(BIG_HEAD);
        head_ref.next = core::ptr::addr_of_mut!(BIG_LIST) as usize;
        head_ref.futex_offset = 8;
        head_ref.pending = 0;
        let entries = &mut *core::ptr::addr_of_mut!(BIG_LIST);
        let mut index = 0usize;
        while index < BIG_ENTRIES {
            entries[index].next = if index + 1 < BIG_ENTRIES {
                core::ptr::addr_of_mut!(BIG_LIST)
                    .cast::<RobustEntry>()
                    .add(index + 1) as usize
            } else {
                head
            };
            entries[index].futex = OWNER;
            entries[index].padding = 0;
            index = index.saturating_add(1);
        }
    }
    let stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return false,
    };
    if thread_create(
        Handle::SELF_PROC,
        big_locker as *const () as usize,
        stack + 2 * PAGE,
        head,
    )
    .is_err()
    {
        return false;
    }
    let mut spins = 0u32;
    while !BIG_LOCKER_DONE.load(Ordering::SeqCst) && spins < 400 {
        let _ = sys::nanosleep(5_000_000);
        spins = spins.saturating_add(1);
    }
    let mut marked = 0usize;
    let mut index = 0usize;
    while index < BIG_ENTRIES {
        // SAFETY: the list is a fixture static and the locker has exited.
        let word = unsafe { (*core::ptr::addr_of!(BIG_LIST))[index].futex };
        if word & OWNER_DIED != 0 {
            marked = marked.saturating_add(1);
        }
        index = index.saturating_add(1);
    }
    let ok = BIG_LOCKER_DONE.load(Ordering::SeqCst) && marked == 2048;
    println!("robust-futex: bound={marked}");
    ok
}

fn terminate_case() -> bool {
    let object = match memobj_create(2 * PAGE) {
        Ok(object) => object,
        Err(_) => return false,
    };
    if map_raw(
        Handle::SELF_PROC,
        object,
        0,
        PARENT_VA,
        2 * PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )
    .is_err()
    {
        return false;
    }
    let head_off = HEAD_OFF;
    let entry_off = ENTRY_OFF;
    let params_off = PARAMS_OFF;
    let head_child = CHILD_DATA_VA + head_off;
    let entry_child = CHILD_DATA_VA + entry_off;
    write_u64(PARENT_VA + head_off, entry_child as u64);
    write_i64(PARENT_VA + head_off + 8, 8);
    write_u64(PARENT_VA + head_off + 16, 0);
    write_u64(PARENT_VA + entry_off, head_child as u64);
    write_u32(PARENT_VA + entry_off + 8, OWNER);
    write_u32(PARENT_VA + entry_off + 12, 0);
    write_u64(PARENT_VA + params_off, head_child as u64);
    write_u64(PARENT_VA + params_off + 16, 0);
    write_u32(PARENT_VA + params_off + 8, 0);

    let stub_start = s49_child_stub as *const () as usize;
    let stub_end = s49_child_stub_end as *const () as usize;
    let stub_len = stub_end.saturating_sub(stub_start);
    // SAFETY: source is the fixture's code blob; destination is the writable
    // half of the shared object.
    unsafe {
        core::ptr::copy_nonoverlapping(
            stub_start as *const u8,
            (PARENT_VA + PAGE) as *mut u8,
            stub_len,
        );
    }

    let child = match proc_create() {
        Ok(child) => child,
        Err(_) => return false,
    };
    if map_raw(
        child,
        object,
        0,
        CHILD_DATA_VA,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )
    .is_err()
    {
        return false;
    }
    if map_raw(
        child,
        object,
        PAGE,
        CHILD_CODE_VA,
        PAGE,
        map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
    )
    .is_err()
    {
        return false;
    }
    if protect_raw(child, CHILD_CODE_VA, PAGE, (prot::READ | prot::EXEC) as u8).is_err() {
        return false;
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
        Err(_) => return false,
    };
    if thread_create(
        child,
        CHILD_CODE_VA,
        stack + 2 * PAGE,
        CHILD_DATA_VA + params_off,
    )
    .is_err()
    {
        return false;
    }
    let mut spins = 0u32;
    while read_u32(PARENT_VA + params_off + 8) != 1 && spins < 200 {
        let _ = sys::nanosleep(5_000_000);
        spins = spins.saturating_add(1);
    }
    if read_u32(PARENT_VA + params_off + 8) != 1 {
        return false;
    }

    let waiter_stack = match sys::map_anonymous(2 * PAGE) {
        Ok(stack) => stack,
        Err(_) => return false,
    };
    let waiter = thread_create(
        Handle::SELF_PROC,
        process_waiter as *const () as usize,
        waiter_stack + 2 * PAGE,
        PARENT_VA + entry_off + 8,
    );
    if waiter.is_err() {
        return false;
    }
    spins = 0;
    while !PROC_WAITER_READY.load(Ordering::SeqCst) && spins < 200 {
        let _ = sys::nanosleep(5_000_000);
        spins = spins.saturating_add(1);
    }
    let _ = sys::nanosleep(50_000_000);
    if sys::proc_terminate(child, 0).is_err() {
        return false;
    }
    let mut code = 0i32;
    let mut rt = RtSys;
    if rt.proc_wait(child, &mut code).is_err() {
        return false;
    }
    spins = 0;
    while !PROC_WAITER_DONE.load(Ordering::SeqCst) && spins < 400 {
        let _ = sys::nanosleep(5_000_000);
        spins = spins.saturating_add(1);
    }
    let died = PROC_WAITER_DIED.load(Ordering::SeqCst);
    let cleared = PROC_WAITER_CLEARED.load(Ordering::SeqCst);
    let ok = code == 0 && PROC_WAITER_DONE.load(Ordering::SeqCst) && died && cleared;
    println!(
        "robust-futex: process waiter owner-died={} cleared={}",
        u8::from(died),
        u8::from(cleared)
    );
    ok
}

extern "C" fn simple_locker(head: usize) -> ! {
    let raw = robust_set(head, 24);
    if raw.is_err() {
        println!("robust-futex: locker set failed");
    }
    SIMPLE_LOCKER_DONE.store(true, Ordering::SeqCst);
    let _ = sys::nanosleep(100_000_000);
    sys::thread_exit(0)
}

extern "C" fn simple_waiter(word: usize) -> ! {
    let raw = futex_raw(word, futex::WAIT | futex::PRIVATE, OWNER, 2_000_000_000);
    let value = read_u32(word);
    let died = raw == 0 && value & OWNER_DIED != 0;
    let cleared = died && value & OWNER_TID == 0;
    if cleared {
        write_u32(word, value & !OWNER_DIED);
    }
    SIMPLE_WAITER_DIED.store(died, Ordering::SeqCst);
    SIMPLE_WAITER_CLEARED.store(cleared, Ordering::SeqCst);
    SIMPLE_WAITER_DONE.store(true, Ordering::SeqCst);
    sys::thread_exit(0)
}

extern "C" fn big_locker(head: usize) -> ! {
    let _ = robust_set(head, 24);
    BIG_LOCKER_DONE.store(true, Ordering::SeqCst);
    sys::thread_exit(0)
}

extern "C" fn process_waiter(word: usize) -> ! {
    PROC_WAITER_READY.store(true, Ordering::SeqCst);
    let raw = futex_raw(word, futex::WAIT, OWNER, 2_000_000_000);
    let value = read_u32(word);
    let died = raw == 0 && value & OWNER_DIED != 0;
    let cleared = died && value & OWNER_TID == 0;
    if cleared {
        write_u32(word, value & !OWNER_DIED);
    }
    PROC_WAITER_DIED.store(died, Ordering::SeqCst);
    PROC_WAITER_CLEARED.store(cleared, Ordering::SeqCst);
    PROC_WAITER_DONE.store(true, Ordering::SeqCst);
    sys::thread_exit(0)
}
