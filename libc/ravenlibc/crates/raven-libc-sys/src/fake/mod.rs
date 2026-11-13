// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic in-memory model of the Raven kernel for host tests (`host-fake`).
//!
//! The fake implements exactly the syscalls of [`crate::sys`] that can be modelled without a
//! real MMU or scheduler. Every handler cites the kernel file and line it mirrors; where the
//! kernel is richer, the fake returns `ENOSYS` instead of guessing. `std` is used only here and
//! in tests, and the module is never part of a shipped artifact.

#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

pub mod clock;
pub mod event;
pub mod futex;
pub mod handle;
pub mod mem;
pub mod process;
pub mod scheme;
pub mod syscall;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, clock as clock_ids, map, namespace, prot};
use raven_abi::uname::Uname;

use host::sync::{Arc, Mutex, MutexGuard};
use host::vec::Vec;

use clock::Clock;
use event::{EventWorld, SIGNALED, Timer, UserEvent};
use handle::{HandleTable, Object};
use mem::Memory;
use process::{ChildProcess, ProcessState};
use scheme::{SchemeHandler, SchemeWorld};

/// Size of a fake page.
pub(crate) const PAGE: usize = raven_abi::PAGE_SIZE;

static KERNEL: Mutex<Option<Kernel>> = Mutex::new(None);

/// A fake process.
#[derive(Clone, Debug)]
pub struct Kernel {
    /// Current process handle table.
    pub handles: HandleTable,
    /// Child processes by index.
    pub children: Vec<ChildProcess>,
    /// Next lineage root id.
    pub next_root: u32,
    /// Current namespace prefixes.
    pub namespace: Vec<Vec<u8>>,
    /// Memory model.
    pub memory: Memory,
    /// Virtual clock.
    pub clock: Clock,
    /// Event model.
    pub events: EventWorld,
    /// Scheme world.
    pub schemes: SchemeWorld,
    /// Debug console output.
    pub debug_output: Vec<u8>,
    /// Exit code recorded by `EXIT`.
    pub exit_code: i32,
    /// Number of created thread handles.
    pub threads: usize,
    /// Deterministic `(syscall, result)` trace.
    pub trace: Vec<u64>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    /// A fresh fake kernel.
    pub fn new() -> Self {
        Self {
            handles: HandleTable::for_self(),
            children: Vec::new(),
            next_root: 1,
            namespace: Vec::new(),
            memory: Memory::new(),
            clock: Clock::new(),
            events: EventWorld::default(),
            schemes: SchemeWorld::default(),
            debug_output: Vec::new(),
            exit_code: 0,
            threads: 1,
            trace: Vec::new(),
        }
    }

    fn insert(&mut self, object: Object, rights: Rights) -> Result<Handle, Errno> {
        let root = self.next_root;
        self.next_root = self.next_root.wrapping_add(1);
        self.handles.insert(object, rights, root)
    }

    fn lineage_exists(&self, root: u32) -> bool {
        self.handles.has_root(root)
            || self
                .children
                .iter()
                .any(|child| child.handles.has_root(root))
    }

    fn close_handle(&mut self, handle: Handle) -> Result<(), Errno> {
        let (object, root) = self.handles.close(handle)?;
        if !self.lineage_exists(root) {
            // The fake never frees memory objects eagerly: mappings may still reference them.
            // Event and timer ids are tombstones so handles stay stable.
            let _ = object;
        }
        Ok(())
    }

    fn dup_handle(&mut self, handle: Handle, mask: u32) -> Result<Handle, Errno> {
        let entry = self.handles.get(handle, Rights::DUP)?.clone();
        let rights = entry.rights.intersect(Rights(mask));
        self.handles.insert(entry.object, rights, entry.root)
    }

