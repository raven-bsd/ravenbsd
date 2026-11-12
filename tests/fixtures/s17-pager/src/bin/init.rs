// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S17 pager controller: random page-ins, a shared write-back round trip, private copy-on-write,
//! a short supply and pager death. Every mapping is gone before the pager exits so the kernel
//! leak counter (`s17-pagecheck`) can be zero at init teardown.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, map, prot};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_rt::sys::{syscall3, syscall6};
use raven_s17_pager::{
    O_RDWR, PAGE, call, fail, handle_close, map_object, nanosleep, open, page_pattern, proc_wait,
    result, unmap,
};

entry!(main);

const INITFS_BASE: usize = 0x1000_0000;
const RETRIES: usize = 2000;
const RANDOM_PAGES: usize = 1024;
const RANDOM_FAULTS: usize = 32;
const SHARED_PAGE: usize = 1;
const PRIVATE_PAGE: usize = 5;
const STATS_WORDS: usize = 6;
const STATS_BYTES: usize = STATS_WORDS * 8;
const PAGE_IN: usize = 0;
const PAGE_OUT: usize = 1;
const WRITEBACK_OK: usize = 2;
const ERROR_PAGE_IN: usize = 3;
const ERROR_SHORT: usize = 4;
const WRITTEN: &[u8] = b"s17-shared!";

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => fail("initfs parse"),
    };
    let server = match spawn_child(&initfs, "/server", &["/server"]) {
        Ok(server) => server,
        Err(_) => fail("server spawn"),
    };
    let stats = match open_retry("pager:stats") {
        Ok(handle) => handle,
        Err(_) => fail("server register"),
    };
    let object = match open_retry("pager:object") {
        Ok(handle) => handle,
        Err(_) => fail("object open"),
    };

    random_page_ins(stats, object);
    let before_private = shared_writeback(stats, object);
    private_cow(stats, object, before_private);
    short_supply(&initfs, stats);
    failed_page(&initfs, stats);

    if handle_close(stats).is_err() {
        fail("stats close");
    }
    pager_death(&initfs, server, object);
    println!("s17: ok");
    0
}

fn random_page_ins(stats: Handle, object: Handle) {
    let length = RANDOM_PAGES * PAGE;
    let base = match map_object(object, 0, length, prot::READ, true) {
        Ok(base) => base,
        Err(_) => fail("random map"),
    };
    let mut state = 0x5eed_1234_9abc_def0u64;
    let mut seen = [0u64; RANDOM_PAGES / 64];
    let mut unique = 0usize;
    let mut index = 0usize;
    while index < RANDOM_FAULTS {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let page = (state % RANDOM_PAGES as u64) as usize;
        let word = page / 64;
        let bit = 1u64 << (page % 64);
        if seen[word] & bit == 0 {
            seen[word] |= bit;
            unique = unique.saturating_add(1);
        }
        let address = base.saturating_add(page.saturating_mul(PAGE));
        // SAFETY: `base..base + RANDOM_PAGES * PAGE` is this process's readable mapping, and
        // `page` is inside it.
        let sample = unsafe { core::slice::from_raw_parts(address as *const u8, PAGE) };
        let mut expected = [0u8; PAGE];
        page_pattern(page as u64, &mut expected);
        if sample != expected {
            fail("random page content");
        }
        index = index.saturating_add(1);
    }
    if unmap(base, length).is_err() {
        fail("random unmap");
    }
    let counters = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("random stats"),
    };
    if counters[PAGE_IN] != unique as u64 {
        fail("random pagein count");
    }
    println!(
        "s17: random pages={} pageins={} ok",
        unique, counters[PAGE_IN]
    );
}

