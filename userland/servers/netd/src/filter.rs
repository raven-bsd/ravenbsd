// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Device-boundary packet-filter hook.
//!
//! The `raven-tcpip` stack talks to the network only through [`Device`]:
//! [`Device::receive`] hands it ingress frames and [`Device::transmit`] lets it fill egress
//! frames. Wrapping the device puts the filter before IP processing in both directions and
//! keeps `netd`, `raven-netcore` and `raven-tcpip` free of filter types.
//!
//! Donor: `pf_test()` / `pf_test_rule()` in FreeBSD `sys/netpfil/pf/pf.c` (see `docs/pf.md`
//! section 3). The wrapper is the Raven `pfil` seam: one [`Engine`] per interface, the
//! `Block` verdict drops the frame, and a `Decision::translation` rewrites the IP addresses /
//! L4 ports in place (recomputing the IPv4 and TCP/UDP checksums).

use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;

use raven_netcore::{Device, DeviceCaps, Instant, Medium, TxError};
use raven_pf::{
    Action, Direction, Engine, IpAddr, LogEntry, Packet, Protocol, Ruleset, Translation, Verdict,
    parse,
};

/// The pf configuration path, read like the network drop-in.
pub const PF_CONF: &str = "/etc/pf.conf";

/// Log lines produced by the filter, shared by every interface of one `netd`.
pub type FilterLog = Rc<RefCell<Vec<String>>>;

/// Create an empty shared log.
#[must_use]
pub fn new_log() -> FilterLog {
    Rc::new(RefCell::new(Vec::new()))
}

/// One immutable filter policy, cloned per interface (the log handle is shared).
#[derive(Clone, Debug)]
pub struct FilterPolicy {
    ruleset: Ruleset,
    log: FilterLog,
}

impl FilterPolicy {
    /// Wrap a parsed ruleset.
    #[must_use]
    pub fn new(ruleset: Ruleset) -> Self {
        Self {
            ruleset,
            log: new_log(),
        }
    }

    /// Parse a pf.conf-compatible text. A parse error is returned unchanged: the caller must
    /// refuse to start (fail-closed).
    ///
    /// # Errors
    ///
    /// Returns the parser error text.
    pub fn from_text(text: &str) -> Result<Self, String> {
        parse(text)
            .map(Self::new)
            .map_err(|error| format!("{error}"))
    }

    /// Attach an existing log handle (the boot path shares one log across interfaces).
    #[must_use]
    pub fn with_log(mut self, log: FilterLog) -> Self {
        self.log = log;
        self
    }

    /// The shared log handle.
    #[must_use]
    pub fn log(&self) -> FilterLog {
        Rc::clone(&self.log)
    }

    /// Number of filter rules.
    #[must_use]
    pub fn rules(&self) -> usize {
        self.ruleset.rules.len()
    }

    /// Number of NAT/RDR rules.
    #[must_use]
    pub fn nat_rules(&self) -> usize {
        self.ruleset.nat.len()
    }

    /// Take the pending log lines.
    pub fn drain_log(&self) -> Vec<String> {
        core::mem::take(&mut *self.log.borrow_mut())
    }

    fn into_parts(self) -> (Ruleset, FilterLog) {
        (self.ruleset, self.log)
    }
}

/// Why the boot-time configuration load did not produce a policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The file service or initfs is not registered yet; retry the boot.
    NotReady,
    /// The file was readable but invalid.
    Invalid(String),
}

impl LoadError {
    /// Human-readable detail.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotReady => String::from("file service is not ready"),
            Self::Invalid(message) => message.clone(),
        }
    }
}

