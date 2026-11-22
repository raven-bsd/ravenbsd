// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K491 rlimit fixture: root and unprivileged `PROC_SETRLIMIT` rules, infinity, inheritance,
//! then the libc `NOFILE` consumer (EMFILE at the soft limit) through an exec'd C child.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_proc_rlimit::{RESOURCE_COUNT, RESOURCE_CPU, RESOURCE_NOFILE};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::proc::{setresgid, setresuid};

entry!(main);

const PROC_FORK: usize = 0x39;
const PROC_HANDLE_INFO: usize = 0x3E;
const PROC_WAIT_EX: usize = 0x48;
const PROC_GETRLIMIT: usize = 0x4E;
const PROC_SETRLIMIT: usize = 0x4F;
const P_PID: usize = 1;
const INF: u64 = u64::MAX;
const PAGE: usize = 4096;
const CONTROL: usize = 0x4000_0000;
const SLOT_FORK_INHERIT: usize = 0;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_rlimit_child_entry
    .type proc_rlimit_child_entry, @function
proc_rlimit_child_entry:
    call proc_rlimit_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size proc_rlimit_child_entry, . - proc_rlimit_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global proc_rlimit_child_entry
    .type proc_rlimit_child_entry, %function
proc_rlimit_child_entry:
    bl proc_rlimit_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size proc_rlimit_child_entry, . - proc_rlimit_child_entry
"#
);

unsafe extern "C" {
    fn proc_rlimit_child_entry();
}

