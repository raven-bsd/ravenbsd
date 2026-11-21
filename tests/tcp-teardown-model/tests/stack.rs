// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Drive the public Stack API through FIN, 2MSL, RST and listener-abort paths.

use raven_tcpip::{
    Duration, Errno, Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};
use tcp_teardown_model::{RefState, deadline};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn stack(endpoint: Endpoint, octets: [u8; 4], seed: u64) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(octets),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

fn pair(seed: u64) -> (Stack<Endpoint>, Stack<Endpoint>, SocketId, SocketId, Link) {
    let (client_endpoint, server_endpoint) = Link::new(LinkConfig::default(), seed);
    let link = client_endpoint.link();
    let mut client = stack(client_endpoint, A, seed.wrapping_add(1));
    let mut server = stack(server_endpoint, B, seed.wrapping_add(2));
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    let listener = server.tcp_listen(80, 8).expect("listen");
    let connected = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump(&mut client, &mut server, 0, 200);
    let accepted = server
        .tcp_accept(listener)
        .expect("accept")
        .expect("connection");
    assert_eq!(state(&client, connected), RefState::Established);
    assert_eq!(state(&server, accepted), RefState::Established);
    (client, server, connected, accepted, link)
}

fn pump(a: &mut Stack<Endpoint>, b: &mut Stack<Endpoint>, from: i64, to: i64) {
    let mut time = from;
    while time <= to {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        time += 5;
    }
}

fn pump_until<F>(
    a: &mut Stack<Endpoint>,
    b: &mut Stack<Endpoint>,
    from: i64,
    to: i64,
    predicate: F,
) -> Option<i64>
where
    F: Fn(&Stack<Endpoint>, &Stack<Endpoint>) -> bool,
{
    let mut time = from;
    while time <= to {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        if predicate(a, b) {
            return Some(time);
        }
        time += 5;
    }
    None
}

fn state(stack: &Stack<Endpoint>, socket: SocketId) -> RefState {
    stack
        .tcp_state(socket)
        .map(RefState::from)
        .unwrap_or(RefState::Closed)
}

fn is_closed(stack: &Stack<Endpoint>, socket: SocketId) -> bool {
    state(stack, socket) == RefState::Closed
}

fn expected_delay(now: i64, deadline: Instant) -> Duration {
    let millis = u64::try_from(deadline.as_millis() - now).unwrap_or(0);
    Duration::from_millis(millis)
}

#[test]
fn half_close_reaches_time_wait_and_expires_after_2msl() {
    let (mut client, mut server, connected, accepted, _link) = pair(0x3160_0001);
    client.shutdown_write(connected).expect("shutdown");
    assert_eq!(state(&client, connected), RefState::FinWait1);
    pump(&mut client, &mut server, 200, 350);
    assert_eq!(state(&server, accepted), RefState::CloseWait);

    server.close(accepted).expect("server close");
    assert_eq!(state(&server, accepted), RefState::LastAck);
    let time_wait_at = pump_until(&mut client, &mut server, 350, 2_000, |client, _| {
        state(client, connected) == RefState::TimeWait
    })
    .expect("no TIME-WAIT");
    let now = Instant::from_millis(time_wait_at);
    let delay = client.next_poll_delay(now).expect("TIME-WAIT timer");
    assert!(
        delay > Duration::from_millis(59_000) && delay <= Duration::from_millis(60_000),
        "TIME-WAIT delay out of range: {delay:?}"
    );

    pump(
        &mut client,
        &mut server,
        time_wait_at,
        time_wait_at + 60_001,
    );
    assert!(
        is_closed(&client, connected),
        "TIME-WAIT did not expire after 2MSL"
    );
}

#[test]
fn fin_wait_two_has_its_own_deadline() {
    let (mut client, mut server, connected, accepted, _link) = pair(0x3160_0002);
    client.shutdown_write(connected).expect("shutdown");
    let fin_wait2_at = pump_until(&mut client, &mut server, 200, 1_000, |client, _| {
        state(client, connected) == RefState::FinWait2
    })
    .expect("no FIN-WAIT-2");
    assert_eq!(state(&server, accepted), RefState::CloseWait);
    let now = Instant::from_millis(fin_wait2_at);
    let expected = deadline(RefState::FinWait2, now).expect("FIN-WAIT-2 deadline");
    assert_eq!(
        client.next_poll_delay(now),
        Some(expected_delay(fin_wait2_at, expected)),
        "FIN-WAIT-2 deadline"
    );
    pump(&mut client, &mut server, fin_wait2_at, expected.as_millis());
    assert!(is_closed(&client, connected), "FIN-WAIT-2 did not expire");
}