/// Read `/etc/pf.conf` from the live file service or the initfs fallback.
///
/// `Ok(None)` means the file is genuinely absent (the filter stays off for compatibility with
/// images that do not ship a ruleset). A malformed file returns [`LoadError::Invalid`], and the
/// caller must exit without starting the stack (fail-closed).
///
/// # Errors
///
/// Returns [`LoadError`] for an I/O failure, a non-UTF-8 file, a parse error or a not-yet-ready
/// file service.
#[cfg(target_os = "none")]
pub fn load() -> Result<Option<Ruleset>, LoadError> {
    let paths = [format!("file:{PF_CONF}"), format!("initfs:{PF_CONF}")];
    let mut io_error = None;
    for path in &paths {
        match raven_rt::fs::read_to_vec(path) {
            Ok(data) => {
                let text = core::str::from_utf8(&data)
                    .map_err(|_| LoadError::Invalid(format!("{path}: file is not UTF-8")))?;
                return parse(text)
                    .map(Some)
                    .map_err(|error| LoadError::Invalid(format!("{path}: {error}")));
            }
            Err(raven_abi::errno::Errno::ENOENT) => continue,
            Err(error) => {
                io_error = Some(format!("{path}: {}", error.description()));
            }
        }
    }
    if let Some(message) = io_error {
        return Err(LoadError::Invalid(message));
    }
    if raven_rt::fs::stat("/").is_ok() || raven_rt::fs::stat("initfs:/").is_ok() {
        Ok(None)
    } else {
        Err(LoadError::NotReady)
    }
}

/// The parsed view of one IP frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FrameView {
    ip_offset: usize,
    ip_len: usize,
    l4_offset: usize,
    l4_len: usize,
    ipv6: bool,
    protocol: Protocol,
    fragment: bool,
    source: IpAddr,
    destination: IpAddr,
    source_port: u16,
    destination_port: u16,
    tcp_flags: u8,
}

impl FrameView {
    fn packet(self, direction: Direction, interface: &'static str) -> Packet<'static> {
        Packet {
            direction,
            interface,
            protocol: self.protocol,
            source: self.source,
            source_port: self.source_port,
            destination: self.destination,
            destination_port: self.destination_port,
            length: u32::try_from(self.ip_len).unwrap_or(u32::MAX),
            tcp_flags: self.tcp_flags,
            fragment: self.fragment,
        }
    }
}

/// Parse the IP view of `frame`; `None` for a non-IP frame (ARP and friends pass unfiltered).
pub(crate) fn parse_frame(caps: DeviceCaps, frame: &[u8]) -> Option<FrameView> {
    let (ip_offset, ethertype) = match caps.medium {
        Medium::Ip => {
            if frame.is_empty() {
                return None;
            }
            let ethertype = match frame[0] >> 4 {
                4 => 0x0800,
                6 => 0x86dd,
                _ => return None,
            };
            (0, ethertype)
        }
        Medium::Ethernet => {
            if frame.len() < 14 {
                return None;
            }
            let mut ethertype = u16::from_be_bytes([frame[12], frame[13]]);
            let mut offset = 14;
            if ethertype == 0x8100 {
                if frame.len() < 18 {
                    return None;
                }
                ethertype = u16::from_be_bytes([frame[16], frame[17]]);
                offset = 18;
            }
            (offset, ethertype)
        }
    };
    match ethertype {
        0x0800 => parse_ipv4(ip_offset, frame),
        0x86dd => parse_ipv6(ip_offset, frame),
        _ => None,
    }
}

fn parse_ipv4(ip: usize, frame: &[u8]) -> Option<FrameView> {
    let header = frame.get(ip..ip + 20)?;
    if header[0] >> 4 != 4 {
        return None;
    }
    let ihl = usize::from(header[0] & 0x0f) * 4;
    if ihl < 20 || frame.len() < ip + ihl {
        return None;
    }
    let total = usize::from(u16::from_be_bytes([header[2], header[3]]));
    if total < ihl || frame.len() < ip + total {
        return None;
    }
    let protocol = match header[9] {
        1 => Protocol::Icmp,
        6 => Protocol::Tcp,
        17 => Protocol::Udp,
        _ => Protocol::Any,
    };
    let flags_frag = u16::from_be_bytes([header[6], header[7]]);
    let fragment = flags_frag & 0x3fff != 0;
    let source = IpAddr::V4(raven_pf::Ipv4Addr::new(
        header[12], header[13], header[14], header[15],
    ));
    let destination = IpAddr::V4(raven_pf::Ipv4Addr::new(
        header[16], header[17], header[18], header[19],
    ));
    let (source_port, destination_port, tcp_flags) =
        l4_ports(frame, ip + ihl, total - ihl, protocol, fragment);
    Some(FrameView {
        ip_offset: ip,
        ip_len: total,
        l4_offset: ip + ihl,
        l4_len: total - ihl,
        ipv6: false,
        protocol,
        fragment,
        source,
        destination,
        source_port,
        destination_port,
        tcp_flags,
    })
}

fn parse_ipv6(ip: usize, frame: &[u8]) -> Option<FrameView> {
    let header = frame.get(ip..ip + 40)?;
    if header[0] >> 4 != 6 {
        return None;
    }
    let payload = usize::from(u16::from_be_bytes([header[4], header[5]]));
    let total = 40 + payload;
    if frame.len() < ip + total {
        return None;
    }
    let next = header[6];
    let protocol = match next {
        1 => Protocol::Icmp,
        6 => Protocol::Tcp,
        17 => Protocol::Udp,
        58 => Protocol::Icmpv6,
        _ => Protocol::Any,
    };
    let fragment = next == 44;
    let mut source = [0u8; 16];
    source.copy_from_slice(&header[8..24]);
    let mut destination = [0u8; 16];
    destination.copy_from_slice(&header[24..40]);
    let (source_port, destination_port, tcp_flags) = if next == 44 {
        (0, 0, 0)
    } else {
        l4_ports(frame, ip + 40, payload, protocol, false)
    };
    Some(FrameView {
        ip_offset: ip,
        ip_len: total,
        l4_offset: ip + 40,
        l4_len: payload,
        ipv6: true,
        protocol,
        fragment,
        source: IpAddr::V6(raven_pf::Ipv6Addr::new(source)),
        destination: IpAddr::V6(raven_pf::Ipv6Addr::new(destination)),
        source_port,
        destination_port,
        tcp_flags,
    })
}

fn l4_ports(
    frame: &[u8],
    offset: usize,
    length: usize,
    protocol: Protocol,
    fragment: bool,
) -> (u16, u16, u8) {
    if fragment {
        return (0, 0, 0);
    }
    match protocol {
        Protocol::Tcp => {
            let header = match frame.get(offset..offset + 20) {
                Some(header) if length >= 20 => header,
                _ => return (0, 0, 0),
            };
            (
                u16::from_be_bytes([header[0], header[1]]),
                u16::from_be_bytes([header[2], header[3]]),
                header[13],
            )
        }
        Protocol::Udp => {
            let header = match frame.get(offset..offset + 8) {
                Some(header) if length >= 8 => header,
                _ => return (0, 0, 0),
            };
            (
                u16::from_be_bytes([header[0], header[1]]),
                u16::from_be_bytes([header[2], header[3]]),
                0,
            )
        }
        _ => (0, 0, 0),
    }
}

enum Outcome {
    Keep,
    Drop,
    Rewrite(Vec<u8>),
}

/// A `Device` wrapper that filters ingress and egress frames.
pub struct FilterDevice<D: Device> {
    inner: D,
    engine: Option<Engine>,
    interface: &'static str,
    log: FilterLog,
    dropped: u64,
}

/// One frame being evaluated, grouped so the filter context is not a long
/// argument list (K423; clippy `too_many_arguments`).
struct FrameContext<'a> {
    caps: DeviceCaps,
    frame: &'a [u8],
    direction: Direction,
    interface: &'static str,
    now: Instant,
}

