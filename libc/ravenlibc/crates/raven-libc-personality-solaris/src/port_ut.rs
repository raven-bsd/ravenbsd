// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Tests for `port.rs` (C67 4.3 rules 1-8 and the 5.D scenarios).

use raven_libc_abi::poll::{
    POLLERR, POLLHUP, POLLIN, POLLNVAL, POLLOUT, POLLPRI, POLLRDBAND, POLLRDNORM, POLLWRNORM,
};
use raven_libc_abi::structs::Timespec;

use crate::consts::{
    DOORBELL_COOKIE, ETIME, PORT_MAX_ASSOCIATIONS, PORT_MAX_PORTS, PORT_MAX_QUEUED_USER_EVENTS,
    PORT_SOURCE_ALERT, PORT_SOURCE_FD, PORT_SOURCE_TIMER, PORT_SOURCE_USER, PortEvent,
};
use crate::port::{
    self, EV_HUP, EV_READABLE, EV_SIGNALED, EV_WRITABLE, PortTable, cookie, cookie_epoch,
    cookie_fd, evq_to_poll, poll_to_evq,
};
use crate::seams::{Errno, EvqCtl, FdKind, FdTable, Handle};
use crate::testutil::{FakeDoorbell, FakeEvq, FakeFdTable};

struct Fixture {
    table: PortTable,
    evq: FakeEvq,
    bell: FakeDoorbell,
    fds: FakeFdTable,
    port: i32,
}

fn fixture() -> Fixture {
    let mut table = PortTable::new();
    let mut evq = FakeEvq::new();
    let mut bell = FakeDoorbell::new();
    let mut fds = FakeFdTable::new();
    let port = port::create(&mut table, &mut evq, &mut bell, &mut fds).unwrap();
    fds.insert_at(5, Handle(5), FdKind::Pipe);
    fds.insert_at(6, Handle(6), FdKind::Socket);
    Fixture {
        table,
        evq,
        bell,
        fds,
        port,
    }
}

fn associate_readable(f: &mut Fixture, fd: i32, user: usize) -> Result<(), Errno> {
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        usize::try_from(fd).unwrap(),
        POLLIN,
        user,
    )
}

fn ready(f: &mut Fixture, fd: i32, events: u32) {
    let epoch = f.fds.epoch(fd);
    f.evq.push(cookie(fd, epoch), events);
}

fn get_zero(f: &mut Fixture) -> Result<PortEvent, Errno> {
    port::get(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        Some(&Timespec::ZERO),
    )
}

#[test]
fn create_installs_a_port_descriptor() {
    let f = fixture();
    assert_eq!(f.fds.kind_of(f.port), Ok(FdKind::Port));
    assert_eq!(f.table.len(), 1);
    assert_eq!(f.bell.created, 1);
    assert_eq!(f.evq.ops.len(), 1);
    assert_eq!(f.evq.ops[0].0, EvqCtl::Add);
    assert_eq!(f.evq.ops[0].2, EV_SIGNALED);
    assert_eq!(f.evq.ops[0].3, DOORBELL_COOKIE);
}

#[test]
fn create_registers_the_doorbell_once() {
    let f = fixture();
    let adds = f
        .evq
        .ops
        .iter()
        .filter(|(op, _, _, _)| *op == EvqCtl::Add)
        .count();
    assert_eq!(adds, 1);
}

#[test]
fn create_evq_error_passes_through() {
    let mut table = PortTable::new();
    let mut evq = FakeEvq::new();
    evq.create_error = Some(Errno::ENOMEM);
    let mut bell = FakeDoorbell::new();
    let mut fds = FakeFdTable::new();
    assert_eq!(
        port::create(&mut table, &mut evq, &mut bell, &mut fds),
        Err(Errno::ENOMEM)
    );
    assert!(table.is_empty());
}

#[test]
fn create_doorbell_error_passes_through() {
    let mut table = PortTable::new();
    let mut evq = FakeEvq::new();
    let mut bell = FakeDoorbell::new();
    bell.create_error = Some(Errno::EMFILE);
    let mut fds = FakeFdTable::new();
    assert_eq!(
        port::create(&mut table, &mut evq, &mut bell, &mut fds),
        Err(Errno::EMFILE)
    );
}

#[test]
fn create_fd_table_full_is_emfile() {
    let mut table = PortTable::new();
    let mut evq = FakeEvq::new();
    let mut bell = FakeDoorbell::new();
    let mut fds = FakeFdTable::new();
    fds.install_error = Some(Errno::EMFILE);
    assert_eq!(
        port::create(&mut table, &mut evq, &mut bell, &mut fds),
        Err(Errno::EMFILE)
    );
}

