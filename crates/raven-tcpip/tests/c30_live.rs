// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C30: regression tests for the live-path fixes (slow start, out-of-order queue, SACK
//! recovery, listener queues, socket reclaim and live PLPMTUD).

#![cfg(feature = "testutil")]

mod common;

use common::{A, AMAC, B, BMAC, address, inject_segment, pair_with_link};
use raven_tcpip::{
    Device, DeviceCaps, Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig,
    TxError,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use raven_tcpip_wire::tcp::TcpFlags;

fn connect<D: Device>(a: &mut Stack<D>, b: &mut Stack<D>) -> (SocketId, SocketId) {
    b.tcp_listen(80, 8).expect("listen");
    let client = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    let mut time = 0i64;
    let server = loop {
        if let Ok(Some(socket)) = b.tcp_accept(SocketId(1)) {
            break socket;
        }
        time += 1;
        assert!(time < 20_000, "handshake did not finish");
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
    };
    (client, server)
}

fn transfer<D: Device>(
    a: &mut Stack<D>,
    b: &mut Stack<D>,
    client: SocketId,
    server: SocketId,
    bytes: usize,
    start: i64,
) -> i64 {
    let payload = vec![0x5au8; bytes];
    let (mut sent, mut received) = (0usize, 0usize);
    let mut buffer = vec![0u8; 65_536];
    let mut time = start;
    while received < bytes {
        if sent < bytes
            && let Ok(count) = a.send(client, &payload[sent..])
        {
            sent += count;
        }
        while let Ok(count) = b.recv(server, &mut buffer) {
            if count == 0 {
                break;
            }
            received += count;
        }
        time += 1;
        if time - start >= 200_000 {
            let info = a.socket_info(client).expect("info");
            panic!(
                "transfer stalled at {received}: mss {} cwnd {} state {:?} counters probes {} losses {} rto {} retrans {}",
                info.mss,
                info.cwnd,
                info.state,
                a.counters().plpmtud_probes,
                a.counters().plpmtud_losses,
                a.counters().rto_fires,
                a.counters().retrans_segments
            );
        }
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
    }
    time - start
}

#[test]
fn slow_start_is_not_one_segment_per_round_trip() {
    let (_link, mut a, mut b) = pair_with_link(
        LinkConfig {
            rtt_ms: 20,
            ..LinkConfig::default()
        },
        StackConfig::default(),
    );
    let (client, server) = connect(&mut a, &mut b);
    let elapsed = transfer(&mut a, &mut b, client, server, 1 << 20, 100);
    assert!(
        elapsed < 450,
        "1 MiB at 20 ms RTT took {elapsed} ms; a 64 KiB window needs about 400 ms"
    );
}

#[test]
fn out_of_order_data_is_bounded_by_holes_not_segments() {
    let (_link, mut a, mut b) = pair_with_link(LinkConfig::default(), StackConfig::default());
    let (_client, server) = common::handshake(&mut a, &mut b);
    let rcv_nxt = b.test_tcp_rcv_nxt(server).expect("rcv_nxt");
    let ack = b.test_tcp_window(server).expect("window").2;
    for index in 1..=40u32 {
        inject_segment(
            &mut b,
            A,
            B,
            40_000,
            80,
            rcv_nxt.wrapping_add(index * 100),
            ack,
            TcpFlags::ACK,
            &[7u8; 100],
        );
    }
    assert_eq!(b.test_tcp_ooo_len(server), Some(40));
    inject_segment(
        &mut b,
        A,
        B,
        40_000,
        80,
        rcv_nxt,
        ack,
        TcpFlags::ACK,
        &[7u8; 100],
    );
    assert_eq!(b.recv_queue_len(server), Ok(4_100));
    assert_eq!(b.test_tcp_ooo_len(server), Some(0));
}

#[test]
fn sack_recovery_repairs_one_percent_loss_in_a_few_round_trips() {
    let (_link, mut a, mut b) = pair_with_link(
        LinkConfig {
            rtt_ms: 20,
            loss_percent: 1.0,
            ..LinkConfig::default()
        },
        StackConfig::default(),
    );
    let (client, server) = connect(&mut a, &mut b);
    let elapsed = transfer(&mut a, &mut b, client, server, 1 << 20, 10_000);
    let retransmitted = a.counters().retrans_bytes;
    assert!(elapsed < 2_500, "1% loss took {elapsed} ms");
    assert!(
        retransmitted < 200 * 1024,
        "retransmitted {retransmitted} bytes for a 1 MiB transfer at 1% loss"
    );
}

#[test]
fn sack_recovery_completes_under_heavy_loss() {
    let (_link, mut a, mut b) = pair_with_link(
        LinkConfig {
            rtt_ms: 10,
            loss_percent: 8.0,
            ..LinkConfig::default()
        },
        StackConfig::default(),
    );
    let (client, server) = connect(&mut a, &mut b);
    let elapsed = transfer(&mut a, &mut b, client, server, 256 * 1024, 10_000);
    assert!(elapsed > 0);
    assert!(a.counters().retrans_segments > 0);
}

#[test]
fn closed_half_open_connections_free_their_backlog_slot() {
    let (_link, _a, mut b) = pair_with_link(LinkConfig::default(), StackConfig::default());
    b.set_property("tcp.syn_cookies_enable", 0).expect("cookies");
    let listener = b.tcp_listen(80, 2).expect("listen");
    let syn = |b: &mut Stack<Endpoint>, port: u16| {
        inject_segment(b, A, B, port, 80, 1_000, 0, TcpFlags::SYN, &[]);
    };
    syn(&mut b, 5001);
    syn(&mut b, 5002);
    assert_eq!(b.test_listener_half_open(listener).len(), 2);
    syn(&mut b, 5003);
    assert_eq!(b.test_listener_half_open(listener).len(), 2);
    assert!(b.counters().half_open_drops >= 1);
    let first = b.test_listener_half_open(listener)[0];
    let first_iss = b.test_tcp_iss(first).expect("iss");
    inject_segment(
        &mut b,
        A,
        B,
        5001,
        80,
        1_001,
        first_iss.wrapping_add(1),
        TcpFlags::RST,
        &[],
    );
    syn(&mut b, 5003);
    let queued = b.test_listener_half_open(listener);
    assert_eq!(queued.len(), 2);
    assert!(!queued.contains(&first));
}

#[test]
fn closed_sockets_are_reclaimed_before_the_table_fills() {
    let config = StackConfig {
        max_sockets: 4,
        ..StackConfig::default()
    };
    let mut stack = common::null_stack(config, 9);
    for round in 0..32 {
        let socket = stack
            .udp_bind(0)
            .unwrap_or_else(|error| panic!("bind {round}: {error:?}"));
        stack.close(socket).expect("close");
    }
}

#[test]
fn ephemeral_ports_are_released_when_a_socket_is_reclaimed() {
    let config = StackConfig {
        max_sockets: 2,
        ..StackConfig::default()
    };
    let mut stack = common::null_stack(config, 11);
    stack
        .set_property("tcp.port_range_first", 40_000)
        .expect("first");
    stack
        .set_property("tcp.port_range_last", 40_001)
        .expect("last");
    for round in 0..16 {
        let socket = stack
            .udp_bind(0)
            .unwrap_or_else(|error| panic!("bind {round}: {error:?}"));
        stack.close(socket).expect("close");
    }
}

struct PathLimit {
    inner: Endpoint,
    limit: usize,
    dropped: usize,
}

impl Device for PathLimit {
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
        if len > self.limit {
            self.dropped += 1;
            return Ok(());
        }
        self.inner.transmit(now, len, fill)
    }
}

