// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! First-party SNTPv4 client core.
//!
//! The crate is a pure, host-testable model of the client half of SNTPv4 (RFC 4330/5905): a
//! 48-byte request/response codec, NTP era and Unix-time conversion, response validation,
//! offset/delay computation, deterministic server selection and a bounded timeout/retry policy.
//! It has no sockets, no clock syscall and no allocator; the caller injects a [`Transport`] and a
//! [`Clock`], so the network service and the wall-clock capability stay outside this crate.
//!
//! The timestamps are kept in NTP's 64-bit `seconds.fraction` form and in an explicit two-era
//! representation. Converting a bare wire timestamp needs a reference time because the 32-bit
//! second field wraps at 2036-02-07; [`NtpTime::from_timestamp`] picks the era nearest the
//! request's transmit time, which covers a normal exchange across the era boundary.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;

/// SNTPv4 packet length in bytes.
pub const PACKET_LEN: usize = 48;
/// Port assigned to the NTP/SNTP service.
pub const DEFAULT_PORT: u16 = 123;
/// Seconds from the NTP epoch (1900-01-01) to the Unix epoch (1970-01-01).
pub const NTP_UNIX_OFFSET_SECS: i64 = 2_208_988_800;
/// One NTP era in seconds (2^32); the NTP second field wraps at this value.
pub const NTP_ERA_SECS: u64 = 1_u64 << 32;
/// Maximum number of configured servers kept without an allocator.
pub const MAX_SERVERS: usize = 4;
/// Maximum accepted attempts per server in one synchronization pass.
pub const MAX_ATTEMPTS: u8 = 8;

const NS_PER_SEC: i128 = 1_000_000_000;
const FRACTION_SCALE: i128 = 1_i128 << 32;

/// The leap indicator field (`LI`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leap {
    /// No warning.
    NoWarning,
    /// The last minute has 61 seconds.
    AddSecond,
    /// The last minute has 59 seconds.
    DeleteSecond,
    /// The server is not synchronized.
    Unsynchronized,
}

impl Leap {
    const fn from_bits(bits: u8) -> Self {
        match bits & 0x3 {
            0 => Self::NoWarning,
            1 => Self::AddSecond,
            2 => Self::DeleteSecond,
            _ => Self::Unsynchronized,
        }
    }
}

/// The NTP era: one 32-bit seconds field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Era {
    /// 1900-02-07 through 2036-02-07.
    Era0,
    /// 2036-02-07 through 2172-02-07.
    Era1,
}

impl Era {
    const fn base_seconds(self) -> i128 {
        match self {
            Self::Era0 => 0,
            Self::Era1 => 1_i128 << 32,
        }
    }
}

/// One NTP timestamp as it appears on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct NtpTimestamp {
    /// Seconds since the start of the current era.
    pub seconds: u32,
    /// Fraction of a second in 2^-32 units.
    pub fraction: u32,
}

impl NtpTimestamp {
    /// The all-zero timestamp (used by the server for invalid or unknown fields).
    pub const ZERO: Self = Self {
        seconds: 0,
        fraction: 0,
    };

    /// Build a timestamp from its two wire words.
    #[must_use]
    pub const fn new(seconds: u32, fraction: u32) -> Self {
        Self { seconds, fraction }
    }

    /// True when both wire words are zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.seconds == 0 && self.fraction == 0
    }

    /// Encode as eight big-endian bytes.
    #[must_use]
    pub fn to_be_bytes(self) -> [u8; 8] {
        let seconds = self.seconds.to_be_bytes();
        let fraction = self.fraction.to_be_bytes();
        [
            seconds[0],
            seconds[1],
            seconds[2],
            seconds[3],
            fraction[0],
            fraction[1],
            fraction[2],
            fraction[3],
        ]
    }

    /// Decode eight big-endian bytes.
    #[must_use]
    pub fn from_be_bytes(bytes: [u8; 8]) -> Self {
        Self {
            seconds: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            fraction: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        }
    }
}

/// A 16.16 fixed-point seconds value (root delay and root dispersion).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NtpShort(pub u32);

impl NtpShort {
    /// Convert to nanoseconds (rounded down).
    #[must_use]
    pub const fn to_ns(self) -> i64 {
        self.0 as i64 * 1_000_000_000 / 65_536
    }

    /// Build from nanoseconds, saturating at the 16.16 maximum.
    #[must_use]
    pub fn from_ns(ns: i64) -> Self {
        let value = if ns <= 0 {
            0
        } else {
            u64::try_from(ns)
                .unwrap_or(u64::MAX)
                .saturating_mul(65_536)
                .saturating_div(1_000_000_000)
                .min(u64::from(u32::MAX))
        };
        Self(u32::try_from(value).unwrap_or(u32::MAX))
    }
}

