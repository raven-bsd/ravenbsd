// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel event-queue objects and timer/user-event readiness state.

#[path = "../../../crates/raven-evq-core/src/lib.rs"]
pub(crate) mod evq_core;

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;
use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use spin::Mutex;

use crate::process::handle::Object;
use crate::process::{Process, ProcessState};
use crate::sched::wait::WaitQueue;
use crate::sync::IrqSpinLock;
use crate::time;

pub use self::evq_core::SIGNALED;
pub use self::evq_core::{HUP, READABLE, WRITABLE};

/// An event queue registration table with waiters.
#[allow(dead_code)] // Wake integration is completed by the source hooks in this module.
pub struct EventQueue {
    /// Process whose handle values form registration keys.
    pub owner: u64,
    /// The pure edge/level state machine.
    pub state: Mutex<evq_core::Queue>,
    /// Threads blocked in `EVQ_WAIT`.
    pub waiters: WaitQueue,
    /// Only one thread may block in one wait call.
    pub waiting: AtomicBool,
    /// Set by the process-exit hook; the queue no longer represents a live event source.
    retired: AtomicBool,
}

impl EventQueue {
    /// Construct an empty event queue.
    pub const fn new(owner: u64) -> Self {
        Self {
            owner,
            state: Mutex::new(evq_core::Queue::new()),
            waiters: WaitQueue::new(),
            waiting: AtomicBool::new(false),
            retired: AtomicBool::new(false),
        }
    }
}

/// A monotonic timer object.
pub struct Timer {
    deadline: AtomicU64,
    interval: AtomicU64,
    quota: Arc<core::sync::atomic::AtomicU32>,
}

/// Live timer objects (S69 cleanup evidence).
static LIVE_TIMERS: AtomicU64 = AtomicU64::new(0);

/// Number of live timer objects across all processes.
pub fn live_timers() -> u64 {
    LIVE_TIMERS.load(Ordering::Acquire)
}

impl Timer {
    /// Construct a disarmed timer.
    pub fn new(quota: Arc<core::sync::atomic::AtomicU32>) -> Self {
        LIVE_TIMERS.fetch_add(1, Ordering::AcqRel);
        Self {
            deadline: AtomicU64::new(0),
            interval: AtomicU64::new(0),
            quota,
        }
    }

    /// Arm or disarm the timer. Periodic deadlines advance from their prior deadline.
    pub fn set(&self, now: u64, initial: u64, interval: u64) {
        if initial == 0 {
            self.interval.store(0, Ordering::Release);
            self.deadline.store(0, Ordering::Release);
            return;
        }
        self.interval.store(interval, Ordering::Release);
        self.deadline
            .store(now.saturating_add(initial), Ordering::Release);
    }

    /// Ready state at `now`, without consuming a period.
    ///
    /// This is the predicate form used by readiness scans and enqueue checks; it must stay
    /// side-effect free so probing twice cannot lose a timer event.
    pub fn peek(&self, now: u64) -> bool {
        let deadline = self.deadline.load(Ordering::Acquire);
        deadline != 0 && now >= deadline
    }