    fn transfer_handle(
        &mut self,
        destination: Handle,
        source: Handle,
        mask: u32,
    ) -> Result<Handle, Errno> {
        if mask == 0 {
            return Err(Errno::EINVAL);
        }
        let entry = match self.handles.get(source, Rights::TRANSFER) {
            Ok(entry) => entry.clone(),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        };
        let destination_entry = match self.handles.get(destination, Rights::WRITE) {
            Ok(entry) => entry.clone(),
            Err(Errno::EACCES) => return Err(Errno::EPERM),
            Err(error) => return Err(error),
        };
        let rights = entry.rights.intersect(Rights(mask));
        match destination_entry.object {
            Object::SelfProcess => self.handles.insert(entry.object, rights, entry.root),
            Object::Process(index) => {
                let child = self.children.get_mut(index).ok_or(Errno::EBADF)?;
                child.handles.insert(entry.object, rights, entry.root)
            }
            _ => Err(Errno::EINVAL),
        }
    }

    fn revoke_handle(&mut self, handle: Handle, mask: u32) -> Result<(), Errno> {
        let entry = self.handles.entry(handle)?.clone();
        let remove = if mask == 0 { entry.rights.bits() } else { mask };
        self.handles.revoke(entry.root, remove);
        for child in &mut self.children {
            child.handles.revoke(entry.root, remove);
        }
        Ok(())
    }

    fn read_user(&self, pointer: usize, len: usize, max: usize) -> Result<Vec<u8>, Errno> {
        if len > max {
            return Err(Errno::EINVAL);
        }
        if len == 0 {
            return Ok(Vec::new());
        }
        if pointer == 0 {
            return Err(Errno::EFAULT);
        }
        // SAFETY: the syscall wrapper passed a live slice pointer; the fake kernel only reads
        // the bytes the caller declared.
        let source = unsafe { core::slice::from_raw_parts(pointer as *const u8, len) };
        Ok(source.to_vec())
    }

