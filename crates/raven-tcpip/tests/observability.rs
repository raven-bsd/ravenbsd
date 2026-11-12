// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C19: the normative inventory, the typed property table and the per-socket view.

#![cfg(feature = "testutil")]

mod common;

use raven_abi::errno::Errno;
use raven_tcpip::testutil::NullDevice;
use raven_tcpip::{Counters, PropKind, PropertyInfo, SocketId, Stability, Stack, TcpState, Unit};

use common::{A, B, address, handshake, pair, pump_ms};

const SPEC: &str = include_str!("../../../spec/tcpip/observability.md");

fn spec_counters() -> Vec<&'static str> {
    let mut names = Vec::new();
    for line in SPEC.lines() {
        if !line.starts_with("| `tcp.") {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() > 3 && fields[2] == "u64" {
            names.push(fields[1].trim_matches('`'));
        }
    }
    names
}

fn spec_properties() -> Vec<PropertyInfo> {
    let mut properties = Vec::new();
    for line in SPEC.lines() {
        if !line.starts_with("| `tcp.") {
            continue;
        }
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        if fields.len() < 9 {
            continue;
        }
        let kind = match fields[2] {
            "uint" => PropKind::Uint,
            "bool" => PropKind::Bool,
            "enum" => PropKind::Enum,
            _ => continue,
        };
        let unit = match fields[6] {
            "ms" => Unit::Ms,
            "bool" => Unit::Bool,
            "count" => Unit::Count,
            "entries" => Unit::Entries,
            "holes" => Unit::Holes,
            "segments" => Unit::Segments,
            "bytes" => Unit::Bytes,
            "enum" => Unit::Enum,
            "ranges" => Unit::Ranges,
            "port" => Unit::Port,
            other => panic!("unknown unit {other}"),
        };
        properties.push(PropertyInfo {
            name: fields[1].trim_matches('`'),
            kind,
            min: fields[3].parse().expect("min"),
            max: fields[4].parse().expect("max"),
            default: fields[5].parse().expect("default"),
            unit,
            stability: match fields[7] {
                "COMMITTED" => Stability::Committed,
                "PRIVATE" => Stability::Private,
                other => panic!("unknown stability {other}"),
            },
        });
    }
    properties
}

#[test]
fn spec_counter_rows_are_complete() {
    let spec = spec_counters();
    assert_eq!(spec.len(), 84, "spec counter rows");
    let mut missing = Vec::new();
    for name in &spec {
        if Counters::default().get(name).is_none() {
            missing.push(*name);
        }
    }
    assert!(missing.is_empty(), "missing counters: {missing:?}");
    let mut extra = Vec::new();
    for name in Counters::names() {
        if !spec.contains(name) {
            extra.push(*name);
        }
    }
    assert!(extra.is_empty(), "extra counters: {extra:?}");
}

#[test]
fn spec_property_rows_are_complete() {
    let spec = spec_properties();
    assert_eq!(spec.len(), 58, "spec property rows");
    let table = Stack::<NullDevice>::properties();
    assert_eq!(table.len(), spec.len());
    for expected in &spec {
        let actual = table
            .iter()
            .find(|info| info.name == expected.name)
            .unwrap_or_else(|| panic!("missing property {}", expected.name));
        assert_eq!(actual.kind, expected.kind, "{}", expected.name);
        assert_eq!(actual.min, expected.min, "{}", expected.name);
        assert_eq!(actual.max, expected.max, "{}", expected.name);
        assert_eq!(actual.default, expected.default, "{}", expected.name);
        assert_eq!(actual.unit, expected.unit, "{}", expected.name);
        assert_eq!(actual.stability, expected.stability, "{}", expected.name);
    }
}

#[test]
fn committed_property_golden_list_is_stable() {
    let committed: Vec<&str> = Stack::<NullDevice>::properties()
        .iter()
        .filter(|info| info.stability == Stability::Committed)
        .map(|info| info.name)
        .collect();
    let golden = [
        "tcp.time_wait_interval",
        "tcp.finwait2_timeout",
        "tcp.user_timeout",
        "tcp.keepalive_enable",
        "tcp.keepalive_interval",
        "tcp.keepalive_probes",
        "tcp.half_open_limit",
        "tcp.accept_queue_limit",
        "tcp.connection_max",
        "tcp.timewait_max",
        "tcp.reassembly_timeout",
        "tcp.rto_initial",
        "tcp.rto_min",
        "tcp.rto_max",
        "tcp.rto_max_retries",
        "tcp.connect_retries",
        "tcp.delack_delay",
        "tcp.delack_segments",
        "tcp.zwprobe_max_interval",
        "tcp.dup_ack_threshold",
        "tcp.initial_window",
        "tcp.cwnd_max",
        "tcp.cwnd_min",
        "tcp.cc_algorithm",
        "tcp.ignore_path_mtu",
        "tcp.pmtu_cache_ttl",
        "tcp.plpmtud_enable",
        "tcp.plpmtud_interval",
        "tcp.reordering_window_base",
        "tcp.tlp_enable",
        "tcp.window_scale_enable",
        "tcp.timestamps_enable",
        "tcp.sack_enable",
        "tcp.ecn_enable",
        "tcp.syn_cookies_enable",
        "tcp.syn_cookie_lifetime",
        "tcp.mss_floor",
        "tcp.mss_default",
        "tcp.mss_ceiling",
        "tcp.send_buffer_default",
        "tcp.send_buffer_max",
        "tcp.recv_buffer_default",
        "tcp.recv_buffer_max",
        "tcp.nagle",
        "tcp.urgent_mode",
        "tcp.port_range_first",
        "tcp.port_range_last",
        "tcp.isn_strength",
        "tcp.challenge_ack_burst",
        "tcp.challenge_ack_interval",
        "tcp.challenge_ack_global_burst",
        "tcp.challenge_ack_global_interval",
    ];
    assert_eq!(committed, golden);
}

