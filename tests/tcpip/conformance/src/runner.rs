// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Executes parsed scripts against `raven-tcpip` through the C04 simulator.

use raven_abi::errno::Errno;
use raven_tcpip::{Duration, Instant, IpAddr, Ipv4Addr, Ipv4Config, SocketId, Stack, StackConfig};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};

use crate::packet::{self, PEER_ADDR, PEER_ISN, PEER_PORT, PORT, Parsed, SUT_ADDR, SUT_MAC};
use crate::script::{self, OptionSpec, PacketSpec, Step, Syscall};

/// Outcome of one script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    /// Script name.
    pub name: String,
    /// Whether the script passed.
    pub passed: bool,
    /// Failure reason for a failing script.
    pub reason: String,
}

/// One script execution environment.
pub struct Runner {
    stack: Stack<Endpoint>,
    link: Link,
    now: i64,
    trace_cursor: usize,
    sut_isn: Option<u32>,
    listener: Option<SocketId>,
    accepted: Option<SocketId>,
    connected: Option<SocketId>,
    peer_port: u16,
    last_read: Option<Result<usize, Errno>>,
}

impl Runner {
    /// Create a runner with a clean stack and a zero-latency link.
    #[must_use]
    pub fn new() -> Self {
        let (sut, _peer) = Link::new(
            LinkConfig {
                rtt_ms: 0,
                ..LinkConfig::default()
            },
            7,
        );
        let link = sut.link();
        let mut stack = Stack::new(
            sut,
            StackConfig {
                hardware_addr: Some(SUT_MAC),
                ..StackConfig::default()
            },
            11,
            Instant::ZERO,
        );
        stack
            .set_ipv4(Ipv4Config {
                addr: Ipv4Addr::new(SUT_ADDR),
                prefix_len: 24,
                gateway: None,
            })
            .expect("ipv4");
        stack.test_insert_neighbor(
            Ipv4Addr::new(PEER_ADDR),
            packet::PEER_MAC,
            Instant::ZERO,
            600_000,
        );
        Self {
            stack,
            link,
            now: 0,
            trace_cursor: 0,
            sut_isn: None,
            listener: None,
            accepted: None,
            connected: None,
            peer_port: PEER_PORT,
            last_read: None,
        }
    }

    /// Run one parsed script.
    pub fn run(&mut self, steps: &[Step]) -> Result<(), String> {
        for (index, step) in steps.iter().enumerate() {
            self.step(step)
                .map_err(|error| format!("step {}: {error}", index + 1))?;
        }
        self.check_tail()
    }

    fn step(&mut self, step: &Step) -> Result<(), String> {
        match step {
            Step::Time(ms) => {
                if *ms < self.now {
                    return Err(format!("backwards time {} < {}", ms, self.now));
                }
                self.now = *ms;
                self.poll();
                Ok(())
            }
            Step::Wait(ms) => {
                self.now = self.now.saturating_add(*ms);
                self.poll();
                Ok(())
            }
            Step::Inbound(spec) => {
                let (src, dst) = if self.connected.is_some() {
                    (PORT, PEER_PORT)
                } else {
                    (self.peer_port, PORT)
                };
                let frame = packet::build_inbound(spec, self.sut_isn.unwrap_or(0), src, dst);
                self.stack
                    .test_inject_frame(&frame, Instant::from_millis(self.now));
                self.poll();
                Ok(())
            }
            Step::Expect(spec) => self.expect(spec),
            Step::Syscall(call) => self.syscall(call),
        }
    }

    fn poll(&mut self) {
        self.stack.poll(Instant::from_millis(self.now));
    }

    fn expect(&mut self, expected: &PacketSpec) -> Result<(), String> {
        let entries = self.link.trace();
        let mut found: Option<(usize, Parsed)> = None;
        for (index, entry) in entries.entries().iter().enumerate().skip(self.trace_cursor) {
            if entry.direction != raven_tcpip_sim::Direction::AtoB {
                continue;
            }
            let Some(parsed) = packet::parse_trace_frame(&entry.bytes) else {
                continue;
            };
            found = Some((index, parsed));
            break;
        }
        let Some((index, actual)) = found else {
            return Err(format!("missing outbound packet {expected:?}"));
        };
        self.trace_cursor = index + 1;
        if actual.syn() && self.sut_isn.is_none() {
            self.sut_isn = Some(actual.seq);
        }
        self.compare(expected, &actual)
    }