    fn write_user(&self, pointer: usize, bytes: &[u8]) -> Result<(), Errno> {
        if bytes.is_empty() {
            return Ok(());
        }
        if pointer == 0 {
            return Err(Errno::EFAULT);
        }
        // SAFETY: the syscall wrapper passed a live writable slice pointer.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer as *mut u8, bytes.len());
        }
        Ok(())
    }

    fn write_user_value<T: Copy>(&self, pointer: usize, value: &T) -> Result<(), Errno> {
        let size = core::mem::size_of::<T>();
        // SAFETY: `value` is a live stack value; the caller's pointer is validated by the
        // wrapper's contract and the fake copies exactly one `T`.
        let bytes = unsafe { core::slice::from_raw_parts((value as *const T).cast::<u8>(), size) };
        self.write_user(pointer, bytes)
    }

    fn scheme_rights(flags: u32) -> (bool, bool) {
        match flags & 3 {
            1 => (false, true),
            2 => (true, true),
            _ => (true, false),
        }
    }

    fn valid_scheme_name(name: &[u8]) -> bool {
        !name.is_empty()
            && name.len() <= 64
            && name.iter().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || *byte == b'_'
                    || *byte == b'-'
                    || *byte == b':'
            })
    }

    /// Dispatch one syscall against this kernel.
    pub fn call(&mut self, number: usize, args: [usize; 6]) -> Result<usize, Errno> {
        let call = Syscall::from_usize(number).ok_or(Errno::ENOSYS)?;
        match call {
            // kernel/src/syscall/basic.rs:20-27
            Syscall::EXIT => {
                self.exit_code = args[0] as i32;
                Ok(0)
            }
            Syscall::YIELD => Ok(0),
            Syscall::DEBUG_WRITE => {
                let bytes = self.read_user(args[0], args[1], 4096)?;
                self.debug_output.extend_from_slice(&bytes);
                Ok(bytes.len())
            }
            Syscall::CLOCK_GET => {
                let timespec = self.clock.get(args[0] as u32)?;
                self.write_user_value(args[1], &timespec)?;
                Ok(0)
            }
            Syscall::NANOSLEEP => {
                self.clock.sleep(args[0] as u64);
                Ok(0)
            }
            Syscall::UNAME => {
                let mut record = Uname::zeroed();
                Uname::set_field(&mut record.sysname, raven_abi::uname::SYSNAME);
                Uname::set_field(&mut record.nodename, "raven-fake");
                Uname::set_field(&mut record.release, raven_abi::uname::RELEASE);
                Uname::set_field(&mut record.version, "fake");
                Uname::set_field(&mut record.machine, "fake");
                self.write_user_value(args[0], &record)?;
                Ok(0)
            }
            Syscall::DEBUG_READ => {
                if args[2] == 0 {
                    // A forever-blocking read cannot be modelled; the kernel would block.
                    Err(Errno::ENOSYS)
                } else {
                    self.clock.sleep(args[2] as u64);
                    Ok(0)
                }
            }
            Syscall::THREAD_CREATE => {
                let id = self.threads;
                self.threads = self.threads.saturating_add(1);
                let handle = self.insert(Object::Thread(id), Rights::ALL)?;
                Ok(handle.0 as usize)
            }
            Syscall::THREAD_EXIT => Ok(0),
            Syscall::FUTEX => Err(Errno::ENOSYS),
            // kernel/src/syscall/process_ctl.rs:22-33
            Syscall::PROC_CREATE => {
                if args[0] != 0 {
                    return Err(Errno::EINVAL);
                }
                let index = self.children.len();
                self.children.push(ChildProcess::new());
                let handle = self.insert(Object::Process(index), Rights::ALL)?;
                Ok(handle.0 as usize)
            }
            // kernel/src/syscall/process_ctl.rs:106-155
            Syscall::PROC_WAIT => {
                let entry = self
                    .handles
                    .get(Handle(args[0] as u32), Rights::READ)?
                    .clone();
                let Object::Process(index) = entry.object else {
                    return Err(match entry.object {
                        Object::SelfProcess => Errno::EINVAL,
                        _ => Errno::EBADF,
                    });
                };
                let child = self.children.get_mut(index).ok_or(Errno::EBADF)?;
                match child.state {
                    ProcessState::Created => {
                        // The kernel reaps a created-but-never-started process immediately.
                        child.state = ProcessState::Exited;
                        child.exit_code = 0;
                    }
                    ProcessState::Running => return Err(Errno::ENOSYS),
                    ProcessState::Exited => {}
                }
                let code = child.exit_code;
                self.write_user_value(args[1], &code)?;
                Ok(0)
            }
            // kernel/src/syscall/process_ctl.rs:39-58
            Syscall::PROC_TERMINATE => {
                let entry = match self.handles.get(Handle(args[0] as u32), Rights::SIGNAL) {
                    Ok(entry) => entry.clone(),
                    Err(Errno::EACCES) => return Err(Errno::EPERM),
                    Err(error) => return Err(error),
                };
                let Object::Process(index) = entry.object else {
                    return Err(match entry.object {
                        Object::SelfProcess => Errno::ENOSYS,
                        _ => Errno::EBADF,
                    });
                };
                let child = self.children.get_mut(index).ok_or(Errno::EBADF)?;
                child.terminate(args[1] as i32)?;
                Ok(0)
            }
            // kernel/src/syscall/process_ctl.rs:65-103
            Syscall::PROC_NAMESPACE => {
                let operation = u32::try_from(args[1]).map_err(|_| Errno::EINVAL)?;
                if args[0] as u32 != Handle::SELF_PROC.0 {
                    self.handles.get(Handle(args[0] as u32), Rights::ADMIN)?;
                    return Err(Errno::ENOSYS);
                }
                match operation {
                    namespace::ADD | namespace::REMOVE => {
                        let prefix = self.read_user(args[2], args[3], 64)?;
                        if !Self::valid_scheme_name(&prefix) {
                            return Err(Errno::EINVAL);
                        }
                        if operation == namespace::ADD {
                            if !self.namespace.iter().any(|entry| entry == &prefix) {
                                self.namespace.push(prefix);
                            }
                        } else {
                            self.namespace.retain(|entry| entry != &prefix);
                        }
                        Ok(0)
                    }
                    namespace::CLEAR if args[3] == 0 => {
                        self.namespace.clear();
                        Ok(0)
                    }
                    _ => Err(Errno::EINVAL),
                }
            }
            // kernel/src/syscall/memory_ctl.rs:117-215
            Syscall::MAP => self.sys_map(args),
            Syscall::UNMAP => self.sys_unmap(args),
            Syscall::PROTECT => self.sys_protect(args),
            // kernel/src/syscall/memory_ctl.rs:38-75
            Syscall::MEMOBJ_CREATE => {
                let id = self.memory.memobj_create(args[0], args[1])?;
                let rights = Rights::READ
                    .union(Rights::WRITE)
                    .union(Rights::MAP)
                    .union(Rights::DUP)
                    .union(Rights::TRANSFER);
                let handle = self.insert(Object::Memory(id), rights)?;
                Ok(handle.0 as usize)
            }
            // kernel/src/syscall/memory_ctl.rs:79-108
            Syscall::PAGER_SUPPLY => {
                let entry = self
                    .handles
                    .get(Handle(args[0] as u32), Rights::WRITE)?
                    .clone();
                let Object::Memory(id) = entry.object else {
                    return Err(Errno::EBADF);
                };
                let data = self.read_user(args[2], args[3], PAGE)?;
                self.memory.pager_supply(id, args[1], &data)?;
                Ok(0)
            }
            Syscall::HANDLE_CLOSE => {
                self.close_handle(Handle(args[0] as u32))?;
                Ok(0)
            }
            Syscall::HANDLE_DUP => {
                let handle = self.dup_handle(Handle(args[0] as u32), args[1] as u32)?;
                Ok(handle.0 as usize)
            }
            Syscall::HANDLE_TRANSFER => {
                let handle = self.transfer_handle(
                    Handle(args[0] as u32),
                    Handle(args[1] as u32),
                    args[2] as u32,
                )?;
                Ok(handle.0 as usize)
            }
            Syscall::HANDLE_REVOKE => {
                self.revoke_handle(Handle(args[0] as u32), args[1] as u32)?;
                Ok(0)
            }
            // kernel/src/syscall/event.rs:47-289
            Syscall::EVQ_CREATE => {
                if args[0] != 0 {
                    return Err(Errno::EINVAL);
                }
                let id = self.events.queues.len();
                self.events.queues.push(event::EventQueue::default());
                let rights = Rights::READ.union(Rights::WRITE).union(Rights::DUP);
                let handle = self.insert(Object::EventQueue(id), rights)?;
                Ok(handle.0 as usize)
            }
            Syscall::EVQ_CTL => self.sys_evq_ctl(args),
            Syscall::EVQ_WAIT => self.sys_evq_wait(args),
            Syscall::TIMER_CREATE => {
                if args[0] as u32 != clock_ids::MONOTONIC || args[1] != 0 {
                    return Err(Errno::EINVAL);
                }
                let id = self.events.timers.len();
                self.events.timers.push(Timer::new(args[0] as u32));
                let rights = Rights::READ
                    .union(Rights::WRITE)
                    .union(Rights::DUP)
                    .union(Rights::TRANSFER);
                let handle = self.insert(Object::Timer(id), rights)?;
                Ok(handle.0 as usize)
            }
            Syscall::EVENT_CREATE => {
                if args[0] != 0 {
                    return Err(Errno::EINVAL);
                }
                let id = self.events.events.len();
                self.events.events.push(UserEvent::default());
                let rights = Rights::READ
                    .union(Rights::SIGNAL)
                    .union(Rights::DUP)
                    .union(Rights::TRANSFER);
                let handle = self.insert(Object::Event(id), rights)?;
                Ok(handle.0 as usize)
            }
            Syscall::EVENT_SIGNAL => {
                let entry = self
                    .handles
                    .get(Handle(args[0] as u32), Rights::SIGNAL)?
                    .clone();
                let Object::Event(id) = entry.object else {
                    return Err(Errno::EBADF);
                };
                self.events.event_mut(id)?.signal(args[1] as u64);
                Ok(0)
            }
            Syscall::TIMER_SET => {
                let entry = self
                    .handles
                    .get(Handle(args[0] as u32), Rights::WRITE)?
                    .clone();
                let Object::Timer(id) = entry.object else {
                    return Err(Errno::EBADF);
                };
                let now = self.clock.monotonic_ns;
                self.events
                    .timer_mut(id)?
                    .set(now, args[1] as u64, args[2] as u64);
                Ok(0)
            }
            // kernel/src/syscall/ipc.rs:40-121
            Syscall::SCHEME_REGISTER => {
                self.handles.get(Handle::SELF_PROC, Rights::ADMIN)?;
                let name = self.read_user(args[0], args[1], 64)?;
                if !Self::valid_scheme_name(&name) {
                    return Err(Errno::EINVAL);
                }
                // The real call registers the calling process as the server. The fake stores
                // the name; tests install a handler through `fake::register_scheme`.
                let rights = Rights::READ
                    .union(Rights::WRITE)
                    .union(Rights::DUP)
                    .union(Rights::TRANSFER);
                let handle = self.insert(Object::SchemeServer { name }, rights)?;
                Ok(handle.0 as usize)
            }
            Syscall::OPEN => self.sys_open(args),
            Syscall::READ => self.sys_read(args),
            Syscall::WRITE => self.sys_write(args),
            Syscall::CLOSE => {
                self.handles.get(Handle(args[0] as u32), Rights::NONE)?;
                self.close_handle(Handle(args[0] as u32))?;
                Ok(0)
            }
            Syscall::DUP => {
                let handle = self.dup_handle(Handle(args[0] as u32), args[1] as u32)?;
                Ok(handle.0 as usize)
            }
            Syscall::FSTAT => self.sys_fstat(args),
            Syscall::CALL => self.sys_call(args),
            Syscall::RECV | Syscall::SEND => Err(Errno::ENOSYS),
            Syscall::MMIO_CREATE
            | Syscall::BOOT_BLOB
            | Syscall::DMA_ALLOC
            | Syscall::DMA_INFO
            | Syscall::IRQ_CREATE
            | Syscall::IRQ_WAIT
            | Syscall::IRQ_ACK => Err(Errno::ENOSYS),
        }
    }

    fn sys_map(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        if args[0] as u32 != Handle::SELF_PROC.0 {
            return Err(Errno::ENOSYS);
        }
        if args[1] as u32 == Handle::INITFS.0 {
            // The fake has no initfs image.
            return Err(Errno::ENOSYS);
        }
        let flags = args[5];
        let allowed = prot::MASK | ((map::FIXED | map::SHARED) << map::SHIFT);
        if flags & !allowed != 0 {
            return Err(Errno::EINVAL);
        }
        let protection = (flags & prot::MASK) as u8;
        let memobj_id = if args[1] as u32 == Handle::NONE.0 {
            if flags & (map::SHARED << map::SHIFT) != 0 || args[2] != 0 {
                return Err(Errno::EINVAL);
            }
            None
        } else {
            let entry = self
                .handles
                .get(Handle(args[1] as u32), Rights::MAP)?
                .clone();
            let Object::Memory(id) = entry.object else {
                return Err(Errno::EBADF);
            };
            if flags & (map::SHARED << map::SHIFT) != 0 && protection & (prot::WRITE as u8) != 0 {
                self.handles.get(Handle(args[1] as u32), Rights::WRITE)?;
            }
            if protection & (prot::EXEC as u8) != 0 {
                self.handles.get(Handle(args[1] as u32), Rights::EXEC)?;
            }
            Some(id)
        };
        let map_flags = flags >> map::SHIFT;
        self.memory
            .map(memobj_id, args[2], args[3], args[4], protection, map_flags)
    }

    fn sys_unmap(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        self.handles.get(Handle(args[0] as u32), Rights::MAP)?;
        self.memory.unmap(args[1], args[2])?;
        Ok(0)
    }

    fn sys_protect(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        self.handles.get(Handle(args[0] as u32), Rights::MAP)?;
        let protection = u8::try_from(args[3]).map_err(|_| Errno::EINVAL)?;
        self.memory.protect(args[1], args[2], protection)?;
        Ok(0)
    }

    fn sys_evq_ctl(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        self.handles.get(Handle(args[0] as u32), Rights::READ)?;
        let operation = u32::try_from(args[1]).map_err(|_| Errno::EINVAL)?;
        let source = Handle(args[2] as u32);
        let events = u32::try_from(args[3]).map_err(|_| Errno::EINVAL)?;
        if events & !event::MASK != 0 {
            return Err(Errno::EINVAL);
        }
        let Object::EventQueue(id) = self.handles.entry(Handle(args[0] as u32))?.object.clone()
        else {
            return Err(Errno::EBADF);
        };
        match operation {
            event::ADD => {
                let object = self.handles.entry(source)?.object.clone();
                match object {
                    Object::Event(_)
                    | Object::Timer(_)
                    | Object::Process(_)
                    | Object::SchemeFile { .. } => {}
                    _ => return Err(Errno::EINVAL),
                }
                let queue = self.events.queue_mut(id)?;
                if queue.registrations.iter().any(|reg| reg.handle == source) {
                    return Err(Errno::EEXIST);
                }
                queue.registrations.push(event::Registration {
                    handle: source,
                    events,
                    cookie: args[4] as u64,
                });
                Ok(0)
            }
            event::MOD => {
                let queue = self.events.queue_mut(id)?;
                let registration = queue
                    .registrations
                    .iter_mut()
                    .find(|reg| reg.handle == source)
                    .ok_or(Errno::ENOENT)?;
                registration.events = events;
                registration.cookie = args[4] as u64;
                Ok(0)
            }
            event::DEL => {
                let queue = self.events.queue_mut(id)?;
                let before = queue.registrations.len();
                queue.registrations.retain(|reg| reg.handle != source);
                if queue.registrations.len() == before {
                    return Err(Errno::ENOENT);
                }
                Ok(0)
            }
            _ => Err(Errno::EINVAL),
        }
    }

    fn sys_evq_wait(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        self.handles.get(Handle(args[0] as u32), Rights::READ)?;
        let Object::EventQueue(id) = self.handles.entry(Handle(args[0] as u32))?.object.clone()
        else {
            return Err(Errno::EBADF);
        };
        let max = args[2];
        if max == 0 {
            return Err(Errno::EINVAL);
        }
        let mut records: Vec<(u64, u32)> = Vec::new();
        let registrations = self.events.queue(id)?.registrations.clone();
        for registration in registrations {
            if records.len() >= max {
                break;
            }
            let Some(wanted) = self
                .handles
                .entry(registration.handle)
                .ok()
                .map(|entry| entry.object.clone())
            else {
                continue;
            };
            let ready = self.consume_source(&wanted);
            let delivered = ready & registration.events;
            if delivered & (event::READABLE | event::WRITABLE | event::HUP | SIGNALED) != 0 {
                records.push((registration.cookie, delivered));
            }
        }
        if records.is_empty() {
            if args[3] == 0 {
                // The kernel blocks forever; the single-threaded fake cannot.
                return Err(Errno::ENOSYS);
            }
            self.clock.sleep(args[3] as u64);
            return Ok(0);
        }
        let mut output: Vec<crate::sys::EvqEvent> = Vec::new();
        for (cookie, events) in records {
            output.push(crate::sys::EvqEvent {
                cookie,
                events,
                pad: 0,
            });
        }
        if args[1] == 0 {
            return Err(Errno::EFAULT);
        }
        // SAFETY: the wrapper passed a valid output slice of at least `max` records.
        unsafe {
            core::ptr::copy_nonoverlapping(
                output.as_ptr(),
                args[1] as *mut crate::sys::EvqEvent,
                output.len(),
            );
        }
        Ok(output.len())
    }

    fn consume_source(&mut self, object: &Object) -> u32 {
        match object {
            Object::Event(id) => {
                if let Ok(event) = self.events.event_mut(*id) {
                    if event.consume() { SIGNALED } else { 0 }
                } else {
                    0
                }
            }
            Object::Timer(id) => {
                let now = self.clock.monotonic_ns;
                if let Ok(timer) = self.events.timer_mut(*id) {
                    if timer.ready(now) {
                        timer.consume(now);
                        SIGNALED
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
            Object::Process(index) => match self.children.get(*index) {
                Some(child) if child.state == ProcessState::Exited => SIGNALED,
                _ => 0,
            },
            // PG-25: the kernel reports only HUP for a scheme file, and only when the server
            // died. The fake has no server death.
            Object::SchemeFile { .. } => 0,
            _ => 0,
        }
    }

    fn sys_open(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        let path = self.read_user(args[0], args[1], 4096)?;
        let (name, suffix) = SchemeWorld::split(&path)?;
        let handler = self.schemes.lookup(name).ok_or(Errno::ENOENT)?;
        let flags = u32::try_from(args[2]).map_err(|_| Errno::EINVAL)?;
        let file = handler.open(suffix, flags)?;
        let (read, write) = Self::scheme_rights(flags);
        let rights = Rights(
            if read { Rights::READ.bits() } else { 0 }
                | if write { Rights::WRITE.bits() } else { 0 }
                | Rights::DUP.bits()
                | Rights::TRANSFER.bits(),
        );
        let handle = self.insert(
            Object::SchemeFile {
                scheme: name.to_vec(),
                file,
                read,
                write,
            },
            rights,
        )?;
        Ok(handle.0 as usize)
    }

    fn sys_read(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        let entry = self
            .handles
            .get(Handle(args[0] as u32), Rights::READ)?
            .clone();
        let Object::SchemeFile { scheme, file, .. } = entry.object else {
            return Err(Errno::EBADF);
        };
        let handler = self.schemes.lookup(&scheme).ok_or(Errno::EBADF)?;
        let mut buffer = host::vec![0u8; args[2]];
        let count = handler.read(file, &mut buffer)?;
        let bytes = buffer.get(..count).ok_or(Errno::EIO)?;
        self.write_user(args[1], bytes)?;
        Ok(count)
    }

    fn sys_write(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        let entry = self
            .handles
            .get(Handle(args[0] as u32), Rights::WRITE)?
            .clone();
        let Object::SchemeFile { scheme, file, .. } = entry.object else {
            return Err(Errno::EBADF);
        };
        let handler = self.schemes.lookup(&scheme).ok_or(Errno::EBADF)?;
        let data = self.read_user(args[1], args[2], 1 << 20)?;
        handler.write(file, &data)
    }

    fn sys_fstat(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        // PG-23 is not landed: the kernel still requires READ for FSTAT
        // (kernel/src/syscall/ipc.rs:99-104).
        let entry = self
            .handles
            .get(Handle(args[0] as u32), Rights::READ)?
            .clone();
        let Object::SchemeFile { scheme, file, .. } = entry.object else {
            return Err(Errno::EBADF);
        };
        let handler = self.schemes.lookup(&scheme).ok_or(Errno::EBADF)?;
        let record = handler.fstat(file)?;
        self.write_user(args[1], &record)?;
        Ok(0)
    }

    fn sys_call(&mut self, args: [usize; 6]) -> Result<usize, Errno> {
        let entry = self
            .handles
            .get(Handle(args[0] as u32), Rights::READ)?
            .clone();
        let Object::SchemeFile { scheme, file, .. } = entry.object else {
            return Err(Errno::EBADF);
        };
        let handler = self.schemes.lookup(&scheme).ok_or(Errno::EBADF)?;
        let request = self.read_user(args[1], args[2], 1 << 20)?;
        let mut response = host::vec![0u8; args[4]];
        let count = handler.call(file, &request, &mut response)?;
        let bytes = response.get(..count).ok_or(Errno::EIO)?;
        self.write_user(args[3], bytes)?;
        Ok(count)
    }
}

#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Serialise tests that share the global fake kernel.
#[cfg(test)]
pub(crate) fn test_lock() -> MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner())
}

