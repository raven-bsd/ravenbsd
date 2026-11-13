// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Socket translation tests over the fake netd.

use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::ErrnoExt;
use crate::fake::{FakeNetd, WireLog};
use raven_libc_abi::socket::{
    AF_INET, AF_INET6, AF_UNIX, MSG_NOSIGNAL, MSG_OOB, SO_ACCEPTCONN, SO_DOMAIN, SO_ERROR,
    SO_PROTOCOL, SO_RCVBUF, SO_RCVTIMEO, SO_TYPE, SOCK_DGRAM, SOCK_NONBLOCK, SOCK_STREAM,
    SOL_SOCKET,
};
use raven_proto_net::{Call, Endpoint, SockOpt};

fn net(caps: Caps) -> Net<FakeNetd, FakeNetd, FakeNetd, FakeNetd> {
    let fake = FakeNetd::with_caps(caps);
    Net::with_unix(fake.clone(), fake.clone(), fake.clone(), Some(fake))
}

fn net_without_unix(caps: Caps) -> Net<FakeNetd, FakeNetd, FakeNetd> {
    let fake = FakeNetd::with_caps(caps);
    Net::new(fake.clone(), fake.clone(), fake.clone())
}

fn addr(ip: [u8; 4], port: u16) -> Vec<u8> {
    let mut out = [0u8; 16];
    crate::write_sockaddr(&crate::SockAddr::In { ip, port }, &mut out);
    out.to_vec()
}

fn logs<U: crate::UnixBackend>(net: &Net<FakeNetd, FakeNetd, FakeNetd, U>) -> Vec<WireLog> {
    net.backend().log()
}

#[test]
fn socket_is_pending_today() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    assert_eq!(logs(&net), Vec::new());
    assert_eq!(net.poll_interest(fd), Err(Errno::EBADF));
}

#[test]
fn bare_socket_opens_immediately() {
    let caps = Caps {
        bare_open: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    assert_eq!(logs(&net), vec![WireLog::Open(b"net:tcp".to_vec())]);
    assert!(net.poll_interest(fd).is_ok());
}

#[test]
fn type_flags_and_errors() {
    let net = net_without_unix(Caps::default());
    let fd = net
        .socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK, 0)
        .expect("socket");
    assert!(net.fds().resolve(fd).expect("resolve").nonblock);
    assert_eq!(
        net.socket(AF_INET, SOCK_STREAM, 99),
        Err(Errno::EPROTONOSUPPORT)
    );
    assert_eq!(net.socket(17, SOCK_STREAM, 0), Err(Errno::EAFNOSUPPORT));
    assert_eq!(
        net.socket(AF_INET6, SOCK_STREAM, 0),
        Err(Errno::EAFNOSUPPORT)
    );
}

#[test]
fn socketpair_default_and_fake() {
    let net = net(Caps::default());
    let pair = net.socketpair(AF_UNIX, SOCK_STREAM, 0).expect("pair");
    let mut out = [0u8; 16];
    net.sendmsg(pair[0], None, &[b"ping"], &[], 0)
        .expect("send");
    let mut buffer = [0u8; 8];
    let mut iov = [&mut buffer[..]];
    let received = net
        .recvmsg(pair[1], &mut out, &mut iov, &mut [], 0)
        .expect("recv");
    assert_eq!(received.len, 4);
    assert_eq!(&buffer[..4], b"ping");
    assert_eq!(
        net.socketpair(AF_INET, SOCK_STREAM, 0),
        Err(Errno::EAFNOSUPPORT)
    );
}

#[test]
fn bind_twice_and_family_mismatch() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    let address = addr([127, 0, 0, 1], 8080);
    net.bind(fd, &address).expect("bind");
    assert_eq!(net.bind(fd, &address), Err(Errno::EINVAL));
    let fd2 = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket2");
    let wrong = addr([127, 0, 0, 1], 1);
    let mut six = [0u8; 28];
    six[0] = 10;
    crate::write_sockaddr(
        &crate::SockAddr::In6 {
            ip: [0; 16],
            port: 1,
            scope: 0,
            flowinfo: 0,
        },
        &mut six,
    );
    assert_eq!(net.bind(fd2, &six), Err(Errno::EAFNOSUPPORT));
    let _ = wrong;
}

