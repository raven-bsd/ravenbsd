// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Adversarial host model for `raven-rmm` (RM1).
//!
//! The model drives the public mapper over a deterministic seed corpus. It mixes 4 KiB and
//! 2 MiB mappings (and the deliberately rejected 1 GiB size), map/unmap/protect/translate,
//! double-map and huge/4K conflicts, table recycle and OOM rollback, over both the x86-64 and
//! the aarch64 descriptor encoding. OOM, direct-map failure and unreadable-table cases use a
//! fault-injecting physical-memory backend. Every run ends by unmapping all live mappings and
//! asserting that only the root frame remains allocated.
//!
//! The long harness is `tests/rmm/run-model.sh`; `RMM_MODEL_OPS` raises the iterations per seed
//! (default 10,000). The printed p50/p90 are model-operation latencies, not a boot-time claim.

use std::cell::UnsafeCell;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use raven_rmm::page::PRESENT;
use raven_rmm::{
    Aarch64Encoding, Encoding, FrameAllocator, MapError, Mapper, PageFlags, PageSize, PageTable,
    PhysAddr, PhysMemory, TlbFlusher, VirtAddr, X86_64Encoding,
};

const FRAME: u64 = 4096;
const ARENA_FRAMES: u64 = 4096;
const DEFAULT_OPS: usize = 10_000;
const SEEDS: [u64; 3] = [
    0x1234_5678_9ABC_DEF0,
    0x0BAD_F00D_DEAD_BEEF,
    0xA5A5_5A5A_1234_4321,
];
const LOW_BASE: u64 = 0x0000_0000_4000_0000;
const HIGH_BASE: u64 = 0xFFFF_8000_4000_0000;
const SMALL_SLOTS: u64 = 64;
const HUGE_SLOTS: u64 = 8;

struct FakePhys {
    memory: UnsafeCell<Box<[u8]>>,
    fail: Option<u64>,
}

impl FakePhys {
    fn new() -> Self {
        Self {
            memory: UnsafeCell::new(vec![0u8; (ARENA_FRAMES * FRAME) as usize].into_boxed_slice()),
            fail: None,
        }
    }

    fn failing(address: u64) -> Self {
        let mut phys = Self::new();
        phys.fail = Some(address);
        phys
    }
}

impl PhysMemory for FakePhys {
    fn phys_to_virt(&self, phys: PhysAddr) -> *mut u8 {
        if self.fail == Some(phys.0) {
            return core::ptr::null_mut();
        }
        assert!(phys.0.is_multiple_of(FRAME));
        assert!(phys.0 / FRAME < ARENA_FRAMES);
        // SAFETY: the arena is a stable boxed slice, UnsafeCell permits writes through a shared
        // reference, and the physical address was bounds checked above.
        unsafe { (*self.memory.get()).as_mut_ptr().add(phys.0 as usize) }
    }
}

struct FakeAlloc {
    next: u64,
    limit: u64,
    free: Vec<u64>,
    allocated: BTreeSet<u64>,
}

impl FakeAlloc {
    fn new(limit: u64) -> Self {
        Self {
            next: 1,
            limit,
            free: Vec::new(),
            allocated: BTreeSet::new(),
        }
    }

    fn freed_count(&self) -> usize {
        self.free.len()
    }

    fn allocated_count(&self) -> usize {
        self.allocated.len()
    }
}

impl FrameAllocator for FakeAlloc {
    fn allocate(&mut self) -> Option<PhysAddr> {
        let frame = match self.free.pop() {
            Some(frame) => frame,
            None if self.next < self.limit => {
                let frame = self.next;
                self.next += 1;
                frame
            }
            None => return None,
        };
        assert!(
            self.allocated.insert(frame),
            "frame {frame} allocated twice"
        );
        Some(PhysAddr(frame * FRAME))
    }