/// Reset the fake kernel; tests start from a known state.
pub fn reset() {
    let mut guard = lock();
    *guard = Some(Kernel::new());
}

fn lock() -> MutexGuard<'static, Option<Kernel>> {
    KERNEL.lock().unwrap_or_else(|error| error.into_inner())
}

fn with_kernel<R>(f: impl FnOnce(&mut Kernel) -> R) -> R {
    let mut guard = lock();
    let kernel = guard.get_or_insert_with(Kernel::new);
    f(kernel)
}

/// Dispatch one raw syscall into the fake kernel. Called by [`crate::arch`] when `host-fake` is
/// enabled.
///
/// # Safety
///
/// The pointer arguments must satisfy the same contract as the real stub: buffers must be live
/// for the duration of the call and have the length the wrapper passes.
pub unsafe fn dispatch(number: usize, args: [usize; 6]) -> isize {
    if number == Syscall::FUTEX as usize {
        let result = match args[1] as u32 {
            raven_abi::types::futex::WAIT => {
                futex::wait(args[0], args[2] as u32, args[3] as u64).map(|_| 0usize)
            }
            raven_abi::types::futex::WAKE => Ok(futex::wake(args[0], args[2] as u32)),
            _ => Err(Errno::EINVAL),
        };
        return result.map_or_else(|error| error.to_ret(), |value| value as isize);
    }
    with_kernel(|kernel| {
        let result = kernel.call(number, args);
        let encoded = match result {
            Ok(value) => value as isize,
            Err(error) => error.to_ret(),
        };
        kernel
            .trace
            .push(((number as u64) << 32) | (encoded as u32 as u64));
        encoded
    })
}

