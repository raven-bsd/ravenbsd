// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IPv6 header and extension-header parsing (RFC 8200).

use core::fmt;

use crate::{ParseError, read_u8, read_u16, slice, write_u16, write_u32};

/// Length of an IPv6 fixed header.
pub const HEADER_LEN: usize = 40;

/// An IPv6 address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ipv6Addr([u8; 16]);

impl Ipv6Addr {
    /// The unspecified address `::`.
    pub const UNSPECIFIED: Self = Self([0; 16]);
    /// The loopback address `::1`.
    pub const LOOPBACK: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

    /// Construct an address from its sixteen octets.
    #[must_use]
    pub const fn new(octets: [u8; 16]) -> Self {
        Self(octets)
    }

    /// Return the sixteen octets in wire order.
    #[must_use]
    pub const fn octets(self) -> [u8; 16] {
        self.0
    }

    /// Construct an address from a slice if it is exactly sixteen octets long.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes {
            &[a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p] => {
                Some(Self([a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p]))
            }
            _ => None,
        }
    }

    /// Whether this is the unspecified address.
    #[must_use]
    pub const fn is_unspecified(self) -> bool {
        matches!(self.0, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    }

    /// Whether this is the loopback address.
    #[must_use]
    pub const fn is_loopback(self) -> bool {
        matches!(self.0, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])
    }

    /// Whether this is a multicast address (`ff00::/8`).
    #[must_use]
    pub const fn is_multicast(self) -> bool {
        self.0[0] == 0xff
    }
}

impl From<[u8; 16]> for Ipv6Addr {
    fn from(octets: [u8; 16]) -> Self {
        Self(octets)
    }
}

impl fmt::Display for Ipv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let o = self.0;
        write!(
            f,
            "{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}",
            u16::from_be_bytes([o[0], o[1]]),
            u16::from_be_bytes([o[2], o[3]]),
            u16::from_be_bytes([o[4], o[5]]),
            u16::from_be_bytes([o[6], o[7]]),
            u16::from_be_bytes([o[8], o[9]]),
            u16::from_be_bytes([o[10], o[11]]),
            u16::from_be_bytes([o[12], o[13]]),
            u16::from_be_bytes([o[14], o[15]]),
        )
    }
}

/// The kind of an IPv6 extension header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtensionKind {
    /// Hop-by-hop options (next header 0).
    HopByHop,
    /// Routing header (next header 43).
    Routing,
    /// Destination options (next header 60).
    Destination,
    /// Authentication header (next header 51).
    Authentication,
    /// Fragment header (next header 44).
    Fragment,
}

/// A borrowed IPv6 extension header.
#[derive(Debug, Clone, Copy)]
pub struct Ipv6Extension<'a> {
    /// Extension kind.
    pub kind: ExtensionKind,
    /// The next-header value carried by this extension.
    pub next_header: u8,
    /// The complete extension header bytes.
    pub bytes: &'a [u8],
}

impl Ipv6Extension<'_> {
    /// Decode a fragment header.
    #[must_use]
    pub fn fragment(&self) -> Option<FragmentHeader> {
        if self.kind != ExtensionKind::Fragment {
            return None;
        }
        let [_, _, hi, lo, a, b, c, d] = self.bytes else {
            return None;
        };
        let offset_flags = u16::from_be_bytes([*hi, *lo]);
        let offset = (offset_flags >> 3).saturating_mul(8);
        Some(FragmentHeader {
            next_header: self.next_header,
            offset,
            more: offset_flags & 1 != 0,
            identification: u32::from_be_bytes([*a, *b, *c, *d]),
        })
    }
}

/// An IPv6 fragment header (RFC 8200 section 4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FragmentHeader {
    /// Upper-layer next header in this fragment.
    pub next_header: u8,
    /// Fragment offset in bytes.
    pub offset: u16,
    /// More-fragments flag.
    pub more: bool,
    /// Fragment identification.
    pub identification: u32,
}

/// A borrowed IPv6 packet.
#[derive(Debug, Clone, Copy)]
pub struct Ipv6Packet<'a> {
    bytes: &'a [u8],
}

