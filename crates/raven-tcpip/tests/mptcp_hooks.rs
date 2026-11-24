// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MPTCP1b: the object-safe subflow surface over plain TCP.
//!
//! Two ordinary TCP connections stand in for MPTCP subflows. The test steers bytes over each,
//! checks live readiness after a drain, a full send window and a close, and exercises the backup
//! flag. `MPTCP_HOOKS_STALE_READINESS=1` enables the test-only cached-readiness mutation; the live
//! assertions must then fail (the negative check).

#![cfg(feature = "testutil")]

mod common;

use raven_abi::errno::Errno;
use raven_tcpip::{IpAddr, SubflowOps};

use common::{A, B, address, pair, pump_ms};

fn open(
    a: &mut raven_tcpip::Stack<raven_tcpip_sim::Endpoint>,
    port: u16,
) -> raven_tcpip::SubflowId {
    a.open_subflow((IpAddr::V4(address(B)), port), Some(address(A)))
        .expect("open_subflow")
}

#[test]
fn subflow_hooks_steer_and_live_readiness() {
    let (_link, mut a, mut b) = pair();
    let listener = b.tcp_listen(80, 8).expect("listen");
    let first = open(&mut a, 80);
    let second = open(&mut a, 80);
    assert_ne!(first, second);
    assert_eq!(a.subflow_local_if(first), Some(address(A)));

    pump_ms(&mut a, &mut b, 0, 200);
    let accepted_first = b
        .tcp_accept(listener)
        .expect("accept first")
        .expect("first connection");
    let accepted_second = b
        .tcp_accept(listener)
        .expect("accept second")
        .expect("second connection");

    // Backup flag is pure metadata on this surface.
    assert!(!a.is_backup(first));
    a.set_backup(first, true);
    assert!(a.is_backup(first));
    a.set_backup(first, false);
    assert!(!a.is_backup(first));

    // Steer one payload through each subflow.
    assert_eq!(a.send_on(first, b"alpha").expect("send first"), 5);
    assert_eq!(a.send_on(second, b"beta").expect("send second"), 4);
    pump_ms(&mut a, &mut b, 200, 400);
    let mut buffer = [0u8; 32];
    let count = b.recv(accepted_first, &mut buffer).expect("recv first");
    assert_eq!(&buffer[..count], b"alpha");
    let count = b.recv(accepted_second, &mut buffer).expect("recv second");
    assert_eq!(&buffer[..count], b"beta");

    if std::env::var("MPTCP_HOOKS_STALE_READINESS").as_deref() == Ok("1") {
        a.test_set_subflow_readiness_stale(true);
    }

    // Live level: writable, nothing to read yet.
    let readiness = SubflowOps::readiness(&a, first);
    assert!(readiness.writable);
    assert!(!readiness.readable);

    // A peer write is visible on the next call without a poll or a remembered mask.
    assert_eq!(b.send(accepted_first, b"reply").expect("peer reply"), 5);
    pump_ms(&mut a, &mut b, 400, 600);
    let readiness = SubflowOps::readiness(&a, first);
    assert!(readiness.readable, "new bytes must be readable");
    let count = a.recv_from(first, &mut buffer).expect("recv_from");
    assert_eq!(&buffer[..count], b"reply");
    let readiness = SubflowOps::readiness(&a, first);
    assert!(!readiness.readable, "drained bytes must not stay readable");

    // Fill the subflow's send queue, then free it; writable tracks the live queue.
    let chunk = [0u8; 4096];
    let mut blocked = false;
    for _ in 0..64 {
        match a.send_on(second, &chunk) {
            Ok(0) => break,
            Ok(_) => {}
            Err(Errno::EAGAIN) => {
                blocked = true;
                break;
            }
            Err(error) => panic!("unexpected send error: {error:?}"),
        }
    }
    assert!(blocked, "the send queue must fill");
    assert!(
        !SubflowOps::readiness(&a, second).writable,
        "a full queue is not writable"
    );
    let mut drain = [0u8; 8192];
    for _ in 0..64 {
        match b.recv(accepted_second, &mut drain) {
            Ok(0) | Err(Errno::EAGAIN) => break,
            Ok(_) => {}
            Err(error) => panic!("unexpected drain error: {error:?}"),
        }
    }
    pump_ms(&mut a, &mut b, 600, 800);
    assert!(
        SubflowOps::readiness(&a, second).writable,
        "a drained queue is writable"
    );

    // Close is terminal for the hook: hup, not writable, and no more sends.
    a.close_subflow(second);
    let readiness = SubflowOps::readiness(&a, second);
    assert!(readiness.hup, "a closed subflow reports hup");
    assert!(!readiness.writable);
    assert_eq!(a.send_on(second, b"x"), Err(Errno::EBADF));

    // The first subflow still works after the second closes.
    assert_eq!(a.send_on(first, b"ok").expect("send after close"), 2);
    pump_ms(&mut a, &mut b, 800, 1000);
    let count = b
        .recv(accepted_first, &mut buffer)
        .expect("recv after close");
    assert_eq!(&buffer[..count], b"ok");
}

#[test]
fn subflow_local_interface_must_match_the_stack() {
    let (_link, mut a, _b) = pair();
    let wrong = raven_tcpip::Ipv4Addr::new([192, 0, 2, 99]);
    assert_eq!(
        a.open_subflow((IpAddr::V4(address(B)), 80), Some(wrong)),
        Err(Errno::EADDRNOTAVAIL)
    );
}