/// An NTP time with an explicit era.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NtpTime {
    /// The era the 32-bit seconds belong to.
    pub era: Era,
    /// Seconds since the start of the era.
    pub seconds: u32,
    /// Fraction of a second in 2^-32 units.
    pub fraction: u32,
}

impl NtpTime {
    /// The all-zero era-0 time.
    pub const ZERO: Self = Self {
        era: Era::Era0,
        seconds: 0,
        fraction: 0,
    };

    /// Build a time from explicit parts.
    #[must_use]
    pub const fn new(era: Era, seconds: u32, fraction: u32) -> Self {
        Self {
            era,
            seconds,
            fraction,
        }
    }

    /// The wire timestamp (era is not carried on the wire).
    #[must_use]
    pub const fn timestamp(self) -> NtpTimestamp {
        NtpTimestamp::new(self.seconds, self.fraction)
    }

    /// Convert to Unix nanoseconds, including the era.
    #[must_use]
    pub fn to_unix_ns(self) -> i128 {
        let seconds =
            self.era.base_seconds() + i128::from(self.seconds) - i128::from(NTP_UNIX_OFFSET_SECS);
        seconds * NS_PER_SEC + i128::from(self.fraction) * NS_PER_SEC / FRACTION_SCALE
    }

    /// Convert Unix nanoseconds into the nearest NTP era representation.
    ///
    /// # Errors
    /// Returns [`Error::EraOutOfRange`] outside the two NTP eras (before 1900 or after 2172).
    pub fn from_unix_ns(ns: i128) -> Result<Self, Error> {
        let unix_seconds = ns.div_euclid(NS_PER_SEC);
        let nanos = ns.rem_euclid(NS_PER_SEC);
        let ntp_seconds = unix_seconds
            .checked_add(i128::from(NTP_UNIX_OFFSET_SECS))
            .ok_or(Error::EraOutOfRange)?;
        if !(0..(1_i128 << 33)).contains(&ntp_seconds) {
            return Err(Error::EraOutOfRange);
        }
        let era = if ntp_seconds < (1_i128 << 32) {
            Era::Era0
        } else {
            Era::Era1
        };
        let seconds =
            u32::try_from(ntp_seconds - era.base_seconds()).map_err(|_| Error::EraOutOfRange)?;
        let fraction = u32::try_from((nanos * FRACTION_SCALE) / NS_PER_SEC)
            .map_err(|_| Error::EraOutOfRange)?;
        Ok(Self {
            era,
            seconds,
            fraction,
        })
    }

    /// Interpret a bare wire timestamp using `reference` to choose the closest era.
    ///
    /// A normal exchange stays in one era; a request sent just before 2036 may receive an era-1
    /// timestamp just after the wrap, and the closest-era rule keeps that exchange correct.
    ///
    /// # Errors
    /// The function cannot fail for the two NTP eras; the result is always one of them.
    pub fn from_timestamp(timestamp: NtpTimestamp, reference: Self) -> Result<Self, Error> {
        let era0 = Self::new(Era::Era0, timestamp.seconds, timestamp.fraction);
        let era1 = Self::new(Era::Era1, timestamp.seconds, timestamp.fraction);
        let reference_ns = reference.to_unix_ns();
        let diff0 = (era0.to_unix_ns() - reference_ns).abs();
        let diff1 = (era1.to_unix_ns() - reference_ns).abs();
        Ok(if diff1 < diff0 { era1 } else { era0 })
    }

    /// Add a signed nanosecond difference and renormalize the era.
    ///
    /// # Errors
    /// Returns [`Error::EraOutOfRange`] when the result leaves the two NTP eras.
    pub fn checked_add_ns(self, delta: i128) -> Result<Self, Error> {
        let value = self
            .to_unix_ns()
            .checked_add(delta)
            .ok_or(Error::EraOutOfRange)?;
        Self::from_unix_ns(value)
    }
}