#[test]
fn create_caps_the_process_at_256_ports() {
    let mut table = PortTable::new();
    let mut evq = FakeEvq::new();
    let mut bell = FakeDoorbell::new();
    let mut fds = FakeFdTable::new();
    for _ in 0..PORT_MAX_PORTS {
        port::create(&mut table, &mut evq, &mut bell, &mut fds).unwrap();
    }
    assert_eq!(
        port::create(&mut table, &mut evq, &mut bell, &mut fds),
        Err(Errno::EMFILE)
    );
    assert_eq!(table.len(), PORT_MAX_PORTS);
}

#[test]
fn create_two_ports_are_independent() {
    let f = fixture();
    let mut f = f;
    let second = port::create(&mut f.table, &mut f.evq, &mut f.bell, &mut f.fds).unwrap();
    assert_ne!(second, f.port);
    assert_eq!(f.table.len(), 2);
}

#[test]
fn create_get_timeout_is_etime() {
    let mut f = fixture();
    let ten_ms = Timespec {
        tv_sec: 0,
        tv_nsec: 10_000_000,
    };
    let result = port::get(&mut f.table, &mut f.evq, &f.fds, f.port, Some(&ten_ms));
    assert_eq!(result, Err(ETIME));
    assert_eq!(f.evq.last_timeout(), Some(10_000_000));
}

#[test]
fn get_null_timeout_waits_forever() {
    let mut f = fixture();
    let result = port::get(&mut f.table, &mut f.evq, &f.fds, f.port, None);
    assert_eq!(result, Err(ETIME));
    assert_eq!(f.evq.last_timeout(), Some(0));
}

#[test]
fn get_zero_timeout_polls() {
    let mut f = fixture();
    let result = get_zero(&mut f);
    assert_eq!(result, Err(ETIME));
    assert_eq!(f.evq.last_timeout(), Some(1));
}

#[test]
fn associate_then_get_returns_the_event() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 0xAA).unwrap();
    ready(&mut f, 5, EV_READABLE);
    let event = get_zero(&mut f).unwrap();
    assert_eq!(event.portev_source, PORT_SOURCE_FD as u16);
    assert_eq!(event.portev_object, 5);
    assert_eq!(event.portev_events, POLLIN);
    assert_eq!(event.portev_user, 0xAA);
}

#[test]
fn delivery_is_one_shot() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    ready(&mut f, 5, EV_READABLE);
    get_zero(&mut f).unwrap();
    assert_eq!(get_zero(&mut f), Err(ETIME));
    assert_eq!(f.table.association_count(f.port), Some(0));
}

#[test]
fn delivery_sends_evq_del() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    ready(&mut f, 5, EV_READABLE);
    get_zero(&mut f).unwrap();
    assert_eq!(f.evq.deletions(), 1);
}

#[test]
fn reassociate_after_delivery_works_again() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    ready(&mut f, 5, EV_READABLE);
    get_zero(&mut f).unwrap();
    associate_readable(&mut f, 5, 2).unwrap();
    ready(&mut f, 5, EV_READABLE);
    let event = get_zero(&mut f).unwrap();
    assert_eq!(event.portev_user, 2);
}

#[test]
fn double_associate_replaces_events_and_user() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLIN,
        1,
    )
    .unwrap();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLOUT,
        2,
    )
    .unwrap();
    let adds = f
        .evq
        .ops
        .iter()
        .filter(|(op, _, _, _)| *op == EvqCtl::Add)
        .count();
    let mods = f
        .evq
        .ops
        .iter()
        .filter(|(op, _, _, _)| *op == EvqCtl::Mod)
        .count();
    assert_eq!(adds, 2); // the doorbell plus the fd
    assert_eq!(mods, 1);
    assert_eq!(f.table.association_count(f.port), Some(1));
    ready(&mut f, 5, EV_WRITABLE);
    let event = get_zero(&mut f).unwrap();
    assert_eq!(event.portev_events, POLLOUT);
    assert_eq!(event.portev_user, 2);
}

#[test]
fn dissociate_removes_the_association() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    port::dissociate(&mut f.table, &mut f.evq, &f.fds, f.port, PORT_SOURCE_FD, 5).unwrap();
    assert_eq!(f.table.association_count(f.port), Some(0));
    ready(&mut f, 5, EV_READABLE);
    assert_eq!(get_zero(&mut f), Err(ETIME));
}

