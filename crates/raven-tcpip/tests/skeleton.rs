// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Skeleton tests for the C15 boundary: devices, timers, events, sockets and determinism.

#![cfg(feature = "testutil")]

use raven_abi::errno::Errno;
use raven_tcpip::{
    Device, Duration, Event, Instant, IpAddr, LoopbackDevice, MAX_TIMERS, NullDevice, Stack,
    StackConfig, TxError,
};

fn stack_with(config: StackConfig, seed: u64) -> Stack<LoopbackDevice> {
    Stack::new(LoopbackDevice::new(), config, seed, Instant::ZERO)
}

fn stack() -> Stack<LoopbackDevice> {
    stack_with(StackConfig::default(), 0x1234_5678)
}

fn tcp(stack: &mut Stack<LoopbackDevice>, local_port: u16) -> raven_tcpip::SocketId {
    stack
        .tcp_connect(
            (raven_tcpip::Ipv4Addr::new([192, 0, 2, 1]), 80),
            Some(local_port),
        )
        .expect("tcp_connect")
}

#[test]
fn loopback_frame_is_received_on_next_poll() {
    let now = Instant::ZERO;
    let mut device = LoopbackDevice::new();
    device
        .transmit(now, 4, &mut |buffer| buffer.copy_from_slice(b"ping"))
        .expect("transmit");
    let mut stack = Stack::new(device, StackConfig::default(), 7, now);
    let result = stack.poll(now);
    assert_eq!(result.rx_frames, 1);
    assert_eq!(result.tx_frames, 0);
}

#[test]
fn next_poll_delay_is_none_without_timers() {
    let mut stack = stack();
    assert_eq!(stack.next_poll_delay(Instant::ZERO), None);
}

#[test]
fn next_poll_delay_is_exact_to_the_millisecond() {
    let mut stack = stack();
    stack
        .set_timer(Instant::from_millis(250))
        .expect("set_timer");
    assert_eq!(
        stack.next_poll_delay(Instant::ZERO),
        Some(Duration::from_millis(250))
    );
}

#[test]
fn past_timer_fires_once_on_the_next_poll() {
    let mut stack = stack();
    stack
        .set_timer(Instant::from_millis(-1))
        .expect("set_timer");
    assert_eq!(stack.next_poll_delay(Instant::ZERO), Some(Duration::ZERO));
    stack.poll(Instant::ZERO);
    assert_eq!(stack.next_poll_delay(Instant::ZERO), None);
    let expired: Vec<_> = stack.take_expired_timers().collect();
    assert_eq!(expired.len(), 1);
    stack.poll(Instant::ZERO);
    assert_eq!(stack.take_expired_timers().count(), 0);
}

#[test]
fn take_events_drains_and_a_second_call_is_empty() {
    let mut stack = stack();
    let socket = tcp(&mut stack, 40000);
    stack.test_set_readable(socket, true).expect("readable");
    stack.poll(Instant::ZERO);
    let events: Vec<_> = stack.take_events().collect();
    assert!(events.contains(&Event::Readable(socket)));
    assert_eq!(stack.take_events().count(), 0);
}

#[test]
fn readable_transition_is_emitted_once_per_edge() {
    let mut stack = stack();
    let socket = tcp(&mut stack, 40000);
    stack.poll(Instant::ZERO);
    let _ = stack.take_events().count();

    stack.test_set_readable(socket, true).expect("readable");
    stack.poll(Instant::from_millis(1));
    let first: Vec<_> = stack.take_events().collect();
    assert_eq!(
        first
            .iter()
            .filter(|event| matches!(event, Event::Readable(id) if *id == socket))
            .count(),
        1
    );

    stack.poll(Instant::from_millis(2));
    let second: Vec<_> = stack.take_events().collect();
    assert!(
        !second
            .iter()
            .any(|event| matches!(event, Event::Readable(id) if *id == socket))
    );

    stack
        .test_set_readable(socket, false)
        .expect("not readable");
    stack.poll(Instant::from_millis(3));
    let _ = stack.take_events().count();
    stack
        .test_set_readable(socket, true)
        .expect("readable again");
    stack.poll(Instant::from_millis(4));
    let third: Vec<_> = stack.take_events().collect();
    assert_eq!(
        third
            .iter()
            .filter(|event| matches!(event, Event::Readable(id) if *id == socket))
            .count(),
        1
    );
}

