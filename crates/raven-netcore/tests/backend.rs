// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A trivial stub backend proves the seam is genuinely abstract.

use raven_abi::errno::Errno;
use raven_netcore::{Core, Event, Ipv4Addr, SocketId, StackBackend};

#[derive(Default)]
struct StubBackend {
    next_id: u32,
    bytes: usize,
    polled: i64,
}

impl StackBackend for StubBackend {
    type Device = ();

    fn set_ipv4(&mut self, _addr: Ipv4Addr, _prefix: u8, _gateway: Option<Ipv4Addr>) {}
    fn enable_dhcp(&mut self) {}
    fn tcp_listen(&mut self, _port: u16, _backlog: usize) -> Result<SocketId, Errno> {
        self.next_id += 1;
        Ok(SocketId(self.next_id))
    }
    fn tcp_accept(&mut self, _listener: SocketId) -> Result<Option<SocketId>, Errno> {
        Ok(None)
    }
    fn tcp_connect(
        &mut self,
        _remote: (Ipv4Addr, u16),
        _local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        self.next_id += 1;
        Ok(SocketId(self.next_id))
    }
    fn send(&mut self, _s: SocketId, data: &[u8]) -> Result<usize, Errno> {
        self.bytes += data.len();
        Ok(data.len())
    }
    fn recv(&mut self, _s: SocketId, _buf: &mut [u8]) -> Result<usize, Errno> {
        Err(Errno::EAGAIN)
    }
    fn shutdown_write(&mut self, _s: SocketId) -> Result<(), Errno> {
        Ok(())
    }
    fn close(&mut self, _s: SocketId) -> Result<(), Errno> {
        Ok(())
    }
    fn udp_bind(&mut self, _port: u16) -> Result<SocketId, Errno> {
        self.next_id += 1;
        Ok(SocketId(self.next_id))
    }
    fn udp_send_to(
        &mut self,
        _s: SocketId,
        data: &[u8],
        _to: (Ipv4Addr, u16),
    ) -> Result<usize, Errno> {
        self.bytes += data.len();
        Ok(data.len())
    }
    fn udp_recv_from(
        &mut self,
        _s: SocketId,
        _buf: &mut [u8],
    ) -> Result<(usize, (Ipv4Addr, u16)), Errno> {
        Err(Errno::EAGAIN)
    }
    fn poll(&mut self, now_ms: i64) -> Vec<Event> {
        self.polled = now_ms;
        Vec::new()
    }
    fn next_poll_delay_ms(&mut self, _now_ms: i64) -> Option<u64> {
        Some(7)
    }
}

#[test]
fn a_stub_backend_drives_the_core() {
    let mut core = Core::from_backend(StubBackend::default());
    let listener = core.tcp_listen(80, 2).expect("listen");
    assert_eq!(listener, SocketId(1));
    let client = core
        .tcp_connect((Ipv4Addr::new(192, 0, 2, 2), 80), None)
        .expect("connect");
    assert_eq!(client, SocketId(2));
    assert_eq!(core.send(client, b"hello"), Ok(5));
    assert_eq!(core.backend().bytes, 5);
    assert_eq!(core.poll(123), Vec::new());
    assert_eq!(core.backend().polled, 123);
    assert_eq!(core.next_poll_delay_ms(123), Some(7));
}