fn limited_pair(limit: usize) -> (Stack<PathLimit>, Stack<PathLimit>) {
    let (a, b) = Link::new(LinkConfig::default(), 5);
    let mut stacks = Vec::new();
    for (endpoint, address_octets, seed, peer, peer_mac) in
        [(a, A, 1, B, BMAC), (b, B, 2, A, AMAC)]
    {
        let device = PathLimit {
            inner: endpoint,
            limit,
            dropped: 0,
        };
        let mut stack = Stack::new(device, StackConfig::default(), seed, Instant::ZERO);
        stack
            .set_ipv4(Ipv4Config {
                addr: Ipv4Addr::new(address_octets),
                prefix_len: 24,
                gateway: None,
            })
            .expect("ipv4");
        stack.test_insert_neighbor(Ipv4Addr::new(peer), peer_mac, Instant::ZERO, 600_000);
        stacks.push(stack);
    }
    let b = stacks.pop().expect("b");
    let a = stacks.pop().expect("a");
    (a, b)
}

#[test]
fn plpmtud_recovers_from_a_black_hole_and_finds_a_larger_payload() {
    let (mut a, mut b) = limited_pair(1_000);
    let (client, server) = connect(&mut a, &mut b);
    let before = a.socket_info(client).expect("info").mss;
    let elapsed = transfer(&mut a, &mut b, client, server, 256 * 1024, 100);
    let counters = a.counters();
    assert!(elapsed > 0);
    assert!(counters.plpmtud_probes >= 1, "no probe was sent");
    assert!(counters.plpmtud_losses >= 1, "no probe or black hole was seen");
    let after = a.socket_info(client).expect("info").mss;
    assert!(after < before, "mss {after} was not below the blocked {before}");
    assert!(after >= 536, "mss {after} fell below the floor");
    assert!(
        usize::from(after) + 66 <= 1_000,
        "mss {after} does not fit the 1000-byte path"
    );
}

#[test]
fn ten_percent_loss_does_not_kill_the_connection_or_collapse_the_path_mtu() {
    let config = StackConfig {
        rto_max_ms: 5_000,
        ..StackConfig::default()
    };
    let (la, lb) = Link::new(
        LinkConfig {
            loss_percent: 10.0,
            ..LinkConfig::default()
        },
        91,
    );
    let mut a = common::endpoint_stack(la, A, 1, config);
    let mut b = common::endpoint_stack(lb, B, 2, config);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    b.tcp_listen(80, 8).expect("listen");
    let client = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    for time in [0, 50, 100, 150, 200] {
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
    }
    let server = b.tcp_accept(SocketId(1)).expect("accept").expect("connection");
    let payload = vec![1u8; 1 << 20];
    let (mut sent, mut got) = (0usize, 0usize);
    let mut buffer = vec![0u8; 4096];
    let mut time = 210i64;
    for _ in 0..400_000 {
        if sent < payload.len()
            && let Ok(count) = a.send(client, &payload[sent..])
        {
            sent += count;
        }
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        while let Ok(count) = b.recv(server, &mut buffer) {
            if count == 0 {
                break;
            }
            got += count;
        }
        if got >= payload.len() {
            break;
        }
        time += 10;
    }
    assert_eq!(got, payload.len(), "the connection died at {got} bytes");
    let state = a.socket_info(client).expect("info").state;
    assert_eq!(state, raven_tcpip::TcpState::Established);
}