#[test]
fn private_property_golden_list_is_stable() {
    let private: Vec<&str> = Stack::<NullDevice>::properties()
        .iter()
        .filter(|info| info.stability == Stability::Private)
        .map(|info| info.name)
        .collect();
    assert_eq!(
        private,
        [
            "tcp.reassembly_holes_max",
            "tcp.timer_late_threshold",
            "tcp.pmtu_cache_max",
            "tcp.sack_ranges_max",
            "tcp.hash_chain_max",
            "tcp.port_alloc_attempts",
        ]
    );
}

#[test]
fn every_spec_counter_is_reachable_on_a_live_stack() {
    let (a, b) = (
        raven_tcpip::testutil::LoopbackDevice::new(),
        raven_tcpip::testutil::LoopbackDevice::new(),
    );
    let a = Stack::new(
        a,
        raven_tcpip::StackConfig::default(),
        1,
        raven_tcpip::Instant::ZERO,
    );
    let b = Stack::new(
        b,
        raven_tcpip::StackConfig::default(),
        2,
        raven_tcpip::Instant::ZERO,
    );
    let _ = (a, b);
    let stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        3,
        raven_tcpip::Instant::ZERO,
    );
    for name in Counters::names() {
        assert!(stack.counters().get(name).is_some(), "{name}");
    }
}

#[test]
fn every_property_round_trips_on_a_live_stack() {
    let mut stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        4,
        raven_tcpip::Instant::ZERO,
    );
    for info in Stack::<NullDevice>::properties() {
        stack
            .set_property(info.name, info.default)
            .unwrap_or_else(|error| panic!("{}: {error:?}", info.name));
        assert_eq!(
            stack.get_property(info.name),
            Ok(info.default),
            "{}",
            info.name
        );
    }
}

#[test]
fn out_of_range_property_is_einval_and_changes_nothing() {
    let mut stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        5,
        raven_tcpip::Instant::ZERO,
    );
    for info in Stack::<NullDevice>::properties() {
        let before = stack.get_property(info.name).expect("before");
        if info.min > 0 {
            assert_eq!(
                stack.set_property(info.name, info.min - 1),
                Err(Errno::EINVAL),
                "{} below min",
                info.name
            );
            assert_eq!(stack.get_property(info.name), Ok(before));
        }
        assert_eq!(
            stack.set_property(info.name, info.max + 1),
            Err(Errno::EINVAL),
            "{} above max",
            info.name
        );
        assert_eq!(stack.get_property(info.name), Ok(before));
    }
}

#[test]
fn cross_field_rto_bounds_are_atomic() {
    let mut stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        6,
        raven_tcpip::Instant::ZERO,
    );
    let max = stack.get_property("tcp.rto_max").expect("max");
    assert_eq!(
        stack.set_property("tcp.rto_min", max + 1),
        Err(Errno::EINVAL)
    );
    let min = stack.get_property("tcp.rto_min").expect("min");
    assert_eq!(
        stack.set_property("tcp.rto_max", min - 1),
        Err(Errno::EINVAL)
    );
    assert_eq!(stack.get_property("tcp.rto_min"), Ok(min));
}

#[test]
fn cross_field_port_range_is_atomic() {
    let mut stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        7,
        raven_tcpip::Instant::ZERO,
    );
    let last = stack.get_property("tcp.port_range_last").expect("last");
    assert_eq!(
        stack.set_property("tcp.port_range_first", last + 1),
        Err(Errno::EINVAL)
    );
    let first = stack.get_property("tcp.port_range_first").expect("first");
    assert_eq!(
        stack.set_property("tcp.port_range_last", first - 1),
        Err(Errno::EINVAL)
    );
}

#[test]
fn cross_field_buffer_defaults_are_atomic() {
    let mut stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        8,
        raven_tcpip::Instant::ZERO,
    );
    let max = stack.get_property("tcp.send_buffer_max").expect("max");
    assert_eq!(
        stack.set_property("tcp.send_buffer_default", max + 1000),
        Err(Errno::EINVAL)
    );
    let max = stack.get_property("tcp.recv_buffer_max").expect("max");
    assert_eq!(
        stack.set_property("tcp.recv_buffer_default", max + 1000),
        Err(Errno::EINVAL)
    );
}