impl<D: Device> FilterDevice<D> {
    /// Wrap `inner`; `None` keeps the historical pass-through behaviour.
    #[must_use]
    pub fn new(inner: D, policy: Option<FilterPolicy>, interface: &'static str) -> Self {
        let (engine, log) = match policy {
            Some(policy) => {
                let (ruleset, log) = policy.into_parts();
                (Some(Engine::new(ruleset)), log)
            }
            None => (None, new_log()),
        };
        Self {
            inner,
            engine,
            interface,
            log,
            dropped: 0,
        }
    }

    /// Frames dropped by the filter since start.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// The shared log handle (the same one the policy was built with, when filtered).
    #[must_use]
    pub fn log(&self) -> FilterLog {
        Rc::clone(&self.log)
    }

    fn evaluate(
        engine: &mut Engine,
        ctx: FrameContext<'_>,
        log: &FilterLog,
        dropped: &mut u64,
    ) -> Outcome {
        let Some(view) = parse_frame(ctx.caps, ctx.frame) else {
            return Outcome::Keep;
        };
        let packet = view.packet(ctx.direction, ctx.interface);
        let decision = engine.evaluate(
            &packet,
            u64::try_from(ctx.now.as_millis().max(0)).unwrap_or(0) / 1000,
        );
        for entry in engine.drain_logs() {
            log.borrow_mut().push(format_log(&entry));
        }
        if decision.verdict == Verdict::Block {
            *dropped = dropped.saturating_add(1);
            return Outcome::Drop;
        }
        match decision.translation {
            None => Outcome::Keep,
            Some(translation) => match rewrite(ctx.frame, view, translation) {
                Some(rewritten) => Outcome::Rewrite(rewritten),
                None => {
                    // A translation the 0.1.0 hook cannot express (fragment or ICMPv6
                    // checksum) is dropped instead of forwarded unrewritten.
                    *dropped = dropped.saturating_add(1);
                    log.borrow_mut().push(format!(
                        "netd: pf drop {} {} translate-unsupported",
                        direction_name(ctx.direction),
                        ctx.interface
                    ));
                    Outcome::Drop
                }
            },
        }
    }
}