#[test]
fn pending_getsockname_returns_the_stored_local() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.bind(fd, &addr([10, 0, 0, 1], 1234)).expect("bind");
    let mut out = [0u8; 16];
    assert_eq!(net.getsockname(fd, &mut out), Ok(16));
    let parsed = crate::parse_sockaddr(&out).expect("parse");
    assert_eq!(parsed.port(), 1234);
    assert_eq!(parsed.ipv4(), Some([10, 0, 0, 1]));
}

#[test]
fn bind_port_zero_in_lazy_mode_stores_zero() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    net.bind(fd, &addr([0, 0, 0, 0], 0)).expect("bind");
    let mut out = [0u8; 16];
    assert_eq!(net.getsockname(fd, &mut out), Ok(16));
    assert_eq!(crate::parse_sockaddr(&out).expect("parse").port(), 0);
}

#[test]
fn tcp_client_wire_sequence() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.setsockopt(fd, SOL_SOCKET, SO_RCVBUF, &65_536i32.to_le_bytes())
        .expect("setopt");
    net.connect(fd, &addr([10, 0, 0, 2], 80)).expect("connect");
    assert_eq!(
        logs(&net),
        vec![
            WireLog::Open(b"net:tcp/10.0.0.2:80".to_vec()),
            WireLog::Call(Call::SetSockOpt {
                opt: SockOpt::RcvBuf,
                value: 65_536
            })
        ]
    );
    let mut out = [0u8; 16];
    assert_eq!(net.getpeername(fd, &mut out), Ok(16));
    assert_eq!(
        crate::parse_sockaddr(&out).expect("parse").ipv4(),
        Some([10, 0, 0, 2])
    );
}

#[test]
fn connect_port_zero_is_refused_without_open() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    assert_eq!(
        net.connect(fd, &addr([10, 0, 0, 2], 0)),
        Err(Errno::ECONNREFUSED)
    );
    assert!(logs(&net).is_empty());
}

#[test]
fn tcp_server_wire_sequence() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.bind(fd, &addr([0, 0, 0, 0], 8080)).expect("bind");
    net.listen(fd, 100).expect("listen");
    assert_eq!(
        logs(&net),
        vec![
            WireLog::Open(b"net:tcp/0.0.0.0:8080".to_vec()),
            WireLog::Call(Call::Listen { backlog: 100 })
        ]
    );
}

#[test]
fn backlog_clamping() {
    for (requested, expected) in [(0, 128), (-1, 128), (10_000, 4096), (4096, 4096)] {
        let net = net_without_unix(Caps::default());
        let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
        net.bind(fd, &addr([0, 0, 0, 0], 8081)).expect("bind");
        net.listen(fd, requested).expect("listen");
        let calls = logs(&net);
        assert!(calls.contains(&WireLog::Call(Call::Listen { backlog: expected })));
    }
}

#[test]
fn listen_errors() {
    let net = net_without_unix(Caps::default());
    let udp = net.socket(AF_INET, SOCK_DGRAM, 0).expect("udp");
    assert_eq!(net.listen(udp, 1), Err(Errno::EOPNOTSUPP));
    let tcp = net.socket(AF_INET, SOCK_STREAM, 0).expect("tcp");
    assert_eq!(net.listen(tcp, 1), Err(Errno::EINVAL));
}

#[test]
fn accept_requires_a_listener_and_valid_flags() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    let mut out = [0u8; 16];
    assert_eq!(net.accept(fd, &mut out, 0), Err(Errno::EINVAL));
    net.bind(fd, &addr([0, 0, 0, 0], 8082)).expect("bind");
    net.listen(fd, 1).expect("listen");
    assert_eq!(net.accept(fd, &mut out, 0x1000), Err(Errno::EINVAL));
}

