// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Event-queue, timer and user-event system calls.

use alloc::sync::Arc;
use core::mem::size_of;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights, clock};
use raven_usercopy::UserSlice;

use crate::event::{self, EventQueue, Timer, UserEvent};
use crate::process::{self, ProcessState, handle::Object};
use crate::syscall::SyscallFrame;
use crate::time;
use crate::usercopy::copy_to_user;

const ADD: u32 = 1;
const MOD: u32 = 2;
const DEL: u32 = 3;
const MAX_TIMERS: u32 = 1024;

/// User-visible event record. Its layout is fixed by the ABI syscall documentation.
#[repr(C)]
struct EvqEvent {
    cookie: u64,
    events: u32,
    pad: u32,
}

/// Dispatch an S26 syscall.
pub fn dispatch(call: Syscall, frame: &SyscallFrame) -> Result<usize, Errno> {
    match call {
        Syscall::EVQ_CREATE => create_queue(frame),
        Syscall::EVQ_CTL => ctl(frame),
        Syscall::EVQ_WAIT => wait(frame),
        Syscall::TIMER_CREATE => create_timer(frame),
        Syscall::EVENT_CREATE => create_event(frame),
        Syscall::EVENT_SIGNAL => signal(frame),
        Syscall::TIMER_SET => timer_set(frame),
        _ => Err(Errno::ENOSYS),
    }
}

fn create_queue(frame: &SyscallFrame) -> Result<usize, Errno> {
    if frame.rdi != 0 {
        return Err(Errno::EINVAL);
    }
    let process = process::current().ok_or(Errno::ESRCH)?;
    let rights = Rights::READ.union(Rights::WRITE).union(Rights::DUP);
    let queue = Arc::new(EventQueue::new(process.id));
    event::register_queue(&queue)?;
    Ok(process
        .handles
        .lock()
        .insert(Object::EventQueue(queue), rights)?
        .0 as usize)
}

fn create_timer(frame: &SyscallFrame) -> Result<usize, Errno> {
    if frame.rdi != u64::from(clock::MONOTONIC) || frame.rsi != 0 {
        return Err(Errno::EINVAL);
    }
    let process = process::current().ok_or(Errno::ESRCH)?;
    let rights = Rights::READ
        .union(Rights::WRITE)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    let _prior = process
        .timer_count
        .fetch_update(
            core::sync::atomic::Ordering::AcqRel,
            core::sync::atomic::Ordering::Acquire,
            |count| (count < MAX_TIMERS).then_some(count.saturating_add(1)),
        )
        .map_err(|_| Errno::ENOSPC)?;
    let timer = Arc::new(Timer::new(Arc::clone(&process.timer_count)));
    let inserted = process.handles.lock().insert(Object::Timer(timer), rights);
    Ok(inserted?.0 as usize)
}

fn create_event(frame: &SyscallFrame) -> Result<usize, Errno> {
    if frame.rdi != 0 {
        return Err(Errno::EINVAL);
    }
    let process = process::current().ok_or(Errno::ESRCH)?;
    let rights = Rights::READ
        .union(Rights::SIGNAL)
        .union(Rights::DUP)
        .union(Rights::TRANSFER);
    Ok(process
        .handles
        .lock()
        .insert(Object::UserEvent(Arc::new(UserEvent::new())), rights)?
        .0 as usize)
}

fn ctl(frame: &SyscallFrame) -> Result<usize, Errno> {
    let queue_handle = checked_handle(frame.rdi)?;
    let source = checked_handle(frame.rdx)?;
    let events = u32::try_from(frame.r10).map_err(|_| Errno::EINVAL)?;
    let process = process::current().ok_or(Errno::ESRCH)?;
    let queue = queue_from_handle(&process, queue_handle, Rights::WRITE)?;
    let mut state = queue.state.lock();
    let result = match u32::try_from(frame.rsi).map_err(|_| Errno::EINVAL)? {
        ADD => {
            source_from_handle(&process, source)?;
            state.add(source.0, events, frame.r8)
        }
        MOD => {
            source_from_handle(&process, source)?;
            state.modify(source.0, events, frame.r8)
        }
        DEL if events == 0 && frame.r8 == 0 => state.delete(source.0),
        _ => return Err(Errno::EINVAL),
    };
    result.map_err(|error| match error {
        crate::event::evq_core::Error::Full => Errno::ENOSPC,
        crate::event::evq_core::Error::Exists => Errno::EEXIST,
        crate::event::evq_core::Error::Missing => Errno::ENOENT,
        crate::event::evq_core::Error::InvalidMask => Errno::EINVAL,
    })?;
    Ok(0)
}

