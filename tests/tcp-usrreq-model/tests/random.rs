// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! 10k randomized listener queue operations against the reference listener.

use raven_tcpip::{Instant, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use tcp_usrreq_model::RefListener;

const SEED: u64 = 0x3210_5a43_5155_4555;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value >> 12;
        value ^= value << 25;
        value ^= value >> 27;
        self.0 = value;
        value.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: u32) -> u32 {
        (self.next() % u64::from(bound.max(1))) as u32
    }
}

fn stack(endpoint: Endpoint) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), SEED, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: Ipv4Addr::new([192, 0, 2, 1]),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

fn discard(stack: &mut Stack<Endpoint>, socket: SocketId) {
    // `test_pending_connection` creates an established record with an
    // unspecified peer. `abort` sends no usable RST but marks it closed;
    // `close` then releases the embryonic record so the table can reclaim it.
    let _ = stack.abort(socket);
    let _ = stack.close(socket);
}

fn drain(
    stack: &mut Stack<Endpoint>,
    listener: SocketId,
    reference: &mut RefListener,
    step: usize,
) {
    loop {
        let actual = stack.tcp_accept(listener);
        let expected = reference.accept();
        match (actual, expected) {
            (Ok(Some(socket)), Some(want)) => {
                assert_eq!(socket.0, want, "step {step}: FIFO mismatch");
                discard(stack, socket);
            }
            (Ok(None), None) => return,
            (other, want) => panic!("step {step}: accept {other:?}, reference {want:?}"),
        }
    }
}

#[test]
fn ten_thousand_random_listener_operations_match_the_reference() {
    let (endpoint, _peer) = Link::new(LinkConfig::default(), SEED);
    let mut stack = stack(endpoint);
    let mut next_port = 8_000u16;
    let mut listener = stack.tcp_listen(next_port, 2).expect("initial listener");
    next_port = next_port.wrapping_add(1);
    let mut reference = RefListener::new(2);
    let mut rng = Rng::new(SEED);
    let mut adds = 0usize;
    let mut accepts = 0usize;
    let mut closes = 0usize;

    for step in 0..10_000 {
        match rng.below(100) {
            0..=59 => {
                let socket = stack
                    .test_pending_connection(listener)
                    .expect("pending connection");
                if reference.push(socket.0) {
                    adds += 1;
                } else {
                    // The product allocates before `AcceptQueue::push`; release the
                    // unqueued record so the table does not grow without bound.
                    discard(&mut stack, socket);
                }
            }
            60..=84 => {
                let actual = stack.tcp_accept(listener);
                let expected = reference.accept();
                assert_eq!(
                    actual.map(|socket| socket.map(|socket| socket.0)),
                    Ok(expected),
                    "step {step}: accept mismatch"
                );
                if let Ok(Some(socket)) = actual {
                    discard(&mut stack, socket);
                    accepts += 1;
                }
            }
            85..=94 => {
                stack.poll(Instant::from_millis(
                    i64::try_from(step).unwrap_or(i64::MAX) * 10,
                ));
            }
            _ => {
                stack.close(listener).expect("close listener");
                reference.close();
                assert_eq!(stack.tcp_accept(listener), Err(raven_tcpip::Errno::EBADF));
                listener = stack.tcp_listen(next_port, 2).expect("reopen listener");
                next_port = next_port.wrapping_add(1);
                reference = RefListener::new(2);
                closes += 1;
            }
        }
        // Reclaim closed test sockets every step; the random queue can otherwise
        // accumulate closed records until the socket table returns EMFILE.
        stack.poll(Instant::from_millis(
            i64::try_from(step).unwrap_or(i64::MAX) * 10,
        ));
        if step % 100 == 0 {
            drain(&mut stack, listener, &mut reference, step);
        }
    }
    drain(&mut stack, listener, &mut reference, 10_000);
    assert_eq!(reference.len(), 0);
    stack.close(listener).expect("final close");
    println!("tcp-usrreq-model: seed={SEED:#x} adds={adds} accepts={accepts} closes={closes}");
}
