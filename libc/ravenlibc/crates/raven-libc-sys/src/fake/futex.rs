// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Futex model of the fake kernel.
//!
//! Mirrors `kernel/src/syscall/futex.rs:31-88`: `WAIT` checks the user word and returns `EAGAIN`
//! when it differs; `WAKE` returns the number of waiters actually woken. The default is a
//! single-threaded cooperative model (`WAIT` returns immediately); tests that need blocking call
//! [`set_blocking`].

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use raven_abi::errno::Errno;

use host::collections::BTreeSet;
use host::sync::{Arc, Condvar, Mutex};
use host::time::Duration;

static BLOCKING: AtomicBool = AtomicBool::new(false);
static SLOTS: Mutex<Option<host::collections::HashMap<usize, Arc<Slot>>>> = Mutex::new(None);

#[derive(Default)]
struct SlotState {
    next_ticket: u64,
    waiting: BTreeSet<u64>,
    woken: BTreeSet<u64>,
}

struct Slot {
    state: Mutex<SlotState>,
    changed: Condvar,
}

fn slots() -> host::sync::MutexGuard<'static, Option<host::collections::HashMap<usize, Arc<Slot>>>>
{
    SLOTS.lock().unwrap_or_else(|error| error.into_inner())
}

fn slot(addr: usize) -> Arc<Slot> {
    let mut guard = slots();
    let map = guard.get_or_insert_with(host::collections::HashMap::new);
    Arc::clone(map.entry(addr).or_insert_with(|| {
        Arc::new(Slot {
            state: Mutex::new(SlotState::default()),
            changed: Condvar::new(),
        })
    }))
}

fn lookup(addr: usize) -> Option<Arc<Slot>> {
    slots()
        .as_ref()
        .and_then(|map| map.get(&addr))
        .map(Arc::clone)
}

/// Enable or disable real blocking waits.
pub fn set_blocking(enabled: bool) {
    BLOCKING.store(enabled, Ordering::Release);
}

/// Number of threads currently waiting on `addr`.
pub fn waiters(addr: usize) -> usize {
    lookup(addr).map_or(0, |slot| {
        slot.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .waiting
            .len()
    })
}

fn read_word(addr: usize) -> u32 {
    // SAFETY: the syscall wrapper passed a live `&AtomicU32` pointer; the fake kernel only
    // dereferences addresses supplied by its own callers.
    unsafe { (*(addr as *const AtomicU32)).load(Ordering::SeqCst) }
}

/// `FUTEX` `WAIT`.
pub fn wait(addr: usize, expected: u32, timeout_ns: u64) -> Result<usize, Errno> {
    if addr & 3 != 0 {
        return Err(Errno::EINVAL);
    }
    if read_word(addr) != expected {
        return Err(Errno::EAGAIN);
    }
    if !BLOCKING.load(Ordering::Acquire) {
        return Ok(0);
    }

    let slot = slot(addr);
    let mut state = slot.state.lock().unwrap_or_else(|error| error.into_inner());
    let ticket = state.next_ticket;
    state.next_ticket = state.next_ticket.wrapping_add(1);
    state.waiting.insert(ticket);

    if timeout_ns == 0 {
        while !state.woken.remove(&ticket) {
            state = slot
                .changed
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    } else {
        let deadline = Duration::from_nanos(timeout_ns);
        let start = host::time::Instant::now();
        while !state.woken.remove(&ticket) {
            let elapsed = start.elapsed();
            if elapsed >= deadline {
                state.waiting.remove(&ticket);
                return Err(Errno::ETIMEDOUT);
            }
            let remaining = deadline.saturating_sub(elapsed);
            let (next, timeout) = slot
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
            if timeout.timed_out() && !state.woken.contains(&ticket) {
                state.waiting.remove(&ticket);
                return Err(Errno::ETIMEDOUT);
            }
        }
    }
    state.waiting.remove(&ticket);
    Ok(0)
}

/// `FUTEX` `WAKE`; returns the exact number of waiters woken.
pub fn wake(addr: usize, count: u32) -> usize {
    let Some(slot) = lookup(addr) else {
        return 0;
    };
    let mut state = slot.state.lock().unwrap_or_else(|error| error.into_inner());
    let mut woken = 0usize;
    let limit = usize::try_from(count).unwrap_or(usize::MAX);
    while woken < limit {
        let Some(ticket) = state.waiting.pop_first() else {
            break;
        };
        state.woken.insert(ticket);
        woken += 1;
    }
    if woken != 0 {
        slot.changed.notify_all();
    }
    woken
}
