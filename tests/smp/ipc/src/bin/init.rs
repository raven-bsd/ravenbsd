// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S85c SMP IPC stress fixture.
//!
//! Runs on a four-CPU QEMU guest. Two independent channel pairs exercise multi-producer and
//! multi-consumer call/reply traffic (one producer and one consumer per channel, four threads in
//! parallel), including cancel and timeout completions. Four independent `raven-ring` SPSC pairs
//! wrap a 64-slot ring twice per round. Every channel payload carries a `raven-trace-core` trace
//! id; the consumer adopts it as a span, records one event and closes the span.
//!
//! The ring crate is single-producer/single-consumer by contract; the multi-producer/consumer
//! coverage is the four concurrent ring pairs and the two channel pairs, not an MPMC ring.

#![no_std]
#![no_main]

extern crate alloc;

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use alloc::boxed::Box;
use alloc::vec::Vec;

use raven_abi::types::Handle;
use raven_chan::{Channel, Completion, Endpoint, Limits};
use raven_ring::{Descriptor, Error as RingError, Ring};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sync::Mutex;
use raven_rt::sys;
use raven_trace_core::{SpanId, TraceContext, TraceId, TraceIdGen, Tracer};

entry!(main);

const CHANNEL_PAIRS: usize = 2;
const RING_PAIRS: usize = 4;
const CHANNEL_THREADS: usize = CHANNEL_PAIRS * 2;
const RING_THREADS: usize = RING_PAIRS * 2;
const THREADS: usize = CHANNEL_THREADS + RING_THREADS;
const CHANNEL_ROUNDS: usize = 50;
const CHANNEL_OPS: usize = 128;
const RING_ROUNDS: usize = 50;
const RING_OPS: usize = 128;
const RING_CAP: usize = 64;
const STACK_SIZE: usize = 64 * 1024;
const REPLY_XOR: u64 = 0x5a5a_5a5a_5a5a_5a5a;
const RING_MAGIC: u64 = 0x1357_9bdf_2468_ace0;
const WAIT_LIMIT_NS: u64 = 90_000_000_000;

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

static mut STACKS: [Stack; THREADS] = [const { Stack([0; STACK_SIZE]) }; THREADS];

static DONE: [AtomicBool; THREADS] = [const { AtomicBool::new(false) }; THREADS];
static FAILED: AtomicBool = AtomicBool::new(false);
static NEXT_TXID: AtomicU64 = AtomicU64::new(1);
static CHANNELS: [Mutex<Option<Box<Channel>>>; CHANNEL_PAIRS] =
    [const { Mutex::new(None) }; CHANNEL_PAIRS];
static CH_PRODUCERS_DONE: AtomicU32 = AtomicU32::new(0);

static CHANNEL_CALLS: AtomicU64 = AtomicU64::new(0);
static CHANNEL_REPLIES: AtomicU64 = AtomicU64::new(0);
static CHANNEL_CANCELLED: AtomicU64 = AtomicU64::new(0);
static CHANNEL_TIMEDOUT: AtomicU64 = AtomicU64::new(0);
static CHANNEL_TRACES: AtomicU64 = AtomicU64::new(0);
static CHANNEL_BACKPRESSURE: AtomicU64 = AtomicU64::new(0);
static CH_P90: [AtomicU64; CHANNEL_PAIRS] = [const { AtomicU64::new(0) }; CHANNEL_PAIRS];
static CH_WORST: [AtomicU64; CHANNEL_PAIRS] = [const { AtomicU64::new(0) }; CHANNEL_PAIRS];
static CH_SAMPLES: [AtomicU64; CHANNEL_PAIRS] = [const { AtomicU64::new(0) }; CHANNEL_PAIRS];

static RING_PTRS: [AtomicUsize; RING_PAIRS] = [const { AtomicUsize::new(0) }; RING_PAIRS];
static RING_SLOTS: [[AtomicU64; RING_CAP]; RING_PAIRS] = [
    [const { AtomicU64::new(0) }; RING_CAP],
    [const { AtomicU64::new(0) }; RING_CAP],
    [const { AtomicU64::new(0) }; RING_CAP],
    [const { AtomicU64::new(0) }; RING_CAP],
];
static RING_DESCRIPTORS: AtomicU64 = AtomicU64::new(0);
static RING_WRAPS: AtomicU64 = AtomicU64::new(0);
static RING_RETRIES: AtomicU64 = AtomicU64::new(0);

fn stack_top(index: usize) -> usize {
    // SAFETY: only the address is taken; the stack memory is never referenced as a Rust value.
    let base = core::ptr::addr_of_mut!(STACKS) as *mut Stack;
    // SAFETY: `index < THREADS` and the array lives for the whole test; the result is only used
    // as a thread stack pointer, never dereferenced.
    let slot = unsafe { base.add(index) };
    (slot as usize).saturating_add(STACK_SIZE)
}