fn wait(frame: &SyscallFrame) -> Result<usize, Errno> {
    let queue_handle = checked_handle(frame.rdi)?;
    let max = usize::try_from(frame.rdx).map_err(|_| Errno::EINVAL)?;
    if max == 0 {
        return Ok(0);
    }
    let bytes = max
        .checked_mul(size_of::<EvqEvent>())
        .ok_or(Errno::EINVAL)?;
    let destination = UserSlice::new(frame.rsi, u64::try_from(bytes).map_err(|_| Errno::EINVAL)?)?;
    let process = process::current().ok_or(Errno::ESRCH)?;
    let queue = queue_from_handle(&process, queue_handle, Rights::READ)?;
    let deadline = (frame.r10 != 0).then(|| time::now_ns().saturating_add(frame.r10));
    let mut ready = collect_ready(&queue, &process, max);
    if ready.is_empty() {
        if queue
            .waiting
            .compare_exchange(
                false,
                true,
                core::sync::atomic::Ordering::AcqRel,
                core::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            return Err(Errno::EBUSY);
        }
        let wait_result = loop {
            let remaining = deadline.map_or(0, |deadline| deadline.saturating_sub(time::now_ns()));
            if deadline.is_some() && remaining == 0 {
                break Ok(());
            }
            let status = match wait_once(&queue, &process, remaining) {
                Ok(status) => status,
                Err(error) => break Err(error),
            };
            if status == Errno::ETIMEDOUT.to_ret() {
                break Ok(());
            }
            ready = collect_ready(&queue, &process, max);
            if !ready.is_empty() {
                break Ok(());
            }
        };
        queue
            .waiting
            .store(false, core::sync::atomic::Ordering::Release);
        wait_result?;
    }
    let mut count = 0usize;
    for item in ready {
        let event = EvqEvent {
            cookie: item.ready.cookie,
            events: item.ready.events,
            pad: 0,
        };
        let offset = count
            .checked_mul(size_of::<EvqEvent>())
            .ok_or(Errno::EINVAL)?;
        let address = frame
            .rsi
            .checked_add(u64::try_from(offset).map_err(|_| Errno::EINVAL)?)
            .ok_or(Errno::EFAULT)?;
        let slot = UserSlice::new(
            address,
            u64::try_from(size_of::<EvqEvent>()).map_err(|_| Errno::EINVAL)?,
        )?;
        // SAFETY: `event` is a live `repr(C)` stack value and `slot` covers exactly its bytes.
        let source = unsafe {
            core::slice::from_raw_parts(
                core::ptr::addr_of!(event).cast::<u8>(),
                size_of::<EvqEvent>(),
            )
        };
        copy_to_user(slot, source)?;
        consume_source(&process, Handle(item.key));
        count = count.saturating_add(1);
    }
    let _ = destination;
    Ok(count)
}

/// Atomically re-check readiness under the wait-queue lock before publishing one waiter.
///
/// A source that transitions before this function takes the wait lock makes `has_ready` true and
/// is returned by the caller's next `collect_sources`; a source that transitions afterwards sees
/// the published waiter and sets its scheduler pending-wake bit.  Therefore neither ordering can
/// lose a wakeup.
fn wait_once(
    queue: &Arc<EventQueue>,
    process: &Arc<process::Process>,
    timeout_ns: u64,
) -> Result<isize, Errno> {
    let thread_id = crate::sched::current_thread()
        .map(|thread| thread.id)
        .ok_or(Errno::ESRCH)?;
    let token = crate::sched::begin_block_interruptible()?;
    if crate::sched::take_kick() {
        crate::sched::end_block(token);
        return Err(Errno::EINTR);
    }
    if timeout_ns != 0 {
        crate::sched::add_sleep(
            token,
            time::now_ns().saturating_add(timeout_ns),
            Errno::ETIMEDOUT.to_ret(),
        );
    }
    let queued = queue.waiters.enqueue_if(process.id, 0, || {
        Ok(!queue
            .state
            .lock()
            .has_ready(|raw| readiness(process, Handle(raw))))
    });
    match queued {
        Ok(true) => {
            let status = crate::sched::block_current();
            let kicked = crate::sched::take_kick();
            queue.waiters.remove(process.id, 0, thread_id);
            crate::sched::end_block(token);
            if status == Errno::EINTR.to_ret() || kicked {
                Err(Errno::EINTR)
            } else {
                Ok(status)
            }
        }
        Ok(false) => {
            crate::sched::end_block(token);
            Ok(0)
        }
        Err(error) => {
            crate::sched::end_block(token);
            Err(error)
        }
    }
}