impl<D: Device> Device for FilterDevice<D> {
    fn capabilities(&self) -> DeviceCaps {
        self.inner.capabilities()
    }

    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let Self {
            inner,
            engine,
            interface,
            log,
            dropped,
        } = self;
        let caps = inner.capabilities();
        let log = Rc::clone(log);
        inner.receive(now, &mut |frame| match engine.as_mut() {
            None => consume(frame),
            Some(engine) => match Self::evaluate(
                engine,
                FrameContext {
                    caps,
                    frame,
                    direction: Direction::In,
                    interface,
                    now,
                },
                &log,
                dropped,
            ) {
                Outcome::Keep => consume(frame),
                Outcome::Drop => {}
                Outcome::Rewrite(rewritten) => consume(&rewritten),
            },
        })
    }

    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        let Self {
            inner,
            engine,
            interface,
            log,
            dropped,
        } = self;
        let Some(engine) = engine.as_mut() else {
            return inner.transmit(now, len, fill);
        };
        let caps = inner.capabilities();
        let mut frame = alloc::vec![0u8; len];
        fill(&mut frame);
        match Self::evaluate(
            engine,
            FrameContext {
                caps,
                frame: &frame,
                direction: Direction::Out,
                interface,
                now,
            },
            log,
            dropped,
        ) {
            Outcome::Drop => Ok(()),
            Outcome::Keep => inner.transmit(now, frame.len(), &mut |out| {
                out.copy_from_slice(&frame);
            }),
            Outcome::Rewrite(rewritten) => inner.transmit(now, rewritten.len(), &mut |out| {
                out.copy_from_slice(&rewritten);
            }),
        }
    }
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::In => "in",
        Direction::Out => "out",
    }
}

fn protocol_name(protocol: Protocol) -> &'static str {
    match protocol {
        Protocol::Any => "any",
        Protocol::Tcp => "tcp",
        Protocol::Udp => "udp",
        Protocol::Icmp => "icmp",
        Protocol::Icmpv6 => "icmp6",
    }
}

/// Format one engine log record for the serial log.
#[must_use]
pub fn format_log(entry: &LogEntry) -> String {
    let action = match entry.action {
        Action::Pass => "pass",
        Action::Block => "block",
    };
    let mut line = format!(
        "netd: pf {action} {} {} {} {}:{} -> {}:{}",
        direction_name(entry.direction),
        entry.interface,
        protocol_name(entry.protocol),
        entry.source,
        entry.source_port,
        entry.destination,
        entry.destination_port
    );
    if let Some(reason) = entry.reason {
        line.push_str(" reason=");
        line.push_str(
            raven_pf::abi::PFRES_NAMES
                .get(reason)
                .copied()
                .unwrap_or("?"),
        );
    }
    line
}