    fn compare(&self, expected: &PacketSpec, actual: &Parsed) -> Result<(), String> {
        if actual.flags != expected.flags {
            return Err(format!(
                "flags expected {:#04x} got {:#04x} ({actual:?})",
                expected.flags, actual.flags
            ));
        }
        let seq = actual.seq.wrapping_sub(self.sut_isn.unwrap_or(0));
        if !expected.seq_any && seq != expected.seq {
            return Err(format!("seq expected {} got {}", expected.seq, seq));
        }
        if let Some(ack) = expected.ack {
            let actual_ack = actual.ack.wrapping_sub(PEER_ISN);
            if actual_ack != ack {
                return Err(format!("ack expected {ack} got {actual_ack}"));
            }
        }
        if let Some(win) = expected.win
            && actual.win != win
        {
            return Err(format!("win expected {win} got {}", actual.win));
        }
        if actual.len != expected.len {
            return Err(format!("len expected {} got {}", expected.len, actual.len));
        }
        if !packet::options_match(&actual.options, &expected.options) {
            return Err(format!(
                "options expected {:?} got {:?}",
                expected.options, actual.options
            ));
        }
        Ok(())
    }

    fn check_tail(&self) -> Result<(), String> {
        let entries = self.link.trace();
        for entry in entries.entries().iter().skip(self.trace_cursor) {
            if entry.direction != raven_tcpip_sim::Direction::AtoB {
                continue;
            }
            if packet::parse_trace_frame(&entry.bytes).is_some() {
                return Err("unexpected outbound packet after the last expected line".to_string());
            }
        }
        Ok(())
    }

    fn socket(&self) -> Result<SocketId, String> {
        self.connected
            .or(self.accepted)
            .ok_or_else(|| "no connected or accepted socket".to_string())
    }

    fn syscall(&mut self, call: &Syscall) -> Result<(), String> {
        match call {
            Syscall::Socket | Syscall::Bind { .. } => Ok(()),
            Syscall::Listen { backlog } => {
                if self.listener.is_none() {
                    self.listener = Some(
                        self.stack
                            .tcp_listen(PORT, *backlog)
                            .map_err(|error| format!("listen: {error:?}"))?,
                    );
                }
                Ok(())
            }
            Syscall::Accept => {
                let listener = self
                    .listener
                    .ok_or_else(|| "accept without listen".to_string())?;
                let accepted = self
                    .stack
                    .tcp_accept(listener)
                    .map_err(|error| format!("accept: {error:?}"))?;
                self.accepted =
                    Some(accepted.ok_or_else(|| "accept returned no connection".to_string())?);
                Ok(())
            }
            Syscall::Connect { port } => {
                self.connected = Some(
                    self.stack
                        .tcp_connect(
                            (IpAddr::V4(Ipv4Addr::new(PEER_ADDR)), *port),
                            Some(PEER_PORT),
                        )
                        .map_err(|error| format!("connect: {error:?}"))?,
                );
                Ok(())
            }
            Syscall::Write { len } => {
                let socket = self.socket()?;
                let payload = vec![b'x'; *len];
                let written = self
                    .stack
                    .send(socket, &payload)
                    .map_err(|error| format!("write: {error:?}"))?;
                if written != *len {
                    return Err(format!("write accepted {written} of {len}"));
                }
                Ok(())
            }
            Syscall::Read { len, expected } => {
                let socket = self.socket()?;
                let mut buffer = vec![0u8; *len];
                let result = self.stack.recv(socket, &mut buffer);
                self.last_read = Some(result);
                if let Some(expected) = expected {
                    let actual = match result {
                        Ok(count) => count as i64,
                        Err(_) => -1,
                    };
                    if actual != *expected {
                        return Err(format!("read expected {expected} got {actual}"));
                    }
                }
                Ok(())
            }
            Syscall::Close => {
                let socket = self.socket()?;
                self.stack
                    .shutdown_write(socket)
                    .map_err(|error| format!("close: {error:?}"))?;
                Ok(())
            }
            Syscall::SetSockOpt { option } => {
                let socket = self.socket()?;
                if *option == Some(1) {
                    self.stack
                        .set_nodelay(socket, true)
                        .map_err(|error| format!("setsockopt: {error:?}"))?;
                }
                Ok(())
            }
            Syscall::PeerPort { port } => {
                self.peer_port = *port;
                Ok(())
            }
            Syscall::Set { name, value } => self
                .stack
                .set_property(name, *value)
                .map_err(|error| format!("sysctl {name}: {error:?}")),
            Syscall::KeepAlive { interval } => {
                let socket = self.socket()?;
                self.stack
                    .tcp_set_keepalive(socket, Duration::from_millis(*interval), 0)
                    .map_err(|error| format!("keepalive: {error:?}"))
            }
            Syscall::Other { .. } => Ok(()),
        }
    }
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

/// Run one script text.
pub fn run_text(text: &str) -> Result<(), String> {
    let steps = script::parse(text).map_err(|error| error.to_string())?;
    let mut runner = Runner::new();
    runner.run(&steps)
}

/// Check that every option in a list is implemented by the runner.
pub fn supported_options(options: &[OptionSpec]) -> bool {
    options.iter().all(|option| {
        matches!(
            option,
            OptionSpec::Mss(_)
                | OptionSpec::Wscale(_)
                | OptionSpec::SackOk
                | OptionSpec::TimestampsAny
                | OptionSpec::Timestamps { .. }
                | OptionSpec::Nop
                | OptionSpec::Eol
                | OptionSpec::Unknown(_)
        )
    })
}