#[cfg(target_arch = "x86_64")]
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
        core::arch::asm!(
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
        core::arch::asm!(
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

fn checked(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn getrlimit(resource: usize) -> Result<[u64; 2], Errno> {
    let mut values = [0u64; 2];
    // SAFETY: the kernel writes exactly two u64 words to the caller's array.
    checked(unsafe {
        syscall6(
            PROC_GETRLIMIT,
            resource,
            values.as_mut_ptr() as usize,
            0,
            0,
            0,
            0,
        )
    })?;
    Ok(values)
}

fn setrlimit(resource: usize, cur: u64, max: u64) -> Result<(), Errno> {
    // SAFETY: three scalar arguments; the kernel validates the resource and the pair.
    checked(unsafe {
        syscall6(
            PROC_SETRLIMIT,
            resource,
            cur as usize,
            max as usize,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn proc_fork(source: Handle) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(PROC_FORK, source.0 as usize, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn handle_info(handle: Handle) -> Result<usize, Errno> {
    checked(unsafe { syscall6(PROC_HANDLE_INFO, handle.0 as usize, 0, 0, 0, 0, 0) })
}

fn thread_create(process: Handle, entry: usize, stack_top: usize) -> Result<Handle, Errno> {
    checked(unsafe {
        syscall6(
            Syscall::THREAD_CREATE as usize,
            process.0 as usize,
            entry,
            stack_top,
            0,
            0,
            0,
        )
    })
    .map(|value| Handle(value as u32))
}

fn wait_ex(idtype: usize, id: i32, status: &mut i32) -> Result<usize, Errno> {
    checked(unsafe {
        syscall6(
            PROC_WAIT_EX,
            idtype,
            id as isize as usize,
            0,
            status as *mut i32 as usize,
            0,
            0,
        )
    })
}

fn close(handle: Handle) -> Result<(), Errno> {
    checked(unsafe {
        syscall6(
            Syscall::HANDLE_CLOSE as usize,
            handle.0 as usize,
            0,
            0,
            0,
            0,
            0,
        )
    })
    .map(|_| ())
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    checked(unsafe { syscall6(Syscall::MEMOBJ_CREATE as usize, len, 0, 0, 0, 0, 0) })
        .map(|value| Handle(value as u32))
}

fn map_raw(process: Handle, object: Handle, address: usize, len: usize) -> Result<usize, Errno> {
    checked(unsafe {
        syscall6(
            Syscall::MAP as usize,
            process.0 as usize,
            object.0 as usize,
            0,
            address,
            len,
            map::pack(prot::READ | prot::WRITE, map::FIXED | map::SHARED),
        )
    })
}

fn control_page() -> Option<usize> {
    let control = memobj_create(PAGE).ok()?;
    map_raw(Handle::SELF_PROC, control, CONTROL, PAGE).ok()?;
    Some(CONTROL)
}

fn read64(address: usize) -> u64 {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write64(address: usize, value: u64) {
    // SAFETY: the fixture owns the shared control mapping.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn sleep_ms() {
    let _ = unsafe { syscall6(Syscall::NANOSLEEP as usize, 1_000_000, 0, 0, 0, 0, 0) };
}

fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

/// Fork child: publish whether the CPU limits survived the fork.
#[unsafe(no_mangle)]
pub extern "C" fn proc_rlimit_child_body() -> i32 {
    let inherited = getrlimit(RESOURCE_CPU) == Ok([1, 1]);
    write64(CONTROL + SLOT_FORK_INHERIT * 8, u64::from(inherited));
    0
}

fn main(_args: raven_rt::Args) -> i32 {
    let Some(_control) = control_page() else {
        println!("proc-rlimit: fail control");
        return 1;
    };
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("proc-rlimit: fail {error:?}");
            1
        }
    }
}

fn run() -> Result<(), Errno> {
    let defaults = getrlimit(RESOURCE_CPU)? == [INF, INF]
        && getrlimit(RESOURCE_NOFILE)? == [256, 256]
        && getrlimit(RESOURCE_COUNT) == Err(Errno::EINVAL);
    let root_raise_cpu =
        setrlimit(RESOURCE_CPU, 1, 2).is_ok() && getrlimit(RESOURCE_CPU)? == [1, 2];
    let root_raise_nofile =
        setrlimit(RESOURCE_NOFILE, 300, 400).is_ok() && getrlimit(RESOURCE_NOFILE)? == [300, 400];
    let root_infinity =
        setrlimit(RESOURCE_NOFILE, INF, INF).is_ok() && getrlimit(RESOURCE_NOFILE)? == [INF, INF];
    let reset = setrlimit(RESOURCE_NOFILE, 256, 256).is_ok()
        && getrlimit(RESOURCE_NOFILE)? == [256, 256]
        && setrlimit(RESOURCE_CPU, 1, 2).is_ok();
    let invalid = setrlimit(99, 1, 1) == Err(Errno::EINVAL);

    if !(defaults && root_raise_cpu && root_raise_nofile && root_infinity && reset && invalid) {
        println!(
            "proc-rlimit: fail root defaults={} cpu={} nofile={} infinity={} reset={} invalid={}",
            u8::from(defaults),
            u8::from(root_raise_cpu),
            u8::from(root_raise_nofile),
            u8::from(root_infinity),
            u8::from(reset),
            u8::from(invalid)
        );
        return Err(Errno::EINVAL);
    }
    println!(
        "proc-rlimit: root defaults={} cpu={} nofile={} infinity={} reset={} invalid={}",
        u8::from(defaults),
        u8::from(root_raise_cpu),
        u8::from(root_raise_nofile),
        u8::from(root_infinity),
        u8::from(reset),
        u8::from(invalid)
    );

    setresgid(1000)?;
    setresuid(1000)?;

    let raise_cpu = setrlimit(RESOURCE_CPU, 1, 3) == Err(Errno::EPERM);
    let raise_nofile = setrlimit(RESOURCE_NOFILE, 10, 300) == Err(Errno::EPERM);
    let soft = setrlimit(RESOURCE_CPU, 2, 2).is_ok() && getrlimit(RESOURCE_CPU)? == [2, 2];
    let clamp = setrlimit(RESOURCE_CPU, 2, 1).is_ok() && getrlimit(RESOURCE_CPU)? == [1, 1];
    let invalid = setrlimit(RESOURCE_CPU, 5, 2) == Err(Errno::EINVAL);
    let infinity = setrlimit(RESOURCE_CPU, 1, INF) == Err(Errno::EPERM);
    if !(raise_cpu && raise_nofile && soft && clamp && invalid && infinity) {
        println!(
            "proc-rlimit: fail user raise-cpu={} raise-nofile={} soft={} clamp={} invalid={} infinity={}",
            u8::from(raise_cpu),
            u8::from(raise_nofile),
            u8::from(soft),
            u8::from(clamp),
            u8::from(invalid),
            u8::from(infinity)
        );
        return Err(Errno::EPERM);
    }
    println!(
        "proc-rlimit: user raise-cpu={} raise-nofile={} soft={} clamp={} invalid={} infinity={}",
        u8::from(raise_cpu),
        u8::from(raise_nofile),
        u8::from(soft),
        u8::from(clamp),
        u8::from(invalid),
        u8::from(infinity)
    );

    let child = proc_fork(Handle::SELF_PROC)?;
    let pid = i32::try_from(handle_info(child)?).map_err(|_| Errno::EINVAL)?;
    thread_create(
        child,
        proc_rlimit_child_entry as *const () as usize,
        stack_top(),
    )?;
    let mut slot = 0;
    for _ in 0..5000 {
        slot = read64(CONTROL + SLOT_FORK_INHERIT * 8);
        if slot != 0 {
            break;
        }
        sleep_ms();
    }
    let mut status = 0i32;
    wait_ex(P_PID, pid, &mut status)?;
    close(child)?;
    if slot != 1 {
        println!("proc-rlimit: fail fork inherit={slot}");
        return Err(Errno::EINVAL);
    }
    println!("proc-rlimit: fork inherit=1");

    let archive = raven_rt::map_initfs()?;
    let initfs = raven_initfs::Initfs::parse(archive).map_err(|_| Errno::ENOEXEC)?;
    let nofile = initfs.get("/bin/proc-rlimit-nofile").ok_or(Errno::ENOENT)?;
    println!("proc-rlimit: ok");
    // The libc fixture is larger than the convenience wrapper's 1 MiB self-limit; the kernel's
    // bound is 16 MiB, so issue the same request through the raw path.
    let path = b"/bin/proc-rlimit-nofile";
    let argv0 = b"/bin/proc-rlimit-nofile\0";
    let mut argv = [argv0.as_ptr() as u64];
    let request = raven_rt::exec::ExecRequest {
        image: nofile.data.as_ptr() as u64,
        image_len: nofile.data.len() as u64,
        path: path.as_ptr() as u64,
        path_len: path.len() as u64,
        argv: argv.as_mut_ptr() as u64,
        argc: 1,
        envp: 0,
        envc: 0,
        close_list: 0,
        close_count: 0,
        flags: 0,
    };
    raven_rt::exec::exec_raw(&request)?;
    Err(Errno::EINVAL)
}
