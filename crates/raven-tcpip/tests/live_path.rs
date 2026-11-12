// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C28: prove the C13/C14 mechanisms move on the live TCP path.

#![cfg(feature = "testutil")]

use raven_tcpip::{Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn pair(config: StackConfig, link: LinkConfig) -> (Link, Stack<Endpoint>, Stack<Endpoint>) {
    let (a, b) = Link::new(link, 31);
    let handle = a.link();
    let mut a = Stack::new(a, config, 1, Instant::ZERO);
    let mut b = Stack::new(b, config, 2, Instant::ZERO);
    for (stack, address) in [(&mut a, A), (&mut b, B)] {
        stack
            .set_ipv4(Ipv4Config {
                addr: Ipv4Addr::new(address),
                prefix_len: 24,
                gateway: None,
            })
            .expect("ipv4");
    }
    a.test_insert_neighbor(Ipv4Addr::new(B), BMAC, Instant::ZERO, 600_000);
    b.test_insert_neighbor(Ipv4Addr::new(A), AMAC, Instant::ZERO, 600_000);
    (handle, a, b)
}

fn handshake(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>) -> (SocketId, SocketId) {
    b.tcp_listen(80, 8).expect("listen");
    let client = a
        .tcp_connect((IpAddr::V4(Ipv4Addr::new(B)), 80), Some(40_000))
        .expect("connect");
    for time in [0, 50, 100, 150, 200] {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    let server = b
        .tcp_accept(SocketId(1))
        .expect("accept")
        .expect("connection");
    (client, server)
}

#[test]
fn initial_cwnd_bounds_the_first_burst() {
    let (link, mut a, mut b) = pair(StackConfig::default(), LinkConfig::default());
    let (client, _server) = handshake(&mut a, &mut b);
    let cwnd = u64::from(a.socket_info(client).expect("info").cwnd);
    let before = a.counters().out_bytes;
    a.send(client, &vec![0u8; 128 * 1024]).expect("send");
    a.poll(Instant::from_millis(210));
    let sent = a.counters().out_bytes.saturating_sub(before);
    assert!(sent > 0, "the live path sent nothing");
    assert!(sent <= cwnd, "sent {sent} exceeds cwnd {cwnd}");
    let _ = link;
}

#[test]
fn tlp_probe_fires_before_the_rto() {
    let config = StackConfig {
        rto_initial_ms: 3_000,
        ..StackConfig::default()
    };
    let (link, mut a, mut b) = pair(config, LinkConfig::default());
    let (client, _server) = handshake(&mut a, &mut b);
    a.send(client, b"rtt-sample").expect("send");
    for time in 205..500 {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..LinkConfig::default()
    });
    a.send(client, b"tail-loss").expect("send");
    eprintln!("tlp deadline {:?}", a.test_tcp_tlp_deadline(client));
    for time in 500..800 {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
    }
    assert!(a.counters().tlp_fires >= 1, "no TLP probe");
    assert_eq!(a.counters().rto_fires, 0, "RTO fired before the TLP");
}