#[test]
fn accept_two_round_trips_and_peer() {
    let net = net_without_unix(Caps::default());
    let listener = net.socket(AF_INET, SOCK_STREAM, 0).expect("listener");
    net.bind(listener, &addr([0, 0, 0, 0], 8083)).expect("bind");
    net.listen(listener, 1).expect("listen");
    let client = net.socket(AF_INET, SOCK_STREAM, 0).expect("client");
    net.connect(client, &addr([127, 0, 0, 1], 8083))
        .expect("connect");
    let mut out = [0u8; 16];
    let (accepted, name_len) = net.accept(listener, &mut out, 0).expect("accept");
    assert_eq!(name_len, 16);
    let calls = logs(&net);
    assert!(
        calls
            .iter()
            .any(|entry| matches!(entry, WireLog::Call(Call::Accept)))
    );
    assert!(
        calls
            .iter()
            .any(|entry| matches!(entry, WireLog::Call(Call::GetPeerName)))
    );
    assert!(net.poll_interest(accepted).is_ok());
}

#[test]
fn connect_twice_is_eisconn() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.connect(fd, &addr([10, 0, 0, 2], 80)).expect("connect");
    assert_eq!(
        net.connect(fd, &addr([10, 0, 0, 2], 80)),
        Err(Errno::EISCONN)
    );
}

#[test]
fn nonblocking_connect_reports_einprogress_then_error_zero() {
    let net = net_without_unix(Caps::default());
    let fd = net
        .socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK, 0)
        .expect("socket");
    assert_eq!(
        net.connect(fd, &addr([10, 0, 0, 2], 80)),
        Err(Errno::EINPROGRESS)
    );
    let mut value = [0xffu8; 4];
    assert_eq!(net.getsockopt(fd, SOL_SOCKET, SO_ERROR, &mut value), Ok(4));
    assert_eq!(i32::from_le_bytes(value), 0);
}

#[test]
fn udp_sendto_two_destinations_one_open() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    let first = addr([192, 0, 2, 7], 53);
    let second = addr([192, 0, 2, 8], 53);
    let sent = net
        .sendmsg(fd, Some(&first), &[b"dns"], &[], 0)
        .expect("first");
    assert_eq!(sent, 3);
    net.sendmsg(fd, Some(&second), &[b"x"], &[], 0)
        .expect("second");
    assert_eq!(
        logs(&net),
        vec![
            WireLog::Open(b"net:udp/192.0.2.7:53".to_vec()),
            WireLog::Write { len: 3, to: None },
            WireLog::Write {
                len: 1,
                to: Some(Endpoint {
                    ip: [192, 0, 2, 8],
                    port: 53
                })
            }
        ]
    );
}

#[test]
fn udp_send_without_destination_is_edestaddrreq() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    let error = net.sendmsg(fd, None, &[b"x"], &[], 0).expect_err("send");
    assert_eq!(error.errno, Errno::EDESTADDRREQ);
}

#[test]
fn stream_round_trip_and_scatter() {
    let net = net_without_unix(Caps::default());
    let (listener, client, server) = connected_pair(&net, 8084);
    let _ = listener;
    let sent = net
        .sendmsg(client, None, &[b"abc", b"def", b"ghij"], &[], 0)
        .expect("send");
    assert_eq!(sent, 10);
    let mut first = [0u8; 4];
    let mut second = [0u8; 4];
    let mut third = [0u8; 4];
    let mut iov = [&mut first[..], &mut second[..], &mut third[..]];
    let received = net
        .recvmsg(server, &mut [], &mut iov, &mut [], 0)
        .expect("recv");
    assert_eq!(received.len, 10);
    assert_eq!(&first, b"abcd");
    assert_eq!(&second, b"efgh");
    assert_eq!(&third[..2], b"ij");
}

#[test]
fn stream_eof_after_peer_shutdown() {
    let net = net_without_unix(Caps::default());
    let (_listener, client, server) = connected_pair(&net, 8085);
    net.shutdown(client, 1).expect("shutdown");
    let mut buffer = [0u8; 8];
    let mut iov = [&mut buffer[..]];
    let received = net
        .recvmsg(server, &mut [], &mut iov, &mut [], 0)
        .expect("recv");
    assert_eq!(received.len, 0);
}