    /// Consume the current period, advancing a periodic timer without drift.
    ///
    /// Returns false when the timer is not ready.  The compare-exchange makes a second consumer
    /// of the same period (another event queue watching the same timer) a no-op instead of
    /// skipping a whole period.
    pub fn consume(&self, now: u64) -> bool {
        loop {
            let deadline = self.deadline.load(Ordering::Acquire);
            if deadline == 0 || now < deadline {
                return false;
            }
            let interval = self.interval.load(Ordering::Acquire);
            let next = if interval == 0 {
                0
            } else {
                let elapsed = now.saturating_sub(deadline);
                let periods = elapsed.checked_div(interval).unwrap_or(0).saturating_add(1);
                deadline.saturating_add(periods.saturating_mul(interval))
            };
            if self
                .deadline
                .compare_exchange(deadline, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.quota.fetch_sub(1, Ordering::AcqRel);
        LIVE_TIMERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A pure user-space doorbell.
pub struct UserEvent {
    signalled: AtomicBool,
}

/// Maximum registered event queues.  The interrupt-context notification scan is bounded by this
/// constant; `EVQ_CREATE` fails with `ENOSPC` when it is exhausted.
pub const MAX_QUEUES: usize = 64;

/// Queues registered for broad source notifications.
///
/// The lock is interrupt-safe: the timer and device-interrupt source hooks take it, and a
/// process-context holder (registration or handle close) has interrupts disabled, so the handler
/// on that CPU cannot observe the lock held.
static QUEUES: IrqSpinLock<Vec<alloc::sync::Weak<EventQueue>>> = IrqSpinLock::new(Vec::new());

/// Record a queue so handle closure can synchronously delete its registrations.
pub fn register_queue(queue: &Arc<EventQueue>) -> Result<(), Errno> {
    let mut queues = QUEUES.lock();
    queues.retain(|weak| weak.strong_count() != 0);
    if queues.len() >= MAX_QUEUES {
        return Err(Errno::ENOSPC);
    }
    queues.push(Arc::downgrade(queue));
    Ok(())
}

/// Remove the dead queue's weak registration.
///
/// A blocked `EVQ_WAIT` keeps an `Arc<EventQueue>` on a kernel stack that thread exit abandons
/// without unwinding, so the object can outlive the process.  The registration table must still
/// drop the entry: otherwise `MAX_QUEUES` dead queues eventually reject a live `EVQ_CREATE`.
fn unregister_queue(queue: &Arc<EventQueue>) {
    QUEUES
        .lock()
        .retain(|weak| !weak.upgrade().is_some_and(|item| Arc::ptr_eq(&item, queue)));
}

/// Number of event queues whose owner still holds a strong reference (S69 evidence).
pub fn live_queues() -> u64 {
    QUEUES
        .lock()
        .iter()
        .filter_map(alloc::sync::Weak::upgrade)
        .filter(|queue| !queue.retired.load(Ordering::Acquire))
        .count() as u64
}

/// Level-true readiness for one registered source.
///
/// This is the `seltrue`/`sel_setevents` donor arm: the value is computed from live state at
/// poll time, never cached. It is called from process context (the waiter's entry scan and the
/// event-signal notification path), not from the timer interrupt.
pub(crate) fn readiness(process: &Arc<Process>, handle: Handle) -> u32 {
    let table = process.handles.lock();
    // Poll observes a source, it does not read or write it. Every end is therefore pollable
    // with the handle alone: a pipe write end carries WRITE, a write-only pty/duplex carries
    // WRITE, and a READ-gated lookup would return EACCES before the readiness arm (K548).
    if let Ok(Object::Pipe(end)) = table.get(handle, Rights::NONE) {
        return end.readiness();
    }
    match table.get(handle, Rights::NONE) {
        Ok(Object::Timer(timer)) if timer.peek(time::now_ns()) => SIGNALED,
        Ok(Object::UserEvent(event)) if event.ready() => SIGNALED,
        Ok(Object::Irq(irq)) if irq.pending() != 0 => SIGNALED,
        Ok(Object::Process(target)) if *target.state.lock() == ProcessState::Exited => SIGNALED,
        Ok(Object::Duplex(end)) => end.readiness(),
        Ok(Object::SchemeServer(server)) if !server.is_live() => HUP,
        Ok(Object::SchemeServer(server)) => server.owner.upgrade().map_or(HUP, |owner| {
            if owner.scheme_queue.lock().is_empty() {
                0
            } else {
                READABLE
            }
        }),
        Ok(Object::SchemeFile(file)) if *file.server.state.lock() == ProcessState::Exited => HUP,
        // K409: a live scheme file is a pollable source even when no event is attached.
        Ok(Object::SchemeFile(_)) => READABLE | WRITABLE,
        _ => 0,
    }
}

/// Watch-table identity for one source object.
///
/// A handle value is local to its process, so the source is identified by a stable key shared by
/// every process that can reach it: the kernel object pointer for the process/timer/IRQ/user
/// event/scheme objects, and the shared pipe/duplex id for the two-ended objects. This is the
/// `struct selinfo` role of the NetBSD donor: the key names the source's watcher list.
pub struct SourceSpec {
    key: u64,
    timer: Option<Arc<Timer>>,
}

impl SourceSpec {
    fn key(key: u64) -> Self {
        Self { key, timer: None }
    }

    fn timer(timer: &Arc<Timer>) -> Self {
        Self {
            key: arc_key(timer),
            timer: Some(Arc::clone(timer)),
        }
    }
}

fn arc_key<T>(arc: &Arc<T>) -> u64 {
    Arc::as_ptr(arc) as usize as u64
}

/// Pipe and duplex ids are tagged above the user address space so they cannot alias an object
/// pointer (kernel pointers are in the high canonical half on x86-64 and below it on aarch64; the
/// tags are reserved and never produced by `Arc::as_ptr`).
const PIPE_KEY_TAG: u64 = 0x4000_0000_0000_0000;
const DUPLEX_KEY_TAG: u64 = 0x2000_0000_0000_0000;

/// Tagged watcher key for a pipe's shared ring id.
pub(crate) fn pipe_key(id: u64) -> u64 {
    PIPE_KEY_TAG | id
}

/// Tagged watcher key for a duplex pair id.
pub(crate) fn duplex_key(id: u64) -> u64 {
    DUPLEX_KEY_TAG | id
}

/// Resolve a handle to the source key used by the watcher table and by the transition call sites.
///
/// This is `selrecord`'s source lookup: the handle is the capability, so poll observes any live
/// source without a read/write data right (K548: a write-only pty/duplex must not be EACCES), and
/// the returned key is what `notify_source` matches.
pub fn source_spec(process: &Arc<Process>, handle: Handle) -> Result<SourceSpec, Errno> {
    let table = process.handles.lock();
    // Both pipe ends are observable even though only the read end carries READ.
    if let Ok(Object::Pipe(end)) = table.get(handle, Rights::NONE) {
        return Ok(SourceSpec::key(end.event_key()));
    }
    match table.get(handle, Rights::NONE) {
        Ok(Object::Timer(timer)) => Ok(SourceSpec::timer(timer)),
        Ok(Object::UserEvent(event)) => Ok(SourceSpec::key(arc_key(event))),
        Ok(Object::Irq(irq)) => Ok(SourceSpec::key(irq.event_key())),
        Ok(Object::Process(target)) => Ok(SourceSpec::key(arc_key(target))),
        Ok(Object::Duplex(end)) => Ok(SourceSpec::key(end.event_key())),
        // The scheme objects are the same `crate::ipc` types on every arch
        // (`arch::aarch64::ipc` re-exports `crate::ipc`); the previous x86-only gate left the
        // aarch64 netd EVQ_CTL ADD on its own `net:` server handle at EBADF.
        Ok(Object::SchemeServer(server)) => Ok(SourceSpec::key(
            server.owner.upgrade().as_ref().map_or_else(
                || Arc::as_ptr(server) as usize as u64,
                |owner| Arc::as_ptr(owner) as usize as u64,
            ),
        )),
        Ok(Object::SchemeFile(file)) => Ok(SourceSpec::key(arc_key(&file.server))),
        _ => Err(Errno::EBADF),
    }
}

/// One queue watching one source (`selinfo`'s named waiter).
struct Watch {
    source: u64,
    queue: Weak<EventQueue>,
    owner: u64,
    key: u32,
}

/// One timer watch: the timer is needed at tick time to apply the level predicate without a
/// source lock.
struct TimerWatch {
    timer: Weak<Timer>,
    queue: Weak<EventQueue>,
    owner: u64,
    key: u32,
}

/// Registered source watchers. This is the per-source `struct selinfo` list, collapsed into one
/// bounded table because the source objects live in modules outside the event core.
static WATCHES: IrqSpinLock<Vec<Watch>> = IrqSpinLock::new(Vec::new());
static TIMER_WATCHES: IrqSpinLock<Vec<TimerWatch>> = IrqSpinLock::new(Vec::new());

/// Register (or refresh) the watcher for one handle after `EVQ_CTL ADD`.
pub fn register_watch(queue: &Arc<EventQueue>, spec: &SourceSpec, handle: Handle) {
    let mut watches = WATCHES.lock();
    watches.retain(|watch| watch.queue.strong_count() != 0);
    if let Some(slot) = watches
        .iter_mut()
        .find(|watch| watch.owner == queue.owner && watch.key == handle.0)
    {
        slot.source = spec.key;
    } else {
        watches.push(Watch {
            source: spec.key,
            queue: Arc::downgrade(queue),
            owner: queue.owner,
            key: handle.0,
        });
    }
    drop(watches);
    if let Some(timer) = &spec.timer {
        let mut timers = TIMER_WATCHES.lock();
        timers.retain(|watch| watch.queue.strong_count() != 0);
        if !timers
            .iter()
            .any(|watch| watch.owner == queue.owner && watch.key == handle.0)
        {
            timers.push(TimerWatch {
                timer: Arc::downgrade(timer),
                queue: Arc::downgrade(queue),
                owner: queue.owner,
                key: handle.0,
            });
        }
    }
}

/// Remove the watcher for one handle after `EVQ_CTL DEL` or a close.
pub fn remove_watch(owner: u64, handle: Handle) {
    WATCHES
        .lock()
        .retain(|watch| !(watch.owner == owner && watch.key == handle.0));
    TIMER_WATCHES
        .lock()
        .retain(|watch| !(watch.owner == owner && watch.key == handle.0));
}

/// Drop every watcher owned by a process at exit.
pub fn remove_owner(owner: u64) {
    WATCHES.lock().retain(|watch| watch.owner != owner);
    TIMER_WATCHES.lock().retain(|watch| watch.owner != owner);
}

/// Notify only the watchers of one source (`selnotify`).
///
/// The transition caller holds the source lock, exactly as the donor calls `selnotify` under the
/// object lock. This routine takes only the IRQ-safe watcher lock and the wait-queue lock: it does
/// not evaluate readiness and does not take a process handle table, so it is safe from the timer
/// and device-interrupt paths. A waiter that has not slept yet re-scans; a blocked waiter is woken.
pub fn notify_source(source: u64) {
    // Copy the matching queues under the table lock, then wake outside it. Each queue has one
    // waiter at most; a queue with several handles on the same source is woken once.
    let mut ready: [Option<Arc<EventQueue>>; MAX_QUEUES] = [const { None }; MAX_QUEUES];
    let mut count = 0usize;
    {
        let mut watches = WATCHES.lock();
        watches.retain(|watch| watch.queue.strong_count() != 0);
        for watch in watches.iter() {
            if watch.source != source {
                continue;
            }
            let Some(queue) = watch.queue.upgrade() else {
                continue;
            };
            if ready[..count]
                .iter()
                .flatten()
                .any(|queued| Arc::ptr_eq(queued, &queue))
            {
                continue;
            }
            if count < ready.len() {
                ready[count] = Some(queue);
                count += 1;
            }
        }
    }
    for queue in ready[..count].iter().flatten() {
        let _ = queue.waiters.wake_one(queue.owner, 0);
    }
}

/// User-event source notification (`selnotify` for a doorbell).
pub fn notify_event(source: &Arc<UserEvent>) {
    notify_source(arc_key(source));
}

/// Timer-tick notification: wake only the queues whose registered timer is expired.
///
/// The timer deadline is atomic, so this runs from the tick without a source lock. A queue whose
/// timer is not expired is not woken; the waiter would only re-sleep.
pub fn notify_timers(now: u64) {
    let mut ready: [Option<Arc<EventQueue>>; MAX_QUEUES] = [const { None }; MAX_QUEUES];
    let mut count = 0usize;
    {
        let mut watches = TIMER_WATCHES.lock();
        watches.retain(|watch| watch.queue.strong_count() != 0);
        for watch in watches.iter() {
            let Some(timer) = watch.timer.upgrade() else {
                continue;
            };
            if !timer.peek(now) {
                continue;
            }
            let Some(queue) = watch.queue.upgrade() else {
                continue;
            };
            if ready[..count]
                .iter()
                .flatten()
                .any(|queued| Arc::ptr_eq(queued, &queue))
            {
                continue;
            }
            if count < ready.len() {
                ready[count] = Some(queue);
                count += 1;
            }
        }
    }
    for queue in ready[..count].iter().flatten() {
        let _ = queue.waiters.wake_one(queue.owner, 0);
    }
}

/// Event registrations removed by the close hook.
///
/// The S38 evidence print reports the delta between process exits: a close path that forgets the
/// hook shows a short count.  Counting removed registrations (not hook calls) keeps the evidence
/// independent of unrelated handle closes such as a spawn's stack object.
static REMOVED_REGISTRATIONS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
/// [`REMOVED_REGISTRATIONS`] observed at the previous process exit (the evidence delta base).
static REMOVED_AT_EXIT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// True when the S38 close-hook evidence print is enabled (`s38-eventcheck` on the command line).
fn close_evidence() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s38-eventcheck" || token.starts_with("s38-eventcheck="))
}

/// True when the S39 aarch64-parity exit evidence is enabled (`s39-eventcheck`).
fn s39_evidence() -> bool {
    crate::memory::paging::cmdline()
        .split_ascii_whitespace()
        .any(|token| token == "s39-eventcheck" || token.starts_with("s39-eventcheck="))
}

/// Upgraded queue snapshot.  Allocation-free: the timer-interrupt path must not allocate, and the
/// same bound/pattern is reused by the close and exit hooks so they stay safe in any context.
///
/// Kept out of line: the 1 KiB array must live in this frame, never in a syscall-dispatch or
/// teardown frame (the kernel stacks have a narrow margin at the teardown evidence parses).
#[inline(never)]
fn queue_snapshot() -> [Option<Arc<EventQueue>>; MAX_QUEUES] {
    let mut queues: [Option<Arc<EventQueue>>; MAX_QUEUES] = [const { None }; MAX_QUEUES];
    let table = QUEUES.lock();
    for (slot, weak) in queues.iter_mut().zip(table.iter()) {
        *slot = weak.upgrade();
    }
    queues
}

/// Remove `handle` from every event queue owned by `process` and count its registration.
///
/// This is the event part of the close hook.  It is idempotent by construction (`Queue::close` is
/// a retain over the key), so a handle with no registration is a no-op and one successful close
/// can never deregister another close's key.
#[inline(never)]
fn deregister(process: &Arc<crate::process::Process>, handle: raven_abi::types::Handle) {
    remove_watch(process.id, handle);
    let queues = queue_snapshot();
    let mut removed = 0usize;
    for queue in queues.iter().flatten() {
        if queue.owner == process.id {
            let mut state = queue.state.lock();
            let before = state.len();
            state.close(handle.0);
            removed = removed.saturating_add(before.saturating_sub(state.len()));
        }
    }
    REMOVED_REGISTRATIONS.fetch_add(removed as u64, core::sync::atomic::Ordering::Relaxed);
}

/// Close `handle` in `process` and run the close hook exactly once.
///
/// This is the single kernel-side close primitive: scheme-file and request-handle close,
/// thread-creation rollback and the process-exit sweep all use it, and the user `HANDLE_CLOSE`
/// path does too, so every close shares the same event hook.  The IRQ object is
/// captured before the close so a blocked `IRQ_WAIT` can be cancelled when the caller has the
/// process context; the event registrations are removed only after the close commits, so a stale
/// handle fails with `EBADF` and leaves every registration intact.
#[inline(never)]
pub fn close_handle(
    process: &Arc<crate::process::Process>,
    handle: raven_abi::types::Handle,
) -> Result<(), Errno> {
    #[cfg(target_arch = "x86_64")]
    let irq = {
        let table = process.handles.lock();
        match table.get(handle, raven_abi::types::Rights::NONE) {
            Ok(crate::process::handle::Object::Irq(irq)) => Some(Arc::clone(irq)),
            _ => None,
        }
    };
    let request = {
        let table = process.handles.lock();
        match table.get(handle, raven_abi::types::Rights::NONE) {
            Ok(crate::process::handle::Object::SchemeRequest(request)) => Some(Arc::clone(request)),
            _ => None,
        }
    };
    process.handles.lock().close(handle)?;
    if let Some(request) = request {
        // A server that closes the request capability cannot send the reply. Fail the client,
        // release the capture and drop the server's active-request record instead of leaving a
        // blocked client and a mapped slot until the server exits.
        request.fail(Errno::EIO);
        request
            .server
            .active_requests
            .lock()
            .retain(|item| !Arc::ptr_eq(item, &request));
        request.release_captures();
    }
    #[cfg(target_arch = "x86_64")]
    if let Some(irq) = irq {
        irq.cancel_waiters(handle.0);
    }
    deregister(process, handle);
    Ok(())
}

/// Process-exit close hook: drop every event registration owned by the dying process.
///
/// The teardown calls this before it replaces the handle table on every architecture.  Every
/// queue with this owner is cleared in one place, so no registration outlives the handle table
/// that keyed it and no waiter is left on a source whose process is gone.  The state lock is
/// released before the waiters are woken to keep the queue lock order (`state` before `waiters`)
/// one-way.
#[inline(never)]
pub fn on_process_exit(process: &Arc<crate::process::Process>) {
    remove_owner(process.id);
    let queues = queue_snapshot();
    let mut events = 0usize;
    let mut cleared = 0usize;
    let mut waiters = 0usize;
    for queue in queues.iter().flatten() {
        if queue.owner != process.id {
            continue;
        }
        let registrations = {
            let mut state = queue.state.lock();
            let count = state.len();
            *state = evq_core::Queue::new();
            count
        };
        queue.retired.store(true, Ordering::Release);
        unregister_queue(queue);
        events = events.saturating_add(registrations);
        cleared = cleared.saturating_add(1);
        waiters = waiters.saturating_add(usize::from(queue.waiting.swap(false, Ordering::AcqRel)));
        let _ = queue.waiters.wake_all();
    }
    let s38 = close_evidence();
    let s39 = s39_evidence();
    if !s38 && !s39 {
        return;
    }
    let total = queues
        .iter()
        .flatten()
        .map(|queue| queue.state.lock().len())
        .sum::<usize>();
    let closes = REMOVED_REGISTRATIONS.load(Ordering::Relaxed);
    let delta = closes.saturating_sub(REMOVED_AT_EXIT.swap(closes, Ordering::AcqRel));
    if s38 {
        crate::kprintln!(
            "s38: exit pid={} events={} cleared={} total={} removed={}",
            process.id,
            events,
            cleared,
            total,
            delta
        );
    }
    if s39 {
        crate::kprintln!(
            "s39: exit pid={} events={} cleared={} total={} waiters={}",
            process.id,
            events,
            cleared,
            total,
            waiters
        );
    }
}

impl UserEvent {
    /// Construct an unsignalled doorbell.
    pub const fn new() -> Self {
        Self {
            signalled: AtomicBool::new(false),
        }
    }

    /// Set the level and return whether this was a new transition.
    pub fn signal(&self) -> bool {
        !self.signalled.swap(true, Ordering::AcqRel)
    }

    /// Consume the reported doorbell level.
    #[allow(dead_code)] // EVQ_WAIT consumes the level after selecting this source.
    pub fn take(&self) -> bool {
        self.signalled.swap(false, Ordering::AcqRel)
    }

    /// Read the current level.
    pub fn ready(&self) -> bool {
        self.signalled.load(Ordering::Acquire)
    }
}