fn shared_writeback(stats: Handle, object: Handle) -> u64 {
    let before = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("shared stats before"),
    };
    let address = match map_object(
        object,
        SHARED_PAGE * PAGE,
        PAGE,
        prot::READ | prot::WRITE,
        true,
    ) {
        Ok(address) => address,
        Err(_) => fail("shared map"),
    };
    // SAFETY: `address` is this process's one-page writable shared mapping.
    unsafe {
        core::ptr::copy_nonoverlapping(WRITTEN.as_ptr(), address as *mut u8, WRITTEN.len());
    }
    // SAFETY: the same mapping is readable for its whole page.
    let readback = unsafe { core::slice::from_raw_parts(address as *const u8, WRITTEN.len()) };
    if readback != WRITTEN {
        fail("shared readback");
    }
    if unmap(address, PAGE).is_err() {
        fail("shared unmap");
    }
    let after = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("shared stats after"),
    };
    if after[PAGE_OUT] != before[PAGE_OUT].saturating_add(1) || after[WRITEBACK_OK] == 0 {
        fail("shared write-back counters");
    }
    println!("s17: shared write-back pageout={} ok", after[PAGE_OUT]);

    // Read the page back through a fresh read-only mapping: the write-back round trip must have
    // preserved the bytes and the object frame.
    let verify = match map_object(object, SHARED_PAGE * PAGE, PAGE, prot::READ, true) {
        Ok(address) => address,
        Err(_) => fail("shared verify map"),
    };
    // SAFETY: `verify` is this process's one-page readable mapping.
    let round_trip = unsafe { core::slice::from_raw_parts(verify as *const u8, WRITTEN.len()) };
    if round_trip != WRITTEN {
        fail("shared round trip");
    }
    if unmap(verify, PAGE).is_err() {
        fail("shared verify unmap");
    }
    after[PAGE_OUT]
}

fn private_cow(stats: Handle, object: Handle, page_out_before: u64) {
    let address = match map_object(
        object,
        PRIVATE_PAGE * PAGE,
        PAGE,
        prot::READ | prot::WRITE,
        false,
    ) {
        Ok(address) => address,
        Err(_) => fail("private map"),
    };
    let mut expected = [0u8; PAGE];
    page_pattern(PRIVATE_PAGE as u64, &mut expected);
    // SAFETY: the private mapping is readable for one page.
    let initial = unsafe { core::slice::from_raw_parts(address as *const u8, PAGE) };
    if initial != expected {
        fail("private initial content");
    }
    // SAFETY: the mapping is writable in the VMA; the first write resolves the private COW.
    unsafe {
        (address as *mut u8).write(0xa5);
    }
    // SAFETY: the byte just written.
    if unsafe { (address as *const u8).read() } != 0xa5 {
        fail("private cow write");
    }
    if unmap(address, PAGE).is_err() {
        fail("private unmap");
    }
    let after = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("private stats"),
    };
    if after[PAGE_OUT] != page_out_before {
        fail("private page-out");
    }
    let shared = match map_object(object, PRIVATE_PAGE * PAGE, PAGE, prot::READ, true) {
        Ok(address) => address,
        Err(_) => fail("private object map"),
    };
    // SAFETY: the shared view is one readable page of the object.
    let original = unsafe { core::slice::from_raw_parts(shared as *const u8, PAGE) };
    if original != expected {
        fail("private cow isolation");
    }
    if unmap(shared, PAGE).is_err() {
        fail("private object unmap");
    }
    println!("s17: private cow ok");
}

fn short_supply(initfs: &Initfs<'_>, stats: Handle) {
    let before = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("error stats before"),
    };
    if spawn_fault(initfs, "pager:error") != -11 {
        fail("short supply exit");
    }
    let after = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("error stats after"),
    };
    if after[ERROR_PAGE_IN] != before[ERROR_PAGE_IN].saturating_add(1) || after[ERROR_SHORT] == 0 {
        fail("short supply counters");
    }
    println!(
        "s17: short supply pagein={} short={} ok",
        after[ERROR_PAGE_IN], after[ERROR_SHORT]
    );
}

fn failed_page(initfs: &Initfs<'_>, stats: Handle) {
    let before = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("failed page stats before"),
    };
    if spawn_fault(initfs, "pager:error") != -11 {
        fail("failed page exit");
    }
    let after = match query_stats(stats) {
        Ok(counters) => counters,
        Err(_) => fail("failed page stats after"),
    };
    if after[ERROR_PAGE_IN] != before[ERROR_PAGE_IN] {
        fail("failed page reopened");
    }
    println!("s17: page eio ok");
}