#[test]
fn epipe_raises_sigpipe_unless_nosignal() {
    let net = net_without_unix(Caps::default());
    let (_listener, client, _server) = connected_pair(&net, 8086);
    net.shutdown(client, 1).expect("shutdown");
    let error = net
        .sendmsg(client, None, &[b"x"], &[], 0)
        .expect_err("send");
    assert_eq!(error.errno, Errno::EPIPE);
    assert!(error.raise_sigpipe);
    let error = net
        .sendmsg(client, None, &[b"x"], &[], MSG_NOSIGNAL)
        .expect_err("send");
    assert_eq!(error.errno, Errno::EPIPE);
    assert!(!error.raise_sigpipe);
}

#[test]
fn send_errors_and_large_datagram() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    assert_eq!(
        net.sendmsg(fd, None, &[b"x"], &[], MSG_OOB)
            .expect_err("oob")
            .errno,
        Errno::EOPNOTSUPP
    );
    let data = vec![0u8; 65_536];
    assert_eq!(
        net.sendmsg(fd, Some(&addr([192, 0, 2, 7], 53)), &[&data], &[], 0)
            .expect_err("big")
            .errno,
        Errno::EMSGSIZE
    );
}

#[test]
fn recv_rejects_peek_without_capability_and_oob() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    let mut buffer = [0u8; 8];
    let mut iov = [&mut buffer[..]];
    assert_eq!(
        net.recvmsg(fd, &mut [], &mut iov, &mut [], MSG_PEEK),
        Err(Errno::EOPNOTSUPP)
    );
    assert_eq!(
        net.recvmsg(fd, &mut [], &mut iov, &mut [], MSG_OOB),
        Err(Errno::EOPNOTSUPP)
    );
}

#[test]
fn options_record_and_error_clear() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    let mut value = [0u8; 4];
    assert_eq!(net.getsockopt(fd, SOL_SOCKET, SO_TYPE, &mut value), Ok(4));
    assert_eq!(i32::from_le_bytes(value), SOCK_STREAM);
    assert_eq!(net.getsockopt(fd, SOL_SOCKET, SO_DOMAIN, &mut value), Ok(4));
    assert_eq!(i32::from_le_bytes(value), AF_INET);
    assert_eq!(
        net.getsockopt(fd, SOL_SOCKET, SO_PROTOCOL, &mut value),
        Ok(4)
    );
    assert_eq!(i32::from_le_bytes(value), 6);
    assert_eq!(
        net.getsockopt(fd, SOL_SOCKET, SO_ACCEPTCONN, &mut value),
        Ok(4)
    );
    assert_eq!(i32::from_le_bytes(value), 0);
    assert_eq!(
        net.getsockopt(fd, SOL_SOCKET, 999, &mut value),
        Err(Errno::ENOPROTOOPT)
    );
    assert_eq!(
        net.getsockopt(fd, SOL_SOCKET, SO_ERROR, &mut [0u8; 3]),
        Err(Errno::EINVAL)
    );
}

#[test]
fn stored_options_roundtrip() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.setsockopt(fd, SOL_SOCKET, 9, &1i32.to_le_bytes())
        .expect("keepalive");
    let mut value = [0u8; 4];
    assert_eq!(net.getsockopt(fd, SOL_SOCKET, 9, &mut value), Ok(4));
    assert_eq!(i32::from_le_bytes(value), 1);
    net.setsockopt(fd, SOL_SOCKET, 13, &[1, 0, 0, 0, 2, 0, 0, 0])
        .expect("linger");
    let mut linger = [0u8; 8];
    assert_eq!(net.getsockopt(fd, SOL_SOCKET, 13, &mut linger), Ok(8));
    assert_eq!(&linger[..4], &1i32.to_le_bytes());
    assert_eq!(&linger[4..], &2i32.to_le_bytes());
}

#[test]
fn timeout_option_storage() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    let mut value = [0u8; 16];
    value[..8].copy_from_slice(&0i64.to_le_bytes());
    value[8..].copy_from_slice(&50_000i64.to_le_bytes());
    net.setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &value)
        .expect("timeout");
    let mut out = [0u8; 16];
    assert_eq!(
        net.getsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &mut out),
        Ok(16)
    );
    assert_eq!(&out[8..], &50_000i64.to_le_bytes());
}

