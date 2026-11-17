// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S69 process-death cleanup fixture.
//!
//! Controller: server startup, ten thousand process kills, one hundred resource-owner kills,
//! a dying-server wake test, and drain markers that print the kernel's S69 evidence.
//!
//! Roles (argv[1]): `server`, `owner`, `victim`, `client`, `drain`.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::AtomicU32;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, clock, map, prot};
use raven_ipc::protocol::{self, Attachment, Opcode, PACKET_SIZE, SchemePacket};
use raven_proc_death_cleanup::*;
use raven_rt::event::{EventQueue, EvqEvent, SIGNALED, event_create};
use raven_rt::println;
use raven_rt::sys;
use raven_scheme::Client;

use raven_rt::entry;

entry!(main);

const ROLE_SERVER: &str = "s69-server";
const ROLE_OWNER: &str = "s69-owner";
const ROLE_VICTIM: &str = "s69-victim";
const ROLE_CLIENT: &str = "s69-client";
const ROLE_DRAIN: &str = "s69-drain";

const KILL_CODE: i32 = 77;
const VICTIM_CYCLES: usize = 10_000;
const RESOURCE_CYCLES: usize = 100;
const DMA_LEN: usize = 1024 * 1024;
const SHARED_LEN: usize = 4 * 1024 * 1024;
const MMIO_PHYS: u64 = 0xF000_0000;

const MODE_NORMAL: u64 = 0;
const MODE_HOLD: u64 = 1;
const MODE_SYNC: u64 = 2;
const MODE_ATTACH: u64 = 3;
const MODE_STATE: u64 = 4;

const O_READ: u32 = 1;
const O_RDWR: u32 = 3;