    fn free(&mut self, frame: PhysAddr) {
        assert!(frame.0.is_multiple_of(FRAME));
        let number = frame.0 / FRAME;
        assert!(
            self.allocated.remove(&number),
            "freeing unallocated frame {number}"
        );
        self.free.push(number);
    }
}

#[derive(Default)]
struct FakeTlb {
    flushed: Vec<u64>,
    all: u32,
}

impl TlbFlusher for FakeTlb {
    fn flush(&mut self, virt: VirtAddr) {
        self.flushed.push(virt.data());
    }

    fn flush_all(&mut self) {
        self.all += 1;
    }
}

struct Harness {
    phys: FakePhys,
    alloc: FakeAlloc,
    tlb: FakeTlb,
    root: PhysAddr,
}

impl Harness {
    fn new(limit: u64) -> Self {
        let phys = FakePhys::new();
        let mut alloc = FakeAlloc::new(limit);
        let root = alloc.allocate().expect("root frame");
        Self {
            phys,
            alloc,
            tlb: FakeTlb::default(),
            root,
        }
    }

    fn failing(limit: u64, fail: u64) -> Self {
        let phys = FakePhys::failing(fail);
        let mut alloc = FakeAlloc::new(limit);
        let root = alloc.allocate().expect("root frame");
        Self {
            phys,
            alloc,
            tlb: FakeTlb::default(),
            root,
        }
    }