#[test]
fn second_dissociate_is_enoent() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    port::dissociate(&mut f.table, &mut f.evq, &f.fds, f.port, PORT_SOURCE_FD, 5).unwrap();
    assert_eq!(
        port::dissociate(&mut f.table, &mut f.evq, &f.fds, f.port, PORT_SOURCE_FD, 5),
        Err(Errno::ENOENT)
    );
}

#[test]
fn dissociate_after_delivery_is_enoent() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    ready(&mut f, 5, EV_READABLE);
    get_zero(&mut f).unwrap();
    assert_eq!(
        port::dissociate(&mut f.table, &mut f.evq, &f.fds, f.port, PORT_SOURCE_FD, 5),
        Err(Errno::ENOENT)
    );
}

#[test]
fn stale_fd_cookie_is_dropped() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    let old_epoch = f.fds.epoch(5);
    let old_cookie = cookie(5, old_epoch);
    f.fds.close(5);
    f.fds.insert_at(5, Handle(55), FdKind::Pipe);
    f.evq.push(old_cookie, EV_READABLE);
    assert_eq!(get_zero(&mut f), Err(ETIME));
}

#[test]
fn stale_fd_number_reuse_is_not_delivered() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    f.fds.close(5);
    f.fds.insert_at(5, Handle(55), FdKind::Pipe);
    ready(&mut f, 5, EV_READABLE);
    assert_eq!(get_zero(&mut f), Err(ETIME));
}

#[test]
fn closed_fd_association_is_ebadf() {
    let mut f = fixture();
    f.fds.close(5);
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_FD,
            5,
            POLLIN,
            1
        ),
        Err(Errno::EBADF)
    );
}

#[test]
fn send_is_returned_before_fd_events() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 0xAA).unwrap();
    port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 7, 1).unwrap();
    ready(&mut f, 5, EV_READABLE);
    let mut out = [PortEvent::default(); 4];
    let mut nget = 0u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        &mut out,
        &mut nget,
        None,
    )
    .unwrap();
    assert_eq!(nget, 2);
    assert_eq!(out[0].portev_source, PORT_SOURCE_USER as u16);
    assert_eq!(out[0].portev_events, 7);
    assert_eq!(out[0].portev_user, 1);
    assert_eq!(out[1].portev_source, PORT_SOURCE_FD as u16);
    assert_eq!(out[1].portev_object, 5);
    assert_eq!(out[1].portev_user, 0xAA);
}

#[test]
fn send_rings_the_doorbell() {
    let mut f = fixture();
    port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 1, 2).unwrap();
    assert_eq!(f.bell.signals.len(), 1);
}

#[test]
fn send_fifo_order_is_kept() {
    let mut f = fixture();
    port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 1, 10).unwrap();
    port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 2, 20).unwrap();
    let first = get_zero(&mut f).unwrap();
    let second = get_zero(&mut f).unwrap();
    assert_eq!(first.portev_user, 10);
    assert_eq!(second.portev_user, 20);
}

#[test]
fn send_full_is_eagain() {
    let mut f = fixture();
    for index in 0..PORT_MAX_QUEUED_USER_EVENTS {
        port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 1, index).unwrap();
    }
    assert_eq!(
        port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 1, 99),
        Err(Errno::EAGAIN)
    );
    get_zero(&mut f).unwrap();
    assert!(port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 1, 100).is_ok());
}

#[test]
fn send_signal_error_rolls_back() {
    let mut f = fixture();
    f.bell.signal_error = Some(Errno::EIO);
    assert_eq!(
        port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 1, 2),
        Err(Errno::EIO)
    );
    assert_eq!(f.table.queued(f.port), Some(0));
}

#[test]
fn send_on_a_bad_port_is_ebadf() {
    let mut f = fixture();
    assert_eq!(
        port::send(&mut f.table, &mut f.bell, &f.fds, 99, 1, 2),
        Err(Errno::EBADF)
    );
}

#[test]
fn getn_min_zero_returns_what_is_ready() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    ready(&mut f, 5, EV_READABLE);
    let mut out = [PortEvent::default(); 4];
    let mut nget = 0u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        &mut out,
        &mut nget,
        None,
    )
    .unwrap();
    assert_eq!(nget, 1);
}