fn pager_death(initfs: &Initfs<'_>, server: Handle, object: Handle) {
    // Keep one present page mapped across the pager death; a present page must stay readable
    // without another PageIn (C18).
    let kept = match map_object(object, SHARED_PAGE * PAGE, PAGE, prot::READ, true) {
        Ok(address) => address,
        Err(_) => fail("present page map"),
    };
    // SAFETY: `kept` is this process's one-page readable mapping of the written page.
    let before = unsafe { core::slice::from_raw_parts(kept as *const u8, WRITTEN.len()) };
    if before != WRITTEN {
        fail("present page content");
    }
    let child = spawn_fault(initfs, "pager:death");
    let server_code = match proc_wait(server) {
        Ok(code) => code,
        Err(_) => fail("server wait"),
    };
    if child != -11 || server_code != 0 {
        fail("pager death exit");
    }
    // SAFETY: the PTE is still present; reading it must not reach the dead pager.
    let after = unsafe { core::slice::from_raw_parts(kept as *const u8, WRITTEN.len()) };
    if after != WRITTEN {
        fail("present page after death");
    }
    if unmap(kept, PAGE).is_err() {
        fail("present page unmap");
    }
    if handle_close(object).is_err() {
        fail("object close");
    }
    match open("pager:stats", O_RDWR) {
        Err(Errno::ENOENT) => {}
        _ => fail("pager still registered"),
    }
    println!("s17: pager death client={child} server={server_code} ok");
    println!("s17: present page survives pager death ok");
}

fn spawn_fault(initfs: &Initfs<'_>, path: &str) -> i32 {
    let child = match spawn_child(initfs, "/fault-client", &["/fault-client", path]) {
        Ok(child) => child,
        Err(_) => fail("fault-client spawn"),
    };
    match proc_wait(child) {
        Ok(code) => code,
        Err(_) => fail("fault-client wait"),
    }
}

fn spawn_child(initfs: &Initfs<'_>, path: &str, args: &[&str]) -> Result<Handle, Errno> {
    let options = SpawnOptions { args, env: &[] };
    spawn(initfs, path, &options).map_err(|_| Errno::EIO)
}

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn query_stats(file: Handle) -> Result<[u64; STATS_WORDS], Errno> {
    let mut buffer = [0u8; STATS_BYTES];
    let length = call(file, b"stats", Some(&mut buffer))?;
    if length != STATS_BYTES {
        return Err(Errno::EIO);
    }
    let mut words = [0u64; STATS_WORDS];
    for (index, word) in words.iter_mut().enumerate() {
        let mut bytes = [0u8; 8];
        let start = index.saturating_mul(8);
        bytes.copy_from_slice(&buffer[start..start.saturating_add(8)]);
        *word = u64::from_le_bytes(bytes);
    }
    Ok(words)
}

fn map_initfs_image() -> Result<&'static [u8], Errno> {
    let page = result(unsafe {
        syscall6(
            raven_abi::syscall::Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::INITFS.0 as usize,
            0,
            0,
            PAGE,
            prot::READ,
        )
    })?;
    // SAFETY: the mapping is one readable page; the C7 header puts total_size at byte 40.
    let total = unsafe { ((page + 40) as *const u64).read_unaligned() as usize };
    let _ = result(unsafe {
        syscall3(
            raven_abi::syscall::Syscall::UNMAP as usize,
            Handle::SELF_PROC.0 as usize,
            page,
            PAGE,
        )
    });
    if total == 0 || !total.is_multiple_of(PAGE) {
        return Err(Errno::EINVAL);
    }
    let base = result(unsafe {
        syscall6(
            raven_abi::syscall::Syscall::MAP as usize,
            Handle::SELF_PROC.0 as usize,
            Handle::INITFS.0 as usize,
            0,
            INITFS_BASE,
            total,
            map::pack(prot::READ, map::FIXED),
        )
    })?;
    // SAFETY: the loader mapped the validated initfs bytes at `base` for `total` bytes.
    Ok(unsafe { core::slice::from_raw_parts(base as *const u8, total) })
}
