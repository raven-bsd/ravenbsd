// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Script execution against `raven-netcore` over a `PairDevice`.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use raven_netcore::testutil::{PairConfig, PairDevice};
use raven_netcore::{Device, Instant as NetInstant, Ipv4Addr, NetCore, SocketId};

use crate::frame::{
    ACK, FIN, PEER_IP, PEER_ISN, PEER_MAC, PEER_PORT, PSH, RST, SUT_IP, SUT_MAC, SUT_PORT, SYN,
    TcpFrame, arp_request_target, build_arp_reply, build_tcp, parse_tcp, payload,
};
use crate::script::{Direction, EventKind, PacketSpec, Script, Syscall};

pub struct Runner {
    script: Script,
    net: NetCore<PairDevice>,
    peer: PairDevice,
    now_ms: i64,
    fds: BTreeMap<u32, SocketId>,
    outgoing: VecDeque<TcpFrame>,
    sut_isn: Option<u32>,
}

impl Runner {
    pub fn new(script: Script) -> Self {
        let (sut_device, peer) = PairDevice::pair(PairConfig::reliable());
        let mut net = NetCore::new(sut_device, SUT_MAC, 0);
        net.set_ipv4(
            Ipv4Addr::new(SUT_IP[0], SUT_IP[1], SUT_IP[2], SUT_IP[3]),
            24,
            None,
        );
        Self {
            script,
            net,
            peer,
            now_ms: 0,
            fds: BTreeMap::new(),
            outgoing: VecDeque::new(),
            sut_isn: None,
        }
    }

    /// Feed one raw Ethernet frame from the peer into the SUT and service the stack.
    ///
    /// Used by the fuzz-lite test; normal scripts go through the parsed event loop.
    pub fn inject_raw(&mut self, frame: &[u8]) -> Result<()> {
        self.transmit(frame.to_vec())?;
        self.service()
    }

    /// Current virtual time of the harness, in milliseconds.
    pub const fn now_ms(&self) -> i64 {
        self.now_ms
    }

    pub fn run(mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        for event in self.script.events.clone() {
            if Instant::now() >= deadline {
                bail!("script timed out after {:?}", timeout);
            }
            self.now_ms = self.now_ms.saturating_add(event.delay_ms);
            match event.kind {
                EventKind::Packet {
                    direction: Direction::Incoming,
                    spec,
                } => {
                    self.inject(spec)?;
                    self.service()?;
                }
                EventKind::Packet {
                    direction: Direction::Outgoing,
                    spec,
                } => self.expect(spec)?,
                EventKind::Syscall(call) => {
                    self.syscall(call)?;
                    self.service()?;
                }
            }
        }
        Ok(())
    }

    fn syscall(&mut self, call: Syscall) -> Result<()> {
        match call {
            Syscall::Socket { fd, expected } => {
                self.check(expected, fd as isize, "socket")?;
            }
            Syscall::Bind { expected } => self.check(expected, 0, "bind")?,
            Syscall::Listen {
                fd,
                backlog,
                expected,
            } => {
                let socket = self.net.tcp_listen(SUT_PORT, backlog).map_err(net_errno)?;
                self.fds.insert(fd, socket);
                self.check(expected, 0, "listen")?;
            }
            Syscall::Accept {
                fd,
                new_fd,
                expected,
            } => {
                let listener = *self
                    .fds
                    .get(&fd)
                    .with_context(|| format!("accept: unknown fd {fd}"))?;
                let mut accepted = None;
                for _ in 0..500 {
                    if let Some(socket) = self.net.tcp_accept(listener).map_err(net_errno)? {
                        accepted = Some(socket);
                        break;
                    }
                    self.now_ms += 1;
                    self.service()?;
                }
                let socket = accepted.context("accept did not complete")?;
                self.fds.insert(new_fd, socket);
                self.check(expected, new_fd as isize, "accept")?;
            }
            Syscall::Connect { fd, expected } => {
                let socket = self
                    .net
                    .tcp_connect(
                        (
                            Ipv4Addr::new(PEER_IP[0], PEER_IP[1], PEER_IP[2], PEER_IP[3]),
                            PEER_PORT,
                        ),
                        None,
                    )
                    .map_err(net_errno)?;
                self.fds.insert(fd, socket);
                self.check(expected, 0, "connect")?;
            }
            Syscall::Read {
                fd,
                len,
                expected,
                error,
            } => {
                let socket = *self
                    .fds
                    .get(&fd)
                    .with_context(|| format!("read: unknown fd {fd}"))?;
                let mut buffer = vec![0u8; len];
                let mut result = None;
                for _ in 0..500 {
                    match self.net.recv(socket, &mut buffer) {
                        Ok(value) => {
                            result = Some(Ok(value as isize));
                            break;
                        }
                        Err(raven_netcore::Errno::EAGAIN) if error.as_deref() == Some("EAGAIN") => {
                            result = Some(Err(raven_netcore::Errno::EAGAIN));
                            break;
                        }
                        Err(raven_netcore::Errno::EAGAIN) => {
                            self.now_ms += 1;
                            self.service()?;
                        }
                        Err(error) => {
                            result = Some(Err(error));
                            break;
                        }
                    }
                }
                let result = result.context("read did not complete")?;
                self.check_call(expected, error.as_deref(), result, "read")?;
            }
            Syscall::Write {
                fd,
                len,
                expected,
                error,
            } => {
                let socket = *self
                    .fds
                    .get(&fd)
                    .with_context(|| format!("write: unknown fd {fd}"))?;
                let data = payload(len);
                let result = self.net.send(socket, &data).map(|value| value as isize);
                self.check_call(expected, error.as_deref(), result, "write")?;
            }
            Syscall::Close { fd, expected } => {
                let socket = *self
                    .fds
                    .get(&fd)
                    .with_context(|| format!("close: unknown fd {fd}"))?;
                self.net.close(socket).map_err(net_errno)?;
                self.check(expected, 0, "close")?;
            }
            Syscall::Ignore { expected } => self.check(expected, 0, "ignored syscall")?,
        }
        Ok(())
    }