#[test]
fn getn_min_zero_with_nothing_is_ok_zero() {
    let mut f = fixture();
    let mut out = [PortEvent::default(); 4];
    let mut nget = 0u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        &mut out,
        &mut nget,
        None,
    )
    .unwrap();
    assert_eq!(nget, 0);
}

#[test]
fn getn_min_three_returns_the_partial_count() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    associate_readable(&mut f, 6, 2).unwrap();
    ready(&mut f, 5, EV_READABLE);
    ready(&mut f, 6, EV_READABLE);
    let five_ms = Timespec {
        tv_sec: 0,
        tv_nsec: 5_000_000,
    };
    let mut out = [PortEvent::default(); 4];
    let mut nget = 3u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        &mut out,
        &mut nget,
        Some(&five_ms),
    )
    .unwrap();
    assert_eq!(nget, 2);
    assert_eq!(f.evq.last_timeout(), Some(5_000_000));
}

#[test]
fn getn_min_three_with_nothing_is_etime() {
    let mut f = fixture();
    let five_ms = Timespec {
        tv_sec: 0,
        tv_nsec: 5_000_000,
    };
    let mut out = [PortEvent::default(); 4];
    let mut nget = 3u32;
    assert_eq!(
        port::getn(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            &mut out,
            &mut nget,
            Some(&five_ms)
        ),
        Err(ETIME)
    );
}

#[test]
fn getn_min_above_max_is_einval() {
    let mut f = fixture();
    let mut out = [PortEvent::default(); 2];
    let mut nget = 3u32;
    assert_eq!(
        port::getn(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            &mut out,
            &mut nget,
            None
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn getn_overflow_keeps_the_extra_event_queued() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    associate_readable(&mut f, 6, 2).unwrap();
    ready(&mut f, 5, EV_READABLE);
    ready(&mut f, 6, EV_READABLE);
    let mut out = [PortEvent::default(); 1];
    let mut nget = 0u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        &mut out,
        &mut nget,
        Some(&Timespec::ZERO),
    )
    .unwrap();
    assert_eq!(nget, 1);
    assert_eq!(f.table.association_count(f.port), Some(0));
    assert_eq!(f.table.queued(f.port), Some(1));
    let extra = get_zero(&mut f).unwrap();
    assert_eq!(extra.portev_object, 6);
}

#[test]
fn getn_nget_receives_the_count() {
    let mut f = fixture();
    port::send(&mut f.table, &mut f.bell, &f.fds, f.port, 1, 1).unwrap();
    let mut out = [PortEvent::default(); 8];
    let mut nget = 0u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        &mut out,
        &mut nget,
        None,
    )
    .unwrap();
    assert_eq!(nget, 1);
}

#[test]
fn gate_4096_associations_then_enomem() {
    let mut f = fixture();
    for index in 0..PORT_MAX_ASSOCIATIONS {
        let fd = i32::try_from(10usize.saturating_add(index)).unwrap();
        f.fds
            .insert_at(fd, Handle(u32::try_from(fd).unwrap()), FdKind::Pipe);
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_FD,
            usize::try_from(fd).unwrap(),
            POLLIN,
            index,
        )
        .unwrap();
    }
    let extra = i32::try_from(10usize.saturating_add(PORT_MAX_ASSOCIATIONS)).unwrap();
    f.fds
        .insert_at(extra, Handle(u32::try_from(extra).unwrap()), FdKind::Pipe);
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_FD,
            usize::try_from(extra).unwrap(),
            POLLIN,
            0
        ),
        Err(Errno::ENOMEM)
    );
}

#[test]
fn gate_replacing_an_association_does_not_hit_the_cap() {
    let mut f = fixture();
    for index in 0..PORT_MAX_ASSOCIATIONS {
        let fd = i32::try_from(10usize.saturating_add(index)).unwrap();
        f.fds
            .insert_at(fd, Handle(u32::try_from(fd).unwrap()), FdKind::Pipe);
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_FD,
            usize::try_from(fd).unwrap(),
            POLLIN,
            index,
        )
        .unwrap();
    }
    assert!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_FD,
            10,
            POLLOUT,
            7
        )
        .is_ok()
    );
}