#[test]
fn k267_fin_wait2_timer_is_still_armed_in_time_wait() {
    // K267/K271 finding: entering TIME-WAIT sets time_wait_deadline but leaves
    // fin_wait2_deadline armed, so next_poll_delay reports the older FIN-WAIT-2
    // deadline. The correct 2MSL delay is 60_000 ms; the observed value is the
    // remainder of the stale FIN-WAIT-2 timer. This test records the finding;
    // the repair belongs to K267/K271.
    let (mut client, mut server, connected, accepted, _link) = pair(0x3160_0007);
    client.shutdown_write(connected).expect("shutdown");
    let fin_wait2_at = pump_until(&mut client, &mut server, 200, 1_000, |client, _| {
        state(client, connected) == RefState::FinWait2
    })
    .expect("no FIN-WAIT-2");
    server.close(accepted).expect("server close");
    let time_wait_at = pump_until(
        &mut client,
        &mut server,
        fin_wait2_at,
        2_000,
        |client, _| state(client, connected) == RefState::TimeWait,
    )
    .expect("no TIME-WAIT");
    let stale = deadline(RefState::FinWait2, Instant::from_millis(fin_wait2_at))
        .expect("FIN-WAIT-2 deadline");
    let observed = client
        .next_poll_delay(Instant::from_millis(time_wait_at))
        .expect("timer");
    assert_eq!(observed, expected_delay(time_wait_at, stale));
    assert!(
        observed < Duration::from_millis(60_000),
        "the stale FIN-WAIT-2 timer must be the earlier deadline: {observed:?}"
    );
}

#[test]
fn simultaneous_close_reaches_time_wait_on_both_sides() {
    let (mut client, mut server, connected, accepted, _link) = pair(0x3160_0003);
    client.shutdown_write(connected).expect("client shutdown");
    server.shutdown_write(accepted).expect("server shutdown");
    let done = pump_until(&mut client, &mut server, 200, 3_000, |client, server| {
        state(client, connected) == RefState::TimeWait
            && state(server, accepted) == RefState::TimeWait
    })
    .expect("simultaneous close did not reach TIME-WAIT");
    assert!(done > 200);
}

#[test]
fn a_duplicate_fin_is_idempotent_on_the_peer() {
    let (mut client, mut server, connected, accepted, link) = pair(0x3160_0004);
    link.set_config(LinkConfig {
        duplicate_percent: 100.0,
        ..LinkConfig::default()
    });
    client.shutdown_write(connected).expect("shutdown");
    let closed = pump_until(&mut client, &mut server, 200, 1_000, |_, server| {
        state(server, accepted) == RefState::CloseWait
    });
    assert!(closed.is_some(), "peer did not accept the FIN");
    assert_eq!(state(&server, accepted), RefState::CloseWait);
    assert!(
        matches!(
            state(&client, connected),
            RefState::FinWait1 | RefState::FinWait2
        ),
        "duplicate FIN changed the sender state: {:?}",
        state(&client, connected)
    );
}

#[test]
fn abort_sends_rst_and_closes_both_sides() {
    let (mut client, mut server, connected, accepted, _link) = pair(0x3160_0005);
    client.abort(connected).expect("abort");
    assert!(is_closed(&client, connected));
    pump(&mut client, &mut server, 200, 500);
    assert!(
        is_closed(&server, accepted),
        "peer did not close on RST: {:?} error {:?}",
        state(&server, accepted),
        server.tcp_error(accepted)
    );
    assert_eq!(server.tcp_error(accepted), Ok(Some(Errno::ECONNRESET)));
}

#[test]
fn listener_abort_resets_a_half_open_child() {
    let (client_endpoint, server_endpoint) = Link::new(LinkConfig::default(), 0x3160_0006);
    let mut client = stack(client_endpoint, A, 7);
    let mut server = stack(server_endpoint, B, 8);
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    let listener = server.tcp_listen(80, 4).expect("listen");
    let connected = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    // Complete the handshake but do not accept: the child is still owned by the
    // listener's pending queue.
    pump(&mut client, &mut server, 0, 200);
    assert_eq!(state(&client, connected), RefState::Established);
    server.close(listener).expect("listener close");
    pump(&mut client, &mut server, 200, 500);
    assert!(
        is_closed(&client, connected),
        "pending child survived listener close: {:?} error {:?}",
        state(&client, connected),
        client.tcp_error(connected)
    );
    // `close` marks the listener record closed; `tcp_state` still reports the
    // listener kind, so the child reset above is the observable abort.
}