#[test]
fn writable_transition_is_emitted_once() {
    let mut stack = stack();
    let socket = tcp(&mut stack, 40000);
    stack.poll(Instant::ZERO);
    let first: Vec<_> = stack.take_events().collect();
    assert_eq!(
        first
            .iter()
            .filter(|event| matches!(event, Event::Writable(id) if *id == socket))
            .count(),
        1
    );
    stack.poll(Instant::from_millis(1));
    let second: Vec<_> = stack.take_events().collect();
    assert!(
        !second
            .iter()
            .any(|event| matches!(event, Event::Writable(id) if *id == socket))
    );
}

#[test]
fn socket_ids_are_not_reused() {
    let mut stack = stack();
    let first = tcp(&mut stack, 40000);
    let second = tcp(&mut stack, 40001);
    stack.close(first).expect("close");
    let third = tcp(&mut stack, 40002);
    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_ne!(first, third);
    assert!(third.0 > second.0);
}

#[test]
fn socket_table_refuses_max_plus_one() {
    let config = StackConfig {
        max_sockets: 2,
        ..StackConfig::default()
    };
    let mut stack = stack_with(config, 1);
    tcp(&mut stack, 40000);
    tcp(&mut stack, 40001);
    let error = stack
        .tcp_connect(
            (raven_tcpip::Ipv4Addr::new([192, 0, 2, 1]), 80),
            Some(40002),
        )
        .expect_err("third socket must fail");
    assert_eq!(error, Errno::EMFILE);
}

#[test]
fn send_exhaustion_returns_eagain_and_counts_a_drop() {
    let config = StackConfig {
        tx_queue_bytes: 4,
        ..StackConfig::default()
    };
    let mut stack = stack_with(config, 1);
    let socket = tcp(&mut stack, 40000);
    assert_eq!(stack.send(socket, b"1234").expect("first send"), 4);
    assert_eq!(stack.send(socket, b"5"), Err(Errno::EAGAIN));
    let result = stack.poll(Instant::ZERO);
    assert_eq!(result.dropped, 1);
    let result = stack.poll(Instant::from_millis(1));
    assert_eq!(result.dropped, 0);
}

#[test]
fn one_hour_jump_runs_each_armed_timer_once() {
    let mut stack = stack();
    stack
        .set_timer(Instant::from_millis(1_000))
        .expect("timer 1");
    stack
        .set_timer(Instant::from_millis(2_000))
        .expect("timer 2");
    stack
        .set_timer(Instant::from_millis(3_000))
        .expect("timer 3");
    stack.poll(Instant::from_millis(3_600_000));
    let expired: Vec<_> = stack.take_expired_timers().collect();
    assert_eq!(expired.len(), 3);
    stack.poll(Instant::from_millis(3_600_001));
    assert_eq!(stack.take_expired_timers().count(), 0);
}

#[test]
fn two_stacks_with_the_same_seed_and_inputs_are_identical() {
    let frames: [&[u8]; 3] = [b"alpha", b"bravo", b"charlie"];
    let preload = |device: &mut LoopbackDevice| {
        for frame in frames {
            device
                .transmit(Instant::ZERO, frame.len(), &mut |buffer| {
                    buffer.copy_from_slice(frame);
                })
                .expect("preload");
        }
    };

    let mut left_device = LoopbackDevice::new();
    let mut right_device = LoopbackDevice::new();
    preload(&mut left_device);
    preload(&mut right_device);
    let config = StackConfig::default();
    let mut left = Stack::new(left_device, config, 99, Instant::ZERO);
    let mut right = Stack::new(right_device, config, 99, Instant::ZERO);
    left.test_set_trace(true);
    right.test_set_trace(true);

    for time in [0, 10, 100] {
        let now = Instant::from_millis(time);
        assert_eq!(left.poll(now), right.poll(now));
    }
    assert_eq!(left.test_trace(), right.test_trace());
    let left_events: Vec<_> = left.take_events().collect();
    let right_events: Vec<_> = right.take_events().collect();
    assert_eq!(left_events, right_events);
    assert_eq!(left.test_trace().len(), frames.len());
}