#[test]
fn source_timer_is_einval() {
    let mut f = fixture();
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_TIMER,
            5,
            POLLIN,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn source_alert_is_einval() {
    let mut f = fixture();
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_ALERT,
            5,
            POLLIN,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn source_user_is_einval() {
    let mut f = fixture();
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_USER,
            5,
            POLLIN,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn source_unknown_is_einval() {
    let mut f = fixture();
    assert_eq!(
        port::associate(&mut f.table, &mut f.evq, &f.fds, f.port, 99, 5, POLLIN, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn events_zero_is_einval() {
    let mut f = fixture();
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_FD,
            5,
            0,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn events_unmapped_is_einval() {
    let mut f = fixture();
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_FD,
            5,
            POLLNVAL,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mapping_pollin_is_readable() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLIN,
        0,
    )
    .unwrap();
    assert_eq!(f.evq.ops.last().map(|op| op.2), Some(EV_READABLE));
}

#[test]
fn mapping_pollpri_is_readable() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLPRI,
        0,
    )
    .unwrap();
    assert_eq!(f.evq.ops.last().map(|op| op.2), Some(EV_READABLE));
}

#[test]
fn mapping_pollout_is_writable() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLOUT,
        0,
    )
    .unwrap();
    assert_eq!(f.evq.ops.last().map(|op| op.2), Some(EV_WRITABLE));
}

#[test]
fn mapping_pollerr_is_hup() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLERR,
        0,
    )
    .unwrap();
    assert_eq!(f.evq.ops.last().map(|op| op.2), Some(EV_HUP));
}

#[test]
fn mapping_pollhup_is_hup() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLHUP,
        0,
    )
    .unwrap();
    assert_eq!(f.evq.ops.last().map(|op| op.2), Some(EV_HUP));
}

#[test]
fn mapping_pollnval_is_unmapped() {
    assert_eq!(poll_to_evq(POLLNVAL), Err(Errno::EINVAL));
}

#[test]
fn mapping_pollrdnorm_is_readable() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLRDNORM,
        0,
    )
    .unwrap();
    assert_eq!(f.evq.ops.last().map(|op| op.2), Some(EV_READABLE));
}

#[test]
fn mapping_pollwrnorm_is_writable() {
    let mut f = fixture();
    port::associate(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        PORT_SOURCE_FD,
        5,
        POLLWRNORM,
        0,
    )
    .unwrap();
    assert_eq!(f.evq.ops.last().map(|op| op.2), Some(EV_WRITABLE));
}

#[test]
fn mapping_combined_bits() {
    assert_eq!(
        poll_to_evq(POLLIN | POLLOUT | POLLHUP),
        Ok(EV_READABLE | EV_WRITABLE | EV_HUP)
    );
}

#[test]
fn output_mapping_is_poll_bits() {
    assert_eq!(evq_to_poll(EV_READABLE), POLLIN);
    assert_eq!(evq_to_poll(EV_WRITABLE), POLLOUT);
    assert_eq!(evq_to_poll(EV_HUP), POLLHUP);
    assert_eq!(evq_to_poll(EV_SIGNALED), 0);
}

#[test]
fn doorbell_event_is_not_a_port_event() {
    let mut f = fixture();
    f.evq.push(DOORBELL_COOKIE, EV_SIGNALED);
    assert_eq!(get_zero(&mut f), Err(ETIME));
}

#[test]
fn unknown_cookie_is_dropped() {
    let mut f = fixture();
    f.evq.push(cookie(99, 1), EV_READABLE);
    assert_eq!(get_zero(&mut f), Err(ETIME));
}

#[test]
fn event_without_a_mapped_bit_is_dropped() {
    let mut f = fixture();
    associate_readable(&mut f, 5, 1).unwrap();
    ready(&mut f, 5, EV_SIGNALED);
    assert_eq!(get_zero(&mut f), Err(ETIME));
}

#[test]
fn get_evq_error_passes_through() {
    let mut f = fixture();
    f.evq.wait_error = Some(Errno::EINTR);
    assert_eq!(get_zero(&mut f), Err(Errno::EINTR));
}

#[test]
fn closed_port_is_ebadf() {
    let mut f = fixture();
    f.fds.close(f.port);
    assert_eq!(get_zero(&mut f), Err(Errno::EBADF));
}

#[test]
fn non_port_descriptor_is_ebadf() {
    let mut f = fixture();
    let result = port::get(&mut f.table, &mut f.evq, &f.fds, 5, Some(&Timespec::ZERO));
    assert_eq!(result, Err(Errno::EBADF));
}

