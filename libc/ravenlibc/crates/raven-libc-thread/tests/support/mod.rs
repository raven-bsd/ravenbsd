// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The host fake kernel for the integration tests: real `std::thread`s, a baton scheduler with a
//! seeded RNG and a virtual clock, a page ledger that detects a late stack unmap, and a futex
//! table implemented by the scheduler itself. `std` is allowed here (test support).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, ThreadId};

use raven_libc_thread::kernel::{CreateArgs, KernelCaps, ThreadKernel};
use raven_libc_thread::limits::{EAGAIN, EINTR, EINVAL, ENOSYS, ETIMEDOUT, Errno};
use raven_libc_thread::raw;
use raven_libc_tls::TlsModuleIn;

/// Serialise tests that touch the process-wide thread state.
static SERIAL: Mutex<()> = Mutex::new(());

/// Take the serial test lock.
pub fn serial() -> MutexGuard<'static, ()> {
    match SERIAL.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Read a seed from `RL_SCHED_SEED`, or return `default`.
pub fn seed_from_env(default: u64) -> u64 {
    std::env::var("RL_SCHED_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// The original C35 seeded RNG (xorshift128+ style); the scheduler uses it so a run is a pure
/// function of `(seed, scenario)`.
#[derive(Clone)]
pub struct Rng {
    state: [u64; 2],
}

impl Rng {
    /// A seeded generator.
    #[must_use]
    pub fn seed_from_u64(seed: u64) -> Self {
        Self {
            state: [
                seed ^ 0x9e37_79b9_7f4a_7c15,
                seed.wrapping_mul(0xbf58_476d_1ce4_e5b9) | 1,
            ],
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut s1 = self.state[0];
        let s0 = self.state[1];
        self.state[0] = s0;
        s1 ^= s1 << 23;
        self.state[1] = s1 ^ s0 ^ (s1 >> 17) ^ (s0 >> 26);
        self.state[1].wrapping_add(s0)
    }

    /// A number below `bound`.
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next_u64() % bound as u64) as usize
        }
    }
}

/// One scheduler run's final state.
#[derive(Debug, Default)]
pub struct RunReport {
    /// Steps executed.
    pub steps: u64,
    /// Whether the run deadlocked.
    pub deadlocked: bool,
    /// The trace, one line per hand-over.
    pub trace: Vec<String>,
}

struct Gate {
    ready: Mutex<bool>,
    cv: Condvar,
}

#[derive(Default)]
struct ThreadState {
    runnable: bool,
    done: bool,
    blocked: Option<Block>,
    wake: Option<i32>,
}

struct Block {
    futex: Option<usize>,
    deadline: Option<u64>,
}

struct SchedState {
    next_id: u64,
    ids: HashMap<ThreadId, u64>,
    states: BTreeMap<u64, ThreadState>,
    gates: HashMap<u64, Arc<Gate>>,
    rng: Rng,
    turn: Option<u64>,
    now: u64,
    steps: u64,
    trace: Vec<String>,
    deadlocked: bool,
    seed: u64,
}

/// The baton scheduler. One per `HostKernel`, shared by all its threads.
pub struct Sched {
    state: Mutex<SchedState>,
}

