// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host regressions for the C76 libc spawn/eventfd fixes.

use std::collections::BTreeMap;

use raven_libc_abi::fcntl::O_RDWR;
use raven_libc_personality_linux::eventfd::{self, EFD_CLOEXEC, EFD_NONBLOCK, EventFdTable};
use raven_libc_personality_linux::seams::{Errno, EventOps, FdKind, FdTable, Handle};
use raven_libc_platform::fd::inherit::{FdMap, FdMapEntry};
use raven_libc_platform::{FakeSys, OfdKind, Platform};

/// One entry of the fake eventfd descriptor table.
struct Entry {
    handle: Handle,
    kind: FdKind,
    cloexec: bool,
}

/// The minimal descriptor table the eventfd engine needs.
struct Table {
    next: i32,
    entries: BTreeMap<i32, Entry>,
}

impl Table {
    fn new() -> Self {
        Self {
            next: 3,
            entries: BTreeMap::new(),
        }
    }

    fn cloexec(&self, fd: i32) -> bool {
        self.entries.get(&fd).is_some_and(|entry| entry.cloexec)
    }
}

impl FdTable for Table {
    fn handle_of(&self, fd: i32) -> Result<Handle, Errno> {
        self.entries
            .get(&fd)
            .map(|entry| entry.handle)
            .ok_or(Errno::EBADF)
    }

    fn install(&mut self, handle: Handle, kind: FdKind, cloexec: bool) -> Result<i32, Errno> {
        let fd = self.next;
        self.next = self.next.saturating_add(1);
        self.entries.insert(
            fd,
            Entry {
                handle,
                kind,
                cloexec,
            },
        );
        Ok(fd)
    }

    fn kind_of(&self, fd: i32) -> Result<FdKind, Errno> {
        self.entries
            .get(&fd)
            .map(|entry| entry.kind)
            .ok_or(Errno::EBADF)
    }

    fn epoch(&self, fd: i32) -> u32 {
        u32::from(self.entries.contains_key(&fd))
    }

    fn close_range_from(&mut self, lo: i32) -> usize {
        let removed: Vec<i32> = self
            .entries
            .keys()
            .copied()
            .filter(|fd| *fd >= lo)
            .collect();
        for fd in &removed {
            self.entries.remove(fd);
        }
        removed.len()
    }
}

/// The readiness doorbell the eventfd engine rings.
#[derive(Default)]
struct Events {
    next: u32,
    signals: usize,
}

impl EventOps for Events {
    fn create(&mut self) -> Result<Handle, Errno> {
        self.next = self.next.saturating_add(1);
        Ok(Handle(self.next))
    }

    fn signal(&mut self, _handle: Handle) -> Result<(), Errno> {
        self.signals = self.signals.saturating_add(1);
        Ok(())
    }
}

#[test]
fn eventfd_write_then_read_round_trips_the_counter() {
    let mut table = Table::new();
    let mut events = Events::default();
    let mut state = EventFdTable::new();
    let fd = eventfd::create(
        &mut table,
        &mut events,
        &mut state,
        0,
        EFD_CLOEXEC | EFD_NONBLOCK,
    )
    .expect("create");

    assert_eq!(table.kind_of(fd), Ok(FdKind::EventFd));
    assert!(table.cloexec(fd));
    assert!(state.get(fd).is_some_and(|counter| counter.nonblock));

    eventfd::write(&table, &mut events, &mut state, fd, 5).expect("write");
    assert_eq!(events.signals, 1);
    assert_eq!(state.get(fd).map(|counter| counter.value), Some(5));

    let mut bytes = [0u8; 8];
    assert_eq!(
        eventfd::read(&table, &mut events, &mut state, fd, &mut bytes),
        Ok(8)
    );
    assert_eq!(u64::from_le_bytes(bytes), 5);
    assert_eq!(state.get(fd).map(|counter| counter.value), Some(0));
}

#[test]
fn eventfd_rejects_a_counter_overflow_instead_of_looping() {
    let mut table = Table::new();
    let mut events = Events::default();
    let mut state = EventFdTable::new();
    let fd = eventfd::create(&mut table, &mut events, &mut state, 1, 0).expect("create");
    assert_eq!(
        eventfd::write(&table, &mut events, &mut state, fd, u64::MAX - 1),
        Err(Errno::EAGAIN)
    );
    assert_eq!(
        eventfd::write(&table, &mut events, &mut state, fd, u64::MAX),
        Err(Errno::EINVAL)
    );
}

#[test]
fn binary_write_keeps_embedded_nuls() {
    let backend = FakeSys::new();
    backend.file(b"/tmp/c76-binary", b"");
    let platform = Platform::new(backend);
    let fd = platform
        .open(b"/tmp/c76-binary", O_RDWR as u32, 0)
        .expect("open");

    let payload = [b'a', 0, b'b', 0, b'c'];
    assert_eq!(platform.write(fd, &payload), Ok(payload.len()));
    assert_eq!(
        platform.backend().contents(b"/tmp/c76-binary"),
        Some(payload.to_vec())
    );
}

#[test]
fn rfdm_restores_a_usable_child_descriptor() {
    let backend = FakeSys::new();
    backend.file(b"/tmp/c76-inherit", b"");
    let platform = Platform::new(backend);
    let source = platform
        .open(b"/tmp/c76-inherit", O_RDWR as u32, 0)
        .expect("open");
    let handle = platform.get(source).expect("source").handle();

    let block = FdMap {
        entries: vec![FdMapEntry::new(
            9,
            handle.0,
            OfdKind::File,
            0,
            O_RDWR as u32,
        )],
    };
    let wire = block.encode().expect("encode");
    let decoded = FdMap::decode(&wire).expect("decode");
    assert_eq!(decoded, block);

    platform.restore_fdmap(&decoded).expect("restore");
    assert_eq!(platform.get(9).expect("restored").kind(), OfdKind::File);

    let payload = [b'x', 0, b'y'];
    assert_eq!(platform.write(9, &payload), Ok(payload.len()));
    assert_eq!(
        platform.backend().contents(b"/tmp/c76-inherit"),
        Some(payload.to_vec())
    );
}
