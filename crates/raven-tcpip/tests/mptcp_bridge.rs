// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MPTCP2a: a MetaSocket over two real TCP subflows.
//!
//! The transport is the real `raven-tcpip` stack; the peer reads the actual TCP bytes and feeds
//! the DSS mapping metadata to a second MetaSocket. This proves the lifecycle bridge, DSS send,
//! receive reordering and failover without any kernel/netd/QEMU code.

#![cfg(all(feature = "mptcp", feature = "testutil"))]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use raven_abi::errno::Errno;
use raven_mptcp::{MetaSocket, MpCapableHandshake, SubflowReadiness};
use raven_tcpip::mptcp_bridge::Mapped;
use raven_tcpip::{IpAddr, SocketId, Stack, StackSubflow, SubflowId, SubflowOps};

use common::{A, B, address, pair};

type SharedStack = Rc<RefCell<Stack<raven_tcpip_sim::Endpoint>>>;

struct Rig {
    _link: raven_tcpip_sim::Link,
    a: SharedStack,
    b: SharedStack,
    accepted_primary: SocketId,
    accepted_backup: SocketId,
    primary: SubflowId,
    backup: SubflowId,
    socket: MetaSocket,
    primary_log: Rc<RefCell<Vec<Mapped>>>,
    backup_log: Rc<RefCell<Vec<Mapped>>>,
    now: i64,
}

impl Rig {
    fn new() -> Self {
        let (link, a, b) = pair();
        let a = Rc::new(RefCell::new(a));
        let b = Rc::new(RefCell::new(b));
        let listener = b.borrow_mut().tcp_listen(80, 8).expect("listen");
        let primary = a
            .borrow_mut()
            .open_subflow((IpAddr::V4(address(B)), 80), Some(address(A)))
            .expect("primary");
        let backup = a
            .borrow_mut()
            .open_subflow((IpAddr::V4(address(B)), 80), Some(address(A)))
            .expect("backup");
        let mut rig = Self {
            _link: link,
            a,
            b,
            accepted_primary: SocketId(0),
            accepted_backup: SocketId(0),
            primary,
            backup,
            socket: MetaSocket::new(1000, 10_000, 10_000, 10_000),
            primary_log: Rc::new(RefCell::new(Vec::new())),
            backup_log: Rc::new(RefCell::new(Vec::new())),
            now: 0,
        };
        rig.pump_to(200);
        rig.accepted_primary = rig
            .b
            .borrow_mut()
            .tcp_accept(listener)
            .expect("accept primary")
            .expect("primary connection");
        rig.accepted_backup = rig
            .b
            .borrow_mut()
            .tcp_accept(listener)
            .expect("accept backup")
            .expect("backup connection");

        let primary = StackSubflow::new(Rc::clone(&rig.a), rig.primary)
            .with_mapping_log(Rc::clone(&rig.primary_log));
        let backup = StackSubflow::new(Rc::clone(&rig.a), rig.backup)
            .with_mapping_log(Rc::clone(&rig.backup_log));
        rig.socket
            .add_subflow(rig.primary.0, false, Box::new(primary))
            .expect("add primary");
        rig.socket
            .add_subflow(rig.backup.0, true, Box::new(backup))
            .expect("add backup");
        rig
    }

    fn pump_to(&mut self, end: i64) {
        while self.now <= end {
            let now = raven_tcpip::Instant::from_millis(self.now);
            self.a.borrow_mut().poll(now);
            self.b.borrow_mut().poll(now);
            self.now += 5;
        }
    }

    fn read(&mut self, socket: SocketId, length: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buffer = [0u8; 2048];
        let mut attempts = 0;
        while out.len() < length {
            attempts += 1;
            assert!(
                attempts <= 4,
                "timed out waiting for {length} bytes (got {})",
                out.len()
            );
            let result = {
                let mut stack = self.b.borrow_mut();
                stack.recv(socket, &mut buffer)
            };
            match result {
                Ok(0) => break,
                Ok(count) => out.extend_from_slice(&buffer[..count]),
                Err(Errno::EAGAIN) => self.pump_to(self.now + 200),
                Err(error) => panic!("recv: {error:?}"),
            }
        }
        out
    }
}

