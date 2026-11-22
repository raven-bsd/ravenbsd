// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `tcpdump` tool.
//!
//! The tool is an offline classic-pcap decoder for the subset exercised by the K277
//! fixture matrix: Ethernet II, ARP, IPv4, IPv6, TCP, UDP with the DNS query/answer
//! display, ICMPv4 and ICMPv6. The pcap reader, the printer and the filter parser are
//! original Rust; the FreeBSD/NetBSD tcpdump and libpcap files consulted for behaviour
//! carry a 4-clause advertising grant, so they are read-only donors (see
//! `docs/tcpdump.md` and the K281 report).

#![cfg_attr(target_os = "none", no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod cli;
pub mod decode;
pub mod filter;
pub mod pcap;

use alloc::string::String;
use alloc::vec::Vec;

/// The timestamp display selected by `-t`/`-tt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timestamps {
    /// Do not print a timestamp.
    None,
    /// Print seconds since the Unix epoch with the savefile fraction.
    Unix,
}

/// A failure before any packet can be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The capture file is malformed or unsupported.
    Pcap(pcap::PcapError),
    /// The filter expression does not compile.
    Filter(filter::FilterError),
    /// Neither `-r` nor `-i` was supplied.
    MissingInput,
    /// Live capture is not available in the userspace tool.
    LiveCaptureUnsupported(String),
}

impl core::fmt::Display for RunError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Pcap(error) => write!(f, "{error}"),
            Self::Filter(error) => write!(f, "{error}"),
            Self::MissingInput => f.write_str("no capture source: use -r <file> or -i <interface>"),
            Self::LiveCaptureUnsupported(interface) => write!(
                f,
                "live capture on {interface} is not available; use -r <pcap file>"
            ),
        }
    }
}

/// The visible result of one offline run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RunOutput {
    /// Decoded packet lines, newline-terminated.
    pub text: String,
    /// A filtered classic-pcap savefile when `-w` was requested.
    pub dump: Option<Vec<u8>>,
    /// Number of packets that passed the filter.
    pub selected: usize,
}

/// Decode one classic-pcap byte image with the parsed command line.
///
/// # Errors
/// Returns [`RunError`] when there is no capture source, the savefile is
/// malformed, the link type is unsupported or the filter does not compile.
pub fn execute(input: &[u8], options: &cli::Options) -> Result<RunOutput, RunError> {
    if options.read.is_none() {
        return match &options.interface {
            Some(interface) => Err(RunError::LiveCaptureUnsupported(interface.clone())),
            None => Err(RunError::MissingInput),
        };
    }
    let file = pcap::parse(input).map_err(RunError::Pcap)?;
    let filter = match options.filter.as_deref() {
        Some(expression) => Some(filter::compile(expression).map_err(RunError::Filter)?),
        None => None,
    };
    let decode_options = decode::DecodeOptions {
        timestamps: options.timestamps,
        verbose: options.verbose,
        absolute_seq: options.absolute_seq,
    };
    let mut decoder = decode::Decoder::new();
    let mut output = RunOutput::default();
    let mut dumped = Vec::new();
    for packet in &file.packets {
        if let Some(filter) = &filter
            && !filter.matches(packet.data)
        {
            continue;
        }
        for line in decoder.decode(packet, &decode_options) {
            output.text.push_str(&line);
            output.text.push('\n');
        }
        output.selected += 1;
        if options.write.is_some() {
            dumped.push(*packet);
        }
        if options.count.is_some_and(|count| output.selected >= count) {
            break;
        }
    }
    if options.write.is_some() {
        output.dump = Some(pcap::write(&dumped, file.link_type, file.snaplen));
    }
    Ok(output)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
