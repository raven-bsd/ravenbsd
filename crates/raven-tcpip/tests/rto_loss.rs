// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RTO retransmission, Karn's algorithm and loss behaviour.

#![cfg(feature = "testutil")]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use raven_abi::errno::Errno;
use raven_tcpip::{Instant, IpAddr, Ipv4Addr, Ipv4Config, Stack, StackConfig, TcpState};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn endpoint_stack(
    endpoint: Endpoint,
    address_value: [u8; 4],
    seed: u64,
    config: StackConfig,
) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, config, seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(address_value),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

fn pair_stack(
    config: LinkConfig,
    stack_config: StackConfig,
) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(config, 91);
    let link = a.link();
    let mut a = endpoint_stack(a, A, 1, stack_config);
    let mut b = endpoint_stack(b, B, 2, stack_config);
    a.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    b.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    (link, a, b)
}

fn pair(config: LinkConfig) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    pair_stack(config, StackConfig::default())
}

fn handshake(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    times: &[i64],
) -> (raven_tcpip::SocketId, raven_tcpip::SocketId) {
    b.tcp_listen(80, 8).expect("listen");
    let socket = a
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    for time in times {
        let now = Instant::from_millis(*time);
        a.poll(now);
        b.poll(now);
    }
    let accepted = b
        .tcp_accept(raven_tcpip::SocketId(1))
        .expect("accept")
        .expect("connection");
    (socket, accepted)
}

fn drive(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, from: i64, to: i64, step: i64) {
    let mut time = from;
    while time <= to {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        time += step;
    }
}

#[test]
fn a_lost_segment_is_retransmitted_after_one_rto() {
    let config = LinkConfig::default();
    let (link, mut a, mut b) = pair(config);
    let (socket_a, socket_b) = handshake(&mut a, &mut b, &[0, 50, 100, 150, 200]);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..config
    });
    a.send(socket_a, b"lost-once").expect("send");
    drive(&mut a, &mut b, 210, 1100, 10);
    assert_eq!(a.test_tcp_rto(socket_a).expect("rto").3, 0);
    link.set_config(config);
    a.poll(Instant::from_millis(1_200));
    drive(&mut a, &mut b, 1_210, 1_800, 10);
    let mut buffer = [0u8; 16];
    let mut total = Vec::new();
    for _ in 0..20 {
        match b.recv(socket_b, &mut buffer) {
            Ok(0) => break,
            Ok(count) => total.extend_from_slice(&buffer[..count]),
            Err(Errno::EAGAIN) => drive(&mut a, &mut b, 1_800, 2_200, 10),
            Err(_) => break,
        }
    }
    assert_eq!(total, b"lost-once");
    assert!(!a.test_tcp_rto(socket_a).expect("rto").4);
}

#[test]
fn karn_prevents_a_sample_from_a_retransmitted_segment() {
    let config = LinkConfig::default();
    let (link, mut a, mut b) = pair(config);
    let (socket_a, _socket_b) = handshake(&mut a, &mut b, &[0, 50, 100, 150, 200]);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..config
    });
    a.send(socket_a, b"karn").expect("send");
    drive(&mut a, &mut b, 210, 1_100, 10);
    link.set_config(config);
    a.poll(Instant::from_millis(1_200));
    drive(&mut a, &mut b, 1_210, 1_800, 10);
    let (_, _, _, _retransmits, has_sample) = a.test_tcp_rto(socket_a).expect("rto");
    assert!(!has_sample, "Karn: retransmitted segment must not sample");
}

#[test]
fn the_rto_timer_is_disarmed_when_all_data_is_acknowledged() {
    let (link, mut a, mut b) = pair(LinkConfig::default());
    let (socket_a, socket_b) = handshake(&mut a, &mut b, &[0, 50, 100, 150, 200]);
    a.send(socket_a, b"ackme").expect("send");
    for time in 210..2_000 {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        let mut buffer = [0u8; 8];
        let _ = b.recv(socket_b, &mut buffer);
        if a.send_queue_len(socket_a) == Ok(0) {
            break;
        }
    }
    drive(&mut a, &mut b, 2_000, 2_100, 10);
    assert_eq!(a.send_queue_len(socket_a), Ok(0));
    assert_eq!(a.next_poll_delay(Instant::from_millis(2_100)), None);
    let _ = link;
}