#[test]
fn sockatmark_and_shutdown_values() {
    let net = net_without_unix(Caps::default());
    let tcp = net.socket(AF_INET, SOCK_STREAM, 0).expect("tcp");
    assert_eq!(net.sockatmark(tcp), Ok(0));
    let udp = net.socket(AF_INET, SOCK_DGRAM, 0).expect("udp");
    assert_eq!(net.sockatmark(udp), Err(Errno::ENOTTY));
    assert_eq!(net.shutdown(tcp, 7), Err(Errno::EINVAL));
    assert_eq!(net.shutdown(tcp, 0), Err(Errno::ENOTCONN));
}

#[test]
fn poll_interest_and_revents_through_net() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.connect(fd, &addr([10, 0, 0, 2], 80)).expect("connect");
    let interest = net.poll_interest(fd).expect("interest");
    assert!(interest.read && interest.write);
    let ev = WaitItem {
        sock: interest.sock,
        got_read: true,
        got_write: true,
        ..WaitItem::default()
    };
    let revents = net.poll_revents(fd, 0x7, &ev) as u16;
    assert_ne!(revents & raven_libc_abi::poll::POLLIN as u16, 0);
    assert_ne!(revents & raven_libc_abi::poll::POLLOUT as u16, 0);
}

#[test]
fn iface_methods_on_net() {
    let net = net_without_unix(Caps::default());
    assert_eq!(net.if_nametoindex(b"lo"), Ok(1));
    let mut name = [0u8; 16];
    assert_eq!(net.if_indextoname(2, &mut name), Ok(4));
    assert_eq!(net.if_nameindex().expect("nameindex").len(), 2);
    assert_eq!(net.getifaddrs().expect("getifaddrs").len(), 2);
}

#[test]
fn dns_io_and_probe_types_are_available() {
    let net = net_without_unix(Caps::default());
    let _dns = net.dns_io();
    let _probe = net.probe();
    let _names = net.ifnames();
}

fn connected_pair(net: &Net<FakeNetd, FakeNetd, FakeNetd>, port: u16) -> (i32, i32, i32) {
    let listener = net.socket(AF_INET, SOCK_STREAM, 0).expect("listener");
    net.bind(listener, &addr([0, 0, 0, 0], port)).expect("bind");
    net.listen(listener, 1).expect("listen");
    let client = net.socket(AF_INET, SOCK_STREAM, 0).expect("client");
    net.connect(client, &addr([127, 0, 0, 1], port))
        .expect("connect");
    let mut out = [0u8; 16];
    let (server, _) = net.accept(listener, &mut out, 0).expect("accept");
    (listener, client, server)
}

#[test]
fn blocking_receive_times_out_with_the_stored_deadline() {
    let net = net_without_unix(Caps {
        bare_open: true,
        ..Caps::default()
    });
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    net.bind(fd, &addr([0, 0, 0, 0], 9100)).expect("bind");
    let mut value = [0u8; 16];
    value[8..].copy_from_slice(&50_000i64.to_le_bytes());
    net.setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &value)
        .expect("timeout");
    let before = net.env().monotonic_ms();
    let mut buffer = [0u8; 8];
    let mut iov = [&mut buffer[..]];
    assert_eq!(
        net.recvmsg(fd, &mut [], &mut iov, &mut [], 0),
        Err(Errno::EAGAIN)
    );
    assert!(net.env().monotonic_ms().saturating_sub(before) >= 50);
}

#[test]
fn waitall_loops_over_short_reads() {
    let net = net_without_unix(Caps::default());
    net.backend().set_max_read(1);
    let (_listener, client, server) = connected_pair(&net, 9101);
    net.sendmsg(client, None, &[b"abcd"], &[], 0).expect("send");
    let mut buffer = [0u8; 4];
    let mut iov = [&mut buffer[..]];
    let received = net
        .recvmsg(server, &mut [], &mut iov, &mut [], MSG_WAITALL)
        .expect("recv");
    assert_eq!(received.len, 4);
    assert_eq!(&buffer[..4], b"abcd");
}

