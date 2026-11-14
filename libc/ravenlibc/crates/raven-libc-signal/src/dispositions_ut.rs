// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn a_fresh_table_is_all_default() {
    let table = DispTable::new();
    for sig in 1..=64 {
        assert_eq!(table.read(sig), Disposition::DEFAULT);
    }
    assert!(table.handler_set().is_empty());
}

#[test]
fn write_returns_the_old_value_and_reads_it_back() {
    let table = DispTable::new();
    let first = Disposition {
        handler: 7,
        mask: 3,
        flags: 5,
    };
    let old = table.write(6, first);
    assert_eq!(old, Disposition::DEFAULT);
    assert_eq!(table.read(6), first);
    let second = Disposition::IGNORE;
    assert_eq!(table.write(6, second), first);
    assert_eq!(table.read(6), second);
}

#[test]
fn entries_do_not_overlap() {
    let table = DispTable::new();
    for sig in 1..=64 {
        table.write(
            sig,
            Disposition {
                handler: sig as usize,
                mask: sig as u64,
                flags: sig as u32,
            },
        );
    }
    for sig in 1..=64 {
        let got = table.read(sig);
        assert_eq!(got.handler, sig as usize);
        assert_eq!(got.mask, sig as u64);
        assert_eq!(got.flags, sig as u32);
    }
}

#[test]
fn handler_set_word_and_note_handler() {
    let table = DispTable::new();
    table.note_handler(3);
    table.note_handler(40);
    table.note_handler(0);
    table.note_handler(65);
    let set = table.handler_set();
    assert!(set.is_member(3).unwrap());
    assert!(set.is_member(40).unwrap());
    assert!(!set.is_member(1).unwrap());
}

#[test]
fn reader_never_sees_a_partial_triple() {
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicBool;

    let table = Arc::new(DispTable::new());
    table.write(
        10,
        Disposition {
            handler: 1,
            mask: !1u64,
            flags: 1,
        },
    );
    let stop = Arc::new(AtomicBool::new(false));
    let barrier = Arc::new(Barrier::new(2));

    let writer_table = Arc::clone(&table);
    let writer_stop = Arc::clone(&stop);
    let writer_barrier = Arc::clone(&barrier);
    let writer = std::thread::spawn(move || {
        writer_barrier.wait();
        let mut value = 2usize;
        while !writer_stop.load(Ordering::Relaxed) && value < 200_000 {
            let disposition = Disposition {
                handler: value,
                mask: !(value as u64),
                flags: value as u32,
            };
            writer_table.write(10, disposition);
            value = value.wrapping_add(1);
        }
    });

    barrier.wait();
    let mut reads = 0u64;
    while reads < 500_000 {
        let got = table.read(10);
        assert_eq!(got.mask, !(got.handler as u64), "partial triple: {got:?}");
        assert_eq!(got.flags, got.handler as u32, "partial triple: {got:?}");
        reads = reads.wrapping_add(1);
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
}

#[test]
fn read_rejects_out_of_range_without_panicking() {
    let table = DispTable::new();
    for sig in [0, -1, 65, 1000] {
        assert_eq!(table.read(sig), Disposition::DEFAULT);
        assert_eq!(table.write(sig, Disposition::IGNORE), Disposition::DEFAULT);
    }
}

#[test]
fn app_mask_word_clears_internal_bits() {
    assert_eq!(app_mask_word(u64::MAX), crate::sigset::APP_MASK);
}