#[test]
fn loopback_rejects_frames_over_the_mtu() {
    let mut device = LoopbackDevice::with_mtu(16);
    let error = device
        .transmit(Instant::ZERO, 17, &mut |_| {})
        .expect_err("too long");
    assert_eq!(error, TxError::TooLong);
    assert_eq!(device.dropped(), 1);
}

#[test]
fn loopback_queue_is_bounded() {
    let mut device = LoopbackDevice::new();
    device.set_queue_limit(4);
    device
        .transmit(Instant::ZERO, 4, &mut |_| {})
        .expect("first frame");
    let error = device
        .transmit(Instant::ZERO, 1, &mut |_| {})
        .expect_err("queue full");
    assert_eq!(error, TxError::Busy);
    assert_eq!(device.queued_frames(), 1);
    assert_eq!(device.dropped(), 1);
}

#[test]
fn null_device_receives_nothing() {
    let mut stack = Stack::new(NullDevice::new(), StackConfig::default(), 3, Instant::ZERO);
    let result = stack.poll(Instant::ZERO);
    assert_eq!(result.rx_frames, 0);
}

#[test]
fn null_device_transmit_discards_silently() {
    let mut device = NullDevice::new();
    device
        .transmit(Instant::ZERO, 9, &mut |buffer| buffer.fill(0xaa))
        .expect("null transmit");
}

#[test]
fn udp_bind_rejects_duplicate_port() {
    let mut stack = stack();
    stack.udp_bind(1234).expect("first bind");
    assert_eq!(stack.udp_bind(1234), Err(Errno::EADDRINUSE));
}

#[test]
fn tcp_listen_rejects_duplicate_port_and_zero() {
    let mut stack = stack();
    stack.tcp_listen(8080, 4).expect("listen");
    assert_eq!(stack.tcp_listen(8080, 4), Err(Errno::EADDRINUSE));
    assert_eq!(stack.tcp_listen(0, 1), Err(Errno::EADDRNOTAVAIL));
}

#[test]
fn tcp_accept_returns_none_when_empty() {
    let mut stack = stack();
    let listener = stack.tcp_listen(8080, 2).expect("listen");
    assert_eq!(stack.tcp_accept(listener), Ok(None));
}

#[test]
fn tcp_accept_returns_a_pending_connection() {
    let mut stack = stack();
    let listener = stack.tcp_listen(8080, 2).expect("listen");
    let connection = stack.test_pending_connection(listener).expect("pending");
    assert_eq!(stack.tcp_accept(listener), Ok(Some(connection)));
    assert_eq!(stack.tcp_accept(listener), Ok(None));
}

#[test]
fn accepted_event_is_emitted_once() {
    let mut stack = stack();
    let listener = stack.tcp_listen(8080, 2).expect("listen");
    let connection = stack.test_pending_connection(listener).expect("pending");
    stack.poll(Instant::ZERO);
    let events: Vec<_> = stack.take_events().collect();
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Accepted { listener: l, conn } if *l == listener && *conn == connection))
            .count(),
        1
    );
    stack.poll(Instant::from_millis(1));
    assert!(
        !stack
            .take_events()
            .any(|event| matches!(event, Event::Accepted { .. }))
    );
}

#[test]
fn send_recv_roundtrip() {
    let mut stack = stack();
    let socket = tcp(&mut stack, 40000);
    assert_eq!(stack.send(socket, b"hello").expect("send"), 5);
    stack.poll(Instant::ZERO);
    stack.test_push_rx(socket, b"world").expect("push");
    stack.poll(Instant::from_millis(1));
    let mut buffer = [0u8; 16];
    assert_eq!(stack.recv(socket, &mut buffer).expect("recv"), 5);
    assert_eq!(&buffer[..5], b"world");
}

