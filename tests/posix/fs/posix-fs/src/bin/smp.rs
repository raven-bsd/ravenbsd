// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S85e VFS/filesystem SMP stress fixture.
//!
//! Starts `initfsd`, `ramfsd` and `vfsd`, then runs four file workers and four mount workers
//! concurrently with `smp_user=on`. Every worker performs 50 iterations of create/write/read/
//! rename/unlink and mmap cycles on its own paths, plus a shared O_EXCL create race; the mount
//! workers run 50 mount/write/read/umount/remount/umount cycles each. Latency samples are kept
//! per worker/op/iteration and reduced to p50/p90/max by the controller. Any error is printed
//! once by the controller and makes `/init` exit non-zero.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, clock, prot};
use raven_initfs::Initfs;
use raven_proto_fs::{Call, CallRequest, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use raven_rt::entry;
use raven_rt::fs::{self, File};
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_rt::sys;
use raven_scheme::Client;

entry!(main);

const START: &str = "/tmp/smp-fs";
const FILE_WORKERS: usize = 4;
const MOUNT_WORKERS: usize = 4;
const WORKERS: usize = FILE_WORKERS + MOUNT_WORKERS;
const ITERATIONS: usize = 50;
const STACK_SIZE: usize = 128 * 1024;
/// Headroom below the slot top for the deepest call frame and its buffers.
const STACK_GUARD: usize = 16 * 1024;
const PAGE: usize = 4096;
const READ_WRITE: u32 = 3;
const MS_SYNC: usize = 4;
const DATA_LEN: usize = 512;

const OP_CREATE: usize = 0;
const OP_WRITE: usize = 1;
const OP_READ: usize = 2;
const OP_RENAME: usize = 3;
const OP_MMAP: usize = 4;
const OP_UNLINK: usize = 5;
const OP_COUNT: usize = 6;
const OP_NAMES: [&str; OP_COUNT] = ["create", "write", "read", "rename", "mmap", "unlink"];

static GO: AtomicU32 = AtomicU32::new(0);
static STARTED: [AtomicBool; WORKERS] = [const { AtomicBool::new(false) }; WORKERS];
static FINISHED: [AtomicBool; WORKERS] = [const { AtomicBool::new(false) }; WORKERS];
static FAIL_INFO: [AtomicUsize; WORKERS] = [const { AtomicUsize::new(0) }; WORKERS];
static CURRENT: [AtomicUsize; WORKERS] = [const { AtomicUsize::new(0) }; WORKERS];
static FAILURES: AtomicUsize = AtomicUsize::new(0);
static RACE_READY: [AtomicUsize; ITERATIONS] = [const { AtomicUsize::new(0) }; ITERATIONS];
static RACE_DONE: [AtomicUsize; ITERATIONS] = [const { AtomicUsize::new(0) }; ITERATIONS];
static RACE_SUCCESSES: AtomicUsize = AtomicUsize::new(0);
static MISMATCH_SEEN: AtomicBool = AtomicBool::new(false);
static MISMATCH: [AtomicU64; 3] = [const { AtomicU64::new(0) }; 3];
static PHASES: [AtomicUsize; WORKERS] = [const { AtomicUsize::new(0) }; WORKERS];
static LAT: [[[AtomicU64; ITERATIONS]; OP_COUNT]; FILE_WORKERS] =
    [const { [const { [const { AtomicU64::new(0) }; ITERATIONS] }; OP_COUNT] }; FILE_WORKERS];
static MOUNT_LAT: [[AtomicU64; ITERATIONS]; MOUNT_WORKERS] =
    [const { [const { AtomicU64::new(0) }; ITERATIONS] }; MOUNT_WORKERS];

#[repr(align(16))]
struct Stack(#[allow(dead_code)] [u8; STACK_SIZE]);

// `static mut` keeps the stacks in writable `.bss`; an immutable `static` would be placed in
// `.rodata`, where `THREAD_CREATE` rejects the stack as non-writable.
static mut STACKS: [Stack; WORKERS] = [const { Stack([0; STACK_SIZE]) }; WORKERS];

fn stack_top(index: usize) -> usize {
    // SAFETY: only the address is taken; the stack memory is never referenced as a Rust value.
    let base = core::ptr::addr_of_mut!(STACKS) as *mut Stack;
    // SAFETY: `index < WORKERS` and the array lives for the whole test, so `base.add(index)` stays
    // within the static; the result is only used as a thread stack pointer, never dereferenced.
    let slot = unsafe { base.add(index) };
    (slot as usize).saturating_add(STACK_SIZE.saturating_sub(STACK_GUARD))
}

fn now_ns() -> u64 {
    let time = raven_rt::clock_get(clock::MONOTONIC).unwrap_or_default();
    let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
    let nanos = u64::try_from(time.tv_nsec).unwrap_or(0);
    seconds.saturating_mul(1_000_000_000).saturating_add(nanos)
}

fn note_failure(worker: usize, phase: u16, error: Errno) {
    let iteration = CURRENT[worker].load(Ordering::Relaxed);
    let packed = (iteration << 32) | (usize::from(phase) << 16) | usize::from(error.0);
    let _ = FAIL_INFO[worker].compare_exchange(0, packed, Ordering::AcqRel, Ordering::Relaxed);
    FAILURES.fetch_add(1, Ordering::Relaxed);
}

fn note_mismatch(iteration: usize, got: &[u8]) {
    if MISMATCH_SEEN.swap(true, Ordering::AcqRel) {
        return;
    }
    MISMATCH[0].store(
        (iteration as u64) << 32 | got.len() as u64,
        Ordering::Release,
    );
    let mut first = [0u8; 8];
    let take = got.len().min(8);
    first[..take].copy_from_slice(&got[..take]);
    MISMATCH[1].store(u64::from_le_bytes(first), Ordering::Release);
    let mut second = [0u8; 8];
    if got.len() > 8 {
        let take = (got.len() - 8).min(8);
        second[..take].copy_from_slice(&got[8..8 + take]);
    }
    MISMATCH[2].store(u64::from_le_bytes(second), Ordering::Release);
}

fn first16(data: &[u8]) -> [u8; 16] {
    let mut first = [0u8; 16];
    let take = data.len().min(first.len());
    first[..take].copy_from_slice(&data[..take]);
    first
}

fn record(op: usize, worker: usize, iteration: usize, start: u64) {
    let elapsed = now_ns().saturating_sub(start);
    LAT[worker][op][iteration].store(elapsed, Ordering::Relaxed);
}

fn pattern(worker: usize, iteration: usize, len: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(len);
    for index in 0..len {
        let value = worker
            .wrapping_mul(31)
            .wrapping_add(iteration.wrapping_mul(17))
            .wrapping_add(index);
        data.push((value & 0xff) as u8);
    }
    data
}

fn file_path(path: &str) -> String {
    format!("file:{path}")
}

fn write_all(file: &mut File, data: &[u8]) -> Result<(), Errno> {
    let mut offset = 0usize;
    while offset < data.len() {
        let written = file.write(&data[offset..])?;
        if written == 0 {
            return Err(Errno::EIO);
        }
        offset = offset.saturating_add(written);
    }
    Ok(())
}

fn file_cycle(worker: usize, iteration: usize) -> Result<(), (u16, Errno)> {
    let path = format!("{START}/work/f{worker}_{iteration}");
    let renamed = format!("{START}/work/r{worker}_{iteration}");
    let data = pattern(worker, iteration, DATA_LEN);
    phase(worker, iteration, "create");

    let start = now_ns();
    let mut file = fs::open(&file_path(&path), O_RDWR | O_CREAT | O_TRUNC).map_err(|e| (1, e))?;
    record(OP_CREATE, worker, iteration, start);
    phase(worker, iteration, "created");

    let start = now_ns();
    write_all(&mut file, &data).map_err(|e| (2, e))?;
    record(OP_WRITE, worker, iteration, start);
    file.close().map_err(|e| (3, e))?;
    phase(worker, iteration, "written");

    let start = now_ns();
    let mut reader = fs::open(&file_path(&path), O_RDONLY).map_err(|e| (4, e))?;
    let contents = reader.read_to_vec().map_err(|e| (4, e))?;
    let _ = reader.close();
    record(OP_READ, worker, iteration, start);
    if contents != data {
        println!(
            "smp-fs: mismatch worker={worker} iter={iteration} phase=5 path={path} expected={:?} got={:?}",
            first16(&data),
            first16(&contents)
        );
        note_mismatch(iteration, &contents);
        return Err((5, Errno::EIO));
    }
    phase(worker, iteration, "read");

    let start = now_ns();
    fs::rename(&path, &renamed).map_err(|e| (6, e))?;
    record(OP_RENAME, worker, iteration, start);
    let renamed_data = fs::read_to_vec(&file_path(&renamed)).map_err(|e| (7, e))?;
    if renamed_data != data {
        println!(
            "smp-fs: mismatch worker={worker} iter={iteration} phase=8 path={renamed} expected={:?} got={:?}",
            first16(&data),
            first16(&renamed_data)
        );
        note_mismatch(iteration, &renamed_data);
        return Err((8, Errno::EIO));
    }
    phase(worker, iteration, "renamed");

    let start = now_ns();
    mmap_cycle(worker, iteration, &renamed).map_err(|e| (9, e))?;
    record(OP_MMAP, worker, iteration, start);
    phase(worker, iteration, "mapped");

    let start = now_ns();
    fs::unlink(&renamed).map_err(|e| (10, e))?;
    record(OP_UNLINK, worker, iteration, start);
    if fs::stat(&file_path(&renamed)) != Err(Errno::ENOENT) {
        return Err((11, Errno::EIO));
    }
    phase(worker, iteration, "unlinked");
    Ok(())
}

fn phase(worker: usize, _iteration: usize, name: &str) {
    let code = match name {
        "create" => 11,
        "created" => 12,
        "written" => 13,
        "read" => 14,
        "renamed" => 15,
        "mapped" => 16,
        "unlinked" => 17,
        "mmap file" => 20,
        "mmap attach" => 21,
        "mmap map" => 22,
        "mmap copy" => 23,
        "mmap sync" => 24,
        "mmap unmap" => 25,
        _ => 0,
    };
    set_phase(worker, code);
}

fn set_phase(slot: usize, code: usize) {
    if let Some(phase) = PHASES.get(slot) {
        phase.store(code, Ordering::Relaxed);
    }
}

fn mmap_cycle(worker: usize, iteration: usize, path: &str) -> Result<(), Errno> {
    let data = pattern(worker, iteration, PAGE);
    let mut file = fs::open(&file_path(path), O_RDWR | O_CREAT | O_TRUNC)?;
    write_all(&mut file, &data)?;
    file.close()?;
    phase(worker, iteration, "mmap file");

    let (object, source) = attach_mmap(path, 0, PAGE as u64, true)?;
    phase(worker, iteration, "mmap attach");
    let result = (|| {
        let address = sys::map_object(object, 0, PAGE, (prot::READ | prot::WRITE) as usize, true)?;
        phase(worker, iteration, "mmap map");
        let marker = [0x53u8, 0x4d, 0x50, 0x2d, 0x4d];
        sys::copy_to_mapping(address, &marker);
        phase(worker, iteration, "mmap copy");
        sys::msync(Handle::SELF_PROC, address, PAGE, MS_SYNC)?;
        phase(worker, iteration, "mmap sync");
        let contents = fs::read_to_vec(&file_path(path))?;
        sys::unmap(Handle::SELF_PROC, address, PAGE)?;
        phase(worker, iteration, "mmap unmap");
        if contents.get(..marker.len()) != Some(&marker) {
            return Err(Errno::EIO);
        }
        Ok(())
    })();
    // The pager keeps the source open-file description; close it only after the mapping is gone.
    let _ = Client::new().close(source);
    result
}

fn attach_mmap(path: &str, offset: u64, len: u64, shared: bool) -> Result<(Handle, Handle), Errno> {
    let client = Client::new();
    let handle = client.open(&format!("file:{path}"), READ_WRITE)?;
    let request = CallRequest::new(Call::Mmap {
        offset,
        len,
        shared,
    });
    let mut encoded = [0u8; 64];
    let encoded_len = request
        .encode(&mut encoded)
        .map_err(|error| error.errno())?;
    let mut response = [0u8; 8];
    let result = client.call(handle, &encoded[..encoded_len], &mut response);
    let count = match result {
        Ok(count) => count,
        Err(error) => {
            let _ = client.close(handle);
            return Err(error);
        }
    };
    if count == 4 {
        let status = i32::from_le_bytes(response[..4].try_into().map_err(|_| Errno::EPROTO)?);
        let _ = client.close(handle);
        return Err(errno_from_status(status));
    }
    if count < 8 {
        let _ = client.close(handle);
        return Err(Errno::EPROTO);
    }
    Ok((
        Handle(u32::from_le_bytes(
            response[..4].try_into().map_err(|_| Errno::EPROTO)?,
        )),
        handle,
    ))
}

fn errno_from_status(status: i32) -> Errno {
    if (-4095..0).contains(&status) {
        Errno(u16::try_from(-status).unwrap_or(Errno::EIO.0))
    } else {
        Errno::EIO
    }
}

fn race_barrier(counter: &AtomicUsize) {
    counter.fetch_add(1, Ordering::AcqRel);
    while counter.load(Ordering::Acquire) < FILE_WORKERS {
        let _ = sys::nanosleep(200_000);
    }
}

fn race_cycle(iteration: usize) -> Result<(), (u16, Errno)> {
    race_barrier(&RACE_READY[iteration]);
    let path = format!("{START}/race/shared_{iteration}");
    let mut won = false;
    let mut outcome = Ok(());
    match fs::open(&file_path(&path), O_RDWR | O_CREAT | O_EXCL) {
        Ok(file) => {
            RACE_SUCCESSES.fetch_add(1, Ordering::AcqRel);
            if let Err(error) = file.close() {
                outcome = Err((12, error));
            }
            won = true;
        }
        Err(Errno::EEXIST) => {}
        Err(error) => outcome = Err((13, error)),
    }
    race_barrier(&RACE_DONE[iteration]);
    if won && let Err(error) = fs::unlink(&path) {
        outcome = Err((14, error));
    }
    outcome
}

extern "C" fn file_worker(index: usize) -> ! {
    STARTED[index].store(true, Ordering::Release);
    while GO.load(Ordering::Acquire) == 0 {
        let _ = sys::nanosleep(200_000);
    }
    let mut failed = false;
    for iteration in 0..ITERATIONS {
        CURRENT[index].store(iteration, Ordering::Relaxed);
        if !failed && let Err((phase, error)) = file_cycle(index, iteration) {
            note_failure(index, phase, error);
            failed = true;
        }
        if failed {
            // Keep every worker in the barrier sequence after the first local failure.
            race_barrier(&RACE_READY[iteration]);
            race_barrier(&RACE_DONE[iteration]);
        } else {
            set_phase(index, 40);
            if let Err((phase, error)) = race_cycle(iteration) {
                note_failure(index, phase, error);
                failed = true;
            }
        }
    }
    FINISHED[index].store(true, Ordering::Release);
    sys::thread_exit(if failed { 1 } else { 0 });
}

fn mount_cycle(worker: usize, iteration: usize) -> Result<(), (u16, Errno)> {
    let mount = format!("{START}/mnt{worker}");
    // Each mount worker owns a distinct file: all `ramfs:` mounts share one backing
    // filesystem in this build, so a shared `/probe` would test cross-worker overwrite
    // instead of mount correctness.
    let file = format!("{mount}/probe{worker}");
    let data = pattern(worker, iteration, DATA_LEN);
    let slot = FILE_WORKERS + worker;

    set_phase(slot, 30);
    fs::mount(&mount, "ramfs:").map_err(|e| (20, e))?;
    set_phase(slot, 31);
    write_file(&file, &data).map_err(|e| (21, e))?;
    set_phase(slot, 32);
    let contents = fs::read_to_vec(&file_path(&file)).map_err(|e| (22, e))?;
    if contents != data {
        println!(
            "smp-fs: mismatch worker={slot} iter={iteration} phase=23 path={file} expected={:?} got={:?}",
            first16(&data),
            first16(&contents)
        );
        note_mismatch(iteration, &contents);
        return Err((23, Errno::EIO));
    }
    set_phase(slot, 33);
    fs::umount(&mount).map_err(|e| (24, e))?;
    set_phase(slot, 34);
    if fs::stat(&file_path(&file)) != Err(Errno::ENOENT) {
        return Err((25, Errno::EIO));
    }

    // Remount gets a fresh ramfs; the previous file must not survive.
    set_phase(slot, 35);
    fs::mount(&mount, "ramfs:").map_err(|e| (26, e))?;
    set_phase(slot, 36);
    if fs::stat(&file_path(&file)) != Err(Errno::ENOENT) {
        return Err((27, Errno::EIO));
    }
    set_phase(slot, 37);
    fs::umount(&mount).map_err(|e| (28, e))?;
    set_phase(slot, 38);
    Ok(())
}

extern "C" fn mount_worker(index: usize) -> ! {
    let slot = FILE_WORKERS + index;
    STARTED[slot].store(true, Ordering::Release);
    while GO.load(Ordering::Acquire) == 0 {
        let _ = sys::nanosleep(200_000);
    }
    let mut failed = false;
    for iteration in 0..ITERATIONS {
        CURRENT[slot].store(iteration, Ordering::Relaxed);
        let start = now_ns();
        if !failed {
            if let Err((phase, error)) = mount_cycle(index, iteration) {
                note_failure(slot, phase, error);
                failed = true;
            }
        }
        MOUNT_LAT[index][iteration].store(now_ns().saturating_sub(start), Ordering::Relaxed);
    }
    FINISHED[slot].store(true, Ordering::Release);
    sys::thread_exit(if failed { 1 } else { 0 });
}

fn write_file(path: &str, data: &[u8]) -> Result<(), Errno> {
    let mut file = fs::open(&file_path(path), O_WRONLY | O_CREAT | O_TRUNC)?;
    write_all(&mut file, data)?;
    file.close()
}

fn start_services() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map".to_string())?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
    for path in ["/sbin/initfsd", "/sbin/ramfsd", "/sbin/vfsd"] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| format!("spawn {path}"))?;
        let scheme = match path {
            "/sbin/initfsd" => "initfs:",
            "/sbin/ramfsd" => "ramfs:",
            _ => "file:",
        };
        wait_scheme(scheme)?;
    }
    println!("smp-fs: services ok");
    Ok(())
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    for _ in 0..3000 {
        if fs::open(scheme, O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = sys::nanosleep(10_000_000);
    }
    Err(format!("scheme {scheme} timeout"))
}