#[test]
fn peek_is_gated_by_the_capability_and_does_not_consume() {
    let caps = Caps {
        bare_open: true,
        peek_flag: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    net.bind(fd, &addr([0, 0, 0, 0], 9102)).expect("bind");
    net.backend().inject_datagram(
        9102,
        Endpoint {
            ip: [192, 0, 2, 7],
            port: 53,
        },
        b"peek",
    );
    let mut first = [0u8; 8];
    let mut iov = [&mut first[..]];
    let peeked = net
        .recvmsg(fd, &mut [], &mut iov, &mut [], MSG_PEEK)
        .expect("peek");
    assert_eq!(peeked.len, 4);
    assert_eq!(&first[..4], b"peek");
    let mut second = [0u8; 8];
    let mut iov = [&mut second[..]];
    let received = net
        .recvmsg(fd, &mut [], &mut iov, &mut [], 0)
        .expect("recv");
    assert_eq!(received.len, 4);
    assert_eq!(&second[..4], b"peek");
}

#[test]
fn truncation_flag_is_gated_by_the_capability() {
    let caps = Caps {
        bare_open: true,
        udp_trunc_flag: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    let receiver = net.socket(AF_INET, SOCK_DGRAM, 0).expect("receiver");
    net.bind(receiver, &addr([0, 0, 0, 0], 9103)).expect("bind");
    net.backend().inject_datagram(
        9103,
        Endpoint {
            ip: [192, 0, 2, 7],
            port: 53,
        },
        &[1u8; 32],
    );
    let mut small = [0u8; 8];
    let mut iov = [&mut small[..]];
    let received = net
        .recvmsg(receiver, &mut [], &mut iov, &mut [], 0)
        .expect("recv");
    assert_eq!(received.len, 8);
    assert_ne!(received.flags & MSG_TRUNC, 0);
}

#[test]
fn restart_surfaces_econnreset() {
    let net = net_without_unix(Caps::default());
    let (_listener, _client, server) = connected_pair(&net, 9104);
    net.backend().restart_netd();
    let mut buffer = [0u8; 8];
    let mut iov = [&mut buffer[..]];
    assert_eq!(
        net.recvmsg(server, &mut [], &mut iov, &mut [], 0),
        Err(Errno::ECONNRESET)
    );
}

#[test]
fn bare_bind_port_zero_gets_two_distinct_ports() {
    let caps = Caps {
        bare_open: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    let first = net.socket(AF_INET, SOCK_DGRAM, 0).expect("first");
    let second = net.socket(AF_INET, SOCK_DGRAM, 0).expect("second");
    net.bind(first, &addr([0, 0, 0, 0], 0)).expect("bind1");
    net.bind(second, &addr([0, 0, 0, 0], 0)).expect("bind2");
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    net.getsockname(first, &mut a).expect("name1");
    net.getsockname(second, &mut b).expect("name2");
    let port_a = crate::parse_sockaddr(&a).expect("parse1").port();
    let port_b = crate::parse_sockaddr(&b).expect("parse2").port();
    assert!((49_152..=65_535).contains(&port_a));
    assert!((49_152..=65_535).contains(&port_b));
    assert_ne!(port_a, port_b);
}

#[test]
fn bare_connect_uses_the_call_operation() {
    let caps = Caps {
        bare_open: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.connect(fd, &addr([10, 0, 0, 2], 80)).expect("connect");
    assert!(logs(&net).iter().any(|entry| matches!(
        entry,
        WireLog::Call(Call::Connect(Endpoint {
            ip: [10, 0, 0, 2],
            port: 80
        }))
    )));
}

#[test]
fn probe_route_failure_and_loopback() {
    use raven_libc_resolv::{IpAddr, Probe, SockAddr};
    let caps = Caps {
        bare_open: true,
        udp_route_fail: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    net.backend().fail_routes();
    let probe = net.probe();
    assert_eq!(
        probe.udp_route(&SockAddr::new(IpAddr::V4([10, 9, 9, 9]), 53)),
        None
    );
    assert_eq!(
        probe.udp_route(&SockAddr::new(IpAddr::V4([127, 0, 0, 1]), 53)),
        None
    );
}

#[test]
fn unix_named_bind_and_control_are_unsupported() {
    let net = net(Caps::default());
    let fd = net.socketpair(AF_UNIX, SOCK_STREAM, 0).expect("pair");
    let mut path = [0u8; 110];
    crate::write_sockaddr(
        &crate::SockAddr::Un {
            path: [0u8; 108],
            len: 0,
        },
        &mut path,
    );
    assert_eq!(net.bind(fd[0], &path), Err(Errno::EAFNOSUPPORT));
    let error = net
        .sendmsg(fd[0], None, &[b"x"], &[0u8; 24], 0)
        .expect_err("control");
    assert_eq!(error.errno, Errno::EOPNOTSUPP);
}

#[test]
fn default_no_unix_backend_rejects_pairs() {
    let net = net_without_unix(Caps::default());
    assert_eq!(
        net.socketpair(AF_UNIX, SOCK_STREAM, 0),
        Err(Errno::EAFNOSUPPORT)
    );
}

#[test]
fn dontwait_never_blocks() {
    let net = net_without_unix(Caps::default());
    let (_listener, _client, server) = connected_pair(&net, 9105);
    let before = net.env().monotonic_ms();
    let mut buffer = [0u8; 4];
    let mut iov = [&mut buffer[..]];
    assert_eq!(
        net.recvmsg(server, &mut [], &mut iov, &mut [], MSG_DONTWAIT),
        Err(Errno::EAGAIN)
    );
    assert_eq!(net.env().monotonic_ms(), before);
}

#[test]
fn getsockname_truncation_and_unconnected_peer() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.bind(fd, &addr([10, 0, 0, 1], 1234)).expect("bind");
    let mut small = [0u8; 4];
    assert_eq!(net.getsockname(fd, &mut small), Ok(16));
    assert_eq!(small, [2, 0, 4, 210]);
    assert_eq!(net.getpeername(fd, &mut small), Err(Errno::ENOTCONN));
}

#[test]
fn option_errors_are_explicit() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    assert_eq!(
        net.setsockopt(fd, SOL_SOCKET, 999, &0i32.to_le_bytes()),
        Err(Errno::ENOPROTOOPT)
    );
    assert_eq!(
        net.setsockopt(fd, 999, SO_RCVBUF, &0i32.to_le_bytes()),
        Err(Errno::ENOPROTOOPT)
    );
    assert_eq!(
        net.setsockopt(fd, SOL_SOCKET, SO_RCVBUF, &[0u8; 3]),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        net.setsockopt(fd, 6, 30, &0i32.to_le_bytes()),
        Err(Errno::ENOPROTOOPT)
    );
    assert_eq!(
        net.setsockopt(fd, 41, 26, &0i32.to_le_bytes()),
        Err(Errno::ENOPROTOOPT)
    );
    assert_eq!(
        net.setsockopt(fd, SOL_SOCKET, 17, &[0u8; 12]),
        Err(Errno::ENOPROTOOPT)
    );
}

#[test]
fn ipv6_v6only_is_stored_with_the_capability() {
    let caps = Caps {
        ipv6: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    let fd = net.socket(AF_INET6, SOCK_STREAM, 0).expect("socket");
    net.setsockopt(fd, 41, 26, &1i32.to_le_bytes())
        .expect("v6only");
    let mut value = [0u8; 4];
    assert_eq!(net.getsockopt(fd, 41, 26, &mut value), Ok(4));
    assert_eq!(i32::from_le_bytes(value), 1);
}

#[test]
fn accept_flags_are_applied_to_the_new_fd() {
    let net = net_without_unix(Caps::default());
    let listener = net.socket(AF_INET, SOCK_STREAM, 0).expect("listener");
    net.bind(listener, &addr([0, 0, 0, 0], 9106)).expect("bind");
    net.listen(listener, 1).expect("listen");
    let client = net.socket(AF_INET, SOCK_STREAM, 0).expect("client");
    net.connect(client, &addr([127, 0, 0, 1], 9106))
        .expect("connect");
    let mut out = [0u8; 16];
    let (accepted, _) = net
        .accept(listener, &mut out, SOCK_NONBLOCK)
        .expect("accept");
    let info = net.fds().resolve(accepted).expect("resolve");
    assert!(info.nonblock);
}

#[test]
fn truncation_flag_absent_without_the_capability() {
    let net = net_without_unix(Caps {
        bare_open: true,
        ..Caps::default()
    });
    let fd = net.socket(AF_INET, SOCK_DGRAM, 0).expect("socket");
    net.bind(fd, &addr([0, 0, 0, 0], 9107)).expect("bind");
    net.backend().inject_datagram(
        9107,
        Endpoint {
            ip: [192, 0, 2, 7],
            port: 53,
        },
        &[1u8; 32],
    );
    let mut small = [0u8; 8];
    let mut iov = [&mut small[..]];
    let received = net
        .recvmsg(fd, &mut [], &mut iov, &mut [], 0)
        .expect("recv");
    assert_eq!(received.flags & MSG_TRUNC, 0);
}

#[test]
fn all_wire_options_reach_the_backend_in_call_order() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.setsockopt(fd, SOL_SOCKET, 2, &1i32.to_le_bytes())
        .expect("reuse");
    net.setsockopt(fd, SOL_SOCKET, SO_RCVBUF, &4096i32.to_le_bytes())
        .expect("rcv");
    net.setsockopt(fd, SOL_SOCKET, 7, &8192i32.to_le_bytes())
        .expect("snd");
    net.setsockopt(fd, 6, 1, &1i32.to_le_bytes())
        .expect("nodelay");
    net.connect(fd, &addr([10, 0, 0, 2], 80)).expect("connect");
    let calls: Vec<_> = logs(&net)
        .into_iter()
        .filter_map(|entry| match entry {
            WireLog::Call(call) => Some(call),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        vec![
            Call::SetSockOpt {
                opt: SockOpt::ReuseAddr,
                value: 1
            },
            Call::SetSockOpt {
                opt: SockOpt::RcvBuf,
                value: 4096
            },
            Call::SetSockOpt {
                opt: SockOpt::SndBuf,
                value: 8192
            },
            Call::SetSockOpt {
                opt: SockOpt::TcpNoDelay,
                value: 1
            }
        ]
    );
}

#[test]
fn ipv6_addresses_are_rejected_even_with_the_capability() {
    let caps = Caps {
        ipv6: true,
        ..Caps::default()
    };
    let net = net_without_unix(caps);
    let fd = net.socket(AF_INET6, SOCK_STREAM, 0).expect("socket");
    let mut six = [0u8; 28];
    six[0] = 10;
    six[3] = 80;
    crate::write_sockaddr(
        &crate::SockAddr::In6 {
            ip: [0; 16],
            port: 80,
            scope: 0,
            flowinfo: 0,
        },
        &mut six,
    );
    assert_eq!(net.connect(fd, &six), Err(Errno::EAFNOSUPPORT));
    assert_eq!(net.bind(fd, &six), Err(Errno::EAFNOSUPPORT));
}

#[test]
fn stored_local_port_orders_open_bind_then_options() {
    let net = net_without_unix(Caps::default());
    let fd = net.socket(AF_INET, SOCK_STREAM, 0).expect("socket");
    net.bind(fd, &addr([10, 0, 0, 5], 1234)).expect("bind");
    net.setsockopt(fd, SOL_SOCKET, SO_RCVBUF, &4096i32.to_le_bytes())
        .expect("opt");
    net.connect(fd, &addr([10, 0, 0, 2], 80)).expect("connect");
    let entries = logs(&net);
    assert!(matches!(entries.first(), Some(WireLog::Open(_))));
    assert!(matches!(
        entries.get(1),
        Some(WireLog::Call(Call::Bind(Endpoint { port: 1234, .. })))
    ));
    assert!(matches!(
        entries.get(2),
        Some(WireLog::Call(Call::SetSockOpt {
            opt: SockOpt::RcvBuf,
            ..
        }))
    ));
}