#[test]
fn child_after_fork_sees_ebadf() {
    let mut f = fixture();
    // The child inherits neither the EVQ handle nor the descriptor entry (C14).
    let child = FakeFdTable::new();
    assert_eq!(
        port::get(
            &mut f.table,
            &mut f.evq,
            &child,
            f.port,
            Some(&Timespec::ZERO)
        ),
        Err(Errno::EBADF)
    );
    assert_eq!(
        port::send(&mut f.table, &mut f.bell, &child, f.port, 1, 2),
        Err(Errno::EBADF)
    );
    assert_eq!(
        port::associate(
            &mut f.table,
            &mut f.evq,
            &child,
            f.port,
            PORT_SOURCE_FD,
            5,
            POLLIN,
            0
        ),
        Err(Errno::EBADF)
    );
}

#[test]
fn alert_is_enosys_for_a_valid_port() {
    let f = fixture();
    assert_eq!(port::alert(&f.fds, f.port), Err(Errno::ENOSYS));
}

#[test]
fn alert_validates_the_port_first() {
    let f = fixture();
    assert_eq!(port::alert(&f.fds, 99), Err(Errno::EBADF));
}

#[test]
fn alert_on_a_non_port_is_ebadf() {
    let f = fixture();
    assert_eq!(port::alert(&f.fds, 5), Err(Errno::EBADF));
}

#[test]
fn cookie_roundtrip() {
    let value = cookie(1234, 0xabcd_ef01);
    assert_eq!(cookie_fd(value), 1234);
    assert_eq!(cookie_epoch(value), 0xabcd_ef01);
}

#[test]
fn port_table_reports_its_state() {
    let mut f = fixture();
    assert_eq!(f.table.association_count(f.port), Some(0));
    assert_eq!(f.table.queued(f.port), Some(0));
    assert!(f.table.state(f.port).is_some());
    assert!(f.table.remove(f.port).is_some());
    assert!(f.table.is_empty());
}

#[test]
fn dissociate_rejects_another_source() {
    let mut f = fixture();
    assert_eq!(
        port::dissociate(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            PORT_SOURCE_TIMER,
            5
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn getn_rejects_a_bad_port() {
    let mut f = fixture();
    let mut out = [PortEvent::default(); 2];
    let mut nget = 0u32;
    assert_eq!(
        port::getn(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            99,
            &mut out,
            &mut nget,
            None
        ),
        Err(Errno::EBADF)
    );
}

#[test]
fn getn_with_zero_room_and_zero_min_is_ok() {
    let mut f = fixture();
    let mut out: [PortEvent; 0] = [];
    let mut nget = 0u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        f.port,
        &mut out,
        &mut nget,
        None,
    )
    .unwrap();
    assert_eq!(nget, 0);
}

#[test]
fn getn_rejects_a_negative_timeout() {
    let mut f = fixture();
    let bad = Timespec {
        tv_sec: -1,
        tv_nsec: 0,
    };
    let mut out = [PortEvent::default(); 2];
    let mut nget = 0u32;
    assert_eq!(
        port::getn(
            &mut f.table,
            &mut f.evq,
            &f.fds,
            f.port,
            &mut out,
            &mut nget,
            Some(&bad)
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn event_table_mapping_for_every_poll_bit_that_maps() {
    let cases = [
        (POLLIN, EV_READABLE),
        (POLLPRI, EV_READABLE),
        (POLLOUT, EV_WRITABLE),
        (POLLERR, EV_HUP),
        (POLLHUP, EV_HUP),
        (POLLRDNORM, EV_READABLE),
        (POLLWRNORM, EV_WRITABLE),
    ];
    for (poll, evq) in cases {
        assert_eq!(poll_to_evq(poll), Ok(evq), "poll bit {poll:#x}");
    }
}

#[test]
fn event_table_ignores_unrelated_bits() {
    // `POLLRDBAND` is neither mapped nor fatal when a mapped bit is present.
    assert_eq!(poll_to_evq(POLLRDBAND | POLLIN), Ok(EV_READABLE));
}

#[test]
fn many_ports_are_independent() {
    let mut f = fixture();
    let second = port::create(&mut f.table, &mut f.evq, &mut f.bell, &mut f.fds).unwrap();
    associate_readable(&mut f, 5, 1).unwrap();
    ready(&mut f, 5, EV_READABLE);
    assert!(get_zero(&mut f).is_ok());
    let mut out = [PortEvent::default(); 2];
    let mut nget = 0u32;
    port::getn(
        &mut f.table,
        &mut f.evq,
        &f.fds,
        second,
        &mut out,
        &mut nget,
        Some(&Timespec::ZERO),
    )
    .unwrap();
    assert_eq!(nget, 0);
}