fn setup() -> Result<(), String> {
    for dir in [START, "/tmp/smp-fs/work", "/tmp/smp-fs/race"] {
        match fs::mkdir(dir, 0o755) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(error) => return Err(format!("mkdir {dir}: {}", error.description())),
        }
    }
    for index in 0..MOUNT_WORKERS {
        let dir = format!("{START}/mnt{index}");
        match fs::mkdir(&dir, 0o755) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(error) => return Err(format!("mkdir {dir}: {}", error.description())),
        }
    }
    println!("smp-fs: setup ok");
    Ok(())
}

fn start_workers() -> Result<(), String> {
    for index in 0..FILE_WORKERS {
        sys::thread_create(
            Handle::SELF_PROC,
            file_worker as *const () as usize,
            stack_top(index),
            index,
        )
        .map_err(|error| format!("file thread {index}: {}", error.description()))?;
    }
    for index in 0..MOUNT_WORKERS {
        sys::thread_create(
            Handle::SELF_PROC,
            mount_worker as *const () as usize,
            stack_top(FILE_WORKERS + index),
            index,
        )
        .map_err(|error| format!("mount thread {index}: {}", error.description()))?;
    }
    for spin in 0..1_000_000usize {
        if STARTED.iter().all(|flag| flag.load(Ordering::Acquire)) {
            GO.store(1, Ordering::Release);
            return Ok(());
        }
        if spin % 1024 == 0 {
            let _ = sys::nanosleep(200_000);
        }
    }
    Err("worker start timeout".to_string())
}