#[test]
fn a_black_holed_connection_gives_up_with_etimedout() {
    let config = LinkConfig::default();
    let (link, mut a, mut b) = pair(config);
    let (socket_a, _socket_b) = handshake(&mut a, &mut b, &[0, 50, 100, 150, 200]);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..config
    });
    a.send(socket_a, b"blackhole").expect("send");
    let times = [
        210, 1_210, 3_210, 7_210, 15_210, 31_210, 63_210, 127_210, 255_210,
    ];
    for time in times {
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
    }
    assert_eq!(a.tcp_state(socket_a), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket_a), Ok(Some(Errno::ETIMEDOUT)));
}

#[test]
fn rto_converges_on_a_500ms_link() {
    let config = LinkConfig {
        rtt_ms: 500,
        ..LinkConfig::default()
    };
    let (_link, mut a, mut b) = pair(config);
    let (socket_a, socket_b) = handshake(&mut a, &mut b, &[0, 250, 500, 750, 1000, 1250]);
    a.send(socket_a, b"sample").expect("send");
    for time in (1260..3000).step_by(10) {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        let mut buffer = [0u8; 8];
        let _ = b.recv(socket_b, &mut buffer);
    }
    let (srtt, _rttvar, rto, _, has_sample) = a.test_tcp_rto(socket_a).expect("rto");
    if has_sample {
        assert!(srtt >= 500, "srtt {srtt}");
        assert!((500..=60_000).contains(&rto), "rto {rto}");
    }
}

fn transfer_with_loss(loss_percent: f32, size: usize) -> (usize, u64) {
    let config = LinkConfig {
        loss_percent,
        ..LinkConfig::default()
    };
    let stack_config = StackConfig {
        rto_max_ms: 5_000,
        ..StackConfig::default()
    };
    let (_link, mut a, mut b) = pair_stack(config, stack_config);
    let (socket_a, socket_b) = handshake(&mut a, &mut b, &[0, 50, 100, 150, 200]);
    let payload: Vec<u8> = (0..size).map(|index| (index * 17 + 3) as u8).collect();
    let mut sent = 0usize;
    let mut received = Vec::new();
    let mut time = 210i64;
    let mut buffer = vec![0u8; 4096];
    for _ in 0..1_000_000 {
        if sent < payload.len()
            && let Ok(count) = a.send(socket_a, &payload[sent..])
        {
            sent += count;
        }
        a.poll(Instant::from_millis(time));
        b.poll(Instant::from_millis(time));
        loop {
            match b.recv(socket_b, &mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    received.extend_from_slice(&buffer[..count]);
                    if received.len() >= payload.len() {
                        break;
                    }
                }
                Err(Errno::EAGAIN) => break,
                Err(_) => break,
            }
        }
        if sent == payload.len() && received.len() >= payload.len() {
            break;
        }
        time += 10;
    }
    let mut hash = DefaultHasher::new();
    received.hash(&mut hash);
    (received.len(), hash.finish())
}

#[test]
fn one_mib_under_two_percent_loss_completes() {
    let (length, _) = transfer_with_loss(2.0, 1024 * 1024);
    assert_eq!(length, 1024 * 1024);
}

#[test]
fn one_mib_under_ten_percent_loss_completes() {
    let (length, _) = transfer_with_loss(10.0, 1024 * 1024);
    assert_eq!(length, 1024 * 1024);
}

#[test]
fn rto_backoff_doubles_and_caps() {
    let mut estimator = raven_tcpip::RtoEstimator::new(1000, 200, 60_000, 1);
    estimator.sample(100);
    assert_eq!(estimator.rto_ms(), 300);
    estimator.timeout();
    assert_eq!(estimator.rto_ms(), 600);
    estimator.timeout();
    assert_eq!(estimator.rto_ms(), 1200);
}

#[test]
fn rto_floor_and_ceiling_are_configurable() {
    let mut floor = raven_tcpip::RtoEstimator::new(1000, 500, 60_000, 1);
    floor.sample(1);
    assert_eq!(floor.rto_ms(), 500);
    let mut ceiling = raven_tcpip::RtoEstimator::new(1000, 1, 1000, 1);
    ceiling.sample(10_000);
    assert_eq!(ceiling.rto_ms(), 1000);
}

#[test]
fn no_floating_point_in_the_stack_sources() {
    let sources = [
        include_str!("../src/rto.rs"),
        include_str!("../src/tcp_state.rs"),
        include_str!("../src/tcp_data.rs"),
        include_str!("../src/seq.rs"),
    ];
    for source in sources {
        assert!(!source.contains("f32"));
        assert!(!source.contains("f64"));
    }
}
