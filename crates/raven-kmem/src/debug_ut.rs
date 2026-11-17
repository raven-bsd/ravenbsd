// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the opt-in debug allocator.

use super::*;
use alloc::collections::BTreeSet;
use raven_vmem::VmemGeneric;

/// The debug allocator used by these tests: small capacities keep the stack footprint low.
type TestDebug = DebugAlloc<32, 256>;

/// Build a debug allocator whose arena covers a real heap buffer of `bytes` bytes. The buffer is
/// returned alongside so it stays alive while the allocator is used.
fn debug(bytes: usize) -> (TestDebug, Box<[u8]>) {
    let backing: Box<[u8]> = alloc::vec![0u8; bytes].into_boxed_slice();
    let base = backing.as_ptr() as usize as u64;
    let arena = VmemGeneric::<256>::new(base, bytes as u64).expect("arena");
    let alloc = TestDebug::new(arena).expect("kmem");
    (alloc, backing)
}

/// Fill a live object so a later check can detect corruption or a missed copy.
fn fill_pattern(ptr: *mut u8, size: usize) {
    // SAFETY: `ptr` is a live object of `size` writable bytes.
    unsafe {
        for i in 0..size {
            ptr.add(i).write((size as u8).wrapping_add(i as u8));
        }
    }
}

fn check_pattern(ptr: *const u8, size: usize, tag: u8) {
    // SAFETY: `ptr` is a live object of `size` readable bytes.
    unsafe {
        for i in 0..size {
            assert_eq!(*ptr.add(i), tag.wrapping_add(i as u8), "byte {i} corrupt");
        }
    }
}

#[test]
fn redzones_detect_overflow_and_underflow() {
    let (mut alloc, _backing) = debug(0x10_0000);
    let addr = alloc.alloc(32, 8).expect("alloc");

    // Write into the trailing redzone (overflow).
    // SAFETY: the redzone byte after the object is within the live allocation.
    unsafe {
        (addr as *mut u8).add(32).write(0x00);
    }
    alloc.free(addr, 32, 8).expect("free");
    assert_eq!(alloc.report().redzone_overflow, 1);
    assert_eq!(alloc.report().redzone_underflow, 0);

    let addr = alloc.alloc(32, 8).expect("alloc");
    // Write into the leading redzone (underflow).
    // SAFETY: the redzone byte before the object is within the live allocation.
    unsafe {
        (addr as *mut u8).sub(1).write(0x00);
    }
    alloc.free(addr, 32, 8).expect("free");
    assert_eq!(alloc.report().redzone_underflow, 1);
}

#[test]
fn double_free_and_poison_are_detected() {
    let (mut alloc, _backing) = debug(0x10_0000);
    let addr = alloc.alloc(48, 8).expect("alloc");
    alloc.free(addr, 48, 8).expect("free");
    // The freed object is poisoned with 0xde.
    // SAFETY: `addr` is a live (freed) object; the bytes remain readable in the backing buffer.
    unsafe {
        assert_eq!(
            *(addr as *const u8),
            POISON_FREE,
            "freed object is poisoned"
        );
    }
    // A second free is a double-free.
    assert_eq!(alloc.free(addr, 48, 8), Err(KmemError::DoubleFree));
    assert_eq!(alloc.report().double_free, 1);
}

#[test]
fn audit_records_events() {
    let (mut alloc, _backing) = debug(0x10_0000);
    let a = alloc.alloc(16, 8).expect("alloc");
    let b = alloc.alloc(16, 8).expect("alloc");
    alloc.free(a, 16, 8).expect("free");
    alloc.free(b, 16, 8).expect("free");
    assert_eq!(alloc.audit_len(), 4);
    assert_eq!(alloc.audit_record(0).op, AuditOp::Alloc);
    assert_eq!(alloc.audit_record(0).addr, a);
    assert_eq!(alloc.audit_record(1).op, AuditOp::Alloc);
    assert_eq!(alloc.audit_record(2).op, AuditOp::Free);
    assert_eq!(alloc.audit_record(2).addr, a);
    assert_eq!(alloc.audit_record(3).op, AuditOp::Free);
    assert_eq!(alloc.audit_record(3).addr, b);
    assert!(alloc.audit_record(3).seq > alloc.audit_record(0).seq);
}