#[test]
fn cross_field_queue_limits_are_atomic() {
    let mut stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        9,
        raven_tcpip::Instant::ZERO,
    );
    let max = stack.get_property("tcp.connection_max").expect("max");
    assert_eq!(
        stack.set_property("tcp.half_open_limit", max + 1),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        stack.set_property("tcp.accept_queue_limit", max + 1),
        Err(Errno::EINVAL)
    );
}

#[test]
fn unknown_property_is_enoent() {
    let mut stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        10,
        raven_tcpip::Instant::ZERO,
    );
    assert_eq!(stack.get_property("tcp.nope"), Err(Errno::ENOENT));
    assert_eq!(stack.set_property("tcp.nope", 1), Err(Errno::ENOENT));
}

#[test]
fn counters_reset_zeroes_a_live_stack() {
    let (_link, mut a, mut b) = pair();
    let (socket, peer) = handshake(&mut a, &mut b);
    a.send(socket, b"move").expect("send");
    pump_ms(&mut a, &mut b, 210, 500);
    let mut buffer = [0u8; 8];
    let _ = b.recv(peer, &mut buffer);
    assert!(a.counters().out_segments > 0);
    a.counters_reset();
    for name in Counters::names() {
        assert_eq!(a.counters().get(name), Some(0), "{name}");
    }
}

#[test]
fn socket_info_for_listener_reports_listen() {
    let mut stack: Stack<raven_tcpip::testutil::LoopbackDevice> = Stack::new(
        raven_tcpip::testutil::LoopbackDevice::new(),
        raven_tcpip::StackConfig::default(),
        11,
        raven_tcpip::Instant::ZERO,
    );
    let listener = stack.tcp_listen(80, 4).expect("listen");
    let info = stack.socket_info(listener).expect("info");
    assert_eq!(info.state, TcpState::Listen);
    assert_eq!(info.local.1, 80);
    assert_eq!(info.remote, None);
    assert_eq!(info.cc_algorithm, "newreno");
}

#[test]
fn socket_info_for_established_reports_state_and_tuple() {
    let (_link, mut a, mut b) = pair();
    let (socket, _peer) = handshake(&mut a, &mut b);
    let info = a.socket_info(socket).expect("info");
    assert_eq!(info.state, TcpState::Established);
    assert_eq!(info.local, (raven_tcpip::IpAddr::V4(address(A)), 40_000));
    assert_eq!(info.remote, Some((raven_tcpip::IpAddr::V4(address(B)), 80)));
    assert!(info.mss >= 536);
    assert!(info.cwnd >= u32::from(info.mss));
}

#[test]
fn sockets_enumerates_listener_and_connection() {
    let (_link, mut a, mut b) = pair();
    let (socket, _peer) = handshake(&mut a, &mut b);
    let a_ids: Vec<SocketId> = a.sockets().collect();
    assert_eq!(a_ids, vec![socket]);
    let b_ids: Vec<SocketId> = b.sockets().collect();
    assert_eq!(b_ids.len(), 2, "listener and accepted connection");
    assert!(b_ids.contains(&SocketId(1)));
}

#[test]
fn socket_info_unknown_socket_is_ebadf() {
    let stack: Stack<NullDevice> = Stack::new(
        NullDevice::new(),
        raven_tcpip::StackConfig::default(),
        12,
        raven_tcpip::Instant::ZERO,
    );
    assert_eq!(stack.socket_info(SocketId(99)), Err(Errno::EBADF));
}

#[test]
fn option_flags_match_the_handshake_trace() {
    let (link, mut a, mut b) = pair();
    let start = link.trace().entries().len();
    let (socket, _peer) = handshake(&mut a, &mut b);
    let info = a.socket_info(socket).expect("info");
    assert!(info.options.wscale.is_some());
    assert!(info.options.timestamps);
    assert!(info.options.sack);
    assert!(info.options.ecn);
    let kinds = common::trace_options(&link, start);
    assert!(kinds.iter().any(|options| options.contains(&3)));
    assert!(kinds.iter().any(|options| options.contains(&4)));
    assert!(kinds.iter().any(|options| options.contains(&8)));
}

#[test]
fn socket_info_mid_transfer_has_plausible_values() {
    let (_link, mut a, mut b) = pair();
    let (socket, peer) = handshake(&mut a, &mut b);
    let payload: Vec<u8> = (0..1_048_576).map(|index| (index * 31 + 7) as u8).collect();
    let received = common::send_all(&mut a, &mut b, socket, peer, &payload, 210);
    assert_eq!(received.len(), payload.len());
    let info = a.socket_info(socket).expect("info");
    assert!(info.send_queue < 4096, "send queue {}", info.send_queue);
    assert!(info.cwnd >= 2 * u32::from(info.mss), "cwnd {}", info.cwnd);
    assert!(info.srtt_ms > 0);
    assert!(info.rto_ms >= 200);
    assert_eq!(info.cc_algorithm, "cubic");
}
