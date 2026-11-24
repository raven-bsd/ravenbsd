// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic meta-socket simulations: reorder, duplicate, failover, close, bounds.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Sent {
    subflow: u32,
    dsn: u64,
    data: Vec<u8>,
    fin: bool,
}

struct Mock {
    id: u32,
    log: Rc<RefCell<Vec<Sent>>>,
    fail: Rc<Cell<usize>>,
}

impl Subflow for Mock {
    fn send(&mut self, mapping: Mapping, payload: &[u8], data_fin: bool) -> Result<(), Error> {
        if self.fail.get() > 0 {
            self.fail.set(self.fail.get() - 1);
            return Err(Error::WouldBlock);
        }
        self.log.borrow_mut().push(Sent {
            subflow: self.id,
            dsn: mapping.data_seq,
            data: payload.to_vec(),
            fin: data_fin,
        });
        Ok(())
    }
}

struct Rig {
    socket: MetaSocket,
    log: Rc<RefCell<Vec<Sent>>>,
    fail: Rc<Cell<usize>>,
}

fn socket(mss: usize, send_cap: usize, rcv_cap: usize, window: u64) -> MetaSocket {
    MetaSocket::new(mss, send_cap, rcv_cap, window)
}

fn add(rig: &mut Rig, id: u32, backup: bool) {
    rig.socket
        .add_subflow(
            id,
            backup,
            Box::new(Mock {
                id,
                log: Rc::clone(&rig.log),
                fail: Rc::clone(&rig.fail),
            }),
        )
        .expect("subflow");
}

fn rig(mss: usize, send_cap: usize, rcv_cap: usize, window: u64) -> Rig {
    Rig {
        socket: socket(mss, send_cap, rcv_cap, window),
        log: Rc::new(RefCell::new(Vec::new())),
        fail: Rc::new(Cell::new(0)),
    }
}

fn deliver(socket: &mut MetaSocket, sent: &Sent) {
    let mapping = Mapping {
        data_seq: sent.dsn,
        subflow_seq: 0,
        data_len: u16::try_from(sent.data.len()).expect("length"),
        checksum: None,
        dsn_64: true,
    };
    socket
        .receive(&mapping, &sent.data, sent.fin)
        .expect("receive");
}

fn payload(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

fn read_all(socket: &mut MetaSocket, length: usize) -> Vec<u8> {
    let mut out = vec![0u8; length];
    assert_eq!(socket.read(&mut out), length, "short read");
    out
}

#[test]
fn peer_carries_no_data_while_the_primary_is_alive() {
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    add(&mut sender, 2, true);
    assert_eq!(sender.socket.send(&payload(2000)), Ok(2000));
    let log = sender.log.borrow();
    assert_eq!(log.len(), 2);
    assert!(log.iter().all(|sent| sent.subflow == 1));
    assert!(log.iter().all(|sent| sent.dsn == 0 || sent.dsn == 1000));
}

#[test]
fn a_primary_failure_reinjects_only_the_unacked_ranges() {
    let data = payload(3000);
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    add(&mut sender, 2, true);
    let mut receiver = rig(1000, 10_000, 10_000, 10_000);
    add(&mut receiver, 2, false);

    assert_eq!(sender.socket.send(&data), Ok(3000));
    let first = sender.log.borrow()[0].clone();
    deliver(&mut receiver.socket, &first);
    assert_eq!(read_all(&mut receiver.socket, 1000), data[..1000].to_vec());
    sender.socket.ack(1000);

    sender.socket.set_alive(1, false).expect("primary dies");
    let log = sender.log.borrow().clone();
    let reinjected: Vec<&Sent> = log.iter().filter(|sent| sent.subflow == 2).collect();
    assert_eq!(reinjected.len(), 2, "two unacked ranges");
    assert_eq!(reinjected[0].dsn, 1000);
    assert_eq!(reinjected[1].dsn, 2000);
    assert!(
        log.iter().filter(|sent| sent.subflow == 1).count() == 3,
        "the dead path sent each range once before the failure"
    );

    for sent in &reinjected {
        deliver(&mut receiver.socket, sent);
    }
    assert_eq!(read_all(&mut receiver.socket, 2000), data[1000..].to_vec());
    sender.socket.ack(receiver.socket.data_ack());
    assert_eq!(sender.socket.send_queue(), 0);
}

#[test]
fn changing_priority_moves_new_data_to_the_new_primary() {
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    add(&mut sender, 2, true);
    assert_eq!(sender.socket.send(&payload(1000)), Ok(1000));
    sender.socket.set_backup(1, true).expect("retire");
    sender.socket.set_backup(2, false).expect("promote");
    assert_eq!(sender.socket.send(&payload(1000)), Ok(1000));
    let log = sender.log.borrow();
    let last = log.last().expect("sent");
    assert_eq!(last.subflow, 2);
}

#[test]
fn out_of_order_and_duplicate_segments_deliver_the_stream_once() {
    let data = payload(3000);
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    let mut receiver = rig(1000, 10_000, 10_000, 10_000);
    add(&mut receiver, 2, false);
    assert_eq!(sender.socket.send(&data), Ok(3000));
    let sent = sender.log.borrow().clone();

    deliver(&mut receiver.socket, &sent[2]);
    deliver(&mut receiver.socket, &sent[0]);
    deliver(&mut receiver.socket, &sent[0]);
    deliver(&mut receiver.socket, &sent[1]);
    deliver(&mut receiver.socket, &sent[2]);
    assert_eq!(read_all(&mut receiver.socket, 3000), data);
    assert_eq!(receiver.socket.data_ack(), 3000);
    assert_eq!(receiver.socket.buffered(), 0);
}

#[test]
fn a_send_error_keeps_the_range_pending_for_the_next_pump() {
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    sender.fail.set(1);
    assert_eq!(sender.socket.send(&payload(1000)), Ok(1000));
    assert_eq!(sender.socket.send_queue(), 1000);
    assert_eq!(sender.socket.pump(), 1);
    assert_eq!(sender.send_log_count(), 1);
}

impl Rig {
    fn send_log_count(&self) -> usize {
        self.log.borrow().len()
    }
}

#[test]
fn a_slow_primary_is_retired_for_the_backup() {
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    add(&mut sender, 2, true);
    sender.fail.set(1);
    assert_eq!(sender.socket.send(&payload(1000)), Ok(1000));
    assert_eq!(sender.socket.send_queue(), 1000);
    sender.socket.set_alive(1, false).expect("retire slow path");
    let log = sender.log.borrow();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].subflow, 2);
    assert_eq!(log[0].dsn, 0);
}