/// Why a packet, sample or synchronization failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The datagram is shorter than a complete SNTP packet.
    PacketTooShort,
    /// The version field is not SNTPv4.
    BadVersion,
    /// The mode field is not a server reply.
    BadMode,
    /// The leap indicator says the server is unsynchronized.
    Unsynchronized,
    /// The stratum is zero (kiss-o'-death) or above 15 (unsynchronized).
    KissOfDeath,
    /// The server did not echo the request transmit timestamp.
    OriginateMismatch,
    /// The server transmit timestamp is zero.
    ZeroTransmit,
    /// A timestamp cannot be represented in the two NTP eras.
    EraOutOfRange,
    /// The server list is full.
    ServerListFull,
    /// No server produced a usable sample.
    NoServer,
    /// The transport reported a timeout.
    Timeout,
    /// The transport failed for another reason.
    Transport,
    /// The transport returned an invalid response length.
    InvalidLength,
    /// The clock rejected the new wall time.
    ClockSet,
}

/// A configured SNTP server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Server {
    /// IPv4 address in network byte order.
    pub address: [u8; 4],
    /// UDP port.
    pub port: u16,
    /// Selection priority; lower values are preferred.
    pub priority: u8,
}

impl Server {
    /// Build a server entry.
    #[must_use]
    pub const fn new(address: [u8; 4], port: u16, priority: u8) -> Self {
        Self {
            address,
            port,
            priority,
        }
    }
}

/// A fixed-capacity server list.
#[derive(Clone, Copy, Debug, Default)]
pub struct ServerList {
    entries: [Option<Server>; MAX_SERVERS],
    len: usize,
}

impl ServerList {
    /// Create an empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [None; MAX_SERVERS],
            len: 0,
        }
    }

    /// Add one server.
    ///
    /// # Errors
    /// Returns [`Error::ServerListFull`] when all [`MAX_SERVERS`] slots are used.
    pub fn push(&mut self, server: Server) -> Result<(), Error> {
        let slot = self
            .entries
            .get_mut(self.len)
            .ok_or(Error::ServerListFull)?;
        *slot = Some(server);
        self.len = self.len.saturating_add(1);
        Ok(())
    }

    /// Number of configured servers.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True when no server is configured.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Iterate the configured servers in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = Server> + '_ {
        self.entries.iter().take(self.len).flatten().copied()
    }

    /// Pick the server with the lowest priority value (first one on a tie).
    #[must_use]
    pub fn select(&self) -> Option<Server> {
        self.iter().min_by_key(|server| server.priority)
    }
}

/// Per-server timeout and attempt policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Attempts per server, clamped to `1..=MAX_ATTEMPTS` by the client.
    pub attempts: u8,
    /// Per-attempt timeout in milliseconds.
    pub timeout_ms: u32,
}

impl RetryPolicy {
    /// A sensible default: three attempts, one second each.
    pub const DEFAULT: Self = Self {
        attempts: 3,
        timeout_ms: 1_000,
    };