fn payload(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

#[test]
fn phase1_handshake_and_dss_reorder_over_two_real_subflows() {
    let mut rig = Rig::new();
    let key_a = 0x1112_1314_1516_1718u64;
    let key_b = 0x0102_0304_0506_0708u64;
    let mut initiator = MpCapableHandshake::initiator(key_a, false);
    let mut responder = MpCapableHandshake::responder(key_b, false);
    let syn = initiator.syn();
    let synack = responder.accept_syn(&syn).expect("synack");
    let ack = initiator.accept_synack(&synack).expect("ack");
    responder.accept_ack(&ack).expect("third ack");
    assert_eq!(initiator.remote_key(), Some(key_b));
    assert_eq!(responder.remote_key(), Some(key_a));

    let data = payload(3000);
    assert_eq!(rig.socket.send(&data), Ok(3000));
    rig.pump_to(rig.now + 300);

    let wire_primary = rig.read(rig.accepted_primary, 3000);
    assert_eq!(wire_primary, data, "real TCP carries the exact stream");
    assert_eq!(rig.primary_log.borrow().len(), 3);
    assert!(rig.backup_log.borrow().is_empty(), "backup is idle");
    assert_eq!(
        rig.b.borrow().recv_queue_len(rig.accepted_backup),
        Ok(0),
        "backup carried no bytes"
    );

    let mut receiver = MetaSocket::new(1000, 10_000, 10_000, 10_000);
    // Deliver the real mappings in reverse order to exercise reordering.
    let log = Rc::clone(&rig.primary_log);
    let mapped = log.borrow().clone();
    for index in [2, 0, 1] {
        receiver
            .receive(&mapped[index].mapping, &mapped[index].data, false)
            .expect("reordered receive");
    }
    let mut out = vec![0u8; 3000];
    assert_eq!(receiver.read(&mut out), 3000);
    assert_eq!(out, data, "byte-exact after reorder");
    assert_eq!(receiver.data_ack(), 3000);
}

#[test]
fn failover_reinjects_only_the_unacked_real_ranges() {
    let mut rig = Rig::new();
    let data = payload(3000);
    assert_eq!(rig.socket.send(&data), Ok(3000));
    rig.pump_to(rig.now + 300);
    assert_eq!(rig.read(rig.accepted_primary, 3000), data);
    assert_eq!(rig.primary_log.borrow().len(), 3);
    assert!(rig.backup_log.borrow().is_empty());

    let mut receiver = MetaSocket::new(1000, 10_000, 10_000, 10_000);
    let first = rig.primary_log.borrow()[0].clone();
    receiver
        .receive(&first.mapping, &first.data, false)
        .expect("first range");
    let mut prefix = [0u8; 1000];
    assert_eq!(receiver.read(&mut prefix), 1000);
    assert_eq!(prefix.to_vec(), data[..1000].to_vec());
    rig.socket.ack(receiver.data_ack());
    assert_eq!(rig.socket.send_queue(), 2000);

    // The real primary TCP subflow is closed mid-stream. The bridge's live readiness marks it
    // closed and the meta-socket requeues DSN 1000/2000 onto the idle backup.
    assert!(matches!(
        rig.socket.subflow_readiness(rig.primary.0),
        Ok(SubflowReadiness { closed: false, .. })
    ));
    rig.a.borrow_mut().close_subflow(rig.primary);
    assert!(matches!(
        rig.socket.subflow_readiness(rig.primary.0),
        Ok(SubflowReadiness { closed: true, .. })
    ));
    rig.socket.refresh();
    rig.pump_to(rig.now + 300);

    let backup = rig.read(rig.accepted_backup, 2000);
    assert_eq!(backup, data[1000..].to_vec(), "backup completes the stream");
    let log = Rc::clone(&rig.backup_log);
    let mapped = log.borrow().clone();
    assert_eq!(mapped.len(), 2, "only the two unacked ranges");
    assert_eq!(mapped[0].mapping.data_seq, 1000);
    assert_eq!(mapped[1].mapping.data_seq, 2000);
    for item in mapped {
        receiver
            .receive(&item.mapping, &item.data, false)
            .expect("backup receive");
    }
    let mut suffix = vec![0u8; 2000];
    assert_eq!(receiver.read(&mut suffix), 2000);
    assert_eq!(suffix, data[1000..].to_vec(), "no duplicate/loss");
    assert_eq!(receiver.data_ack(), 3000);
    rig.socket.ack(receiver.data_ack());
    assert_eq!(rig.socket.send_queue(), 0);
}

#[test]
fn readiness_is_live_before_and_after_a_real_write() {
    let mut rig = Rig::new();
    let primary = rig.primary;
    let readiness = SubflowOps::readiness(&*rig.a.borrow(), primary);
    assert!(readiness.writable);
    assert!(!readiness.readable);

    assert_eq!(
        rig.b
            .borrow_mut()
            .send(rig.accepted_primary, b"reply")
            .expect("peer write"),
        5
    );
    rig.pump_to(rig.now + 200);
    let readiness = rig.socket.subflow_readiness(primary.0).expect("live");
    assert!(readiness.readable, "new bytes are visible without a poll");
    let mut buffer = [0u8; 8];
    assert_eq!(rig.a.borrow_mut().recv_from(primary, &mut buffer), Ok(5));
    assert_eq!(&buffer[..5], b"reply");
}
