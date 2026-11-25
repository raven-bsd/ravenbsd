// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S164 C13 memory-object SHARED visibility fixture.
//!
//! The C half of the k495-cow fixture covers private fork COW and one anonymous SHARED mapping
//! inherited by fork. This helper covers the two rows that need a real memory-object handle:
//! two SHARED mappings of one object in one process, and an object handle inherited across
//! fork+exec whose child mapping must see the parent's store. It is exec'd by the C fixture at
//! the end of `all`, so the whole case still runs from one `/bin/libc-test` service.

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

const DATA_ADDR: usize = 0x5a00_0000;
const SECOND_ADDR: usize = 0x5a01_0000;
const CONTROL_ADDR: usize = 0x5a02_0000;

const OFF_READY: usize = 0;
const OFF_STORE: usize = 8;
const OFF_DONE: usize = 16;
const OFF_DATA_HANDLE: usize = 24;
const OFF_CONTROL_HANDLE: usize = 32;

const HELPER_PATH: &[u8] = b"/bin/k495-cow-memobj";
const MODE_SHARED: &str = "shared-handle";

/// Raw `proc_fork(src, flags) -> Handle` number (the ABI crate is reviewer-owned).
const PROC_FORK: usize = 0x39;
const SPIN_LIMIT: usize = 20_000_000;

fn checked(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn handle_from(raw: isize) -> Result<Handle, Errno> {
    checked(raw).and_then(|value| u32::try_from(value).map(Handle).map_err(|_| Errno::EBADF))
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes scalar arguments only; the kernel validates the length.
    handle_from(unsafe {
        raven_rt::sys::syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0)
    })
}

fn map_shared(object: Handle, address: usize, len: usize) -> Result<usize, Errno> {
    // SAFETY: MAP takes scalar arguments only; the kernel validates the handle, range and flags.
    checked(unsafe {
        raven_rt::sys::syscall6(
            Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            object.0 as usize,
            0,
            address,
            len,
            map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
        )
    })
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
            )
        };
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

fn run_remap() -> Result<(), Errno> {
    let object = memobj_create(PAGE)?;
    let first = map_shared(object, DATA_ADDR, PAGE)?;
    fill(first, PAGE, FILL);
    let second = map_shared(object, SECOND_ADDR, PAGE)?;
    let before = read8(first);
    write8(first, PARENT_STORE);
    let after = read8(second);
    println!(
        "K495-COW remap-parent pid={} first={before:02x} after-store={after:02x}",
        pid()
    );
    if before != FILL || after != PARENT_STORE || read8(second + PAGE - 1) != FILL {
        println!("K495-COW FAIL remap store");
        return Err(Errno::EINVAL);
    }
    raven_rt::sys::unmap(Handle::SELF_PROC, first, PAGE)?;
    raven_rt::sys::unmap(Handle::SELF_PROC, second, PAGE)?;
    Ok(())
}

extern "C" fn exec_child_entry(argument: usize) -> ! {
    let control = argument;
    let data = Handle(read64(control + OFF_DATA_HANDLE) as u32);
    let control_object = Handle(read64(control + OFF_CONTROL_HANDLE) as u32);
    let data_text = format!("{}", data.0);
    let control_text = format!("{}", control_object.0);
    let args: [&[u8]; 4] = [
        HELPER_PATH,
        MODE_SHARED.as_bytes(),
        data_text.as_bytes(),
        control_text.as_bytes(),
    ];
    match raven_rt::exec::execve(HELPER_PATH, &args, &[], &[]) {
        Ok(()) => println!("K495-COW FAIL exec returned"),
        Err(error) => println!("K495-COW FAIL exec {error:?}"),
    }
    raven_rt::sys::thread_exit(1)
}

fn run_spawn() -> Result<(), Errno> {
    let data_object = memobj_create(PAGE)?;
    let control_object = memobj_create(PAGE)?;
    let data = map_shared(data_object, DATA_ADDR, PAGE)?;
    let control = map_shared(control_object, CONTROL_ADDR, PAGE)?;
    fill(data, PAGE, FILL);
    write64(control + OFF_READY, 0);
    write64(control + OFF_STORE, 0);
    write64(control + OFF_DONE, 0);
    write64(control + OFF_DATA_HANDLE, u64::from(data_object.0));
    write64(control + OFF_CONTROL_HANDLE, u64::from(control_object.0));
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
    if !wait_value(control + OFF_READY, 1) {
        println!("K495-COW FAIL spawn ready");
        return Err(Errno::EINVAL);
    }
    write8(data, PARENT_STORE);
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    write64(control + OFF_STORE, 1);
    if !wait_value(control + OFF_DONE, 1) {
        println!("K495-COW FAIL spawn done");
        return Err(Errno::EINVAL);
    }
    let mut code = 0i32;
    proc_wait(child, &mut code)?;
    println!(
        "K495-COW spawn-parent status={code} after-store={:02x} last={:02x}",
        read8(data),
        read8(data + PAGE - 1)
    );
    if code != 0 || read8(data) != PARENT_STORE || read8(data + PAGE - 1) != FILL {
        println!("K495-COW FAIL shared store");
        return Err(Errno::EINVAL);
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
    let data = map_shared(data_object, DATA_ADDR, PAGE)?;
    let control = map_shared(control_object, CONTROL_ADDR, PAGE)?;
    let first = read8(data);
    let last = read8(data + PAGE - 1);
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    write64(control + OFF_READY, 1);
    if !wait_value(control + OFF_STORE, 1) {
        println!("K495-COW FAIL exec store");
        return Err(Errno::EINVAL);
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    let after = read8(data);
    println!(
        "K495-COW exec-child pid={} object={} addr=0x{data:x} first={first:02x} before-store={first:02x} after-store={after:02x} last={last:02x}",
        pid(),
        data_object.0
    );
    write64(control + OFF_DONE, 1);
    if first != FILL || after != PARENT_STORE || last != FILL {
        println!("K495-COW FAIL exec store");
        return Err(Errno::EINVAL);
    }
    Ok(())
}

fn run() -> Result<(), Errno> {
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