fn collect_ready(
    queue: &Arc<EventQueue>,
    process: &Arc<process::Process>,
    max: usize,
) -> alloc::vec::Vec<crate::event::evq_core::Delivery> {
    queue
        .state
        .lock()
        .collect_sources(max, |raw| readiness(process, Handle(raw)))
}

fn signal(frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = checked_handle(frame.rdi)?;
    let process = process::current().ok_or(Errno::ESRCH)?;
    let event = match process.handles.lock().get(handle, Rights::SIGNAL)? {
        Object::UserEvent(event) => Arc::clone(event),
        _ => return Err(Errno::EBADF),
    };
    let _ = frame.rsi;
    let _ = event.signal();
    event::notify_all();
    Ok(0)
}

fn timer_set(frame: &SyscallFrame) -> Result<usize, Errno> {
    let handle = checked_handle(frame.rdi)?;
    let process = process::current().ok_or(Errno::ESRCH)?;
    let timer = match process.handles.lock().get(handle, Rights::WRITE)? {
        Object::Timer(timer) => Arc::clone(timer),
        _ => return Err(Errno::EBADF),
    };
    timer.set(time::now_ns(), frame.rsi, frame.rdx);
    Ok(0)
}

fn checked_handle(raw: u64) -> Result<Handle, Errno> {
    Ok(Handle(u32::try_from(raw).map_err(|_| Errno::EBADF)?))
}

fn queue_from_handle(
    process: &Arc<process::Process>,
    handle: Handle,
    right: Rights,
) -> Result<Arc<EventQueue>, Errno> {
    match process.handles.lock().get(handle, right)? {
        Object::EventQueue(queue) => Ok(Arc::clone(queue)),
        _ => Err(Errno::EBADF),
    }
}

fn source_from_handle(process: &Arc<process::Process>, handle: Handle) -> Result<(), Errno> {
    match process.handles.lock().get(handle, Rights::READ)? {
        Object::Timer(_) | Object::UserEvent(_) | Object::Process(_) => Ok(()),
        Object::Irq(_) => Ok(()),
        #[cfg(target_arch = "x86_64")]
        Object::SchemeFile(_) | Object::SchemeServer(_) => Ok(()),
        _ => Err(Errno::EBADF),
    }
}

fn readiness(process: &Arc<process::Process>, handle: Handle) -> u32 {
    let table = process.handles.lock();
    match table.get(handle, Rights::READ) {
        Ok(Object::Timer(timer)) if timer.peek(time::now_ns()) => event::SIGNALED,
        Ok(Object::UserEvent(event)) if event.ready() => event::SIGNALED,
        Ok(Object::Irq(irq)) if irq.pending() != 0 => event::SIGNALED,
        Ok(Object::Process(target)) if *target.state.lock() == ProcessState::Exited => {
            event::SIGNALED
        }
        #[cfg(target_arch = "x86_64")]
        Ok(Object::SchemeServer(server)) if !server.is_live() => event::HUP,
        #[cfg(target_arch = "x86_64")]
        Ok(Object::SchemeServer(server)) => server.owner.upgrade().map_or(event::HUP, |owner| {
            if owner.scheme_queue.lock().is_empty() {
                0
            } else {
                event::READABLE
            }
        }),
        #[cfg(target_arch = "x86_64")]
        Ok(Object::SchemeFile(file)) if *file.server.state.lock() == ProcessState::Exited => {
            event::HUP
        }
        _ => 0,
    }
}

/// Consume the source level only after its record reached user memory.
///
/// Timer readiness is a predicate (`peek`) during the scan; the period is consumed here, exactly
/// once per delivered record.  Irq readiness is level-triggered and consumed by `IRQ_WAIT`.
fn consume_source(process: &Arc<process::Process>, handle: Handle) {
    let table = process.handles.lock();
    match table.get(handle, Rights::READ) {
        Ok(Object::Timer(timer)) => {
            let _ = timer.consume(time::now_ns());
        }
        Ok(Object::UserEvent(event)) => {
            let _ = event.take();
        }
        _ => {}
    }
}