#[test]
fn correct_use_has_no_detections() {
    let (mut alloc, _backing) = debug(0x10_0000);
    let mut live = alloc::vec::Vec::new();
    for i in 0..200 {
        let size = 1 + (i * 7) % 512;
        let addr = alloc.alloc(size, 8).expect("alloc");
        fill_pattern(addr as *mut u8, size);
        live.push((addr, size));
    }
    for (addr, size) in live.drain(..) {
        check_pattern(addr as *const u8, size, size as u8);
        alloc.free(addr, size, 8).expect("free");
    }
    let report = alloc.report();
    assert_eq!(report.redzone_overflow, 0);
    assert_eq!(report.redzone_underflow, 0);
    assert_eq!(report.double_free, 0);
    assert_eq!(report.allocs, report.frees);
}

#[test]
fn failure_injection_returns_exhausted_not_a_bad_pointer() {
    // A tiny arena that exhausts quickly: every successful alloc is tracked and freed, and no
    // address is ever handed out twice.
    let backing: Box<[u8]> = alloc::vec![0u8; 0x4000].into_boxed_slice();
    let base = backing.as_ptr() as usize as u64;
    let arena = VmemGeneric::<256>::new(base, 0x4000).expect("arena");
    let mut alloc = TestDebug::new(arena).expect("kmem");
    let mut live: BTreeSet<u64> = BTreeSet::new();
    let mut handed = 0usize;
    for _ in 0..10_000 {
        match alloc.alloc(4096, 8) {
            Ok(addr) => {
                assert!(live.insert(addr), "address handed out twice");
                handed += 1;
            }
            Err(KmemError::Exhausted) => break,
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }
    // The arena is 16 KiB, so only a few 4 KiB + guards fit.
    assert!(handed < 10);
    assert_eq!(alloc.report().allocs as usize, handed);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }
}

#[test]
fn debug_hammer_millions() {
    let (mut alloc, _backing) = debug(0x2_0000);
    let mut rng = Rng(0x0ddc_0ffe_e123_4567);
    let mut live: Vec<(usize, usize, usize)> = Vec::new();

    for _ in 0..1_000_000 {
        if live.is_empty() || rng.below(100) < 55 {
            let size = 1 + rng.below(1500) as usize;
            let align = 1usize << rng.below(4); // 1, 2, 4, 8
            match alloc.alloc(size, align) {
                Ok(addr) => {
                    assert_eq!(addr % align as u64, 0);
                    fill_pattern(addr as *mut u8, size);
                    live.push((addr as usize, size, align));
                }
                Err(KmemError::Exhausted) => {}
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        } else if rng.below(100) < 82 {
            let idx = rng.below(live.len() as u64) as usize;
            let (addr, size, align) = live[idx];
            let new_size = 1 + rng.below(1500) as usize;
            match alloc.realloc(addr as u64, size, new_size, align) {
                Ok(new_addr) => {
                    check_pattern(
                        new_addr as *const u8,
                        core::cmp::min(size, new_size),
                        size as u8,
                    );
                    fill_pattern(new_addr as *mut u8, new_size);
                    live[idx] = (new_addr as usize, new_size, align);
                }
                Err(KmemError::Exhausted) => {}
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        } else {
            let idx = rng.below(live.len() as u64) as usize;
            let (addr, size, align) = live.swap_remove(idx);
            check_pattern(addr as *const u8, size, size as u8);
            alloc
                .free(addr as u64, size, align)
                .expect("free live block");
        }
    }

    for (addr, size, align) in live.drain(..) {
        check_pattern(addr as *const u8, size, size as u8);
        alloc
            .free(addr as u64, size, align)
            .expect("free remaining");
    }

    let report = alloc.report();
    assert_eq!(report.allocs, report.frees);
    assert_eq!(report.double_free, 0, "no double-free on correct use");
    assert_eq!(report.redzone_overflow, 0, "no overflow on correct use");
    assert_eq!(report.redzone_underflow, 0, "no underflow on correct use");
}