impl<'a> Ipv6Packet<'a> {
    /// Parse and validate an IPv6 packet, including its extension-header chain.
    ///
    /// Validation order is pinned by `spec/tcpip/vectors/headers.md`: truncation, version,
    /// jumbogram, payload length, then extension-chain lengths.
    ///
    /// # Errors
    /// Returns `ipv6.truncated`, `ipv6.version`, `ipv6.jumbogram`, `ipv6.payload_len` or
    /// `ipv6.ext_len`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ParseError> {
        if bytes.len() < HEADER_LEN {
            return Err(ParseError::Truncated {
                tag: "ipv6.truncated",
                need: HEADER_LEN,
                got: bytes.len(),
            });
        }
        let first = read_u8(bytes, 0, "ipv6.truncated")?;
        let version = first >> 4;
        if version != 6 {
            return Err(ParseError::BadVersion {
                tag: "ipv6.version",
                version,
            });
        }
        let payload_len = usize::from(read_u16(bytes, 4, "ipv6.truncated")?);
        if payload_len == 0 {
            return Err(ParseError::Malformed {
                tag: "ipv6.jumbogram",
            });
        }
        if HEADER_LEN.saturating_add(payload_len) != bytes.len() {
            return Err(ParseError::Malformed {
                tag: "ipv6.payload_len",
            });
        }
        let mut next = read_u8(bytes, 6, "ipv6.truncated")?;
        let mut offset = HEADER_LEN;
        while let Some((extension, next_offset)) = walk_extension(bytes, offset, next)? {
            next = extension.next_header;
            offset = next_offset;
        }
        Ok(Self { bytes })
    }

    /// IP version (always 6 after a successful parse).
    #[must_use]
    pub fn version(&self) -> u8 {
        match self.bytes.first() {
            Some(first) => first >> 4,
            None => 0,
        }
    }

    /// Traffic class.
    #[must_use]
    pub fn traffic_class(&self) -> u8 {
        match self.bytes.get(0..2) {
            Some(&[first, second]) => (first & 0x0f) << 4 | (second >> 4),
            _ => 0,
        }
    }

    /// Flow label (20 bits).
    #[must_use]
    pub fn flow_label(&self) -> u32 {
        match self.bytes.get(1..4) {
            Some(&[a, b, c]) => (u32::from(a & 0x0f) << 16) | (u32::from(b) << 8) | u32::from(c),
            _ => 0,
        }
    }

    /// Payload length from the wire, including extension headers.
    #[must_use]
    pub fn payload_len(&self) -> u16 {
        read_u16(self.bytes, 4, "ipv6.truncated").unwrap_or(0)
    }

    /// The first next-header value, before any extension chain.
    #[must_use]
    pub fn next_header(&self) -> u8 {
        match self.bytes.get(6) {
            Some(&value) => value,
            None => 0,
        }
    }

    /// Hop limit.
    #[must_use]
    pub fn hop_limit(&self) -> u8 {
        match self.bytes.get(7) {
            Some(&value) => value,
            None => 0,
        }
    }

    /// Source address.
    #[must_use]
    pub fn src(&self) -> Ipv6Addr {
        let mut octets = [0u8; 16];
        if let Some(part) = self.bytes.get(8..24) {
            octets.copy_from_slice(part);
        }
        Ipv6Addr::new(octets)
    }

    /// Destination address.
    #[must_use]
    pub fn dst(&self) -> Ipv6Addr {
        let mut octets = [0u8; 16];
        if let Some(part) = self.bytes.get(24..40) {
            octets.copy_from_slice(part);
        }
        Ipv6Addr::new(octets)
    }

    /// The complete payload, including extension headers.
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        self.bytes.get(HEADER_LEN..).unwrap_or(&[])
    }

    /// Walk the extension chain, returning the final next-header value and the upper-layer
    /// payload after the last extension header.
    #[must_use]
    pub fn upper_layer(&self) -> (u8, &'a [u8]) {
        let mut next = self.next_header();
        let mut offset = HEADER_LEN;
        while let Ok(Some((extension, next_offset))) = walk_extension(self.bytes, offset, next) {
            next = extension.next_header;
            offset = next_offset;
        }
        (next, self.bytes.get(offset..).unwrap_or(&[]))
    }

    /// The first fragment header in the extension chain, if any.
    #[must_use]
    pub fn fragment_header(&self) -> Option<FragmentHeader> {
        let mut next = self.next_header();
        let mut offset = HEADER_LEN;
        loop {
            match walk_extension(self.bytes, offset, next) {
                Ok(Some((extension, next_offset))) => {
                    if let Some(fragment) = extension.fragment() {
                        return Some(fragment);
                    }
                    next = extension.next_header;
                    offset = next_offset;
                }
                _ => return None,
            }
        }
    }

    /// Iterate over the extension headers.
    #[must_use]
    pub fn extensions(&self) -> Extensions<'a> {
        Extensions {
            bytes: self.bytes,
            next: self.next_header(),
            offset: HEADER_LEN,
            done: false,
        }
    }

    /// The complete packet bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.bytes
    }
}