fn wait_workers() -> Result<(), String> {
    let deadline = now_ns().saturating_add(180_000_000_000);
    loop {
        if FINISHED.iter().all(|flag| flag.load(Ordering::Acquire)) {
            return Ok(());
        }
        if now_ns() > deadline {
            let pending: Vec<String> = FINISHED
                .iter()
                .enumerate()
                .filter(|(_, flag)| !flag.load(Ordering::Acquire))
                .map(|(index, _)| format!("{index}:{}", PHASES[index].load(Ordering::Relaxed)))
                .collect();
            return Err(format!("worker timeout pending={pending:?}"));
        }
        let _ = sys::nanosleep(10_000_000);
    }
}

fn report_failures() -> Result<(), String> {
    if MISMATCH_SEEN.load(Ordering::Acquire) {
        let packed = MISMATCH[0].load(Ordering::Acquire);
        let iteration = packed >> 32;
        let len = packed & 0xffff_ffff;
        let first = MISMATCH[1].load(Ordering::Acquire).to_le_bytes();
        let second = MISMATCH[2].load(Ordering::Acquire).to_le_bytes();
        println!("smp-fs: mismatch iter={iteration} len={len} first={first:?} second={second:?}");
    }
    if FAILURES.load(Ordering::Acquire) == 0 {
        return Ok(());
    }
    for (worker, info) in FAIL_INFO.iter().enumerate() {
        let packed = info.load(Ordering::Acquire);
        if packed == 0 {
            continue;
        }
        let iteration = packed >> 32;
        let phase = (packed >> 16) & 0xffff;
        let errno = Errno(u16::try_from(packed & 0xffff).unwrap_or(0));
        println!(
            "smp-fs: failed worker={worker} iter={iteration} phase={phase} errno={}",
            errno.description()
        );
    }
    Err(format!(
        "{} worker failure(s)",
        FAILURES.load(Ordering::Acquire)
    ))
}