    /// Build a policy.
    #[must_use]
    pub const fn new(attempts: u8, timeout_ms: u32) -> Self {
        Self {
            attempts,
            timeout_ms,
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The wall-clock seam.
///
/// The real implementation reads `CLOCK_REALTIME`/`CLOCK_MONOTONIC` and sets `CLOCK_REALTIME`;
/// the core only sees the resulting [`NtpTime`]. This keeps the wall-clock privilege outside the
/// packet/selection logic and lets host tests use a mock clock.
pub trait Clock {
    /// Read the current wall time.
    fn now(&self) -> NtpTime;

    /// Apply a new wall time.
    ///
    /// # Errors
    /// An implementation returns [`Error::ClockSet`] when the kernel rejects the value or the
    /// caller lacks the wall-clock capability.
    fn set(&mut self, time: NtpTime) -> Result<(), Error>;
}

/// One UDP exchange with a server.
///
/// The real transport opens `net:udp/<server>:123` through netd; the core owns only the request
/// bytes, the receiving buffer and the timeout value.
pub trait Transport {
    /// Send `request` to `server` and write at most [`PACKET_LEN`] reply bytes.
    ///
    /// # Errors
    /// Returns [`Error::Timeout`] for a timeout, or [`Error::Transport`] for any other failure.
    fn exchange(
        &mut self,
        server: Server,
        request: &[u8; PACKET_LEN],
        response: &mut [u8; PACKET_LEN],
        timeout_ms: u32,
    ) -> Result<usize, Error>;
}

/// One decoded server reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response {
    /// Leap indicator.
    pub leap: Leap,
    /// Version field (4 for SNTPv4).
    pub version: u8,
    /// Mode field (4 for a server reply).
    pub mode: u8,
    /// Stratum; 0 is kiss-o'-death, 1..15 are usable.
    pub stratum: u8,
    /// Poll exponent.
    pub poll: i8,
    /// Precision exponent.
    pub precision: i8,
    /// Root delay.
    pub root_delay: NtpShort,
    /// Root dispersion.
    pub root_dispersion: NtpShort,
    /// Reference identifier.
    pub reference_id: u32,
    /// Reference timestamp.
    pub reference: NtpTimestamp,
    /// Originate timestamp (the client's transmit time echoed by the server).
    pub originate: NtpTimestamp,
    /// Server receive timestamp.
    pub receive: NtpTimestamp,
    /// Server transmit timestamp.
    pub transmit: NtpTimestamp,
}

/// One validated synchronization sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    /// The server that produced the sample.
    pub server: Server,
    /// Leap indicator from the reply.
    pub leap: Leap,
    /// Stratum from the reply.
    pub stratum: u8,
    /// Estimated server-minus-client offset in nanoseconds.
    pub offset_ns: i128,
    /// Round-trip delay in nanoseconds (may be negative on an invalid measurement).
    pub delay_ns: i128,
    /// Root delay in nanoseconds.
    pub root_delay_ns: i64,
    /// Root dispersion in nanoseconds.
    pub root_dispersion_ns: i64,
    /// Reference identifier from the reply.
    pub reference_id: u32,
    /// Server receive time.
    pub receive: NtpTime,
    /// Server transmit time.
    pub transmit: NtpTime,
}

impl Sample {
    /// Whether this sample should beat `other` for the same synchronization pass.
    #[must_use]
    pub fn better_than(&self, other: &Self) -> bool {
        if self.stratum != other.stratum {
            return self.stratum < other.stratum;
        }
        if self.delay_ns != other.delay_ns {
            return self.delay_ns < other.delay_ns;
        }
        self.server.priority < other.server.priority
    }
}

/// Encode an SNTPv4 client request with the given transmit timestamp.
#[must_use]
pub fn encode_request(transmit: NtpTime) -> [u8; PACKET_LEN] {
    let mut packet = [0u8; PACKET_LEN];
    // LI = 0, VN = 4, mode = 3 (client).
    packet[0] = 0x23;
    let timestamp = transmit.timestamp().to_be_bytes();
    packet[40..48].copy_from_slice(&timestamp);
    packet
}

fn be_u32(packet: &[u8], offset: usize) -> Result<u32, Error> {
    let bytes = packet
        .get(offset..offset.saturating_add(4))
        .ok_or(Error::PacketTooShort)?;
    let array: [u8; 4] = bytes.try_into().map_err(|_| Error::PacketTooShort)?;
    Ok(u32::from_be_bytes(array))
}

fn be_u64(packet: &[u8], offset: usize) -> Result<u64, Error> {
    let bytes = packet
        .get(offset..offset.saturating_add(8))
        .ok_or(Error::PacketTooShort)?;
    let array: [u8; 8] = bytes.try_into().map_err(|_| Error::PacketTooShort)?;
    Ok(u64::from_be_bytes(array))
}

/// Decode the fixed 48-byte SNTPv4 server reply prefix.
///
/// # Errors
/// Returns [`Error::PacketTooShort`], [`Error::BadVersion`] or [`Error::BadMode`] for a malformed
/// packet. Semantic checks (leap, stratum, originate, zero transmit) belong to
/// [`Response::sample`].
pub fn decode_response(packet: &[u8]) -> Result<Response, Error> {
    if packet.len() < PACKET_LEN {
        return Err(Error::PacketTooShort);
    }
    let first = *packet.first().ok_or(Error::PacketTooShort)?;
    let leap = Leap::from_bits(first >> 6);
    let version = (first >> 3) & 0x7;
    if version != 4 {
        return Err(Error::BadVersion);
    }
    let mode = first & 0x7;
    if mode != 4 {
        return Err(Error::BadMode);
    }
    let stratum = *packet.get(1).ok_or(Error::PacketTooShort)?;
    let poll = *packet.get(2).ok_or(Error::PacketTooShort)? as i8;
    let precision = *packet.get(3).ok_or(Error::PacketTooShort)? as i8;
    let timestamp = |offset: usize| -> Result<NtpTimestamp, Error> {
        let bytes = be_u64(packet, offset)?.to_be_bytes();
        Ok(NtpTimestamp::from_be_bytes(bytes))
    };
    Ok(Response {
        leap,
        version,
        mode,
        stratum,
        poll,
        precision,
        root_delay: NtpShort(be_u32(packet, 4)?),
        root_dispersion: NtpShort(be_u32(packet, 8)?),
        reference_id: be_u32(packet, 12)?,
        reference: timestamp(16)?,
        originate: timestamp(24)?,
        receive: timestamp(32)?,
        transmit: timestamp(40)?,
    })
}

impl Response {
    /// Validate the reply against the request and compute the offset/delay sample.
    ///
    /// # Errors
    /// Rejects unsynchronized servers, kiss-o'-death, a mismatched originate timestamp and a zero
    /// transmit timestamp before doing any timestamp arithmetic.
    pub fn sample(
        &self,
        server: Server,
        request: NtpTime,
        received: NtpTime,
    ) -> Result<Sample, Error> {
        if self.leap == Leap::Unsynchronized || self.stratum > 15 {
            return Err(Error::Unsynchronized);
        }
        if self.stratum == 0 {
            return Err(Error::KissOfDeath);
        }
        if self.originate != request.timestamp() {
            return Err(Error::OriginateMismatch);
        }
        if self.transmit.is_zero() {
            return Err(Error::ZeroTransmit);
        }
        let receive = NtpTime::from_timestamp(self.receive, request)?;
        let transmit = NtpTime::from_timestamp(self.transmit, request)?;
        let t1 = request.to_unix_ns();
        let t2 = receive.to_unix_ns();
        let t3 = transmit.to_unix_ns();
        let t4 = received.to_unix_ns();
        let offset_ns = ((t2 - t1) + (t3 - t4)) / 2;
        let delay_ns = (t4 - t1) - (t3 - t2);
        Ok(Sample {
            server,
            leap: self.leap,
            stratum: self.stratum,
            offset_ns,
            delay_ns,
            root_delay_ns: self.root_delay.to_ns(),
            root_dispersion_ns: self.root_dispersion.to_ns(),
            reference_id: self.reference_id,
            receive,
            transmit,
        })
    }
}

/// A deterministic SNTP client over an injected transport and clock.
#[derive(Clone, Copy, Debug)]
pub struct Client {
    servers: ServerList,
    retry: RetryPolicy,
}

impl Client {
    /// Create a client.
    #[must_use]
    pub const fn new(servers: ServerList, retry: RetryPolicy) -> Self {
        Self { servers, retry }
    }