/// Iterator over a validated IPv6 extension chain.
#[derive(Debug, Clone)]
pub struct Extensions<'a> {
    bytes: &'a [u8],
    next: u8,
    offset: usize,
    done: bool,
}

impl<'a> Iterator for Extensions<'a> {
    type Item = Ipv6Extension<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match walk_extension(self.bytes, self.offset, self.next) {
            Ok(Some((extension, next_offset))) => {
                self.next = extension.next_header;
                self.offset = next_offset;
                Some(extension)
            }
            _ => {
                self.done = true;
                None
            }
        }
    }
}

fn walk_extension(
    bytes: &[u8],
    offset: usize,
    next_header: u8,
) -> Result<Option<(Ipv6Extension<'_>, usize)>, ParseError> {
    let kind = match next_header {
        0 => ExtensionKind::HopByHop,
        43 => ExtensionKind::Routing,
        60 => ExtensionKind::Destination,
        51 => ExtensionKind::Authentication,
        44 => ExtensionKind::Fragment,
        _ => return Ok(None),
    };
    let length = match kind {
        ExtensionKind::Fragment => 8,
        ExtensionKind::Authentication => {
            let value = read_u8(bytes, offset.saturating_add(1), "ipv6.ext_len")?;
            (usize::from(value) + 2).saturating_mul(4)
        }
        ExtensionKind::HopByHop | ExtensionKind::Routing | ExtensionKind::Destination => {
            let value = read_u8(bytes, offset.saturating_add(1), "ipv6.ext_len")?;
            (usize::from(value) + 1).saturating_mul(8)
        }
    };
    let end = offset.checked_add(length).ok_or(ParseError::Malformed {
        tag: "ipv6.ext_len",
    })?;
    if end > bytes.len() {
        return Err(ParseError::Malformed {
            tag: "ipv6.ext_len",
        });
    }
    let header = slice(bytes, offset, end, "ipv6.ext_len")?;
    let extension_next = read_u8(header, 0, "ipv6.ext_len")?;
    Ok(Some((
        Ipv6Extension {
            kind,
            next_header: extension_next,
            bytes: header,
        },
        end,
    )))
}

/// The fields of an IPv6 fixed header that a sender controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv6Header {
    /// Traffic class.
    pub traffic_class: u8,
    /// Flow label (20 bits).
    pub flow_label: u32,
    /// Next-header value.
    pub next_header: u8,
    /// Hop limit.
    pub hop_limit: u8,
    /// Source address.
    pub src: Ipv6Addr,
    /// Destination address.
    pub dst: Ipv6Addr,
}

impl Default for Ipv6Header {
    fn default() -> Self {
        Self {
            traffic_class: 0,
            flow_label: 0,
            next_header: 59,
            hop_limit: 64,
            src: Ipv6Addr::UNSPECIFIED,
            dst: Ipv6Addr::UNSPECIFIED,
        }
    }
}

/// Builder for an IPv6 packet with a fixed 40-byte header.
#[derive(Debug)]
pub struct Ipv6Builder<'a> {
    buffer: &'a mut [u8],
    total_len: usize,
}

impl<'a> Ipv6Builder<'a> {
    /// Write an IPv6 header for a payload of `payload_len` octets.
    ///
    /// # Errors
    /// Returns `ipv6.jumbogram` for a zero payload length, `ipv6.payload_len` when the payload
    /// does not fit the 16-bit field, or `ipv6.truncated` when the buffer is too short.
    pub fn new(
        buffer: &'a mut [u8],
        header: &Ipv6Header,
        payload_len: usize,
    ) -> Result<Self, ParseError> {
        if payload_len == 0 {
            return Err(ParseError::Malformed {
                tag: "ipv6.jumbogram",
            });
        }
        if payload_len > usize::from(u16::MAX) {
            return Err(ParseError::Malformed {
                tag: "ipv6.payload_len",
            });
        }
        let total_len = HEADER_LEN
            .checked_add(payload_len)
            .ok_or(ParseError::Malformed {
                tag: "ipv6.payload_len",
            })?;
        if buffer.len() < total_len {
            return Err(ParseError::Truncated {
                tag: "ipv6.truncated",
                need: total_len,
                got: buffer.len(),
            });
        }
        let Some(fixed) = buffer.get_mut(..HEADER_LEN) else {
            return Err(ParseError::Truncated {
                tag: "ipv6.truncated",
                need: total_len,
                got: buffer.len(),
            });
        };
        let short_payload = u16::try_from(payload_len).map_err(|_| ParseError::Malformed {
            tag: "ipv6.payload_len",
        })?;
        let flow = header.flow_label & 0x000f_ffff;
        fixed.fill(0);
        fixed[0] = 0x60 | ((header.traffic_class >> 4) & 0x0f);
        fixed[1] = ((header.traffic_class & 0x0f) << 4) | ((flow >> 16) & 0x0f) as u8;
        fixed[2] = (flow >> 8) as u8;
        fixed[3] = flow as u8;
        fixed[4..6].copy_from_slice(&short_payload.to_be_bytes());
        fixed[6] = header.next_header;
        fixed[7] = header.hop_limit;
        fixed[8..24].copy_from_slice(&header.src.octets());
        fixed[24..40].copy_from_slice(&header.dst.octets());
        Ok(Self { buffer, total_len })
    }