fn now_ns() -> u64 {
    match sys::clock_get(1) {
        Ok(time) => (time.tv_sec.max(0) as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add(time.tv_nsec.max(0) as u64),
        Err(_) => 0,
    }
}

fn worker_fail(message: &str) -> ! {
    println!("smp-ipc: failed {message}");
    FAILED.store(true, Ordering::Release);
    sys::thread_exit(1)
}

fn fail(message: &str) -> i32 {
    println!("smp-ipc: failed {message}");
    1
}

fn ring(pair: usize) -> &'static Ring<'static> {
    let address = RING_PTRS[pair].load(Ordering::Acquire);
    // SAFETY: `main` publishes a leaked `Ring` over a `static` slot array before any worker
    // starts, and the pointer is never cleared. `Ring` is Sync and the pair has one producer and
    // one consumer, which is its contract.
    unsafe { &*(address as *const Ring<'static>) }
}

fn ring_cancel_check() -> Result<(), &'static str> {
    let slots: [AtomicU64; 8] = [const { AtomicU64::new(0) }; 8];
    let ring = Ring::new(&slots).map_err(|_| "ring-cancel capacity")?;
    for value in [0x11u64, 0x22, 0x33] {
        ring.submit(Descriptor::new(value))
            .map_err(|_| "ring-cancel submit")?;
    }
    if ring.cancel(0x22) != 1 {
        return Err("ring-cancel count");
    }
    let first = ring.complete().ok_or("ring-cancel first")?;
    let second = ring.complete().ok_or("ring-cancel second")?;
    let third = ring.complete().ok_or("ring-cancel third")?;
    if first.value() != 0x11
        || !second.is_cancelled()
        || second.value() != 0x22
        || third.value() != 0x33
    {
        return Err("ring-cancel order");
    }
    Ok(())
}

fn drain_channel(
    pair: usize,
    pending: &mut Vec<(u64, u64, u64)>,
    cancelling: &mut Vec<(u64, bool)>,
    latencies: &mut Vec<u64>,
) {
    loop {
        let completion = {
            let mut guard = CHANNELS[pair].lock();
            match guard.as_mut() {
                Some(channel) => channel.pop_completion(Endpoint::A),
                None => None,
            }
        };
        let Some(completion) = completion else {
            break;
        };
        match completion {
            Completion::Replied { txid, payload_id } => {
                let Some(position) = pending.iter().position(|(id, _, _)| *id == txid) else {
                    worker_fail("channel reply without pending call");
                };
                let (_, payload, started) = pending.remove(position);
                if payload_id != payload ^ REPLY_XOR {
                    worker_fail("channel reply payload");
                }
                latencies.push(now_ns().saturating_sub(started));
                CHANNEL_REPLIES.fetch_add(1, Ordering::Relaxed);
            }
            Completion::Cancelled { txid } => {
                let Some(position) = cancelling
                    .iter()
                    .position(|(id, timeout)| *id == txid && !*timeout)
                else {
                    worker_fail("channel unexpected cancellation");
                };
                cancelling.remove(position);
                CHANNEL_CANCELLED.fetch_add(1, Ordering::Relaxed);
            }
            Completion::TimedOut { txid } => {
                let Some(position) = cancelling
                    .iter()
                    .position(|(id, timeout)| *id == txid && *timeout)
                else {
                    worker_fail("channel unexpected timeout");
                };
                cancelling.remove(position);
                CHANNEL_TIMEDOUT.fetch_add(1, Ordering::Relaxed);
            }
            Completion::PeerClosed { .. } | Completion::PeerDied { .. } => {
                worker_fail("channel peer closed");
            }
        }
    }
}

fn channel_producer(pair: usize) -> ! {
    let mut generator = TraceIdGen::new(0x5a5a, pair as u8);
    let mut pending: Vec<(u64, u64, u64)> = Vec::new();
    let mut cancelling: Vec<(u64, bool)> = Vec::new();
    let mut latencies: Vec<u64> = Vec::new();

    for _round in 0..CHANNEL_ROUNDS {
        for op in 0..CHANNEL_OPS {
            let txid = NEXT_TXID.fetch_add(1, Ordering::AcqRel);
            let payload = generator.next_id().raw();
            let started = now_ns();
            let cancel = op % 17 == 0;
            let timeout = (op / 17) % 2 == 1;
            {
                loop {
                    let outcome = {
                        let mut guard = CHANNELS[pair].lock();
                        let Some(channel) = guard.as_mut() else {
                            worker_fail("channel missing");
                        };
                        match channel.call(Endpoint::A, txid, payload) {
                            Ok(()) => {
                                if cancel {
                                    let result = if timeout {
                                        channel.timeout(Endpoint::A, txid)
                                    } else {
                                        channel.cancel(Endpoint::A, txid)
                                    };
                                    if result.is_err() {
                                        worker_fail("channel cancel");
                                    }
                                    cancelling.push((txid, timeout));
                                }
                                Ok(())
                            }
                            Err(error) => Err(error),
                        }
                    };
                    match outcome {
                        Ok(()) => break,
                        Err(raven_chan::Error::QueueFull)
                        | Err(raven_chan::Error::InFlightFull) => {
                            CHANNEL_BACKPRESSURE.fetch_add(1, Ordering::Relaxed);
                            drain_channel(pair, &mut pending, &mut cancelling, &mut latencies);
                            sys::yield_now();
                        }
                        Err(error) => {
                            println!(
                                "smp-ipc: channel call failed pair={pair} txid={txid} err={}",
                                error.as_str()
                            );
                            worker_fail("channel call");
                        }
                    }
                }
                CHANNEL_CALLS.fetch_add(1, Ordering::Relaxed);
            }
            if !cancel {
                pending.push((txid, payload, started));
            }
            drain_channel(pair, &mut pending, &mut cancelling, &mut latencies);
        }
    }

    while !pending.is_empty() || !cancelling.is_empty() {
        drain_channel(pair, &mut pending, &mut cancelling, &mut latencies);
        if !pending.is_empty() || !cancelling.is_empty() {
            sys::yield_now();
        }
    }

    latencies.sort_unstable();
    let samples = latencies.len() as u64;
    let p90 = latencies
        .get((latencies.len().saturating_mul(9)) / 10)
        .copied()
        .unwrap_or(0);
    let worst = latencies.last().copied().unwrap_or(0);
    CH_P90[pair].store(p90, Ordering::Release);
    CH_WORST[pair].store(worst, Ordering::Release);
    CH_SAMPLES[pair].store(samples, Ordering::Release);
    CH_PRODUCERS_DONE.fetch_add(1, Ordering::AcqRel);
    DONE[pair].store(true, Ordering::Release);
    sys::thread_exit(0)
}

fn channel_consumer(pair: usize) -> ! {
    let mut tracer = Tracer::with_capacity(4096, 16);
    loop {
        let (received, producers_done) = {
            let mut guard = CHANNELS[pair].lock();
            let Some(channel) = guard.as_mut() else {
                worker_fail("channel missing");
            };
            (
                channel.recv(Endpoint::B),
                CH_PRODUCERS_DONE.load(Ordering::Acquire),
            )
        };
        match received {
            Ok(call) => {
                if tracer.len() >= 4000 {
                    // `Trace` keeps closed spans in its bounded registry; recycle the registry
                    // before the 4096-span cap while the test runs its long stress loop.
                    tracer = Tracer::with_capacity(4096, 16);
                }
                let context =
                    TraceContext::new(TraceId::from_raw(call.payload_id), SpanId::NONE, true);
                let Some(span) = tracer.start_span(context) else {
                    println!(
                        "smp-ipc: trace start failed pair={pair} txid={} payload={:#x}",
                        call.txid, call.payload_id
                    );
                    worker_fail("trace start");
                };
                if !tracer.record(span, call.txid) || !tracer.close(span) {
                    worker_fail("trace record");
                }
                CHANNEL_TRACES.fetch_add(1, Ordering::Relaxed);
                let mut guard = CHANNELS[pair].lock();
                let Some(channel) = guard.as_mut() else {
                    worker_fail("channel missing");
                };
                if channel
                    .reply(Endpoint::B, call.txid, call.payload_id ^ REPLY_XOR)
                    .is_err()
                {
                    worker_fail("channel reply");
                }
            }
            Err(raven_chan::Error::WouldBlock) => {
                if producers_done == CHANNEL_PAIRS as u32 {
                    break;
                }
                sys::yield_now();
            }
            Err(_) => worker_fail("channel recv"),
        }
    }
    DONE[CHANNEL_PAIRS + pair].store(true, Ordering::Release);
    sys::thread_exit(0)
}

fn ring_producer(pair: usize) -> ! {
    let ring = ring(pair);
    let mut retries = 0u64;
    for round in 0..RING_ROUNDS {
        for op in 0..RING_OPS {
            let value = ((round * RING_OPS + op) as u64) ^ RING_MAGIC;
            let descriptor = Descriptor::new(value);
            loop {
                match ring.submit(descriptor) {
                    Ok(()) => break,
                    Err(RingError::Full) => {
                        retries = retries.saturating_add(1);
                        sys::yield_now();
                    }
                    Err(_) => worker_fail("ring submit"),
                }
            }
            RING_DESCRIPTORS.fetch_add(1, Ordering::Relaxed);
        }
    }
    RING_RETRIES.fetch_add(retries, Ordering::Relaxed);
    RING_WRAPS.fetch_add(
        ((RING_ROUNDS * RING_OPS) / RING_CAP) as u64,
        Ordering::Relaxed,
    );
    DONE[CHANNEL_THREADS + pair].store(true, Ordering::Release);
    sys::thread_exit(0)
}

fn ring_consumer(pair: usize) -> ! {
    let ring = ring(pair);
    let total = RING_ROUNDS * RING_OPS;
    let mut count = 0usize;
    while count < total {
        match ring.complete() {
            Some(descriptor) => {
                let expected = (count as u64) ^ RING_MAGIC;
                if descriptor.value() != expected || descriptor.is_cancelled() {
                    worker_fail("ring descriptor");
                }
                count = count.saturating_add(1);
            }
            None => sys::yield_now(),
        }
    }
    DONE[CHANNEL_THREADS + RING_PAIRS + pair].store(true, Ordering::Release);
    sys::thread_exit(0)
}

extern "C" fn worker(index: usize) -> ! {
    match index {
        0..=1 => channel_producer(index),
        2..=3 => channel_consumer(index - CHANNEL_PAIRS),
        4..=7 => ring_producer(index - CHANNEL_THREADS),
        8..=11 => ring_consumer(index - CHANNEL_THREADS - RING_PAIRS),
        _ => worker_fail("bad thread index"),
    }
}

fn wait_all() -> bool {
    let started = now_ns();
    loop {
        if FAILED.load(Ordering::Acquire) {
            return false;
        }
        if DONE.iter().all(|done| done.load(Ordering::Acquire)) {
            return true;
        }
        if now_ns().saturating_sub(started) > WAIT_LIMIT_NS {
            return false;
        }
        let _ = sys::nanosleep(1_000_000);
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    println!(
        "smp-ipc: start rounds={CHANNEL_ROUNDS} channel_pairs={CHANNEL_PAIRS} ring_pairs={RING_PAIRS}"
    );
    match ring_cancel_check() {
        Ok(()) => println!("smp-ipc: ring-cancel ok"),
        Err(reason) => return fail(reason),
    }

    for pair in 0..CHANNEL_PAIRS {
        *CHANNELS[pair].lock() = Some(Box::new(Channel::with_limits(Limits::new(1024, 1024))));
    }
    for pair in 0..RING_PAIRS {
        let ring = match Ring::new(&RING_SLOTS[pair]) {
            Ok(ring) => Box::leak(Box::new(ring)),
            Err(_) => return fail("ring init"),
        };
        RING_PTRS[pair].store(ring as *const Ring<'static> as usize, Ordering::Release);
    }

    for index in 0..THREADS {
        if let Err(error) = sys::thread_create(
            Handle::SELF_PROC,
            worker as *const () as usize,
            stack_top(index),
            index,
        ) {
            println!("smp-ipc: thread {index} create errno={}", error.0);
            return fail("thread create");
        }
    }

    if !wait_all() {
        return fail("worker timeout");
    }

    let p90 = CH_P90
        .iter()
        .map(|value| value.load(Ordering::Acquire))
        .max()
        .unwrap_or(0);
    let worst = CH_WORST
        .iter()
        .map(|value| value.load(Ordering::Acquire))
        .max()
        .unwrap_or(0);
    let samples: u64 = CH_SAMPLES
        .iter()
        .map(|value| value.load(Ordering::Acquire))
        .sum();
    println!(
        "smp-ipc: channel calls={} replies={} cancelled={} timedout={} backpressure={} traces={}",
        CHANNEL_CALLS.load(Ordering::Acquire),
        CHANNEL_REPLIES.load(Ordering::Acquire),
        CHANNEL_CANCELLED.load(Ordering::Acquire),
        CHANNEL_TIMEDOUT.load(Ordering::Acquire),
        CHANNEL_BACKPRESSURE.load(Ordering::Acquire),
        CHANNEL_TRACES.load(Ordering::Acquire)
    );
    println!("smp-ipc: channel latency p90_ns={p90} worst_ns={worst} samples={samples}");
    println!(
        "smp-ipc: ring descriptors={} wraps={} retries={}",
        RING_DESCRIPTORS.load(Ordering::Acquire),
        RING_WRAPS.load(Ordering::Acquire),
        RING_RETRIES.load(Ordering::Acquire)
    );
    if CHANNEL_REPLIES.load(Ordering::Acquire)
        + CHANNEL_CANCELLED.load(Ordering::Acquire)
        + CHANNEL_TIMEDOUT.load(Ordering::Acquire)
        != CHANNEL_CALLS.load(Ordering::Acquire)
    {
        return fail("channel completion count");
    }
    if CHANNEL_TRACES.load(Ordering::Acquire) != CHANNEL_REPLIES.load(Ordering::Acquire) {
        return fail("trace completion count");
    }
    println!("smp-ipc: done");
    0
}
