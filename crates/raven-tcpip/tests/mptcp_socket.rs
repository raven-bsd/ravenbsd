// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MPTCP2b: `IPPROTO_MPTCP` socket API, path manager and fallback over the host stack.

#![cfg(all(feature = "mptcp", feature = "testutil"))]

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use raven_abi::errno::Errno;
use raven_tcpip::{AF_INET, IPPROTO_MPTCP, IpAddr, MptcpSocket, SocketId, Stack};

use common::{A, B, address, pair};

type Shared = Rc<RefCell<Stack<raven_tcpip_sim::Endpoint>>>;

struct Rig {
    _link: raven_tcpip_sim::Link,
    a: Shared,
    b: Shared,
    listener: SocketId,
    now: i64,
}

impl Rig {
    fn new() -> Self {
        let (link, a, b) = pair();
        let a = Rc::new(RefCell::new(a));
        let b = Rc::new(RefCell::new(b));
        let listener = b.borrow_mut().tcp_listen(80, 8).expect("listen");
        Self {
            _link: link,
            a,
            b,
            listener,
            now: 0,
        }
    }

    fn open(&self, capable: bool) -> MptcpSocket<raven_tcpip_sim::Endpoint> {
        MptcpSocket::open(
            Rc::clone(&self.a),
            AF_INET,
            IPPROTO_MPTCP,
            (IpAddr::V4(address(B)), 80),
            Some(address(A)),
            capable,
        )
        .expect("mptcp open")
    }

    fn pump_to(&mut self, end: i64) {
        while self.now <= end {
            let now = raven_tcpip::Instant::from_millis(self.now);
            self.a.borrow_mut().poll(now);
            self.b.borrow_mut().poll(now);
            self.now += 5;
        }
    }

    fn accept(&mut self) -> SocketId {
        self.pump_to(self.now + 200);
        self.b
            .borrow_mut()
            .tcp_accept(self.listener)
            .expect("accept")
            .expect("connection")
    }

    fn drain(&mut self, socket: SocketId) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buffer = [0u8; 2048];
        loop {
            let result = {
                let mut stack = self.b.borrow_mut();
                stack.recv(socket, &mut buffer)
            };
            match result {
                Ok(0) => break,
                Ok(count) => out.extend_from_slice(&buffer[..count]),
                Err(Errno::EAGAIN) => break,
                Err(error) => panic!("recv: {error:?}"),
            }
        }
        out
    }
}

#[test]
fn ipproto_mptcp_opens_two_paths_and_moves_data() {
    let mut rig = Rig::new();
    let mut socket = rig.open(true);
    let primary = rig.accept();
    let backup = socket
        .add_path((IpAddr::V4(address(B)), 80), Some(address(A)), true)
        .expect("add backup");
    let backup_peer = rig.accept();

    let subflows = socket.subflows();
    assert_eq!(subflows.len(), 2);
    assert!(!subflows[0].backup || !subflows[1].backup);
    assert!(subflows.iter().any(|path| path.backup));

    assert_eq!(socket.send(b"hello"), Ok(5));
    rig.pump_to(rig.now + 200);
    assert_eq!(rig.drain(primary), b"hello");

    assert_eq!(
        rig.b
            .borrow_mut()
            .send(primary, b"reply")
            .expect("peer write"),
        5
    );
    rig.pump_to(rig.now + 200);
    assert!(socket.readiness().readable, "live readiness sees the reply");
    let mut buffer = [0u8; 8];
    assert_eq!(socket.recv(&mut buffer), Ok(5));
    assert_eq!(&buffer[..5], b"reply");

    // Promote the backup: regular paths are preferred for new data.
    let primary_subflow = socket.subflows()[0].subflow_id;
    socket
        .set_backup(primary_subflow, true)
        .expect("retire primary");
    assert_eq!(socket.set_backup(backup, false), Ok(()));
    assert_eq!(socket.send(b"world"), Ok(5));
    rig.pump_to(rig.now + 200);
    assert_eq!(rig.drain(backup_peer), b"world");

    assert_eq!(socket.remove_path(backup), Ok(()));
    assert_eq!(socket.subflows().len(), 1);
    assert_eq!(socket.close(), Ok(()));
    assert!(socket.is_closed());
}

#[test]
fn a_plain_peer_falls_back_to_one_tcp_connection() {
    let mut rig = Rig::new();
    let mut socket = rig.open(false);
    assert!(socket.is_fallback());
    let peer = rig.accept();
    assert_eq!(socket.send(b"plain"), Ok(5));
    rig.pump_to(rig.now + 200);
    assert_eq!(rig.drain(peer), b"plain");

    assert_eq!(rig.b.borrow_mut().send(peer, b"tcp").expect("peer"), 3);
    rig.pump_to(rig.now + 200);
    assert!(socket.readiness().readable);
    let mut buffer = [0u8; 8];
    assert_eq!(socket.recv(&mut buffer), Ok(3));
    assert_eq!(&buffer[..3], b"tcp");
    assert_eq!(
        socket.add_path((IpAddr::V4(address(B)), 80), Some(address(A)), true),
        Err(Errno::EOPNOTSUPP)
    );
    assert_eq!(socket.close(), Ok(()));
}

#[test]
fn aggregation_is_opt_in_and_uses_both_regular_paths() {
    let mut rig = Rig::new();
    let mut socket = rig.open(true);
    let first = rig.accept();
    let _second = socket
        .add_path((IpAddr::V4(address(B)), 80), Some(address(A)), false)
        .expect("second path");
    let second_peer = rig.accept();
    assert!(!socket.aggregation(), "failover is the default");

    socket.set_aggregation(true);
    assert!(socket.aggregation());
    let data: Vec<u8> = (0..3000).map(|index| (index % 251) as u8).collect();
    assert_eq!(socket.send(&data), Ok(3000));
    rig.pump_to(rig.now + 300);
    let first_bytes = rig.drain(first);
    let second_bytes = rig.drain(second_peer);
    assert!(!first_bytes.is_empty(), "first regular path carried data");
    assert!(!second_bytes.is_empty(), "second regular path carried data");
    assert_eq!(first_bytes.len() + second_bytes.len(), data.len());
    assert_eq!(socket.subflows().len(), 2);
    assert_eq!(socket.close(), Ok(()));
}