    fn with<R>(&mut self, f: impl FnOnce(&mut Mapper<'_, FakeAlloc, FakePhys, FakeTlb>) -> R) -> R {
        let mut mapper =
            Mapper::new(self.root, &mut self.alloc, &self.phys, &mut self.tlb).expect("valid root");
        f(&mut mapper)
    }

    fn with_encoding<E: Encoding, R>(
        &mut self,
        root: PhysAddr,
        f: impl FnOnce(&mut Mapper<'_, FakeAlloc, FakePhys, FakeTlb, E>) -> R,
    ) -> R {
        let mut mapper = Mapper::with_encoding(root, &mut self.alloc, &self.phys, &mut self.tlb)
            .expect("valid root");
        f(&mut mapper)
    }

    fn table(&self) -> PageTable {
        PageTable::new(self.root, 3)
    }
}

fn virt(address: u64) -> VirtAddr {
    VirtAddr::new_raw(address).expect("non-sentinel address")
}

fn phys(frame: u64) -> PhysAddr {
    PhysAddr(frame * FRAME)
}

fn model_ops() -> usize {
    std::env::var("RMM_MODEL_OPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_OPS)
}

fn pick_flags(rng: u64, allow_user: bool) -> PageFlags {
    if allow_user {
        match rng % 7 {
            0 => PageFlags::user_rw(),
            1 => PageFlags::user_ro(),
            2 => PageFlags::user_rx(),
            3 => PageFlags::kernel_rw(),
            4 => PageFlags::kernel_ro(),
            5 => PageFlags::kernel_rx(),
            _ => PageFlags::mmio(),
        }
    } else {
        match rng % 5 {
            0 => PageFlags::kernel_rw(),
            1 => PageFlags::kernel_ro(),
            2 => PageFlags::kernel_rx(),
            3 => PageFlags::mmio(),
            _ => PageFlags::kernel_device(),
        }
    }
}

struct Stats {
    ops: usize,
    p50_ns: u128,
    p90_ns: u128,
}

fn run_model<E: Encoding>(seed: u64, base: u64, allow_user: bool, ops: usize) -> Stats {
    let mut harness = Harness::new(ARENA_FRAMES);
    let root = harness.root;
    let mut mapper: Mapper<'_, FakeAlloc, FakePhys, FakeTlb, E> =
        Mapper::with_encoding(root, &mut harness.alloc, &harness.phys, &mut harness.tlb)
            .expect("valid root");
    let mut state = seed;
    let mut small: BTreeMap<u64, (PhysAddr, PageFlags)> = BTreeMap::new();
    let mut huge: BTreeMap<u64, (PhysAddr, PageFlags)> = BTreeMap::new();
    let mut durations = Vec::with_capacity(ops);
    let mut phys_counter = 0u64;

    for _ in 0..ops {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let started = Instant::now();

        match state % 100 {
            0..=44 => {
                let slot = state % SMALL_SLOTS;
                let address = base + slot * FRAME;
                let flags = pick_flags(state, allow_user);
                let physical = phys(100 + (phys_counter % 3000));
                phys_counter += 1;
                let result = mapper.map(virt(address), physical, flags, PageSize::Size4K);
                if huge.contains_key(&base) {
                    assert_eq!(result, Err(MapError::HugePageConflict));
                } else {
                    match small.entry(address) {
                        Entry::Vacant(entry) => {
                            assert_eq!(result, Ok(()));
                            entry.insert((physical, flags));
                        }
                        Entry::Occupied(_) => {
                            assert_eq!(result, Err(MapError::AlreadyMapped));
                        }
                    }
                }
            }
            45..=59 => {
                let slot = state % HUGE_SLOTS;
                let address = base + slot * PageSize::Size2M.bytes();
                let flags = pick_flags(state, allow_user);
                let physical = phys(512 * (1 + (phys_counter % 4)));
                phys_counter += 1;
                let result = mapper.map(virt(address), physical, flags, PageSize::Size2M);
                let conflict = slot == 0
                    && small
                        .range(address..address + PageSize::Size2M.bytes())
                        .next()
                        .is_some();
                if conflict {
                    assert_eq!(result, Err(MapError::HugePageConflict));
                } else {
                    match huge.entry(address) {
                        Entry::Vacant(entry) => {
                            assert_eq!(result, Ok(()));
                            entry.insert((physical, flags));
                        }
                        Entry::Occupied(_) => {
                            assert_eq!(result, Err(MapError::AlreadyMapped));
                        }
                    }
                }
            }
            60..=74 => {
                let slot = state % SMALL_SLOTS;
                let address = base + slot * FRAME;
                if let Some((physical, _)) = huge.get(&base).copied() {
                    assert_eq!(mapper.unmap(virt(address)), Ok(physical));
                    huge.remove(&base);
                } else if let Some((physical, _)) = small.get(&address).copied() {
                    assert_eq!(mapper.unmap(virt(address)), Ok(physical));
                    small.remove(&address);
                } else {
                    assert_eq!(mapper.unmap(virt(address)), Err(MapError::NotMapped));
                }
            }
            75..=84 => {
                let slot = state % HUGE_SLOTS;
                let address = base + slot * PageSize::Size2M.bytes();
                if let Some((physical, _)) = huge.get(&address).copied() {
                    assert_eq!(mapper.unmap(virt(address)), Ok(physical));
                    huge.remove(&address);
                } else if slot == 0 {
                    if let Some((physical, _)) = small.get(&address).copied() {
                        assert_eq!(mapper.unmap(virt(address)), Ok(physical));
                        small.remove(&address);
                    } else {
                        assert_eq!(mapper.unmap(virt(address)), Err(MapError::NotMapped));
                    }
                } else {
                    assert_eq!(mapper.unmap(virt(address)), Err(MapError::NotMapped));
                }
            }
            85..=94 => {
                let slot = state % SMALL_SLOTS;
                let address = base + slot * FRAME;
                let flags = pick_flags(state, allow_user);
                if let Some((physical, _)) = huge.get(&base).copied() {
                    assert_eq!(mapper.protect(virt(address), flags), Ok(()));
                    huge.insert(base, (physical, flags));
                } else if let Some((physical, _)) = small.get(&address).copied() {
                    assert_eq!(mapper.protect(virt(address), flags), Ok(()));
                    small.insert(address, (physical, flags));
                } else {
                    assert_eq!(
                        mapper.protect(virt(address), flags),
                        Err(MapError::NotMapped)
                    );
                }
            }
            95..=98 => {
                let slot = state % SMALL_SLOTS;
                let address = base + slot * FRAME;
                let expected = huge
                    .get(&base)
                    .map(|(physical, flags)| (*physical, *flags, PageSize::Size2M))
                    .or_else(|| {
                        small
                            .get(&address)
                            .map(|(physical, flags)| (*physical, *flags, PageSize::Size4K))
                    });
                assert_eq!(mapper.translate(virt(address)), expected);
            }
            _ => {
                // 1 GiB mappings are deliberately unsupported; the public API rejects them.
                assert_eq!(
                    mapper.map(
                        virt(base),
                        phys(0),
                        PageFlags::kernel_rw(),
                        PageSize::Size1G
                    ),
                    Err(MapError::HugePageConflict)
                );
            }
        }

        durations.push(started.elapsed().as_nanos());
    }

    for (address, (physical, _)) in huge {
        assert_eq!(mapper.unmap(virt(address)), Ok(physical));
    }
    for (address, (physical, _)) in small {
        assert_eq!(mapper.unmap(virt(address)), Ok(physical));
    }
    assert_eq!(
        harness.alloc.allocated_count(),
        1,
        "seed {seed:#x} base {base:#x} leaked page-table frames"
    );

    durations.sort_unstable();
    let p50 = durations[durations.len() / 2];
    let p90 = durations[(durations.len() * 9 / 10).min(durations.len() - 1)];
    Stats {
        ops: durations.len(),
        p50_ns: p50,
        p90_ns: p90,
    }
}

fn print_and_sum(label: &str, seed: u64, base: u64, stats: &Stats) {
    println!(
        "{label}: seed={seed:#x} base={base:#x} ops={} p50_ns={} p90_ns={}",
        stats.ops, stats.p50_ns, stats.p90_ns
    );
}

#[test]
fn x86_model_matches_over_fixed_seed_corpus() {
    let ops = model_ops();
    let mut total = 0usize;
    let mut max_p90 = 0u128;
    for seed in SEEDS {
        for (base, allow_user) in [(LOW_BASE, true), (HIGH_BASE, false)] {
            let stats = run_model::<X86_64Encoding>(seed, base, allow_user, ops);
            print_and_sum("rmm-model-x86", seed, base, &stats);
            total += stats.ops;
            max_p90 = max_p90.max(stats.p90_ns);
        }
    }
    println!("rmm-model-x86: total_ops={total} max_p90_ns={max_p90}");
}

#[test]
fn aarch64_model_matches_over_fixed_seed_corpus() {
    let ops = model_ops();
    let mut total = 0usize;
    let mut max_p90 = 0u128;
    for seed in SEEDS {
        for (base, allow_user) in [(LOW_BASE, true), (HIGH_BASE, false)] {
            let stats = run_model::<Aarch64Encoding>(seed, base, allow_user, ops);
            print_and_sum("rmm-model-aarch64", seed, base, &stats);
            total += stats.ops;
            max_p90 = max_p90.max(stats.p90_ns);
        }
    }
    println!("rmm-model-aarch64: total_ops={total} max_p90_ns={max_p90}");
}

#[test]
fn map_oom_frees_new_tables_and_clears_the_root() {
    for (limit, expected_freed) in [(3u64, 1usize), (4, 2)] {
        let mut harness = Harness::new(limit);
        let address = virt(0x0000_0000_5000_0000);
        let before = harness.alloc.freed_count();
        harness.with(|mapper| {
            assert_eq!(
                mapper.map(address, phys(100), PageFlags::kernel_rw(), PageSize::Size4K),
                Err(MapError::OutOfFrames)
            );
        });
        assert_eq!(
            harness.alloc.freed_count(),
            before + expected_freed,
            "limit {limit}"
        );
        assert_eq!(harness.alloc.allocated_count(), 1, "limit {limit}");
        let root = harness.table();
        assert_eq!(
            root.entry(&harness.phys, root.index_of(address))
                .expect("root entry")
                .raw(),
            0,
            "limit {limit}"
        );
    }
}

#[test]
fn new_address_space_frees_the_new_root_on_direct_map_failure() {
    for fail in [phys(1), phys(2)] {
        let mut harness = Harness::failing(8, fail.0);
        let root = harness.root;
        let before = harness.alloc.freed_count();
        harness.with(|mapper| {
            assert_eq!(mapper.new_address_space(root), Err(MapError::OutOfFrames));
        });
        assert_eq!(
            harness.alloc.freed_count(),
            before + 1,
            "fail frame {:#x}",
            fail.0
        );
        assert_eq!(harness.alloc.allocated_count(), 1);
    }
}

#[test]
fn page_table_is_empty_is_conservative_when_unreadable() {
    let harness = Harness::new(8);
    let root = harness.table();
    root.set_entry(&harness.phys, 0, raven_rmm::Entry::from_raw(PRESENT));
    assert!(!root.is_empty(&harness.phys));
    let unreadable = FakePhys::failing(harness.root.0);
    assert!(
        !root.is_empty(&unreadable),
        "an unreadable table must not be treated as empty"
    );
}

#[test]
fn user_mapping_outside_the_user_half_is_rejected() {
    let mut x86 = Harness::new(32);
    x86.with(|mapper| {
        let kernel = virt(0xFFFF_8000_0000_0000);
        assert_eq!(
            mapper.map(kernel, phys(20), PageFlags::user_rw(), PageSize::Size4K),
            Err(MapError::UserKernelSplit)
        );
        assert_eq!(
            mapper.map(kernel, phys(20), PageFlags::kernel_rw(), PageSize::Size4K),
            Ok(())
        );
        assert_eq!(
            mapper.protect(kernel, PageFlags::user_rw()),
            Err(MapError::UserKernelSplit)
        );
        let user = virt(0x0000_0000_5000_0000);
        assert_eq!(
            mapper.map(user, phys(21), PageFlags::user_rw(), PageSize::Size4K),
            Ok(())
        );
    });

    let mut arm = Harness::new(32);
    let root = arm.root;
    arm.with_encoding::<Aarch64Encoding, _>(root, |mapper| {
        let kernel = virt(0xFFFF_8000_4000_0000);
        assert_eq!(
            mapper.map(kernel, phys(20), PageFlags::user_rw(), PageSize::Size4K),
            Err(MapError::UserKernelSplit)
        );
        assert_eq!(
            mapper.map(kernel, phys(20), PageFlags::kernel_rw(), PageSize::Size4K),
            Ok(())
        );
        assert_eq!(
            mapper.protect(kernel, PageFlags::user_rw()),
            Err(MapError::UserKernelSplit)
        );
        let user = virt(0x0000_0000_5000_0000);
        assert_eq!(
            mapper.map(user, phys(21), PageFlags::user_rw(), PageSize::Size4K),
            Ok(())
        );
    });
}

#[test]
fn aarch64_translate_flags_include_present() {
    let mut harness = Harness::new(32);
    let root = harness.root;
    harness.with_encoding::<Aarch64Encoding, _>(root, |mapper| {
        let address = virt(0x0000_0000_5000_0000);
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .expect("mapped");
        let (physical, flags, size) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(20));
        assert_eq!(size, PageSize::Size4K);
        assert!(flags.is_present(), "aarch64 translate must report PRESENT");
    });
}

#[test]
fn tlb_flush_is_emitted_for_map_protect_unmap() {
    let mut harness = Harness::new(16);
    let address = virt(0x0000_0000_5000_0000);
    let before = harness.tlb.flushed.len();
    harness.with(|mapper| {
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .expect("mapped");
        mapper
            .protect(address, PageFlags::kernel_ro())
            .expect("protected");
        mapper.unmap(address).expect("unmapped");
    });
    let flushed = &harness.tlb.flushed[before..];
    assert!(flushed.contains(&address.data()));
    assert!(flushed.len() >= 3, "map, protect and unmap each flush");
    assert_eq!(harness.alloc.allocated_count(), 1);
}