/// Number of mapped pages in bytes.
pub fn mapped_bytes() -> usize {
    with_kernel(|kernel| kernel.memory.mapped_pages() * PAGE)
}

/// Copy bytes out of fake memory.
pub fn peek(addr: usize, len: usize) -> Option<Vec<u8>> {
    with_kernel(|kernel| kernel.memory.peek(addr, len))
}

/// Write bytes into fake memory.
pub fn poke(addr: usize, data: &[u8]) -> bool {
    with_kernel(|kernel| kernel.memory.poke(addr, data))
}

/// Protection bits of a fake page.
pub fn protection(addr: usize) -> Option<u8> {
    with_kernel(|kernel| kernel.memory.protection_at(addr))
}

/// Advance the virtual clock.
pub fn advance_ns(ns: u64) {
    with_kernel(|kernel| kernel.clock.advance(ns));
}

/// Current monotonic nanoseconds.
pub fn now_ns() -> u64 {
    with_kernel(|kernel| kernel.clock.monotonic_ns)
}

/// FNV-1a hash of the deterministic `(syscall, result)` trace.
pub fn trace_hash() -> u64 {
    with_kernel(|kernel| {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for entry in &kernel.trace {
            hash ^= *entry;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    })
}

/// Register a scheme server.
pub fn register_scheme(name: &[u8], handler: Arc<dyn SchemeHandler>) {
    with_kernel(|kernel| kernel.schemes.register(name, handler));
}

/// Character-device output written by `DEBUG_WRITE`.
pub fn debug_output() -> Vec<u8> {
    with_kernel(|kernel| kernel.debug_output.clone())
}

/// Deterministic `FUTEX` waiters on `addr`.
pub fn futex_waiters(addr: usize) -> usize {
    futex::waiters(addr)
}

/// Enable real blocking futex waits.
pub fn set_futex_blocking(enabled: bool) {
    futex::set_blocking(enabled);
}

/// The current fake thread pointer.
pub fn thread_pointer() -> *mut u8 {
    crate::tp::thread_pointer()
}

/// Reset the deterministic trace.
pub fn reset_trace() {
    with_kernel(|kernel| kernel.trace.clear());
}

const _: () = assert!(PAGE == 4096);

#[cfg(test)]
#[path = "fake_ut.rs"]
mod tests;