#[repr(align(16))]
struct HelperStack(#[allow(dead_code)] [u8; 32 * 1024]);

static mut FUTEX_STACK: HelperStack = HelperStack([0; 32 * 1024]);
static mut PIPE_STACK: HelperStack = HelperStack([0; 32 * 1024]);
static mut SIGNAL_STACK: HelperStack = HelperStack([0; 32 * 1024]);
static mut SLEEP_STACK: HelperStack = HelperStack([0; 32 * 1024]);
#[cfg(target_arch = "x86_64")]
static mut IRQ_STACK: HelperStack = HelperStack([0; 32 * 1024]);
static mut IPC_STACK: HelperStack = HelperStack([0; 32 * 1024]);
static mut ATTACH_STACK: HelperStack = HelperStack([0; 32 * 1024]);

static FUTEX_WORD: AtomicU32 = AtomicU32::new(0);

fn fail(message: &str) -> i32 {
    println!("s69: failed {message}");
    1
}

fn helper_stack_top(stack: *mut HelperStack) -> usize {
    (stack as usize).saturating_add(32 * 1024)
}

fn raw_handle(raw: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(raw)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

fn memobj_create(len: usize) -> Result<Handle, Errno> {
    // SAFETY: MEMOBJ_CREATE takes a page-aligned length and zero flags.
    raw_handle(unsafe { sys::syscall2(Syscall::MEMOBJ_CREATE as usize, len, 0) })
}

fn timer_create() -> Result<Handle, Errno> {
    // SAFETY: TIMER_CREATE takes the monotonic clock id and zero flags.
    raw_handle(unsafe {
        sys::syscall2(Syscall::TIMER_CREATE as usize, clock::MONOTONIC as usize, 0)
    })
}

fn timer_set(timer: Handle, initial_ns: u64, interval_ns: u64) -> Result<(), Errno> {
    // SAFETY: TIMER_SET takes a validated timer handle and scalar nanosecond values.
    Errno::from_ret(unsafe {
        sys::syscall3(
            Syscall::TIMER_SET as usize,
            timer.0 as usize,
            initial_ns as usize,
            interval_ns as usize,
        )
    })
    .map(|_| ())
}

fn irq_create(root: Handle, kind: u32, number: u32) -> Result<Handle, Errno> {
    sys::irq_create(root, kind, number, None)
}

fn sigrecv() -> Result<usize, Errno> {
    let mut out = [0u8; 24];
    // SAFETY: PROC_SIGRECV writes up to 24 bytes to the live array; 0x4B is the documented raw
    // number for the non-blocking flag set to "block".
    Errno::from_ret(unsafe { sys::syscall2(0x4B, out.as_mut_ptr() as usize, 0) })
}

struct IrqSpec {
    kind: u32,
    number: u32,
    handle: Handle,
}

fn make_irq(root: Handle) -> Option<IrqSpec> {
    #[cfg(target_arch = "x86_64")]
    const NUMBERS: &[u32] = &[10, 11, 9, 5, 15, 14, 3, 4];
    #[cfg(target_arch = "aarch64")]
    const NUMBERS: &[u32] = &[32, 33, 34, 35, 36, 37, 38, 39];
    let kind = sys::IRQ_LEGACY;
    for number in NUMBERS.iter().copied() {
        if let Ok(handle) = irq_create(root, kind, number) {
            return Some(IrqSpec {
                kind,
                number,
                handle,
            });
        }
    }
    None
}

fn make_mmio(root: Handle) -> Option<Handle> {
    for phys in [MMIO_PHYS, 0xE000_0000, 0xFD00_0000] {
        if let Ok(handle) = sys::mmio_create(root, phys, PAGE, 0) {
            return Some(handle);
        }
    }
    None
}

fn transfer(child: Handle, source: Handle) -> Result<Handle, Errno> {
    sys::handle_transfer(child, source, Rights::READ)
}

fn main(args: raven_rt::Args) -> i32 {
    if let Some(role) = args.get(1) {
        return match role {
            ROLE_SERVER => server_main(),
            ROLE_OWNER => owner_main(),
            ROLE_VICTIM => victim_main(),
            ROLE_CLIENT => client_main(),
            ROLE_DRAIN => drain_main(),
            _ => 2,
        };
    }
    controller_main()
}

fn controller_main() -> i32 {
    let _ = raven_rt::proc::set_command(Handle::SELF_PROC, "s69-init");
    let Some(initfs) = map_initfs() else {
        return fail("initfs map");
    };
    let server = match spawn_child(&initfs, "/init", ROLE_SERVER) {
        Ok(server) => server,
        Err(_) => return fail("server spawn"),
    };
    let state = match open_retry("s69:state", O_READ) {
        Ok(handle) => handle,
        Err(_) => return fail("server register"),
    };
    let sync = match open_retry("s69:sync", O_READ) {
        Ok(handle) => handle,
        Err(_) => return fail("sync open"),
    };
    if drain_once(&initfs).is_err() {
        return fail("baseline drain");
    }
    println!("s69: baseline ok");

    let mut victim = 0usize;
    while victim < VICTIM_CYCLES {
        // The first hundred victims block in a futex, exercising the waiter sweep.  The rest are
        // created-but-never-started processes: ten thousand process-object teardown cycles
        // without depending on the scheduler's abandoned-context bookkeeping.
        let child = if victim < 100 {
            let child = match spawn_child(&initfs, "/init", ROLE_VICTIM) {
                Ok(child) => child,
                Err(_) => return fail("victim spawn"),
            };
            sys::yield_now();
            child
        } else {
            match proc_create(0) {
                Ok(child) => child,
                Err(_) => return fail("victim create"),
            }
        };
        match terminate_and_wait(child, KILL_CODE) {
            Ok(code) if code == KILL_CODE => {}
            _ => return fail("victim wait"),
        }
        let _ = sys::handle_close(child);
        victim = victim.saturating_add(1);
    }
    println!("s69: victims done={VICTIM_CYCLES}");

    let root = Handle::ROOT_DEVICE;
    let mut cycle = 0usize;
    while cycle < RESOURCE_CYCLES {
        if let Err(error) = resource_cycle(&initfs, root, cycle, state, sync) {
            println!("s69: cycle error {:?}", error);
            return fail("resource cycle");
        }
        if cycle.is_multiple_of(25) {
            println!("s69: cycle {cycle}");
        }
        cycle = cycle.saturating_add(1);
    }
    println!("s69: cycles done={RESOURCE_CYCLES}");

    // Dying-server wake: a live client has a blocked WRITE on the server.  Killing the server
    // must complete that request with EIO and leave the client running.
    let client = match spawn_child(&initfs, "/init", ROLE_CLIENT) {
        Ok(client) => client,
        Err(_) => return fail("client spawn"),
    };
    if wait_held(state, 1).is_err() {
        return fail("client hold");
    }
    if sys::proc_terminate(server, KILL_CODE).is_err() {
        return fail("server terminate");
    }
    match wait(server) {
        Ok(code) if code == KILL_CODE => {}
        _ => return fail("server wait"),
    }
    let _ = sys::handle_close(server);
    match wait(client) {
        Ok(0) => {}
        _ => return fail("client did not survive server death"),
    }
    let _ = sys::handle_close(client);
    let _ = sys::handle_close(state);
    let _ = sys::handle_close(sync);
    println!("s69: server death client ok");

    // The dead server's name must be immediately reusable.
    let server = match spawn_child(&initfs, "/init", ROLE_SERVER) {
        Ok(server) => server,
        Err(_) => return fail("server respawn"),
    };
    let state = match open_retry("s69:state", O_READ) {
        Ok(handle) => handle,
        Err(_) => return fail("server reregister"),
    };
    println!("s69: server reregistered");

    if drain_once(&initfs).is_err() {
        return fail("final drain");
    }
    println!("s69: cleanup ok");

    // Keep the respawned server alive through the final drain; the controller's EXIT tears the
    // system down through the normal test-exit path.
    let _ = state;
    let _ = server;
    0
}

fn drain_once(initfs: &raven_initfs::Initfs<'_>) -> Result<(), Errno> {
    let drain = spawn_child(initfs, "/init", ROLE_DRAIN)?;
    let code = wait(drain)?;
    let _ = sys::handle_close(drain);
    if code == 0 { Ok(()) } else { Err(Errno::EIO) }
}

fn wait_held(state: Handle, minimum: u32) -> Result<(), Errno> {
    let mut attempt = 0usize;
    while attempt < 500_000 {
        match held_count(state) {
            Ok(count) if count >= minimum => return Ok(()),
            Ok(_) | Err(Errno::EIO) | Err(Errno::EAGAIN) => {
                sys::yield_now();
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn resource_cycle(
    initfs: &raven_initfs::Initfs<'_>,
    root: Handle,
    cycle: usize,
    state: Handle,
    sync: Handle,
) -> Result<(), Errno> {
    let control = memobj_create(PAGE)?;
    sys::map_object_in(
        Handle::SELF_PROC,
        control,
        0,
        CTRL_ADDR,
        PAGE,
        prot::READ | prot::WRITE,
        map::FIXED | map::SHARED,
    )?;
    ctrl_write(CTRL_READY, 0);
    ctrl_write(CTRL_STATUS, 0);
    ctrl_write(CTRL_CYCLE, cycle as u64);
    for index in CTRL_IRQ..=CTRL_MEMOBJ {
        ctrl_write(index, 0);
    }

    let timer = timer_create()?;
    timer_set(timer, 10_000_000_000, 0)?;
    let dma = sys::dma_alloc(root, DMA_LEN, PAGE, 32, 0)?;
    let mmio = make_mmio(root);
    let irq = make_irq(root);
    let shared = memobj_create(SHARED_LEN)?;

    let child = spawn_stopped(initfs, ROLE_OWNER)?;
    let prepared = {
        let resources = OwnerResources {
            control,
            shared,
            timer,
            dma,
            mmio,
            irq: irq.as_ref(),
            cycle,
        };
        prepare_owner(&child, &resources)
    };
    if let Err(error) = prepared {
        let _ = sys::proc_terminate(child.process, KILL_CODE);
        let _ = wait(child.process);
        let _ = sys::handle_close(child.process);
        let _ = sys::unmap(Handle::SELF_PROC, CTRL_ADDR, PAGE);
        let _ = sys::handle_close(control);
        return Err(error);
    }
    sys::handle_close(control)?;
    sys::handle_close(shared)?;
    sys::handle_close(timer)?;
    sys::handle_close(dma)?;
    if let Some(mmio) = mmio {
        sys::handle_close(mmio)?;
    }
    if let Some(irq) = irq.as_ref() {
        sys::handle_close(irq.handle)?;
    }

    start_child(&child)?;
    let _ = raven_rt::proc::set_command(child.process, "s69-owner");
    if wait_owner_ready().is_err() {
        let _ = sys::proc_terminate(child.process, KILL_CODE);
        let _ = wait(child.process);
        let _ = sys::handle_close(child.process);
        let _ = sys::unmap(Handle::SELF_PROC, CTRL_ADDR, PAGE);
        return Err(Errno::ETIMEDOUT);
    }
    if wait_held(state, 1).is_err() {
        let _ = sys::proc_terminate(child.process, KILL_CODE);
        let _ = wait(child.process);
        let _ = sys::handle_close(child.process);
        let _ = sys::unmap(Handle::SELF_PROC, CTRL_ADDR, PAGE);
        return Err(Errno::ETIMEDOUT);
    }

    sys::proc_terminate(child.process, KILL_CODE)?;
    match wait(child.process) {
        Ok(code) if code == KILL_CODE => {}
        _ => return Err(Errno::EIO),
    }
    sys::handle_close(child.process)?;
    sys::unmap(Handle::SELF_PROC, CTRL_ADDR, PAGE)?;

    // Let the woken helpers leave their syscalls and release the IRQ/event/timer objects.
    let _ = sys::nanosleep(2_000_000);
    drain_held(sync)?;

    // The line/vector claim must be immediately reusable; a leaked `Irq` keeps the handle table
    // entry alive through the helper's syscall frame and this fails with EBUSY.
    if let Some(spec) = irq {
        let again = irq_create(root, spec.kind, spec.number)?;
        let _ = sys::handle_close(again);
    }
    Ok(())
}

struct OwnerResources<'a> {
    control: Handle,
    shared: Handle,
    timer: Handle,
    dma: Handle,
    mmio: Option<Handle>,
    irq: Option<&'a IrqSpec>,
    cycle: usize,
}

fn prepare_owner(child: &StoppedChild, resources: &OwnerResources<'_>) -> Result<(), Errno> {
    sys::map_object_in(
        child.process,
        resources.control,
        0,
        CTRL_ADDR,
        PAGE,
        prot::READ | prot::WRITE,
        map::FIXED | map::SHARED,
    )?;
    sys::map_object_in(
        child.process,
        resources.shared,
        0,
        SHARED_ADDR,
        SHARED_LEN,
        prot::READ | prot::WRITE,
        map::FIXED | map::SHARED,
    )?;

    let timer_handle = transfer(child.process, resources.timer)?;
    let dma_handle = transfer(child.process, resources.dma)?;
    let mmio_handle = match resources.mmio {
        Some(mmio) => transfer(child.process, mmio)?,
        None => Handle::NONE,
    };
    let irq_handle = match resources.irq {
        Some(irq) => transfer(child.process, irq.handle)?,
        None => Handle::NONE,
    };
    let memory_handle = transfer(child.process, resources.shared)?;

    ctrl_write(CTRL_TIMER, timer_handle.0 as u64);
    ctrl_write(CTRL_DMA, dma_handle.0 as u64);
    ctrl_write(CTRL_MMIO, mmio_handle.0 as u64);
    ctrl_write(CTRL_IRQ, irq_handle.0 as u64);
    ctrl_write(CTRL_MEMOBJ, memory_handle.0 as u64);
    ctrl_write(CTRL_CYCLE, resources.cycle as u64);
    ctrl_write(CTRL_READY, 1);
    Ok(())
}

fn drain_held(sync: Handle) -> Result<(), Errno> {
    let mut response = [0u8; 1];
    let _ = Client::new().call(sync, b"", &mut response)?;
    Ok(())
}

fn server_main() -> i32 {
    if sys::scheme_register("s69").is_err() {
        return fail("server register");
    }
    println!("s69: server ready");
    let mut held: Vec<(Handle, SchemePacket)> = Vec::new();
    let mut bytes = [0u8; PACKET_SIZE];
    loop {
        let request = match sys::recv(&mut bytes) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let mut reply = match protocol::decode(&bytes) {
            Ok(reply) => reply,
            Err(_) => continue,
        };
        reply.status = 0;
        let mode = reply.server_cookie;
        match reply.opcode {
            value if value == Opcode::Open.as_u16() => {
                reply.server_cookie = mode_for_path(reply.path_bytes());
                reply.buffer_len = 0;
            }
            value if value == Opcode::Write.as_u16() && mode == MODE_HOLD => {
                held.push((request, reply));
                continue;
            }
            value if value == Opcode::Call.as_u16() && mode == MODE_SYNC => {
                let pending = held.len();
                let released = release_held(&mut held);
                if pending != 0 {
                    println!("s69: server sync drained={pending} replied={released}");
                }
                reply.buffer_len = 0;
            }
            value if value == Opcode::Call.as_u16() && mode == MODE_STATE => {
                let count = u64::try_from(held.len()).unwrap_or(u64::MAX);
                // A `Call` reply without attachments uses the server-mapped response capture, not
                // packet inline data.  Write the count straight into the client's buffer.
                if reply.response_addr != 0 {
                    sys::copy_to_mapping(reply.response_addr as usize, &count.to_le_bytes());
                }
                reply.data_len = 8;
                reply.buffer_len = 8;
            }
            value if value == Opcode::Call.as_u16() && mode == MODE_ATTACH => {
                match memobj_create(4 * PAGE) {
                    Ok(object) => {
                        reply.attach_count = 1;
                        if let Some(slot) = reply.attach.first_mut() {
                            *slot = Attachment {
                                handle: object.0,
                                rights_mask: 0,
                            };
                        }
                        match send_reply(request, &reply) {
                            Ok(()) => {
                                let _ = sys::handle_close(object);
                                continue;
                            }
                            Err(error) => {
                                let _ = sys::handle_close(object);
                                println!("s69: server attach send errno={}", error.0);
                                continue;
                            }
                        }
                    }
                    Err(error) => {
                        reply.status = i32::from(error.0);
                        reply.buffer_len = 0;
                    }
                }
            }
            value if value == Opcode::Call.as_u16() => {
                reply.buffer_len = 0;
            }
            value if value == Opcode::Close.as_u16() || value == Opcode::Dup.as_u16() => {
                // One-way notification: there is no SEND to retire the request handle, so the
                // server closes it itself.  Ignoring it leaks one server-side handle (and its
                // lineage node) per file close until the server exits.
                let _ = sys::handle_close(request);
                continue;
            }
            value if value == Opcode::Fstat.as_u16() => {
                reply.data_len = 0;
            }
            _ => {
                reply.status = i32::from(Errno::EPROTO.0);
            }
        }
        if let Err(error) = send_reply(request, &reply) {
            println!("s69: server send errno={}", error.0);
        }
    }
}

fn mode_for_path(path: &[u8]) -> u64 {
    match path {
        b"hold" => MODE_HOLD,
        b"sync" => MODE_SYNC,
        b"attach" => MODE_ATTACH,
        b"state" => MODE_STATE,
        _ => MODE_NORMAL,
    }
}

fn send_reply(request: Handle, reply: &SchemePacket) -> Result<(), Errno> {
    let mut bytes = [0u8; PACKET_SIZE];
    protocol::encode(reply, &mut bytes).map_err(|_| Errno::EPROTO)?;
    sys::send(request, &bytes)
}

fn release_held(held: &mut Vec<(Handle, SchemePacket)>) -> usize {
    let mut count = 0usize;
    for (request, reply) in held.drain(..) {
        if send_reply(request, &reply).is_ok() {
            count = count.saturating_add(1);
        }
    }
    count
}

fn helpers_ready(status: u64, expected: u64) -> bool {
    let base = status & expected == expected;
    #[cfg(target_arch = "x86_64")]
    {
        base && (ctrl_read(CTRL_IRQ) == 0 || status & STATUS_IRQ != 0)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        base
    }
}

fn owner_main() -> i32 {
    let mut attempt = 0usize;
    while ctrl_read(CTRL_READY) == 0 {
        sys::yield_now();
        attempt = attempt.saturating_add(1);
        if attempt > 2_000_000 {
            return fail("owner control");
        }
    }
    let cycle = ctrl_read(CTRL_CYCLE);
    if cycle == 0 {
        println!("s69: owner ready");
    }

    // Touch the controller-provided shared mapping and keep the transferred handles in the
    // table.  The handle values themselves do not need to be used.
    // SAFETY: `prepare_owner` mapped a writable page at SHARED_ADDR before the thread started.
    unsafe {
        (SHARED_ADDR as *mut u64).write_volatile(0x5336_3900_0000_0000 ^ cycle);
    }
    let _dma = ctrl_read(CTRL_DMA);
    let _mmio = ctrl_read(CTRL_MMIO);
    let _timer = ctrl_read(CTRL_TIMER);
    let _memory = ctrl_read(CTRL_MEMOBJ);

    let queue = match EventQueue::create() {
        Ok(queue) => queue,
        Err(_) => return fail("owner evq"),
    };
    let event = match event_create() {
        Ok(event) => event,
        Err(_) => return fail("owner event"),
    };
    if queue.add(event, SIGNALED, 0x69).is_err() {
        return fail("owner evq add");
    }
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_EVENT);
    let own_timer = match timer_create() {
        Ok(timer) => timer,
        Err(_) => return fail("owner timer"),
    };
    if timer_set(own_timer, 10_000_000_000, 0).is_err() {
        return fail("owner timer set");
    }
    if queue.add(own_timer, SIGNALED, 0x6a).is_err() {
        return fail("owner timer add");
    }
    let own_memory = match memobj_create(2 * PAGE) {
        Ok(memory) => memory,
        Err(_) => return fail("owner memobj"),
    };
    let _ = own_memory;
    let own_map = match sys::map_object(own_memory, 0, 2 * PAGE, prot::READ | prot::WRITE, true) {
        Ok(address) => address,
        Err(_) => return fail("owner map"),
    };
    // SAFETY: the object was mapped read/write for two pages.
    unsafe {
        (own_map as *mut u64).write_volatile(0x69);
    }

    let (pipe_reader, _pipe_writer) = match raven_rt::pipe::pipe() {
        Ok(pipe) => pipe,
        Err(_) => return fail("owner pipe"),
    };

    if thread_create_helper(futex_helper, core::ptr::addr_of_mut!(FUTEX_STACK), 0).is_err() {
        return fail("owner futex thread");
    }
    if thread_create_helper(
        pipe_helper,
        core::ptr::addr_of_mut!(PIPE_STACK),
        pipe_reader.0 as usize,
    )
    .is_err()
    {
        return fail("owner pipe thread");
    }
    if thread_create_helper(signal_helper, core::ptr::addr_of_mut!(SIGNAL_STACK), 0).is_err() {
        return fail("owner signal thread");
    }
    if thread_create_helper(sleep_helper, core::ptr::addr_of_mut!(SLEEP_STACK), 0).is_err() {
        return fail("owner sleep thread");
    }
    if thread_create_helper(ipc_helper, core::ptr::addr_of_mut!(IPC_STACK), 0).is_err() {
        return fail("owner ipc thread");
    }
    if thread_create_helper(attach_helper, core::ptr::addr_of_mut!(ATTACH_STACK), 0).is_err() {
        return fail("owner attach thread");
    }
    #[cfg(target_arch = "x86_64")]
    {
        let irq = ctrl_read(CTRL_IRQ);
        if irq != 0
            && thread_create_helper(irq_helper, core::ptr::addr_of_mut!(IRQ_STACK), irq as usize)
                .is_err()
        {
            return fail("owner irq thread");
        }
    }

    let expected = STATUS_FUTEX
        | STATUS_PIPE
        | STATUS_SIGNAL
        | STATUS_SLEEP
        | STATUS_IPC
        | STATUS_EVENT
        | STATUS_ATTACH_DONE;
    let mut spins = 0usize;
    loop {
        let status = ctrl_read(CTRL_STATUS);
        if helpers_ready(status, expected) {
            break;
        }
        sys::yield_now();
        spins = spins.saturating_add(1);
        if spins.is_multiple_of(200_000) {
            println!("s69: owner status={:#x} expected={:#x}", status, expected);
        }
        if spins > 2_000_000 {
            println!(
                "s69: owner status final={:#x} expected={:#x}",
                ctrl_read(CTRL_STATUS),
                expected
            );
            return fail("owner helpers");
        }
    }
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_READY);
    ctrl_write(CTRL_READY, 2);

    let mut records = [EvqEvent::default(); 1];
    let _ = queue.wait(&mut records, 0);
    fail("owner event wait returned")
}

/// ATTACH_DONE status bit (not part of the tiny control vocabulary in lib.rs).
const STATUS_ATTACH_DONE: u64 = 1 << 8;

fn thread_create_helper(
    entry: extern "C" fn(usize) -> !,
    stack: *mut HelperStack,
    arg: usize,
) -> Result<(), Errno> {
    let thread = sys::thread_create(
        Handle::SELF_PROC,
        entry as usize,
        helper_stack_top(stack),
        arg,
    )?;
    // Keep the handle in the owner table: process teardown uses it to wake a helper that is in a
    // pure scheduler sleep.  Dropping it would make that helper undiscoverable.
    let _thread = thread;
    Ok(())
}

extern "C" fn futex_helper(_arg: usize) -> ! {
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_FUTEX);
    let _ = sys::futex_wait(&FUTEX_WORD, 0, None);
    sys::thread_exit(1)
}

extern "C" fn pipe_helper(arg: usize) -> ! {
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_PIPE);
    let mut byte = [0u8; 1];
    let _ = sys::read(Handle(arg as u32), &mut byte);
    sys::thread_exit(2)
}

extern "C" fn signal_helper(_arg: usize) -> ! {
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_SIGNAL);
    let _ = sigrecv();
    sys::thread_exit(3)
}

