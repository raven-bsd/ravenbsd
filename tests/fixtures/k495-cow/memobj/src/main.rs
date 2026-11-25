// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S164 C13 fork COW and memory-object SHARED visibility fixture.
//!
//! One Rust service covers every contract row: private fork COW in both write orders, one
//! SHARED memory object inherited across fork, two SHARED mappings of one object in one process,
//! and an object handle inherited across fork+exec. The K495 C blob is a published diagnostic
//! fixture and stays byte-for-byte unchanged; this fixture replaces only the qtest service
//! binary, which is overlaid as `/bin/libc-test`.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

const PAGE: usize = 4096;
const FILL: u8 = 0xAA;
const PARENT_STORE: u8 = 0xBB;
const CHILD_STORE: u8 = 0xCC;

const PRIVATE_FIRST: usize = 0x5a00_0000;
const CONTROL_FIRST: usize = 0x5a01_0000;
const PRIVATE_SECOND: usize = 0x5a02_0000;
const CONTROL_SECOND: usize = 0x5a03_0000;
const SHARED_FORK_DATA: usize = 0x5a04_0000;
const SHARED_FORK_CONTROL: usize = 0x5a05_0000;
const REMAP_FIRST: usize = 0x5a06_0000;
const REMAP_SECOND: usize = 0x5a07_0000;
const SPAWN_DATA: usize = 0x5a08_0000;
const SPAWN_CONTROL: usize = 0x5a09_0000;

const C_READY: usize = 0;
const C_STORE: usize = 8;
const C_DONE: usize = 16;
const C_DATA_HANDLE: usize = 24;
const C_CONTROL_HANDLE: usize = 32;

const SERVICE_PATH: &[u8] = b"/bin/libc-test";
const MODE_SHARED: &str = "shared-handle";

/// Raw `proc_fork(src, flags) -> Handle` number (the ABI crate is reviewer-owned).
const PROC_FORK: usize = 0x39;
const SPIN_LIMIT: usize = 20_000_000;

fn fence() {
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
}

fn checked(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle_from(raw: isize) -> Result<Handle, Errno> {
    checked(raw).and_then(|value| u32::try_from(value).map(Handle).map_err(|_| Errno::EBADF))
}

fn pid() -> u64 {
    let mut values = [0u64; 8];
    // SAFETY: PROC_SELF_INFO writes eight u64 identity words to the live caller-owned array.
    let result = unsafe {
        raven_rt::sys::syscall6(0x35, values.as_mut_ptr() as usize, values.len(), 0, 0, 0, 0)
    };
    if Errno::from_ret(result).is_ok() {
        values[0]
    } else {
        0
    }
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes scalar arguments only; the kernel validates the length.
    handle_from(unsafe {
        raven_rt::sys::syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0)
    })
}

fn map_object(object: Handle, address: usize, len: usize, shared: bool) -> Result<usize, Errno> {
    // SAFETY: MAP takes scalar arguments only; the kernel validates the handle, range and flags.
    checked(unsafe {
        raven_rt::sys::syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            0,
            address,
            len,
            map::pack(
                prot::READ | prot::WRITE,
                map::FIXED | if shared { map::SHARED } else { 0 },
            ),
        )
    })
}

fn map_private(address: usize, len: usize) -> Result<usize, Errno> {
    // SAFETY: MAP takes scalar arguments only; the kernel validates the range and flags.
    checked(unsafe {
        raven_rt::sys::syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::NONE.0 as usize,
            0,
            address,
            len,
            map::pack(prot::READ | prot::WRITE, map::FIXED),
        )
    })
}

fn proc_fork() -> Result<Handle, Errno> {
    // SAFETY: PROC_FORK takes scalar arguments only.
    handle_from(unsafe {
        raven_rt::sys::syscall6(PROC_FORK, Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0)
    })
}