    /// The configured server list.
    #[must_use]
    pub const fn servers(&self) -> &ServerList {
        &self.servers
    }

    /// The retry policy.
    #[must_use]
    pub const fn retry(&self) -> RetryPolicy {
        self.retry
    }

    /// Run one synchronization pass, select the best valid sample and apply its offset.
    ///
    /// Every server is attempted up to `retry.attempts` times. A timeout or an invalid reply moves
    /// on to the next attempt/server; a non-timeout transport failure moves to the next server. The
    /// best sample is chosen by lowest stratum, then lowest round-trip delay, then server priority.
    /// The wall clock is set once, after the pass, to `clock.now() + sample.offset_ns`.
    ///
    /// # Errors
    /// Returns the last server/transport error, [`Error::NoServer`] when the list is empty, or
    /// [`Error::ClockSet`]/[`Error::EraOutOfRange`] when the clock update fails.
    pub fn sync<C: Clock, T: Transport>(
        &self,
        clock: &mut C,
        transport: &mut T,
    ) -> Result<Sample, Error> {
        if self.servers.is_empty() {
            return Err(Error::NoServer);
        }
        let attempts = self.retry.attempts.clamp(1, MAX_ATTEMPTS);
        let mut best: Option<Sample> = None;
        let mut last = Error::NoServer;
        for server in self.servers.iter() {
            for _ in 0..attempts {
                let request_time = clock.now();
                let request = encode_request(request_time);
                let mut response = [0u8; PACKET_LEN];
                match transport.exchange(server, &request, &mut response, self.retry.timeout_ms) {
                    Ok(length) if length <= PACKET_LEN => {
                        let received = clock.now();
                        let packet = response.get(..length).unwrap_or(&[]);
                        match decode_response(packet) {
                            Ok(parsed) => match parsed.sample(server, request_time, received) {
                                Ok(sample) => {
                                    let replace = best
                                        .as_ref()
                                        .is_none_or(|current| sample.better_than(current));
                                    if replace {
                                        best = Some(sample);
                                    }
                                    break;
                                }
                                Err(error) => last = error,
                            },
                            Err(error) => last = error,
                        }
                    }
                    Ok(_) => last = Error::InvalidLength,
                    Err(Error::Timeout) => last = Error::Timeout,
                    Err(error) => {
                        last = error;
                        break;
                    }
                }
            }
        }
        let sample = best.ok_or(last)?;
        let target = clock.now().checked_add_ns(sample.offset_ns)?;
        clock.set(target)?;
        Ok(sample)
    }
}
