// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A TCP write that the stack accepted must not be reported as failed because the device was
//! momentarily out of transmit buffers (K231).
//!
//! `Stack::send` queues the bytes first and then tries to put segments on the wire. When the
//! device answers `TxError::Busy` (the netd NIC has three transmit buffers), `send` used to return
//! `EAGAIN` with the bytes already queued. A non-blocking caller then retried the same bytes, the
//! queue held them twice, and the peer received the data duplicated: `sshd-session` sent its
//! KEXINIT twice and the client aborted with `strict KEX violation`.
//!
//! NetBSD (`tcp_output.c`, label `out`) turns `ENOBUFS` into success after `tcp_send` has
//! appended to `so_snd`; illumos `tcp_send_data` discards the result of `conn_ip_output` and keeps
//! the data in `tcp_xmit_head` for the retransmission timer.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::{A, AMAC, B, BMAC, address};
use raven_abi::errno::Errno;
use raven_tcpip::{
    Device, DeviceCaps, Instant, IpAddr, Ipv4Config, SocketId, Stack, StackConfig, TxError,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};

/// An endpoint that refuses a chosen number of transmits with `TxError::Busy`.
struct Gated {
    inner: Endpoint,
    refuse: Rc<Cell<usize>>,
    refused: Rc<Cell<usize>>,
}

impl Device for Gated {
    fn capabilities(&self) -> DeviceCaps {
        self.inner.capabilities()
    }

    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        self.inner.receive(now, consume)
    }

    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        let remaining = self.refuse.get();
        if remaining > 0 {
            self.refuse.set(remaining - 1);
            self.refused.set(self.refused.get() + 1);
            return Err(TxError::Busy);
        }
        self.inner.transmit(now, len, fill)
    }
}

struct Rig {
    client: Stack<Endpoint>,
    server: Stack<Gated>,
    refuse: Rc<Cell<usize>>,
    refused: Rc<Cell<usize>>,
    accepted: SocketId,
    connected: SocketId,
    now: i64,
}