    /// The payload area.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        match self.buffer.get_mut(HEADER_LEN..self.total_len) {
            Some(payload) => payload,
            None => &mut [],
        }
    }

    /// Finish the packet and return the written bytes.
    pub fn finish(self) -> &'a mut [u8] {
        match self.buffer.get_mut(..self.total_len) {
            Some(packet) => packet,
            None => &mut [],
        }
    }
}

/// Write an IPv6 extension header into `buffer` and return its length.
///
/// `kind` selects the extension; `next_header` is the value stored in the header's first octet.
/// `body_len` is the number of octets after the first two, and must be representable for the
/// chosen extension. The returned length includes the first two octets.
pub fn write_extension(
    buffer: &mut [u8],
    kind: ExtensionKind,
    next_header: u8,
    body_len: usize,
) -> Result<usize, ParseError> {
    let length = match kind {
        ExtensionKind::Fragment => {
            if body_len != 6 {
                return Err(ParseError::Malformed {
                    tag: "ipv6.ext_len",
                });
            }
            8
        }
        ExtensionKind::Authentication => {
            if body_len < 6 || !(body_len + 2).is_multiple_of(4) {
                return Err(ParseError::Malformed {
                    tag: "ipv6.ext_len",
                });
            }
            body_len + 2
        }
        ExtensionKind::HopByHop | ExtensionKind::Routing | ExtensionKind::Destination => {
            if body_len < 6 || !(body_len + 2).is_multiple_of(8) {
                return Err(ParseError::Malformed {
                    tag: "ipv6.ext_len",
                });
            }
            body_len + 2
        }
    };
    let Some(bytes) = buffer.get_mut(..length) else {
        return Err(ParseError::Truncated {
            tag: "ipv6.ext_len",
            need: length,
            got: buffer.len(),
        });
    };
    bytes.fill(0);
    bytes[0] = next_header;
    bytes[1] = match kind {
        ExtensionKind::Fragment => 0,
        ExtensionKind::Authentication => {
            u8::try_from(length / 4 - 2).map_err(|_| ParseError::Malformed {
                tag: "ipv6.ext_len",
            })?
        }
        ExtensionKind::HopByHop | ExtensionKind::Routing | ExtensionKind::Destination => {
            u8::try_from(length / 8 - 1).map_err(|_| ParseError::Malformed {
                tag: "ipv6.ext_len",
            })?
        }
    };
    Ok(length)
}

/// Write a fragment header body (six octets) and return the total extension length.
///
/// # Errors
/// Returns `ipv6.ext_len` when the offset is not a multiple of eight or does not fit 13 bits,
/// and `ipv6.truncated` when the buffer is too short.
pub fn write_fragment(
    buffer: &mut [u8],
    next_header: u8,
    offset: u16,
    more: bool,
    identification: u32,
) -> Result<usize, ParseError> {
    if !offset.is_multiple_of(8) || offset > 0xfff8 {
        return Err(ParseError::Malformed {
            tag: "ipv6.ext_len",
        });
    }
    let length = write_extension(buffer, ExtensionKind::Fragment, next_header, 6)?;
    let Some(bytes) = buffer.get_mut(..length) else {
        return Err(ParseError::Truncated {
            tag: "ipv6.ext_len",
            need: length,
            got: buffer.len(),
        });
    };
    let mut offset_flags = (offset / 8) << 3;
    if more {
        offset_flags |= 1;
    }
    if !write_u16(bytes, 2, offset_flags) || !write_u32(bytes, 4, identification) {
        return Err(ParseError::Truncated {
            tag: "ipv6.ext_len",
            need: length,
            got: bytes.len(),
        });
    }
    Ok(length)
}