#[test]
fn mp_fail_keeps_the_prefix_before_the_failed_dsn() {
    let mut receiver = rig(1000, 10_000, 10_000, 10_000);
    add(&mut receiver, 2, false);

    let first = payload(1000);
    let mapping = Mapping {
        data_seq: 0,
        subflow_seq: 0,
        data_len: 1000,
        checksum: None,
        dsn_64: true,
    };
    receiver
        .socket
        .receive(&mapping, &first, false)
        .expect("first range");
    assert_eq!(read_all(&mut receiver.socket, 1000), first);

    receiver.socket.on_mp_fail(1500);
    let second = payload(1000);
    let mapping = Mapping {
        data_seq: 1000,
        subflow_seq: 0,
        data_len: 1000,
        checksum: None,
        dsn_64: true,
    };
    receiver
        .socket
        .receive(&mapping, &second, false)
        .expect("prefix accepted");
    assert_eq!(read_all(&mut receiver.socket, 500), second[..500].to_vec());
    assert_eq!(receiver.socket.data_ack(), 1500);
}

#[test]
fn simultaneous_fin_is_connection_level_and_reads_eof() {
    let mut a = rig(1000, 10_000, 10_000, 10_000);
    let mut b = rig(1000, 10_000, 10_000, 10_000);
    add(&mut a, 1, false);
    add(&mut b, 2, false);
    a.socket.close();
    b.socket.close();
    let a_fin = a.log.borrow()[0].clone();
    let b_fin = b.log.borrow()[0].clone();
    assert!(a_fin.fin && b_fin.fin);
    deliver(&mut a.socket, &b_fin);
    deliver(&mut b.socket, &a_fin);
    assert!(a.socket.peer_fin());
    assert!(b.socket.peer_fin());
    let mut buffer = [0u8; 8];
    assert_eq!(a.socket.read(&mut buffer), 0);
    assert_eq!(b.socket.read(&mut buffer), 0);
    assert_eq!(a.socket.send(&[1]), Err(Error::Closed));
}

#[test]
fn mp_fail_discards_data_at_and_after_the_failed_dsn() {
    let data = payload(3000);
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    let mut receiver = rig(1000, 10_000, 10_000, 10_000);
    add(&mut receiver, 2, false);
    assert_eq!(sender.socket.send(&data), Ok(3000));
    let sent = sender.log.borrow().clone();
    deliver(&mut receiver.socket, &sent[0]);
    assert_eq!(read_all(&mut receiver.socket, 1000), data[..1000].to_vec());
    receiver.socket.on_mp_fail(1000);
    deliver(&mut receiver.socket, &sent[1]);
    deliver(&mut receiver.socket, &sent[2]);
    let mut buffer = [0u8; 8];
    assert_eq!(receiver.socket.read(&mut buffer), 0);
    assert_eq!(receiver.socket.buffered(), 0);
}

#[test]
fn fastclose_clears_the_connection_and_buffers() {
    let mut sender = rig(1000, 10_000, 10_000, 10_000);
    add(&mut sender, 1, false);
    let mut receiver = rig(1000, 10_000, 10_000, 10_000);
    add(&mut receiver, 2, false);
    assert_eq!(sender.socket.send(&payload(1000)), Ok(1000));
    let sent = sender.log.borrow()[0].clone();
    deliver(&mut receiver.socket, &sent);
    receiver.socket.on_fastclose();
    assert!(receiver.socket.is_closed());
    let mut buffer = [0u8; 8];
    assert_eq!(receiver.socket.read(&mut buffer), 0);
    assert_eq!(receiver.socket.buffered(), 0);
}

#[test]
fn send_and_receive_memory_are_bounded() {
    let mut sender = rig(1000, 1500, 10_000, 10_000);
    add(&mut sender, 1, false);
    assert_eq!(sender.socket.send(&payload(3000)), Ok(1500));
    assert_eq!(sender.socket.send_queue(), 1500);
    assert_eq!(sender.socket.send(&[1]), Err(Error::WouldBlock));

    let mut receiver = rig(1000, 10_000, 500, 1000);
    add(&mut receiver, 2, false);
    let mapping = Mapping {
        data_seq: 2000,
        subflow_seq: 0,
        data_len: 100,
        checksum: None,
        dsn_64: true,
    };
    assert_eq!(
        receiver.socket.receive(&mapping, &[0u8; 100], false),
        Err(Error::WindowViolation)
    );
    let mapping = Mapping {
        data_seq: 1000,
        subflow_seq: 0,
        data_len: 600,
        checksum: None,
        dsn_64: true,
    };
    assert_eq!(
        receiver.socket.receive(&mapping, &[0u8; 600], false),
        Err(Error::MemoryLimit)
    );
    assert_eq!(receiver.socket.buffered(), 0);
}
