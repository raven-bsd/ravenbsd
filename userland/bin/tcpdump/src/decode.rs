// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Packet decoders and the tcpdump-compatible text printer.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::Timestamps;
use crate::pcap::{Packet, TimestampPrecision};

/// Options that affect the printed form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeOptions {
    /// Do not print timestamps, or print Unix seconds.
    pub timestamps: Timestamps,
    /// `-v` level.
    pub verbose: u8,
    /// `-S`: keep absolute TCP sequence numbers.
    pub absolute_seq: bool,
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            timestamps: Timestamps::None,
            verbose: 0,
            absolute_seq: false,
        }
    }
}

/// The per-run decoder state (currently the TCP initial-sequence map).
#[derive(Debug, Default)]
pub struct Decoder {
    sequences: BTreeMap<FlowKey, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Addr {
    V4([u8; 4]),
    V6([u8; 16]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FlowKey {
    src: Addr,
    sport: u16,
    dst: Addr,
    dport: u16,
}

impl Decoder {
    /// Create a decoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode one packet into zero or more output lines (without newlines).
    pub fn decode(&mut self, packet: &Packet<'_>, options: &DecodeOptions) -> Vec<String> {
        let prefix = timestamp_prefix(packet, options.timestamps);
        let mut lines = self.decode_frame(packet.data, options);
        if let Some(first) = lines.first_mut() {
            *first = format!("{prefix}{first}");
        }
        lines
    }

    fn decode_frame(&mut self, frame: &[u8], options: &DecodeOptions) -> Vec<String> {
        if frame.len() < 14 {
            return vec![String::from("[|ether]")];
        }
        let mut ether_type = read_be16(frame, 12).unwrap_or_default();
        let mut offset = 14usize;
        while ether_type == 0x8100 {
            let Some(inner) = frame.get(offset + 2..offset + 4) else {
                return vec![String::from("[|vlan]")];
            };
            ether_type = u16::from_be_bytes([inner[0], inner[1]]);
            offset += 4;
        }
        let payload = frame.get(offset..).unwrap_or(&[]);
        match ether_type {
            0x0800 => self.decode_ipv4(payload, options),
            0x86dd => self.decode_ipv6(payload, options),
            0x0806 => decode_arp(payload),
            other => vec![format!(
                "ethertype Unknown (0x{other:04x}), length {}",
                payload.len()
            )],
        }
    }

    fn decode_ipv4(&mut self, bytes: &[u8], options: &DecodeOptions) -> Vec<String> {
        if bytes.len() < 20 || bytes[0] >> 4 != 4 {
            return vec![String::from("[|ip]")];
        }
        let header_len = usize::from(bytes[0] & 0x0f) * 4;
        if header_len < 20 || header_len > bytes.len() {
            return vec![String::from("[|ip]")];
        }
        let total_len = usize::from(read_be16(bytes, 2).unwrap_or_default());
        if total_len != 0 && total_len < header_len {
            return vec![String::from("[|ip]")];
        }
        let end = if total_len == 0 {
            bytes.len()
        } else {
            total_len.min(bytes.len())
        };
        if end < header_len {
            return vec![String::from("[|ip]")];
        }
        let payload = &bytes[header_len..end];
        let fragment = read_be16(bytes, 6).unwrap_or_default();
        let fragment_offset = usize::from(fragment & 0x1fff) * 8;
        let protocol = bytes[9];
        let src = ipv4_text(&bytes[12..16]);
        let dst = ipv4_text(&bytes[16..20]);
        let mut lines = Vec::new();
        let indent = if options.verbose > 0 {
            let mut header = format!("IP (tos 0x{:x}", bytes[1]);
            let ttl = bytes[8];
            if ttl >= 1 {
                let _ = write!(header, ", ttl {ttl}");
            }
            let _ = write!(
                header,
                ", id {}, offset {}, flags [{}], proto {} ({}), length {}",
                read_be16(bytes, 4).unwrap_or_default(),
                fragment_offset,
                fragment_flags(fragment),
                protocol_name(protocol),
                protocol,
                total_len
            );
            if internet_checksum(&[&bytes[..header_len]]) != 0 {
                let _ = write!(
                    header,
                    ", bad cksum 0x{:04x}!",
                    read_be16(bytes, 10).unwrap_or_default()
                );
            }
            header.push(')');
            lines.push(header);
            String::from("    ")
        } else {
            String::new()
        };
        let line_prefix = if options.verbose > 0 {
            indent
        } else {
            String::from("IP ")
        };
        if fragment_offset != 0 {
            lines.push(format!("{line_prefix}{src} > {dst}: ip-proto-{protocol}"));
            return lines;
        }
        let transport_prefix = match protocol {
            6 | 17 => String::new(),
            _ => format!("{src} > {dst}: "),
        };
        let transport_line = match protocol {
            6 => {
                let source = addr_from_v4(&bytes[12..16]);
                let destination = addr_from_v4(&bytes[16..20]);
                self.decode_tcp(payload, source, destination, options)
            }
            17 => {
                let source = addr_from_v4(&bytes[12..16]);
                let destination = addr_from_v4(&bytes[16..20]);
                decode_udp(payload, source, destination)
            }
            1 => decode_icmp4(payload),
            _ => format!("ip-proto-{protocol}"),
        };
        lines.push(format!("{line_prefix}{transport_prefix}{transport_line}"));
        lines
    }

    fn decode_ipv6(&mut self, bytes: &[u8], options: &DecodeOptions) -> Vec<String> {
        if bytes.len() < 40 || bytes[0] >> 4 != 6 {
            return vec![String::from("[|ip6]")];
        }
        let payload_len = usize::from(read_be16(bytes, 4).unwrap_or_default());
        let end = if payload_len == 0 {
            bytes.len()
        } else {
            (40usize.saturating_add(payload_len)).min(bytes.len())
        };
        if end < 40 {
            return vec![String::from("[|ip6]")];
        }
        let src_bytes: [u8; 16] = match bytes[8..24].try_into() {
            Ok(value) => value,
            Err(_) => return vec![String::from("[|ip6]")],
        };
        let dst_bytes: [u8; 16] = match bytes[24..40].try_into() {
            Ok(value) => value,
            Err(_) => return vec![String::from("[|ip6]")],
        };
        let Some((protocol, transport)) = walk_ipv6(&bytes[..end]) else {
            return vec![String::from("[|ip6]")];
        };
        let src = format_ipv6(&src_bytes);
        let dst = format_ipv6(&dst_bytes);
        let mut lines = Vec::new();
        let indent = if options.verbose > 0 {
            let flow = read_be32(bytes, 0).unwrap_or_default();
            let mut header = String::from("IP6 (");
            let class = (flow & 0x0ff0_0000) >> 20;
            if class != 0 {
                let _ = write!(header, "class 0x{class:02x}, ");
            }
            let label = flow & 0x000f_ffff;
            if label != 0 {
                let _ = write!(header, "flowlabel 0x{label:05x}, ");
            }
            let _ = write!(
                header,
                "hlim {}, next-header {} ({}), payload length {})",
                bytes[7],
                protocol_name(protocol),
                protocol,
                payload_len
            );
            lines.push(header);
            String::from("    ")
        } else {
            String::new()
        };
        let line_prefix = if options.verbose > 0 {
            indent
        } else {
            String::from("IP6 ")
        };
        let Some(transport) = transport else {
            lines.push(format!("{line_prefix}{src} > {dst}: ip-proto-{protocol}"));
            return lines;
        };
        let source = Addr::V6(src_bytes);
        let destination = Addr::V6(dst_bytes);
        let transport_prefix = match protocol {
            6 | 17 => String::new(),
            _ => format!("{src} > {dst}: "),
        };
        let transport_line = match protocol {
            6 => self.decode_tcp(transport, source, destination, options),
            17 => decode_udp(transport, source, destination),
            58 => decode_icmp6(transport),
            _ => format!("ip-proto-{protocol}"),
        };
        lines.push(format!("{line_prefix}{transport_prefix}{transport_line}"));
        lines
    }

    fn decode_tcp(
        &mut self,
        bytes: &[u8],
        src: Addr,
        dst: Addr,
        options: &DecodeOptions,
    ) -> String {
        let source = format!(
            "{}.{}",
            addr_text(src),
            read_be16(bytes, 0).unwrap_or_default()
        );
        let destination = format!(
            "{}.{}",
            addr_text(dst),
            read_be16(bytes, 2).unwrap_or_default()
        );
        if bytes.len() < 20 {
            return format!("{source} > {destination}: tcp [|tcp]");
        }
        let header_len = usize::from(bytes[12] >> 4) * 4;
        if header_len < 20 || header_len > bytes.len() {
            return format!(
                "{source} > {destination}: tcp [bad hdr length {} - too long, > {}]",
                header_len,
                bytes.len()
            );
        }
        let sequence = read_be32(bytes, 4).unwrap_or_default();
        let acknowledgement = read_be32(bytes, 8).unwrap_or_default();
        let flags = bytes[13];
        let window = read_be16(bytes, 14).unwrap_or_default();
        let urgent = read_be16(bytes, 18).unwrap_or_default();
        let payload_len = bytes.len().saturating_sub(header_len);
        let mut shown_sequence = sequence;
        let mut shown_acknowledgement = acknowledgement;
        if !options.absolute_seq {
            let key = FlowKey {
                src,
                sport: read_be16(bytes, 0).unwrap_or_default(),
                dst,
                dport: read_be16(bytes, 2).unwrap_or_default(),
            };
            if flags & 0x02 != 0 {
                self.sequences.insert(key, sequence);
            } else {
                if let Some(base) = self.sequences.get(&key) {
                    shown_sequence = sequence.wrapping_sub(*base);
                }
                let peer = FlowKey {
                    src: dst,
                    sport: key.dport,
                    dst: src,
                    dport: key.sport,
                };
                if let Some(base) = self.sequences.get(&peer) {
                    shown_acknowledgement = acknowledgement.wrapping_sub(*base);
                }
            }
        }
        let mut line = format!(
            "{source} > {destination}: Flags [{}]",
            format_tcp_flags(flags)
        );
        if options.verbose > 1 || payload_len > 0 || flags & (0x02 | 0x01 | 0x04) != 0 {
            let _ = write!(line, ", seq {shown_sequence}");
            if payload_len > 0 {
                let _ = write!(line, ":{}", shown_sequence.wrapping_add(payload_len as u32));
            }
        }
        if flags & 0x10 != 0 {
            let _ = write!(line, ", ack {shown_acknowledgement}");
        }
        let _ = write!(line, ", win {window}");
        if flags & 0x20 != 0 {
            let _ = write!(line, ", urg {urgent}");
        }
        if header_len > 20
            && let Some(options_text) = tcp_options(&bytes[20..header_len])
        {
            let _ = write!(line, ", options [{options_text}]");
        }
        let _ = write!(line, ", length {payload_len}");
        if payload_len > 0
            && let Some(summary) = payload_summary(&bytes[header_len..])
        {
            let _ = write!(line, ": {summary}");
        }
        line
    }
}

fn timestamp_prefix(packet: &Packet<'_>, timestamps: Timestamps) -> String {
    match timestamps {
        Timestamps::None => String::new(),
        Timestamps::Unix => match packet.precision {
            TimestampPrecision::Microseconds => {
                format!("{}.{:06} ", packet.ts_sec, packet.ts_frac)
            }
            TimestampPrecision::Nanoseconds => {
                format!("{}.{:09} ", packet.ts_sec, packet.ts_frac)
            }
        },
    }
}

/// Render an IPv6 address with the longest zero run compressed to `::` (RFC 5952).
#[must_use]
pub fn format_ipv6(address: &[u8; 16]) -> String {
    let mut groups = [0u16; 8];
    for (index, group) in groups.iter_mut().enumerate() {
        *group = u16::from_be_bytes([address[index * 2], address[index * 2 + 1]]);
    }
    let mut best_start = 0usize;
    let mut best_len = 0usize;
    let mut index = 0usize;
    while index < groups.len() {
        if groups[index] == 0 {
            let start = index;
            while index < groups.len() && groups[index] == 0 {
                index += 1;
            }
            let length = index - start;
            if length > best_len && length >= 2 {
                best_start = start;
                best_len = length;
            }
        } else {
            index += 1;
        }
    }
    if best_len == 0 {
        return groups
            .iter()
            .map(|group| format!("{group:x}"))
            .collect::<Vec<_>>()
            .join(":");
    }
    let mut output = String::new();
    let mut position = 0usize;
    while position < groups.len() {
        if position == best_start {
            output.push_str("::");
            position += best_len;
            if position == groups.len() {
                break;
            }
        } else {
            if !output.is_empty() && !output.ends_with(':') {
                output.push(':');
            }
            let _ = write!(output, "{:x}", groups[position]);
            position += 1;
        }
    }
    output
}

/// Render the TCP flag set in tcpdump's `S.`/`P.` form.
#[must_use]
pub fn format_tcp_flags(flags: u8) -> String {
    let mut output = String::new();
    if flags & 0x01 != 0 {
        output.push('F');
    }
    if flags & 0x02 != 0 {
        output.push('S');
    }
    if flags & 0x04 != 0 {
        output.push('R');
    }
    if flags & 0x08 != 0 {
        output.push('P');
    }
    if flags & 0x10 != 0 {
        output.push('.');
    }
    if flags & 0x20 != 0 {
        output.push('U');
    }
    if flags & 0x40 != 0 {
        output.push('E');
    }
    if flags & 0x80 != 0 {
        output.push('W');
    }
    if output.is_empty() {
        output.push_str("none");
    }
    output
}

fn decode_arp(bytes: &[u8]) -> Vec<String> {
    if bytes.len() < 28 {
        return vec![String::from("[|arp]")];
    }
    let hardware = read_be16(bytes, 0).unwrap_or_default();
    let protocol = read_be16(bytes, 2).unwrap_or_default();
    let hardware_len = bytes[4];
    let protocol_len = bytes[5];
    let operation = read_be16(bytes, 6).unwrap_or_default();
    if hardware != 1 || protocol != 0x0800 || hardware_len != 6 || protocol_len != 4 {
        return vec![format!(
            "ARP, Unknown ({operation}), length {}",
            bytes.len()
        )];
    }
    let sender_hardware = mac_text(&bytes[8..14]);
    let sender_protocol = ipv4_text(&bytes[14..18]);
    let target_hardware = &bytes[18..24];
    let target_protocol = ipv4_text(&bytes[24..28]);
    let line = match operation {
        1 => {
            let mut line = format!("ARP, Request who-has {target_protocol} tell {sender_protocol}");
            if target_hardware.iter().any(|byte| *byte != 0) {
                let _ = write!(line, " ({})", mac_text(target_hardware));
            }
            let _ = write!(line, ", length {}", bytes.len());
            line
        }
        2 => format!(
            "ARP, Reply {sender_protocol} is-at {sender_hardware}, length {}",
            bytes.len()
        ),
        _ => format!("ARP, Unknown ({operation}), length {}", bytes.len()),
    };
    vec![line]
}

fn decode_udp(bytes: &[u8], src: Addr, dst: Addr) -> String {
    let source = read_be16(bytes, 0).unwrap_or_default();
    let destination = read_be16(bytes, 2).unwrap_or_default();
    let prefix = format!(
        "{}.{} > {}.{}: ",
        addr_text(src),
        source,
        addr_text(dst),
        destination
    );
    if bytes.len() < 8 {
        return format!("{prefix}truncated-udp {}", bytes.len());
    }
    let declared = usize::from(read_be16(bytes, 4).unwrap_or_default());
    if declared < 8 {
        return format!("{prefix}truncated-udplength {declared}");
    }
    let payload_len = declared - 8;
    let payload = &bytes[8..];
    if (source == 53 || destination == 53 || source == 5353 || destination == 5353)
        && let Some(dns) = decode_dns(payload, payload_len)
    {
        return format!("{prefix}{dns}");
    }
    format!("{prefix}UDP, length {payload_len}")
}

fn decode_dns(payload: &[u8], display_len: usize) -> Option<String> {
    if payload.len() < 12 {
        return None;
    }
    let id = read_be16(payload, 0)?;
    let flags = read_be16(payload, 2)?;
    let questions = read_be16(payload, 4)?;
    let answers = read_be16(payload, 6)?;
    let authority = read_be16(payload, 8)?;
    let additional = read_be16(payload, 10)?;
    if questions > 0 && answers == 0 {
        let mut offset = 12usize;
        let name = dns_name(payload, &mut offset)?;
        let qtype = read_be16(payload, offset)?;
        let recursion = if flags & 0x0100 != 0 { "+" } else { "" };
        return Some(format!(
            "{id}{recursion} {}? {name} ({display_len})",
            dns_type(qtype)
        ));
    }
    if answers == 0 {
        return None;
    }
    let mut offset = 12usize;
    for _ in 0..questions {
        let _ = dns_name(payload, &mut offset)?;
        offset = offset.checked_add(4)?;
    }
    let _ = dns_name(payload, &mut offset)?;
    let qtype = read_be16(payload, offset)?;
    offset = offset.checked_add(8)?;
    let rdata_len = usize::from(read_be16(payload, offset)?);
    offset = offset.checked_add(2)?;
    let rdata = payload.get(offset..offset.checked_add(rdata_len)?)?;
    let value = match qtype {
        1 if rdata.len() == 4 => ipv4_text(rdata),
        28 if rdata.len() == 16 => {
            let address: [u8; 16] = rdata.try_into().ok()?;
            format_ipv6(&address)
        }
        _ => format!("{rdata_len} bytes"),
    };
    Some(format!(
        "{id} {answers}/{authority}/{additional} {} {value} ({display_len})",
        dns_type(qtype)
    ))
}

fn dns_name(payload: &[u8], offset: &mut usize) -> Option<String> {
    let mut name = String::new();
    let mut guard = 0usize;
    loop {
        if guard > 128 {
            return None;
        }
        guard += 1;
        let length = *payload.get(*offset)?;
        if length == 0 {
            *offset = offset.checked_add(1)?;
            return Some(name);
        }
        if length & 0xc0 == 0xc0 {
            let pointer =
                usize::from(length & 0x3f) * 256 + usize::from(*payload.get(*offset + 1)?);
            *offset = offset.checked_add(2)?;
            let _ = pointer;
            return Some(name);
        }
        let length = usize::from(length);
        let start = offset.checked_add(1)?;
        let end = start.checked_add(length)?;
        let label = payload.get(start..end)?;
        for byte in label {
            if !byte.is_ascii() {
                return None;
            }
        }
        name.push_str(core::str::from_utf8(label).ok()?);
        name.push('.');
        *offset = end;
    }
}

fn dns_type(value: u16) -> String {
    match value {
        1 => String::from("A"),
        2 => String::from("NS"),
        5 => String::from("CNAME"),
        12 => String::from("PTR"),
        15 => String::from("MX"),
        16 => String::from("TXT"),
        28 => String::from("AAAA"),
        other => format!("TYPE{other}"),
    }
}

fn decode_icmp4(bytes: &[u8]) -> String {
    if bytes.len() < 8 {
        return String::from("ICMP [|icmp]");
    }
    let kind = bytes[0];
    let identifier = read_be16(bytes, 4).unwrap_or_default();
    let sequence = read_be16(bytes, 6).unwrap_or_default();
    match kind {
        0 => format!(
            "ICMP echo reply, id {identifier}, seq {sequence}, length {}",
            bytes.len()
        ),
        8 => format!(
            "ICMP echo request, id {identifier}, seq {sequence}, length {}",
            bytes.len()
        ),
        3 => String::from("ICMP unreachable"),
        11 => String::from("ICMP time exceeded"),
        other => format!("ICMP type {other}, length {}", bytes.len()),
    }
}

fn decode_icmp6(bytes: &[u8]) -> String {
    if bytes.len() < 8 {
        return String::from("ICMP6 [|icmp6]");
    }
    let kind = bytes[0];
    let identifier = read_be16(bytes, 4).unwrap_or_default();
    let sequence = read_be16(bytes, 6).unwrap_or_default();
    match kind {
        128 => format!(
            "ICMP6, echo request, id {identifier}, seq {sequence}, length {}",
            bytes.len()
        ),
        129 => format!(
            "ICMP6, echo reply, id {identifier}, seq {sequence}, length {}",
            bytes.len()
        ),
        other => format!("ICMP6, type {other}, length {}", bytes.len()),
    }
}

fn tcp_options(bytes: &[u8]) -> Option<String> {
    let mut options = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        let kind = bytes[offset];
        match kind {
            0 => {
                options.push(String::from("eol"));
                break;
            }
            1 => {
                options.push(String::from("nop"));
                offset += 1;
            }
            _ => {
                let Some(length) = bytes.get(offset + 1).copied().map(usize::from) else {
                    options.push(String::from("bad"));
                    break;
                };
                if length < 2 || offset.saturating_add(length) > bytes.len() {
                    options.push(String::from("bad"));
                    break;
                }
                let value = match kind {
                    2 if length == 4 => {
                        format!("mss {}", read_be16(bytes, offset + 2).unwrap_or_default())
                    }
                    3 if length == 3 => format!("wscale {}", bytes[offset + 2]),
                    4 if length == 2 => String::from("sackOK"),
                    5 => String::from("sack"),
                    8 if length == 10 => format!(
                        "TS {} {}",
                        read_be32(bytes, offset + 2).unwrap_or_default(),
                        read_be32(bytes, offset + 6).unwrap_or_default()
                    ),
                    _ => format!("unknown-{kind}"),
                };
                options.push(value);
                offset = offset.saturating_add(length);
            }
        }
    }
    if options.is_empty() {
        None
    } else {
        Some(options.join(","))
    }
}

fn payload_summary(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == b'\r' || *byte == b'\n')
        .unwrap_or(bytes.len());
    let line = bytes.get(..end)?;
    if line.is_empty() || !line.iter().all(|byte| (0x20..=0x7e).contains(byte)) {
        return None;
    }
    let text = core::str::from_utf8(line).ok()?;
    if text.starts_with("GET ")
        || text.starts_with("POST ")
        || text.starts_with("HEAD ")
        || text.starts_with("HTTP/")
    {
        Some(format!("HTTP: {text}"))
    } else {
        Some(text.to_string())
    }
}

fn walk_ipv6(payload: &[u8]) -> Option<(u8, Option<&[u8]>)> {
    let mut protocol = *payload.get(6)?;
    let mut cursor = 40usize;
    loop {
        match protocol {
            0 | 43 | 60 => {
                let next = *payload.get(cursor)?;
                let length = usize::from(*payload.get(cursor + 1)?)
                    .checked_add(1)?
                    .checked_mul(8)?;
                cursor = cursor.checked_add(length)?;
                protocol = next;
            }
            44 => {
                let next = *payload.get(cursor)?;
                let flags = read_be16(payload, cursor + 2)?;
                cursor = cursor.checked_add(8)?;
                protocol = next;
                if flags & 0xfff8 != 0 {
                    return Some((protocol, None));
                }
            }
            51 => {
                let next = *payload.get(cursor)?;
                let length = usize::from(*payload.get(cursor + 1)?)
                    .checked_add(2)?
                    .checked_mul(4)?;
                cursor = cursor.checked_add(length)?;
                protocol = next;
            }
            _ => return Some((protocol, payload.get(cursor..))),
        }
        if cursor > payload.len() {
            return None;
        }
    }
}

fn fragment_flags(fragment: u16) -> String {
    let mut flags = Vec::new();
    if fragment & 0x2000 != 0 {
        flags.push("+");
    }
    if fragment & 0x4000 != 0 {
        flags.push("DF");
    }
    if fragment & 0x8000 != 0 {
        flags.push("rsvd");
    }
    if flags.is_empty() {
        String::from("none")
    } else {
        flags.join(",")
    }
}

fn protocol_name(protocol: u8) -> &'static str {
    match protocol {
        1 => "ICMP",
        6 => "TCP",
        17 => "UDP",
        58 => "ICMP6",
        _ => "unknown",
    }
}

fn addr_text(address: Addr) -> String {
    match address {
        Addr::V4(octets) => ipv4_text(&octets),
        Addr::V6(octets) => format_ipv6(&octets),
    }
}

fn addr_from_v4(bytes: &[u8]) -> Addr {
    match bytes.try_into() {
        Ok(octets) => Addr::V4(octets),
        Err(_) => Addr::V4([0; 4]),
    }
}

fn ipv4_text(bytes: &[u8]) -> String {
    match bytes {
        [a, b, c, d] => format!("{a}.{b}.{c}.{d}"),
        _ => String::from("?"),
    }
}

fn mac_text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn read_be16(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([pair[0], pair[1]]))
}

fn read_be32(bytes: &[u8], offset: usize) -> Option<u32> {
    let word = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([word[0], word[1], word[2], word[3]]))
}

fn internet_checksum(parts: &[&[u8]]) -> u16 {
    raven_tcpip_wire::internet_checksum(parts)
}

#[cfg(test)]
#[path = "decode_ut.rs"]
mod tests;