impl Sched {
    fn new(seed: u64) -> Self {
        Self {
            state: Mutex::new(SchedState {
                next_id: 1,
                ids: HashMap::new(),
                states: BTreeMap::new(),
                gates: HashMap::new(),
                rng: Rng::seed_from_u64(seed),
                turn: None,
                now: 1_000_000,
                steps: 0,
                trace: Vec::new(),
                deadlocked: false,
                seed,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, SchedState> {
        match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn register(&self) -> u64 {
        let mut state = self.lock();
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        state.ids.insert(thread::current().id(), id);
        state.states.insert(
            id,
            ThreadState {
                runnable: true,
                ..ThreadState::default()
            },
        );
        state.gates.insert(
            id,
            Arc::new(Gate {
                ready: Mutex::new(false),
                cv: Condvar::new(),
            }),
        );
        id
    }

    fn init_main(&self) -> u64 {
        let id = self.register();
        let mut state = self.lock();
        state.turn = Some(id);
        id
    }

    fn set_runnable(&self, id: u64, value: bool) {
        let mut state = self.lock();
        if let Some(thread) = state.states.get_mut(&id) {
            thread.runnable = value;
        }
    }

    fn me(&self, state: &SchedState) -> Option<u64> {
        state.ids.get(&thread::current().id()).copied()
    }

    fn choose(&self, state: &mut SchedState, _me: u64) -> Option<u64> {
        if state.deadlocked {
            return None;
        }
        let candidates: Vec<u64> = state
            .states
            .iter()
            .filter(|(id, thread)| **id != 0 && thread.runnable && !thread.done)
            .map(|(id, _)| *id)
            .collect();
        if candidates.is_empty() {
            // Advance the virtual clock to the nearest timeout.
            let mut nearest: Option<u64> = None;
            for thread in state.states.values() {
                if let Some(block) = &thread.blocked
                    && let Some(deadline) = block.deadline
                {
                    nearest = Some(nearest.map_or(deadline, |value| value.min(deadline)));
                }
            }
            if let Some(deadline) = nearest {
                state.now = state.now.max(deadline);
                for thread in state.states.values_mut() {
                    if let Some(block) = &thread.blocked
                        && block.deadline.is_some_and(|value| value <= state.now)
                    {
                        thread.wake = Some(ETIMEDOUT.0 as i32);
                        thread.blocked = None;
                        thread.runnable = true;
                    }
                }
                return self.choose(state, _me);
            }
            state.deadlocked = true;
            let dump: Vec<String> = state
                .states
                .iter()
                .map(|(id, t)| {
                    format!(
                        "{id}:r{} d{} b{:?}",
                        t.runnable,
                        t.done,
                        t.blocked.as_ref().and_then(|b| b.futex)
                    )
                })
                .collect();
            state.trace.push(format!(
                "deadlock at step {} (seed {}) [{}]",
                state.steps,
                state.seed,
                dump.join(",")
            ));
            for id in state.states.keys() {
                if let Some(gate) = state.gates.get(id) {
                    *gate.ready.lock().unwrap_or_else(|e| e.into_inner()) = true;
                    gate.cv.notify_all();
                }
            }
            return None;
        }
        let index = state.rng.below(candidates.len());
        Some(candidates[index])
    }

    fn wake_gate(state: &SchedState, id: u64) {
        if let Some(gate) = state.gates.get(&id) {
            *gate.ready.lock().unwrap_or_else(|e| e.into_inner()) = true;
            gate.cv.notify_all();
        }
    }

    fn wait_gate(&self, id: u64) {
        loop {
            let (gate, turn) = {
                let state = self.lock();
                (state.gates.get(&id).cloned(), state.turn)
            };
            if turn == Some(id) {
                return;
            }
            let Some(gate) = gate else {
                return;
            };
            let mut ready = gate.ready.lock().unwrap_or_else(|e| e.into_inner());
            while !*ready {
                ready = gate.cv.wait(ready).unwrap_or_else(|e| e.into_inner());
            }
            *ready = false;
        }
    }

    /// Hand the baton over at `point` and wait until this thread is chosen again.
    fn hand_over(&self, point: &str) -> Result<(), Errno> {
        let me = {
            let mut state = self.lock();
            let Some(me) = self.me(&state) else {
                return Ok(());
            };
            state.steps = state.steps.wrapping_add(1);
            let steps = state.steps;
            state.trace.push(format!("{steps}:{me}:{point}"));
            if let Some(thread) = state.states.get_mut(&me) {
                thread.runnable = thread.blocked.is_none();
            }
            // A non-blocking hand-over can always continue on the calling thread; this also
            // recovers from a thread that still carries a stale blocked flag after a wake.
            let next = self.choose(&mut state, me).unwrap_or(me);
            state.turn = Some(next);
            if next == me {
                if let Some(thread) = state.states.get_mut(&me)
                    && thread.wake.is_some()
                {
                    thread.blocked = None;
                }
                if let Some(gate) = state.gates.get(&me) {
                    *gate.ready.lock().unwrap_or_else(|e| e.into_inner()) = false;
                }
            } else {
                Self::wake_gate(&state, next);
            }
            me
        };
        self.wait_gate(me);
        let mut state = self.lock();
        if state.deadlocked {
            let trace = state.trace.join("\n");
            drop(state);
            panic!("scheduler deadlock: {trace}");
        }
        state
            .states
            .get_mut(&me)
            .and_then(|thread| thread.wake.take())
            .map_or(Ok(()), |error| Err(Errno(error as u16)))
    }

    fn block(&self, futex: Option<usize>, deadline: Option<u64>) -> Result<(), Errno> {
        let me = {
            let mut state = self.lock();
            let Some(me) = self.me(&state) else {
                return Ok(());
            };
            if let Some(thread) = state.states.get_mut(&me) {
                thread.blocked = Some(Block { futex, deadline });
                thread.runnable = false;
            }
            // A non-blocking hand-over can always continue on the calling thread; this also
            // recovers from a thread that still carries a stale blocked flag after a wake.
            let next = self.choose(&mut state, me).unwrap_or(me);
            state.turn = Some(next);
            if next == me {
                if let Some(thread) = state.states.get_mut(&me)
                    && thread.wake.is_some()
                {
                    thread.blocked = None;
                }
                if let Some(gate) = state.gates.get(&me) {
                    *gate.ready.lock().unwrap_or_else(|e| e.into_inner()) = false;
                }
            } else {
                Self::wake_gate(&state, next);
            }
            me
        };
        self.wait_gate(me);
        let mut state = self.lock();
        if state.deadlocked {
            let trace = state.trace.join("\n");
            drop(state);
            panic!("scheduler deadlock: {trace}");
        }
        state
            .states
            .get_mut(&me)
            .and_then(|thread| thread.wake.take())
            .map_or(Ok(()), |error| Err(Errno(error as u16)))
    }

    fn finish(&self) {
        let mut state = self.lock();
        let Some(me) = self.me(&state) else {
            return;
        };
        if let Some(thread) = state.states.get_mut(&me) {
            thread.done = true;
            thread.runnable = false;
            thread.blocked = None;
        }
        if let Some(next) = self.choose(&mut state, me) {
            state.turn = Some(next);
            Self::wake_gate(&state, next);
        }
    }

    /// The number of scheduled hand-overs so far.
    #[must_use]
    pub fn steps(&self) -> u64 {
        self.lock().steps
    }

    /// A copy of the trace.
    #[must_use]
    pub fn trace(&self) -> Vec<String> {
        self.lock().trace.clone()
    }

    /// Whether the run deadlocked.
    #[must_use]
    pub fn deadlocked(&self) -> bool {
        self.lock().deadlocked
    }
}

struct Region {
    data: Box<[u128]>,
    guard: Option<(usize, usize)>,
    owner: Option<u64>,
}

struct KernelState {
    regions: HashMap<usize, Region>,
    dead: HashSet<usize>,
    tcb_by_id: HashMap<u64, usize>,
    id_by_tcb: HashMap<usize, u64>,
    handles: HashMap<u32, u64>,
    start_allowed: HashSet<u64>,
    violations: Vec<String>,
    next_handle: u32,
    next_tid: i32,
}

struct Inner {
    caps: KernelCaps,
    sched: Sched,
    state: Mutex<KernelState>,
    start_cv: Condvar,
    tp: AtomicUsize,
}

thread_local! {
    static HOST_TP: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The host fake kernel.
#[derive(Clone)]
pub struct HostKernel {
    inner: Arc<Inner>,
}

impl HostKernel {
    /// A kernel with `caps` and a scheduler seeded by `seed`.
    #[must_use]
    pub fn new(caps: KernelCaps, seed: u64) -> Self {
        Self {
            inner: Arc::new(Inner {
                caps,
                sched: Sched::new(seed),
                start_cv: Condvar::new(),
                state: Mutex::new(KernelState {
                    regions: HashMap::new(),
                    dead: HashSet::new(),
                    tcb_by_id: HashMap::new(),
                    id_by_tcb: HashMap::new(),
                    handles: HashMap::new(),
                    start_allowed: HashSet::new(),
                    violations: Vec::new(),
                    next_handle: 1,
                    next_tid: 1000,
                }),
                tp: AtomicUsize::new(0),
            }),
        }
    }

    /// Register the calling thread as the main thread and install `tp`.
    pub fn init_main(&self, tp: usize) {
        self.inner.tp.store(tp, Ordering::Release);
        HOST_TP.with(|cell| cell.set(tp));
        let id = self.inner.sched.init_main();
        let mut state = self.lock();
        state
            .tcb_by_id
            .insert(id, raven_libc_tls::tcb::tp_to_pthread(tp));
        state
            .id_by_tcb
            .insert(raven_libc_tls::tcb::tp_to_pthread(tp), id);
    }

    fn lock(&self) -> MutexGuard<'_, KernelState> {
        match self.inner.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Record a violation; the A-thr-3 test asserts this is empty.
    pub fn violations(&self) -> Vec<String> {
        self.lock().violations.clone()
    }

    /// Whether a TCB is on the kernel's dead set.
    #[must_use]
    pub fn is_dead(&self, tcb: usize) -> bool {
        self.lock().dead.contains(&tcb)
    }

    /// The scheduler.
    #[must_use]
    pub fn sched(&self) -> &Sched {
        &self.inner.sched
    }

    /// Number of live regions in the ledger.
    #[must_use]
    pub fn mapped_regions(&self) -> usize {
        self.lock().regions.len()
    }

    /// A page-rounded mapping length.
    #[must_use]
    pub fn page_round(len: usize) -> usize {
        (len.wrapping_add(4095)) & !4095
    }

    fn region_for(&self, address: usize) -> Option<usize> {
        let state = self.lock();
        for (base, region) in &state.regions {
            let end = base.wrapping_add(region.data.len().saturating_mul(16));
            if address >= *base && address < end {
                return Some(*base);
            }
        }
        None
    }

    /// Mark the region containing `stack_top` as owned by the child registered as `id`.
    fn own_stack(&self, id: u64, stack_top: usize) {
        if let Some(base) = self.region_for(stack_top) {
            let mut state = self.lock();
            if let Some(region) = state.regions.get_mut(&base) {
                region.owner = Some(id);
            }
        }
    }
}

impl ThreadKernel for HostKernel {
    fn caps(&self) -> KernelCaps {
        self.inner.caps
    }

    fn thread_pointer(&self) -> usize {
        HOST_TP.with(|cell| cell.get())
    }

    fn set_thread_pointer(&self, tp: usize) -> Result<(), Errno> {
        HOST_TP.with(|cell| cell.set(tp));
        self.inner.tp.store(tp, Ordering::Release);
        Ok(())
    }

    fn tid_self(&self) -> Result<i32, Errno> {
        if !self.inner.caps.tid {
            return Err(ENOSYS);
        }
        let mut state = self.lock();
        state.next_tid = state.next_tid.wrapping_add(1);
        Ok(state.next_tid)
    }

    fn create(&self, args: &CreateArgs) -> Result<u32, Errno> {
        let handle = {
            let mut state = self.lock();
            let handle = state.next_handle;
            state.next_handle = state.next_handle.wrapping_add(1);
            handle
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let child = HostKernel {
            inner: Arc::clone(&self.inner),
        };
        let arg = args.arg;
        let tp = args.tp;
        thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || {
                let id = child.inner.sched.register();
                child.inner.sched.set_runnable(id, false);
                child.inner.tp.store(tp, Ordering::Release);
                HOST_TP.with(|cell| cell.set(tp));
                let tcb = raven_libc_tls::tcb::tp_to_pthread(tp);
                {
                    let mut state = child.lock();
                    state.tcb_by_id.insert(id, tcb);
                    state.id_by_tcb.insert(tcb, id);
                }
                let _ = sender.send(id);
                {
                    let mut state = child.lock();
                    while !state.start_allowed.contains(&id) {
                        state = child
                            .inner
                            .start_cv
                            .wait(state)
                            .unwrap_or_else(|e| e.into_inner());
                    }
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    raven_libc_thread::thread::create::thread_start(&child, arg);
                }));
                match result {
                    Ok(()) | Err(_) => {}
                }
            })
            .map_err(|_| EAGAIN)?;
        let id = receiver.recv().map_err(|_| EAGAIN)?;
        let mut state = self.lock();
        state.handles.insert(handle, id);
        drop(state);
        self.own_stack(id, args.stack_top);
        let _ = self.inner.sched.hand_over("create_return");
        {
            let mut state = self.lock();
            state.start_allowed.insert(id);
            self.inner.start_cv.notify_all();
        }
        self.inner.sched.set_runnable(id, true);
        Ok(handle)
    }

    fn exit(&self, code: i32) -> ! {
        let tcb = raven_libc_tls::tcb::tp_to_pthread(self.thread_pointer());
        {
            let mut state = self.lock();
            state.dead.insert(tcb);
        }
        self.inner.sched.finish();
        std::panic::resume_unwind(Box::new(HostExit(code)))
    }

    fn exit_ex(&self, code: i32, clear_addr: usize, unmap_addr: usize, unmap_len: usize) -> ! {
        raw::write_u32(clear_addr, 0);
        let _ = self.unmap(unmap_addr, unmap_len);
        self.exit(code)
    }

    fn futex_wait(
        &self,
        word: &AtomicU32,
        expected: u32,
        rel_ns: Option<u64>,
    ) -> Result<(), Errno> {
        if word.load(Ordering::Acquire) != expected {
            return Err(EAGAIN);
        }
        // A real kernel sleeps forever without a timeout. The fake has no independent timer
        // thread, so a missed wakeup would park the caller with no runnable peer; a bounded
        // virtual poll interval makes the wait return a spurious success that every caller
        // re-tests its word after.
        let (deadline, spurious) = match rel_ns {
            Some(rel) => {
                let now = self.inner.sched.lock().now;
                (now.saturating_add(rel), false)
            }
            None => {
                let now = self.inner.sched.lock().now;
                (now.saturating_add(1_000_000), true)
            }
        };
        let result = self
            .inner
            .sched
            .block(Some(word as *const AtomicU32 as usize), Some(deadline));
        if spurious && result == Err(ETIMEDOUT) {
            Ok(())
        } else {
            result
        }
    }

    fn futex_wake(&self, word: &AtomicU32, n: u32) -> u32 {
        let address = word as *const AtomicU32 as usize;
        let mut state = self.inner.sched.lock();
        let mut count = 0u32;
        let limit = if n == 0 || n == u32::MAX { u32::MAX } else { n };
        for thread in state.states.values_mut() {
            if count >= limit {
                break;
            }
            if let Some(block) = &thread.blocked
                && block.futex == Some(address)
            {
                thread.wake = Some(0);
                thread.blocked = None;
                thread.runnable = true;
                count += 1;
            }
        }
        count
    }

    fn futex_requeue(
        &self,
        word: &AtomicU32,
        wake: u32,
        to: &AtomicU32,
        max: u32,
    ) -> Result<u32, Errno> {
        if !self.inner.caps.requeue {
            return Err(ENOSYS);
        }
        let from = word as *const AtomicU32 as usize;
        let target = to as *const AtomicU32 as usize;
        let mut state = self.inner.sched.lock();
        let mut woken = 0u32;
        let mut moved = 0u32;
        for thread in state.states.values_mut() {
            let blocked = thread
                .blocked
                .as_ref()
                .map(|block| (block.futex, block.deadline));
            if let Some((Some(address), deadline)) = blocked
                && address == from
            {
                if woken < wake {
                    thread.wake = Some(0);
                    thread.blocked = None;
                    thread.runnable = true;
                    woken += 1;
                } else if moved < max {
                    thread.blocked = Some(Block {
                        futex: Some(target),
                        deadline,
                    });
                    moved += 1;
                }
            }
        }
        Ok(woken.wrapping_add(moved))
    }

    fn map(&self, len: usize) -> Result<usize, Errno> {
        let length = Self::page_round(len);
        if length == 0 {
            return Err(EINVAL);
        }
        let words = length / 16;
        let data = vec![0u128; words].into_boxed_slice();
        let base = data.as_ptr() as usize;
        let mut state = self.lock();
        state.regions.insert(
            base,
            Region {
                data,
                guard: None,
                owner: None,
            },
        );
        Ok(base)
    }

    fn unmap(&self, addr: usize, len: usize) -> Result<(), Errno> {
        let mut state = self.lock();
        let Some(region) = state.regions.get(&addr) else {
            state
                .violations
                .push(format!("unmap of unknown range {addr:#x}+{len:#x}"));
            return Err(EINVAL);
        };
        let region_len = region.data.len().saturating_mul(16);
        if len > region_len {
            state.violations.push(format!(
                "unmap of {addr:#x}+{len:#x} larger than the region"
            ));
            return Err(EINVAL);
        }
        if let Some(owner) = region.owner {
            let me = state
                .id_by_tcb
                .get(&raven_libc_tls::tcb::tp_to_pthread(self.thread_pointer()))
                .copied();
            let owner_tcb = state.tcb_by_id.get(&owner).copied().unwrap_or(0);
            if Some(owner) != me && !state.dead.contains(&owner_tcb) {
                state.violations.push(format!(
                    "A-thr-3: stack {addr:#x} unmapped while thread {owner} is alive"
                ));
                // Keep the mapping: the test must be able to observe the violation without
                // unmapping memory that the live thread is still using.
                return Ok(());
            }
        }
        state.regions.remove(&addr);
        Ok(())
    }

    fn protect_none(&self, addr: usize, len: usize) -> Result<(), Errno> {
        let mut state = self.lock();
        let Some(region) = state.regions.get_mut(&addr) else {
            return Err(EINVAL);
        };
        region.guard = Some((addr, len));
        Ok(())
    }

    fn kick(&self, handle: u32, _bits: u64) -> Result<(), Errno> {
        if !self.inner.caps.kick {
            return Err(ENOSYS);
        }
        let state = self.lock();
        let Some(id) = state.handles.get(&handle).copied() else {
            return Err(EINVAL);
        };
        drop(state);
        let mut state = self.inner.sched.lock();
        if let Some(thread) = state.states.get_mut(&id)
            && thread.blocked.is_some()
        {
            thread.wake = Some(EINTR.0 as i32);
            thread.blocked = None;
            thread.runnable = true;
        }
        Ok(())
    }

    fn yield_now(&self) {
        let _ = self.inner.sched.hand_over("yield");
    }

    fn now_ns(&self, _clock: u32) -> u64 {
        self.inner.sched.lock().now
    }

    fn set_robust_list(&self, _head: usize) -> Result<(), Errno> {
        if self.inner.caps.robust {
            Ok(())
        } else {
            Err(ENOSYS)
        }
    }

    fn thread_is_dead(&self, tcb: usize) -> bool {
        self.is_dead(tcb)
    }

    fn fork_process(&self) -> Result<i32, Errno> {
        if self.inner.caps.proc_fork {
            Ok(0)
        } else {
            Err(ENOSYS)
        }
    }

    fn start_trampoline(&self) -> fn(usize) -> ! {
        |_| panic!("the host kernel calls thread_start directly")
    }

    fn sched_point(&self, name: &'static str) {
        let _ = self.inner.sched.hand_over(name);
    }
}

struct HostExit(i32);

/// A TLS image static for the host tests.
pub static TLS_IMAGE: [u8; 64] = [0x5a; 64];

/// Initialise the main thread with one 64-byte TLS module.
pub fn init_main(kernel: &HostKernel, stack_base: usize, stack_size: usize) -> usize {
    let modules = [TlsModuleIn::new(TLS_IMAGE.as_ptr() as usize, 64, 64, 16)];
    let tcb = raven_libc_thread::thread::create::init_main_thread(
        kernel, &modules, stack_base, stack_size,
    )
    .expect("main thread");
    kernel.init_main(raven_libc_tls::tcb::pthread_to_tp(tcb));
    tcb
}