fn proc_wait(process: Handle, code: &mut i32) -> Result<(), Errno> {
    // SAFETY: `code` is a live writable i32 and the kernel writes at most four bytes.
    checked(unsafe {
        raven_rt::sys::syscall6(
            Syscall::PROC_WAIT as usize,
            process.0 as usize,
            code as *mut i32 as usize,
            0,
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
    // SAFETY: THREAD_CREATE takes scalar arguments and a live entry/stack; the kernel validates them.
    handle_from(unsafe {
        raven_rt::sys::syscall6(
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

fn read64(address: usize) -> u64 {
    // SAFETY: callers pass an address inside the control mapping they own.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write64(address: usize, value: u64) {
    // SAFETY: callers pass an address inside the control mapping they own.
    unsafe { core::ptr::write_volatile(address as *mut u64, value) }
}

fn read8(address: usize) -> u8 {
    // SAFETY: callers pass an address inside a readable data mapping they own.
    unsafe { core::ptr::read_volatile(address as *const u8) }
}

fn write8(address: usize, value: u8) {
    // SAFETY: callers pass an address inside a writable data mapping they own.
    unsafe { core::ptr::write_volatile(address as *mut u8, value) }
}

fn fill(address: usize, len: usize, value: u8) {
    for offset in (0..len).step_by(8) {
        // SAFETY: the caller mapped `len` writable bytes at `address`; the stride stays in range.
        unsafe {
            core::ptr::write_volatile(
                (address + offset) as *mut u64,
                u64::from_ne_bytes([value; 8]),
            );
        }
    }
}

fn zero_control(address: usize) {
    for offset in (0..PAGE).step_by(8) {
        write64(address + offset, 0);
    }
}

fn wait_value(address: usize, expected: u64) -> bool {
    for _ in 0..SPIN_LIMIT {
        if read64(address) == expected {
            return true;
        }
        raven_rt::sys::yield_now();
    }
    false
}

fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

fn new_shared(address: usize) -> Result<(Handle, usize), Errno> {
    let object = memobj_create(PAGE)?;
    let mapping = map_object(object, address, PAGE, true)?;
    Ok((object, mapping))
}

fn new_control(address: usize) -> Result<(Handle, usize), Errno> {
    let (object, mapping) = new_shared(address)?;
    zero_control(mapping);
    Ok((object, mapping))
}

fn fail(what: &str) -> Errno {
    println!("K495-COW FAIL {what}");
    Errno::EINVAL
}

extern "C" fn child_private_first(control: usize) -> ! {
    let page = PRIVATE_FIRST;
    let inherited = read8(page);
    write8(page, CHILD_STORE);
    fence();
    write64(control + C_READY, 1);
    if !wait_value(control + C_STORE, 1) {
        println!("K495-COW FAIL private-first store");
        raven_rt::sys::thread_exit(1);
    }
    fence();
    let after_parent = inherited;
    let after_child = read8(page);
    println!(
        "K495-COW fork-child pid={} addr=0x{page:x} inherited={inherited:02x} first={inherited:02x} after-parent={after_parent:02x} after-child={after_child:02x} last={:02x}",
        pid(),
        read8(page + PAGE - 1)
    );
    write64(control + C_DONE, 1);
    let ok = inherited == FILL && after_child == CHILD_STORE && read8(page + PAGE - 1) == FILL;
    raven_rt::sys::thread_exit(if ok { 0 } else { 2 })
}

fn run_private_first() -> Result<(), Errno> {
    let page = map_private(PRIVATE_FIRST, PAGE)?;
    fill(page, PAGE, FILL);
    println!(
        "K495-COW fork-parent pid={} addr=0x{page:x} initial={:02x} last={:02x}",
        pid(),
        read8(page),
        read8(page + PAGE - 1)
    );
    let (_object, control) = new_control(CONTROL_FIRST)?;
    let child = proc_fork()?;
    thread_create(
        child,
        child_private_first as *const () as usize,
        stack_top(),
        control,
    )?;
    if !wait_value(control + C_READY, 1) {
        return Err(fail("private-first ready"));
    }
    write8(page, PARENT_STORE);
    fence();
    write64(control + C_STORE, 1);
    if !wait_value(control + C_DONE, 1) {
        return Err(fail("private-first done"));
    }
    let mut code = 0i32;
    proc_wait(child, &mut code)?;
    println!(
        "K495-COW fork-parent status={code} after-parent={:02x} last={:02x}",
        read8(page),
        read8(page + PAGE - 1)
    );
    if code != 0 || read8(page) != PARENT_STORE || read8(page + PAGE - 1) != FILL {
        return Err(fail("fork isolation"));
    }
    Ok(())
}

extern "C" fn child_private_first_child(control: usize) -> ! {
    let page = PRIVATE_SECOND;
    write8(page, CHILD_STORE);
    fence();
    write64(control + C_READY, 1);
    if !wait_value(control + C_STORE, 1) {
        println!("K495-COW FAIL private-child-first store");
        raven_rt::sys::thread_exit(1);
    }
    fence();
    let after_parent = read8(page);
    println!(
        "K495-COW fork-child-first-child pid={} addr=0x{page:x} stored={CHILD_STORE:02x} after-parent-store={after_parent:02x}",
        pid()
    );
    write64(control + C_DONE, 1);
    let ok = after_parent == CHILD_STORE && read8(page + PAGE - 1) == FILL;
    raven_rt::sys::thread_exit(if ok { 0 } else { 2 })
}

fn run_private_child_first() -> Result<(), Errno> {
    let page = map_private(PRIVATE_SECOND, PAGE)?;
    fill(page, PAGE, FILL);
    println!(
        "K495-COW fork-child-first-parent pid={} initial={:02x}",
        pid(),
        read8(page)
    );
    let (_object, control) = new_control(CONTROL_SECOND)?;
    let child = proc_fork()?;
    thread_create(
        child,
        child_private_first_child as *const () as usize,
        stack_top(),
        control,
    )?;
    if !wait_value(control + C_READY, 1) {
        return Err(fail("private-child-first ready"));
    }
    let after_child = read8(page);
    write8(page, PARENT_STORE);
    fence();
    write64(control + C_STORE, 1);
    if !wait_value(control + C_DONE, 1) {
        return Err(fail("private-child-first done"));
    }
    let mut code = 0i32;
    proc_wait(child, &mut code)?;
    println!(
        "K495-COW fork-child-first-parent status={code} after-child={after_child:02x} after-parent={:02x}",
        read8(page)
    );
    if code != 0 || after_child != FILL || read8(page) != PARENT_STORE {
        return Err(fail("fork child-first isolation"));
    }
    Ok(())
}

extern "C" fn child_shared_fork(control: usize) -> ! {
    let inherited = read8(SHARED_FORK_DATA);
    fence();
    write64(control + C_READY, 1);
    if !wait_value(control + C_STORE, 1) {
        println!("K495-COW FAIL shared-fork store");
        raven_rt::sys::thread_exit(1);
    }
    fence();
    let after = read8(SHARED_FORK_DATA);
    println!(
        "K495-COW shared-fork-child pid={} addr=0x{SHARED_FORK_DATA:x} inherited={inherited:02x} after-store={after:02x}",
        pid()
    );
    write64(control + C_DONE, 1);
    let ok = inherited == FILL && after == PARENT_STORE;
    raven_rt::sys::thread_exit(if ok { 0 } else { 2 })
}

fn run_shared_fork() -> Result<(), Errno> {
    let (_object, data) = new_shared(SHARED_FORK_DATA)?;
    fill(data, PAGE, FILL);
    println!(
        "K495-COW shared-fork-parent pid={} addr=0x{data:x} initial={:02x}",
        pid(),
        read8(data)
    );
    let (_control_object, control) = new_control(SHARED_FORK_CONTROL)?;
    let child = proc_fork()?;
    thread_create(
        child,
        child_shared_fork as *const () as usize,
        stack_top(),
        control,
    )?;
    if !wait_value(control + C_READY, 1) {
        return Err(fail("shared-fork ready"));
    }
    write8(data, PARENT_STORE);
    fence();
    write64(control + C_STORE, 1);
    if !wait_value(control + C_DONE, 1) {
        return Err(fail("shared-fork done"));
    }
    let mut code = 0i32;
    proc_wait(child, &mut code)?;
    println!(
        "K495-COW shared-fork-parent status={code} after-store={:02x}",
        read8(data)
    );
    if code != 0 || read8(data) != PARENT_STORE {
        return Err(fail("shared fork store"));
    }
    Ok(())
}

fn run_remap() -> Result<(), Errno> {
    let object = memobj_create(PAGE)?;
    let first = map_object(object, REMAP_FIRST, PAGE, true)?;
    fill(first, PAGE, FILL);
    let second = map_object(object, REMAP_SECOND, PAGE, true)?;
    let before = read8(first);
    write8(first, PARENT_STORE);
    fence();
    let after = read8(second);
    println!(
        "K495-COW remap-parent pid={} first={before:02x} after-store={after:02x}",
        pid()
    );
    if before != FILL || after != PARENT_STORE || read8(second + PAGE - 1) != FILL {
        return Err(fail("remap store"));
    }
    Ok(())
}

extern "C" fn exec_child_entry(argument: usize) -> ! {
    let control = argument;
    let data = Handle(read64(control + C_DATA_HANDLE) as u32);
    let control_object = Handle(read64(control + C_CONTROL_HANDLE) as u32);
    let data_text = format!("{}", data.0);
    let control_text = format!("{}", control_object.0);
    let args: [&[u8]; 4] = [
        SERVICE_PATH,
        MODE_SHARED.as_bytes(),
        data_text.as_bytes(),
        control_text.as_bytes(),
    ];
    match raven_rt::exec::execve(SERVICE_PATH, &args, &[], &[]) {
        Ok(()) => println!("K495-COW FAIL exec returned"),
        Err(error) => println!("K495-COW FAIL exec {error:?}"),
    }
    raven_rt::sys::thread_exit(1)
}

fn run_spawn() -> Result<(), Errno> {
    let data_object = memobj_create(PAGE)?;
    let control_object = memobj_create(PAGE)?;
    let data = map_object(data_object, SPAWN_DATA, PAGE, true)?;
    let control = map_object(control_object, SPAWN_CONTROL, PAGE, true)?;
    fill(data, PAGE, FILL);
    zero_control(control);
    write64(control + C_DATA_HANDLE, u64::from(data_object.0));
    write64(control + C_CONTROL_HANDLE, u64::from(control_object.0));
    println!(
        "K495-COW spawn-parent pid={} addr=0x{data:x} seeded={:02x} store={PARENT_STORE:02x} first={:02x} last={:02x}",
        pid(),
        read8(data),
        read8(data),
        read8(data + PAGE - 1)
    );

    let child = proc_fork()?;
    thread_create(
        child,
        exec_child_entry as *const () as usize,
        stack_top(),
        control,
    )?;
    if !wait_value(control + C_READY, 1) {
        return Err(fail("spawn ready"));
    }
    write8(data, PARENT_STORE);
    fence();
    write64(control + C_STORE, 1);
    if !wait_value(control + C_DONE, 1) {
        return Err(fail("spawn done"));
    }
    let mut code = 0i32;
    proc_wait(child, &mut code)?;
    println!(
        "K495-COW spawn-parent status={code} after-store={:02x} last={:02x}",
        read8(data),
        read8(data + PAGE - 1)
    );
    if code != 0 || read8(data) != PARENT_STORE || read8(data + PAGE - 1) != FILL {
        return Err(fail("shared store"));
    }
    Ok(())
}

fn run_shared_handle(data_argument: &str, control_argument: &str) -> Result<(), Errno> {
    let data_object = data_argument
        .parse::<u32>()
        .map(Handle)
        .map_err(|_| Errno::EINVAL)?;
    let control_object = control_argument
        .parse::<u32>()
        .map(Handle)
        .map_err(|_| Errno::EINVAL)?;
    let data = map_object(data_object, SPAWN_DATA, PAGE, true)?;
    let control = map_object(control_object, SPAWN_CONTROL, PAGE, true)?;
    let first = read8(data);
    let last = read8(data + PAGE - 1);
    fence();
    write64(control + C_READY, 1);
    if !wait_value(control + C_STORE, 1) {
        return Err(fail("exec store"));
    }
    fence();
    let after = read8(data);
    println!(
        "K495-COW exec-child pid={} object={} addr=0x{data:x} first={first:02x} before-store={first:02x} after-store={after:02x} last={last:02x}",
        pid(),
        data_object.0
    );
    write64(control + C_DONE, 1);
    if first != FILL || after != PARENT_STORE || last != FILL {
        return Err(fail("exec store"));
    }
    Ok(())
}

fn run() -> Result<(), Errno> {
    run_private_first()?;
    run_private_child_first()?;
    run_shared_fork()?;
    run_remap()?;
    run_spawn()?;
    println!("K495-COW ok");
    Ok(())
}

fn main(arguments: raven_rt::Args) -> i32 {
    if arguments.get(1) == Some(MODE_SHARED) {
        let Some(data) = arguments.get(2) else {
            return 64;
        };
        let Some(control) = arguments.get(3) else {
            return 64;
        };
        return match run_shared_handle(data, control) {
            Ok(()) => 0,
            Err(error) => {
                println!("K495-COW memobj error {error:?}");
                1
            }
        };
    }
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("K495-COW memobj error {error:?}");
            1
        }
    }
}