/// Rewrite the addresses/ports of `frame` and fix the IPv4 and TCP/UDP checksums.
///
/// Returns `None` when the packet cannot be rewritten safely (fragment or ICMPv6, whose
/// checksum covers the pseudo-header and is deferred in 0.1.0).
fn rewrite(frame: &[u8], view: FrameView, translation: Translation) -> Option<Vec<u8>> {
    if view.fragment || view.protocol == Protocol::Icmpv6 {
        return None;
    }
    let mut out = frame.to_vec();
    if view.ipv6 {
        match (translation.source, translation.destination) {
            (IpAddr::V6(source), IpAddr::V6(destination)) => {
                out.get_mut(view.ip_offset + 8..view.ip_offset + 24)?
                    .copy_from_slice(&source.octets());
                out.get_mut(view.ip_offset + 24..view.ip_offset + 40)?
                    .copy_from_slice(&destination.octets());
            }
            _ => return None,
        }
    } else {
        match (translation.source, translation.destination) {
            (IpAddr::V4(source), IpAddr::V4(destination)) => {
                out.get_mut(view.ip_offset + 12..view.ip_offset + 16)?
                    .copy_from_slice(&source.octets());
                out.get_mut(view.ip_offset + 16..view.ip_offset + 20)?
                    .copy_from_slice(&destination.octets());
            }
            _ => return None,
        }
        let ihl = view.l4_offset.checked_sub(view.ip_offset)?;
        out.get_mut(view.ip_offset + 10..view.ip_offset + 12)?
            .copy_from_slice(&[0, 0]);
        let checksum = checksum(out.get(view.ip_offset..view.ip_offset + ihl)?);
        out.get_mut(view.ip_offset + 10..view.ip_offset + 12)?
            .copy_from_slice(&checksum.to_be_bytes());
    }
    match view.protocol {
        Protocol::Tcp | Protocol::Udp => {
            out.get_mut(view.l4_offset..view.l4_offset + 2)?
                .copy_from_slice(&translation.source_port.to_be_bytes());
            out.get_mut(view.l4_offset + 2..view.l4_offset + 4)?
                .copy_from_slice(&translation.destination_port.to_be_bytes());
            let offset = if view.protocol == Protocol::Tcp {
                16
            } else {
                6
            };
            let at = view.l4_offset + offset;
            let old = u16::from_be_bytes([*out.get(at)?, *out.get(at + 1)?]);
            if view.protocol == Protocol::Udp && !view.ipv6 && old == 0 {
                return Some(out);
            }
            out.get_mut(at..at + 2)?.copy_from_slice(&[0, 0]);
            let sum = l4_checksum(&out, view)?;
            let checksum = if sum == 0 { 0xffff } else { sum };
            out.get_mut(at..at + 2)?
                .copy_from_slice(&checksum.to_be_bytes());
        }
        _ => {}
    }
    Some(out)
}

fn l4_checksum(frame: &[u8], view: FrameView) -> Option<u16> {
    let mut sum = 0u32;
    // The pseudo-header uses the addresses as they are in the (already rewritten) frame.
    if view.ipv6 {
        sum = add_bytes(sum, frame.get(view.ip_offset + 8..view.ip_offset + 24)?);
        sum = add_bytes(sum, frame.get(view.ip_offset + 24..view.ip_offset + 40)?);
        sum = add_bytes(sum, &(u32::try_from(view.l4_len).ok()?).to_be_bytes());
        sum = add_bytes(sum, &[0, 0, 0, protocol_number(view.protocol)?]);
    } else {
        sum = add_bytes(sum, frame.get(view.ip_offset + 12..view.ip_offset + 16)?);
        sum = add_bytes(sum, frame.get(view.ip_offset + 16..view.ip_offset + 20)?);
        sum = add_bytes(
            sum,
            &[
                0,
                protocol_number(view.protocol)?,
                u8::try_from(view.l4_len >> 8).ok()?,
                u8::try_from(view.l4_len & 0xff).ok()?,
            ],
        );
    }
    let segment = frame.get(view.l4_offset..view.l4_offset + view.l4_len)?;
    sum = add_bytes(sum, segment);
    Some(fold(sum))
}

fn protocol_number(protocol: Protocol) -> Option<u8> {
    match protocol {
        Protocol::Tcp => Some(6),
        Protocol::Udp => Some(17),
        Protocol::Icmp => Some(1),
        Protocol::Icmpv6 => Some(58),
        Protocol::Any => None,
    }
}

fn add_bytes(mut sum: u32, data: &[u8]) -> u32 {
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        sum += u32::from(u16::from_be_bytes([chunk[0], chunk[1]]));
    }
    if let [last] = chunks.remainder() {
        sum += u32::from(*last) << 8;
    }
    sum
}

fn fold(mut sum: u32) -> u16 {
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn checksum(data: &[u8]) -> u16 {
    fold(add_bytes(0, data))
}

#[cfg(test)]
#[path = "filter_ut.rs"]
mod tests;