    fn check(&self, expected: isize, actual: isize, name: &str) -> Result<()> {
        if expected != actual {
            bail!("{name}: expected {expected}, got {actual}");
        }
        Ok(())
    }

    fn check_call(
        &self,
        expected: isize,
        error: Option<&str>,
        result: std::result::Result<isize, raven_netcore::Errno>,
        name: &str,
    ) -> Result<()> {
        match result {
            Ok(value) => self.check_result(expected, error, value, name),
            Err(errno) => self.check_result(expected, error, -(errno.0 as isize), name),
        }
    }

    fn check_result(
        &self,
        expected: isize,
        error: Option<&str>,
        actual: isize,
        name: &str,
    ) -> Result<()> {
        if let Some(error) = error {
            if actual >= 0 {
                bail!("{name}: expected -{error}, got {actual}");
            }
            let expected_errno =
                errno_number(error).with_context(|| format!("unknown errno {error}"))?;
            if -actual != expected_errno {
                bail!("{name}: expected -{error} ({expected_errno}), got {actual}");
            }
            return Ok(());
        }
        self.check(expected, actual, name)
    }

    fn inject(&mut self, spec: PacketSpec) -> Result<()> {
        let seq = PEER_ISN.wrapping_add(spec.seq_start);
        let ack = self
            .sut_isn
            .unwrap_or(0)
            .wrapping_add(spec.ack.unwrap_or(0));
        let body = payload(spec.seq_len);
        let frame = build_tcp(
            PEER_MAC,
            SUT_MAC,
            PEER_IP,
            SUT_IP,
            PEER_PORT,
            SUT_PORT,
            seq,
            ack,
            spec.flags,
            spec.window,
            &body,
        );
        self.transmit(frame)
    }

    fn expect(&mut self, spec: PacketSpec) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            self.service()?;
            let mut found = None;
            for (index, frame) in self.outgoing.iter().enumerate() {
                let isn = self.sut_isn.unwrap_or(frame.seq);
                if self.matches(frame, spec, isn) {
                    found = Some((index, self.sut_isn.is_none().then_some(frame.seq)));
                    break;
                }
            }
            if let Some((index, learned)) = found {
                if let Some(isn) = learned {
                    self.sut_isn = Some(isn);
                }
                self.outgoing.remove(index);
                return Ok(());
            }
            if Instant::now() >= deadline {
                let seen: Vec<String> = self
                    .outgoing
                    .iter()
                    .map(|frame| {
                        format!(
                            "{} {}:{} ack {} len {}",
                            frame.flags_text(),
                            frame.seq,
                            frame.seq,
                            frame.ack,
                            frame.payload_len
                        )
                    })
                    .collect();
                bail!("expected outgoing {spec:?}, saw {seen:?}");
            }
            self.now_ms += 1;
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn matches(&self, frame: &TcpFrame, spec: PacketSpec, sut_isn: u32) -> bool {
        if frame.src_port != SUT_PORT || frame.dst_port != PEER_PORT {
            return false;
        }
        let relative_seq = frame.seq.wrapping_sub(sut_isn);
        let relative_ack = frame.ack.wrapping_sub(PEER_ISN);
        let mask = ACK | SYN | PSH | FIN | RST;
        frame.flags & mask == spec.flags
            && relative_seq == spec.seq_start
            && frame.payload_len == spec.seq_len
            && spec.ack.is_none_or(|ack| relative_ack == ack)
    }

    fn service(&mut self) -> Result<()> {
        for _ in 0..8 {
            self.net.poll(self.now_ms);
            let mut arp_request = false;
            while let Some(frame) = self.receive()? {
                if let Some(target) = arp_request_target(&frame) {
                    arp_request |= target == PEER_IP;
                } else if let Some(tcp) = parse_tcp(&frame) {
                    self.outgoing.push_back(tcp);
                }
            }
            if arp_request {
                let reply = build_arp_reply(SUT_MAC, PEER_MAC, PEER_IP, SUT_IP, SUT_MAC);
                self.transmit(reply)?;
                continue;
            }
            break;
        }
        Ok(())
    }

    fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        let mut frame = Vec::new();
        let got = self
            .peer
            .receive(NetInstant::from_millis(self.now_ms), &mut |bytes| {
                frame.extend_from_slice(bytes);
            });
        Ok(got.then_some(frame))
    }

    fn transmit(&mut self, frame: Vec<u8>) -> Result<()> {
        self.peer
            .transmit(
                NetInstant::from_millis(self.now_ms),
                frame.len(),
                &mut |buffer| {
                    buffer.copy_from_slice(&frame);
                },
            )
            .map_err(|error| anyhow::anyhow!("peer device cannot transmit: {error:?}"))
    }
}

fn net_errno(errno: raven_netcore::Errno) -> anyhow::Error {
    anyhow::anyhow!("netcore error {} ({})", errno.0, errno.description())
}

fn errno_number(name: &str) -> Option<isize> {
    match name {
        "EAGAIN" => Some(11),
        "EINVAL" => Some(22),
        "ENOTCONN" => Some(107),
        _ => None,
    }
}