fn stack<D: Device>(device: D, octets: [u8; 4], seed: u64) -> Stack<D> {
    let mut stack = Stack::new(device, StackConfig::default(), seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(octets),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

fn pump<X: Device, Y: Device>(a: &mut Stack<X>, b: &mut Stack<Y>, from: i64, to: i64) {
    let mut time = from;
    while time <= to {
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        time += 5;
    }
}

/// A connected pair; the server side is the gated one, as netd's NIC is.
fn rig() -> Rig {
    let (client_end, server_end) = Link::new(LinkConfig::default(), 71);
    let refuse = Rc::new(Cell::new(0));
    let refused = Rc::new(Cell::new(0));
    let mut client = stack(client_end, A, 1);
    let mut server = stack(
        Gated {
            inner: server_end,
            refuse: Rc::clone(&refuse),
            refused: Rc::clone(&refused),
        },
        B,
        2,
    );
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    server.tcp_listen(80, 8).expect("listen");
    let connected = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump(&mut client, &mut server, 0, 200);
    let accepted = server
        .tcp_accept(SocketId(1))
        .expect("accept")
        .expect("connection");
    Rig {
        client,
        server,
        refuse,
        refused,
        accepted,
        connected,
        now: 200,
    }
}

fn payload(length: usize) -> Vec<u8> {
    (0..length).map(|index| (index % 251) as u8).collect()
}

/// Everything the client can read right now.
fn drain(rig: &mut Rig) -> Vec<u8> {
    let mut received = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match rig.client.recv(rig.connected, &mut buffer) {
            Ok(0) | Err(Errno::EAGAIN) => break,
            Ok(count) => received.extend_from_slice(&buffer[..count]),
            Err(error) => panic!("client recv: {error:?}"),
        }
    }
    received
}

fn settle(rig: &mut Rig, millis: i64) {
    let end = rig.now + millis;
    pump(&mut rig.client, &mut rig.server, rig.now, end);
    rig.now = end;
}

#[test]
fn a_write_the_device_refused_once_is_accepted_and_delivered_exactly_once() {
    let mut rig = rig();
    let data = payload(960);

    rig.refuse.set(1);
    let written = rig.server.send(rig.accepted, &data);
    assert_eq!(
        written,
        Ok(data.len()),
        "the stack owns the bytes once it queued them; a busy device is not a failed write"
    );
    assert_eq!(rig.refused.get(), 1, "the device really refused a transmit");
    assert_eq!(
        rig.server
            .socket_info(rig.accepted)
            .expect("info")
            .send_queue,
        data.len(),
        "queued once"
    );

    // The caller does NOT retry. The unsent bytes must go out by themselves at the next poll.
    settle(&mut rig, 400);
    let received = drain(&mut rig);
    assert_eq!(
        received.len(),
        data.len(),
        "delivered exactly once, not duplicated or lost"
    );
    assert_eq!(received, data);
}

#[test]
fn a_caller_that_trusts_the_result_does_not_duplicate_a_record() {
    let mut rig = rig();
    let first = payload(22);
    let second = payload(960);

    assert_eq!(rig.server.send(rig.accepted, &first), Ok(first.len()));
    // Let the banner be acknowledged: with nothing in flight Nagle sends the next record at once,
    // which is the write that meets the busy device.
    settle(&mut rig, 600);
    assert_eq!(
        rig.server
            .socket_info(rig.accepted)
            .expect("info")
            .send_queue,
        0,
        "the banner was acknowledged, so nothing is in flight"
    );
    // The record that used to fail: the device has no buffer for its frame.
    rig.refuse.set(2);
    let mut attempts = 0;
    let mut offset = 0;
    while offset < second.len() {
        attempts += 1;
        assert!(attempts <= 10, "the write never completed");
        match rig.server.send(rig.accepted, &second[offset..]) {
            Ok(count) => offset += count,
            Err(Errno::EAGAIN) => {
                // What sshd's `atomicio` does with EAGAIN: poll, then write the same bytes again.
                settle(&mut rig, 5);
            }
            Err(error) => panic!("send: {error:?}"),
        }
    }
    assert!(
        rig.refused.get() >= 1,
        "the second record really met the busy device during the write"
    );
    settle(&mut rig, 400);
    let mut expected = first;
    expected.extend_from_slice(&second);
    assert_eq!(
        drain(&mut rig),
        expected,
        "the stream is byte-for-byte what was written"
    );
}

#[test]
fn a_refused_transmit_does_not_stall_data_queued_behind_it() {
    let mut rig = rig();
    let data = payload(4000);
    rig.refuse.set(3);
    assert_eq!(rig.server.send(rig.accepted, &data), Ok(data.len()));
    settle(&mut rig, 800);
    assert_eq!(drain(&mut rig), data);
}

#[test]
fn a_small_write_after_prior_unacked_data_is_flushed_on_the_next_poll() {
    let mut rig = rig();
    let first = payload(100);
    assert_eq!(rig.server.send(rig.accepted, &first), Ok(first.len()));

    // One-way delay is 50 ms, so the first segment reaches the client at 250. Its delayed ACK
    // leaves on the client's next poll, but the server has not processed it yet.
    rig.client.poll(Instant::from_millis(250));
    assert_eq!(rig.client.recv_queue_len(rig.connected), Ok(first.len()));
    rig.client.poll(Instant::from_millis(300));

    // The small write meets an unacknowledged first segment, so Nagle holds it in the queue.
    let second = payload(44);
    assert_eq!(rig.server.send(rig.accepted, &second), Ok(second.len()));
    assert_eq!(
        rig.server
            .socket_info(rig.accepted)
            .expect("info")
            .send_queue,
        first.len() + second.len(),
        "the small write is queued behind the unacknowledged data"
    );

    // One server poll processes the ACK and must flush the queued small write without another
    // application write.
    rig.server.poll(Instant::from_millis(360));
    rig.client.poll(Instant::from_millis(420));
    let mut expected = first;
    expected.extend_from_slice(&second);
    assert_eq!(drain(&mut rig), expected);
}

#[test]
fn a_zero_window_write_is_probed_and_flushed_when_the_peer_answers() {
    let mut rig = rig();
    // The peer advertised a closed window and the update that reopened it was lost. A bare ACK
    // would never be answered, so the stack must send a one-byte persist probe.
    assert!(rig.server.test_set_tcp_snd_wnd(rig.accepted, 0));
    let data = payload(44);
    assert_eq!(rig.server.send(rig.accepted, &data), Ok(data.len()));
    assert_eq!(
        rig.server
            .socket_info(rig.accepted)
            .expect("info")
            .send_queue,
        data.len(),
        "the closed window holds the whole write"
    );
    assert_eq!(rig.client.recv_queue_len(rig.connected), Ok(0));

    // The first persist probe is due one second after the write. The client accepts one byte
    // and its delayed ACK follows on its next poll.
    rig.server.poll(Instant::from_millis(1200));
    rig.client.poll(Instant::from_millis(1250));
    assert_eq!(rig.client.recv_queue_len(rig.connected), Ok(1));
    rig.client.poll(Instant::from_millis(1300));

    // The ACK carries the peer's real window; the same server poll flushes the rest.
    rig.server.poll(Instant::from_millis(1350));
    rig.client.poll(Instant::from_millis(1410));
    assert_eq!(drain(&mut rig), data);
}