fn percentile(samples: &mut [u64], percent: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let index = samples
        .len()
        .saturating_mul(percent)
        .saturating_div(100)
        .min(samples.len().saturating_sub(1));
    samples[index]
}

fn report_latencies() {
    for op in 0..OP_COUNT {
        let mut samples = Vec::with_capacity(FILE_WORKERS * ITERATIONS);
        for worker in 0..FILE_WORKERS {
            for iteration in 0..ITERATIONS {
                let value = LAT[worker][op][iteration].load(Ordering::Relaxed);
                if value != 0 {
                    samples.push(value);
                }
            }
        }
        let p50 = percentile(&mut samples, 50) / 1000;
        let p90 = percentile(&mut samples, 90) / 1000;
        let max = samples.last().copied().unwrap_or(0) / 1000;
        println!(
            "smp-fs: latency {}_us p50={p50} p90={p90} max={max}",
            OP_NAMES[op]
        );
    }
    let mut samples = Vec::with_capacity(MOUNT_WORKERS * ITERATIONS);
    for worker in 0..MOUNT_WORKERS {
        for iteration in 0..ITERATIONS {
            let value = MOUNT_LAT[worker][iteration].load(Ordering::Relaxed);
            if value != 0 {
                samples.push(value);
            }
        }
    }
    let p50 = percentile(&mut samples, 50) / 1000;
    let p90 = percentile(&mut samples, 90) / 1000;
    let max = samples.last().copied().unwrap_or(0) / 1000;
    println!("smp-fs: latency mount_us p50={p50} p90={p90} max={max}");
}

fn run() -> Result<(), String> {
    start_services()?;
    setup()?;
    start_workers()?;
    let wait = wait_workers();
    report_latencies();
    if let Err(error) = wait {
        let _ = report_failures();
        return Err(error);
    }
    report_failures()?;
    let successes = RACE_SUCCESSES.load(Ordering::Acquire);
    println!("smp-fs: race successes={successes}/{ITERATIONS}");
    if successes != ITERATIONS {
        return Err(format!(
            "O_EXCL race produced {successes} winners, expected {ITERATIONS}"
        ));
    }
    println!("smp-fs: done workers={WORKERS} iterations={ITERATIONS}");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("smp-fs: ok");
            0
        }
        Err(message) => {
            println!("smp-fs: failed {message}");
            1
        }
    }
}
