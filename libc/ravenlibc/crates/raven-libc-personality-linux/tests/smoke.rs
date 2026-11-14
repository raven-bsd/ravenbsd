// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Integration smoke test: the public API of the crate over minimal local fakes (the
//! `cfg(test)` fakes of `src/testutil.rs` are not visible here).

use std::collections::BTreeMap;

use raven_libc_personality_linux::epoll::{self, EpollEvent, EpollTable};
use raven_libc_personality_linux::error::{ErrorState, format_error};
use raven_libc_personality_linux::seams::{
    Errno, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, Handle,
};

#[derive(Default)]
struct TestTable {
    map: BTreeMap<i32, (Handle, FdKind, u32)>,
    next: i32,
    epoch: u32,
}

impl FdTable for TestTable {
    fn handle_of(&self, fd: i32) -> Result<Handle, Errno> {
        self.map.get(&fd).map(|entry| entry.0).ok_or(Errno::EBADF)
    }
    fn install(&mut self, handle: Handle, kind: FdKind, _cloexec: bool) -> Result<i32, Errno> {
        let fd = if self.next == 0 { 3 } else { self.next };
        self.next = fd + 1;
        self.epoch += 1;
        self.map.insert(fd, (handle, kind, self.epoch));
        Ok(fd)
    }
    fn kind_of(&self, fd: i32) -> Result<FdKind, Errno> {
        self.map.get(&fd).map(|entry| entry.1).ok_or(Errno::EBADF)
    }
    fn epoch(&self, fd: i32) -> u32 {
        self.map.get(&fd).map_or(0, |entry| entry.2)
    }
    fn close_range_from(&mut self, lo: i32) -> usize {
        let keys: Vec<i32> = self.map.keys().copied().filter(|fd| *fd >= lo).collect();
        let count = keys.len();
        for fd in keys {
            self.map.remove(&fd);
        }
        count
    }
}

#[derive(Default)]
struct TestEvq {
    events: Vec<EvqEvent>,
}

impl EvqOps for TestEvq {
    fn create(&mut self) -> Result<Handle, Errno> {
        Ok(Handle(100))
    }
    fn ctl(
        &mut self,
        _q: Handle,
        _op: EvqCtl,
        _h: Handle,
        _events: u32,
        _cookie: u64,
    ) -> Result<(), Errno> {
        Ok(())
    }
    fn wait(&mut self, _q: Handle, out: &mut [EvqEvent], _timeout: u64) -> Result<usize, Errno> {
        let count = out.len().min(self.events.len());
        for (slot, event) in out.iter_mut().zip(self.events.drain(..count)) {
            *slot = event;
        }
        Ok(count)
    }
}

#[test]
fn epoll_delivers_a_registered_event() {
    let mut evq = TestEvq::default();
    let mut table = TestTable::default();
    let mut epoll_table = EpollTable::new();
    let epfd = epoll::create1(&mut evq, &mut table, 0).unwrap();
    table.map.insert(10, (Handle(710), FdKind::Pipe, 1));
    table.epoch = 1;
    let event = EpollEvent {
        events: epoll::EPOLLIN,
        data: 7,
    };
    epoll::ctl(
        &mut evq,
        &mut table,
        &mut epoll_table,
        epfd,
        epoll::EPOLL_CTL_ADD,
        10,
        Some(&event),
    )
    .unwrap();
    evq.events.push(EvqEvent {
        cookie: epoll::cookie(10, 1),
        events: epoll::EV_READABLE,
        pad: 0,
    });
    let mut out = [EpollEvent { events: 0, data: 0 }; 1];
    assert_eq!(
        epoll::wait(&mut evq, &table, &mut epoll_table, epfd, &mut out, 0, None),
        Ok(1)
    );
    let data = out[0].data;
    assert_eq!(data, 7);
}

#[test]
fn error_line_is_golden() {
    let mut out = Vec::new();
    format_error(b"prog", 2, b"cannot open", Some(b"No such file"), &mut out);
    assert_eq!(out, b"prog: cannot open: No such file\n");
}

#[test]
fn error_at_line_suppresses_the_same_pair() {
    let mut state = ErrorState::new();
    state.one_per_line = 1;
    let mut out = Vec::new();
    assert!(raven_libc_personality_linux::error::format_error_at_line(
        &mut state, b"p", 0, b"a.c", 1, b"m", None, &mut out
    ));
    assert!(!raven_libc_personality_linux::error::format_error_at_line(
        &mut state, b"p", 0, b"a.c", 1, b"m", None, &mut out
    ));
}