#[test]
fn recv_on_an_empty_socket_is_eagain() {
    let mut stack = stack();
    let socket = tcp(&mut stack, 40000);
    let mut buffer = [0u8; 4];
    assert_eq!(stack.recv(socket, &mut buffer), Err(Errno::EAGAIN));
}

#[test]
fn recv_after_peer_close_is_eof() {
    let mut stack = stack();
    let socket = tcp(&mut stack, 40000);
    stack.test_set_closed(socket, true).expect("closed");
    stack.poll(Instant::ZERO);
    let mut buffer = [0u8; 4];
    assert_eq!(stack.recv(socket, &mut buffer), Ok(0));
    let events: Vec<_> = stack.take_events().collect();
    assert!(events.contains(&Event::Closed(socket)));
}

#[test]
fn close_invalidates_the_socket() {
    let mut stack = stack();
    let socket = tcp(&mut stack, 40000);
    stack.close(socket).expect("close");
    assert_eq!(stack.send(socket, b"x"), Err(Errno::EBADF));
    assert_eq!(stack.recv(socket, &mut [0u8; 1]), Ok(0));
}

#[test]
fn shutdown_write_on_a_listener_is_enotconn() {
    let mut stack = stack();
    let listener = stack.tcp_listen(8080, 1).expect("listen");
    assert_eq!(stack.shutdown_write(listener), Err(Errno::ENOTCONN));
}

#[test]
fn udp_recv_from_returns_the_source() {
    let mut stack = stack();
    let socket = stack.udp_bind(5353).expect("bind");
    let source = raven_tcpip::Ipv4Addr::new([192, 0, 2, 9]);
    stack
        .test_push_udp(socket, b"dns!", (source, 53))
        .expect("push");
    let mut buffer = [0u8; 8];
    assert_eq!(
        stack.udp_recv_from(socket, &mut buffer),
        Ok((4, (IpAddr::V4(source), 53)))
    );
    assert_eq!(&buffer[..4], b"dns!");
}

#[test]
fn udp_send_to_bounds_the_write() {
    let config = StackConfig {
        tx_queue_bytes: 4,
        ..StackConfig::default()
    };
    let mut stack = stack_with(config, 1);
    let socket = stack.udp_bind(5353).expect("bind");
    assert_eq!(
        stack.udp_send_to(
            socket,
            b"12345",
            (raven_tcpip::Ipv4Addr::new([192, 0, 2, 1]), 53)
        ),
        Err(Errno::EAGAIN)
    );
    assert_eq!(stack.poll(Instant::ZERO).dropped, 5);
}

#[test]
fn timer_capacity_is_bounded() {
    let mut stack = stack();
    for index in 0..MAX_TIMERS {
        stack
            .set_timer(Instant::from_millis(index as i64))
            .expect("timer");
    }
    assert_eq!(stack.set_timer(Instant::from_millis(1)), Err(Errno::ENOMEM));
}

#[test]
fn unknown_socket_is_ebadf() {
    let mut stack = stack();
    let bogus = raven_tcpip::SocketId(999);
    assert_eq!(stack.send(bogus, b"x"), Err(Errno::EBADF));
    assert_eq!(stack.close(bogus), Err(Errno::EBADF));
    assert_eq!(stack.tcp_accept(bogus), Err(Errno::EBADF));
}

#[test]
fn config_defaults_are_bounded() {
    let config = StackConfig::default();
    assert!(config.max_sockets > 0 && config.max_sockets <= 1024);
    assert!(config.rx_queue_bytes > 0 && config.rx_queue_bytes <= 1024 * 1024);
    assert!(config.tx_queue_bytes > 0 && config.tx_queue_bytes <= 1024 * 1024);
    assert!(config.reassembly_bytes > 0 && config.reassembly_bytes <= 4 * 1024 * 1024);
    assert!(config.neighbor_cache_entries > 0);
}