extern "C" fn sleep_helper(_arg: usize) -> ! {
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_SLEEP);
    let _ = sys::nanosleep(2_000_000_000);
    sys::thread_exit(4)
}

#[cfg(target_arch = "x86_64")]
extern "C" fn irq_helper(arg: usize) -> ! {
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_IRQ);
    let _ = sys::irq_wait(Handle(arg as u32), 0);
    sys::thread_exit(5)
}

extern "C" fn ipc_helper(_arg: usize) -> ! {
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_IPC);
    let file = match open_retry("s69:hold", O_RDWR) {
        Ok(file) => file,
        Err(_) => sys::thread_exit(6),
    };
    let payload = [0x69u8; 64];
    let _ = sys::write(file, &payload);
    sys::thread_exit(7)
}

extern "C" fn attach_helper(_arg: usize) -> ! {
    let file = match open_retry("s69:attach", O_RDWR) {
        Ok(file) => file,
        Err(_) => sys::thread_exit(8),
    };
    let mut response = [0u8; 16];
    if Client::new().call(file, b"", &mut response).is_err() {
        sys::thread_exit(9);
    }
    ctrl_write(CTRL_STATUS, ctrl_read(CTRL_STATUS) | STATUS_ATTACH_DONE);
    let _ = sys::nanosleep(2_000_000_000);
    sys::thread_exit(10)
}

fn victim_main() -> i32 {
    // The main thread has no Thread handle of its own; it blocks on a futex so the process-exit
    // futex sweep can wake it without relying on a handle-table entry.
    let _ = sys::futex_wait(&FUTEX_WORD, 0, None);
    0
}

fn client_main() -> i32 {
    let file = match open_retry("s69:hold", O_RDWR) {
        Ok(file) => file,
        Err(_) => return fail("client open"),
    };
    let payload = [0x27u8; 64];
    match sys::write(file, &payload) {
        Err(Errno::EIO) => {
            println!("s69: client eio ok");
            0
        }
        Ok(_) => fail("client write returned"),
        Err(_) => fail("client write"),
    }
}

fn drain_main() -> i32 {
    // Give the scheduler time to reap the threads the previous kills woke.
    let _ = sys::nanosleep(50_000_000);
    println!("s69: drain done");
    0
}
