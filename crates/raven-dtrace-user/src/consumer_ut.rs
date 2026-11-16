// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the consumer state machine.

use core::cell::{Cell, RefCell};

use super::*;
use crate::abi::{CpuStats, ProbeInfo, Record};

#[derive(Default)]
struct Fake {
    enabled: RefCell<Vec<u32>>,
    disabled: RefCell<Vec<u32>>,
    records: RefCell<Vec<Record>>,
    stats: RefCell<Vec<CpuStats>>,
    cleared: Cell<bool>,
    fail: Cell<bool>,
}

impl TraceKernel for Fake {
    fn probe_list(&self, out: &mut [ProbeInfo]) -> Result<usize, Errno> {
        if let Some(slot) = out.first_mut() {
            slot.id = 1;
            slot.provider[..7].copy_from_slice(b"syscall");
        }
        Ok(1)
    }

    fn enable(&self, probe: u32, _event: Handle) -> Result<(), Errno> {
        if self.fail.get() {
            return Err(Errno::EACCES);
        }
        self.enabled.borrow_mut().push(probe);
        Ok(())
    }

    fn disable(&self, probe: u32) -> Result<(), Errno> {
        self.disabled.borrow_mut().push(probe);
        Ok(())
    }

    fn read(&self, out: &mut [Record]) -> Result<usize, Errno> {
        let mut records = self.records.borrow_mut();
        let count = out.len().min(records.len());
        for (slot, record) in out.iter_mut().take(count).zip(records.drain(..count)) {
            *slot = record;
        }
        Ok(count)
    }

    fn stats(&self, out: &mut [CpuStats]) -> Result<usize, Errno> {
        let stats = self.stats.borrow();
        let count = out.len().min(stats.len());
        if let (Some(target), Some(source)) = (out.get_mut(..count), stats.get(..count)) {
            target.copy_from_slice(source);
        }
        Ok(count)
    }

    fn clear(&self) -> Result<(), Errno> {
        self.cleared.set(true);
        Ok(())
    }
}

fn record(sequence: u64) -> Record {
    Record {
        sequence,
        probe: 1,
        syscall: 0x24,
        ..Record::default()
    }
}

#[test]
fn list_enable_disable_clear_round_trip() {
    let fake = Fake::default();
    let consumer = Consumer::new(fake);
    let mut probes = [ProbeInfo::default(); 2];
    assert_eq!(consumer.list_probes(&mut probes), Ok(1));
    assert_eq!(probes[0].id, 1);
    assert_eq!(consumer.enable(1), Ok(()));
    assert_eq!(consumer.disable(1), Ok(()));
    assert_eq!(consumer.clear(), Ok(()));
    assert_eq!(consumer.kernel.enabled.borrow().as_slice(), &[1]);
    assert_eq!(consumer.kernel.disabled.borrow().as_slice(), &[1]);
    assert!(consumer.kernel.cleared.get());
}

#[test]
fn read_and_drain_visit_every_record() {
    let fake = Fake::default();
    fake.records
        .borrow_mut()
        .extend([record(1), record(2), record(3)]);
    let consumer = Consumer::new(fake);
    let mut buffer = [Record::default(); 2];
    assert_eq!(consumer.read(&mut buffer), Ok(2));
    assert_eq!(buffer[0].sequence, 1);
    assert_eq!(buffer[1].sequence, 2);
    let mut seen = Vec::new();
    assert_eq!(
        consumer.drain(&mut buffer, 4, |record| seen.push(record.sequence)),
        Ok(1)
    );
    assert_eq!(seen, [3]);
}

#[test]
fn errors_propagate() {
    let fake = Fake::default();
    fake.fail.set(true);
    let consumer = Consumer::new(fake);
    assert_eq!(consumer.enable(1), Err(Errno::EACCES));
}

#[test]
fn stats_are_copied() {
    let fake = Fake::default();
    fake.stats.borrow_mut().push(CpuStats {
        produced: 10,
        consumed: 9,
        dropped: 1,
        capacity: 4096,
    });
    let consumer = Consumer::new(fake);
    let mut stats = [CpuStats::default(); 2];
    assert_eq!(consumer.stats(&mut stats), Ok(1));
    assert_eq!(stats[0].produced, 10);
    assert_eq!(stats[0].capacity, 4096);
}

#[test]
fn format_record_is_stable() {
    let mut text = String::new();
    let mut record = record(1);
    record.cpu = 2;
    record.tid = 7;
    record.timestamp_ns = 1234;
    record.arg0 = 1;
    record.arg1 = 2;
    assert_eq!(format_record(&record, &mut text), Ok(()));
    assert_eq!(
        text,
        "cpu=2 probe=1 tid=7 syscall=36 ts=1234 args=0x1,0x2,0x0,0x0"
    );
}
