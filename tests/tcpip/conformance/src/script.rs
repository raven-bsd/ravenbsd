// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for the packetdrill-style script subset documented in `SCRIPTS.md`.

use std::fmt;

use crate::packet::PEER_PORT;

/// One parsed script line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Absolute virtual time in milliseconds.
    Time(i64),
    /// Relative virtual time in milliseconds.
    Wait(i64),
    /// A packet injected from the scripted peer into the stack under test.
    Inbound(PacketSpec),
    /// A packet expected from the stack under test.
    Expect(PacketSpec),
    /// A socket call.
    Syscall(Syscall),
}

/// A TCP packet template. Sequence and acknowledgement numbers are relative to
/// the peer ISN and the SUT ISN respectively, as in packetdrill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PacketSpec {
    /// Raw TCP flag byte.
    pub flags: u8,
    /// Relative sequence number.
    pub seq: u32,
    /// Whether any sequence number matches.
    pub seq_any: bool,
    /// Payload length.
    pub len: u32,
    /// Relative acknowledgement number, when present.
    pub ack: Option<u32>,
    /// Advertised window, when present.
    pub win: Option<u16>,
    /// Options that must be present.
    pub options: Vec<OptionSpec>,
}

/// One TCP option listed in a script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OptionSpec {
    /// Maximum segment size.
    Mss(u16),
    /// Window scale shift.
    Wscale(u8),
    /// SACK permitted.
    SackOk,
    /// Timestamps with any values.
    TimestampsAny,
    /// Timestamps.
    Timestamps {
        /// TSvalue.
        val: u32,
        /// TSecr.
        ecr: u32,
    },
    /// No-operation padding.
    Nop,
    /// End of option list.
    Eol,
    /// Any other option kind.
    Unknown(u8),
}

/// A parsed socket call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Syscall {
    /// `socket(...)`.
    Socket,
    /// `bind(...)`; the last numeric argument is the port.
    Bind {
        /// Bound port.
        port: u16,
    },
    /// `listen(...)`; the last numeric argument is the backlog.
    Listen {
        /// Backlog.
        backlog: usize,
    },
    /// `accept(...)`.
    Accept,
    /// `connect(...)`.
    Connect {
        /// Peer port.
        port: u16,
    },
    /// `write(...)` with a payload length.
    Write {
        /// Payload length.
        len: usize,
    },
    /// `read(...)` with a buffer length and an optional expected return value.
    Read {
        /// Buffer length.
        len: usize,
        /// Expected return value, when the script gives one.
        expected: Option<i64>,
    },
    /// `close(...)`.
    Close,
    /// `setsockopt(...)`; the first numeric argument after the fd is the option.
    SetSockOpt {
        /// Numeric option name, when present.
        option: Option<u64>,
    },
    /// `sysctl("tcp.name", value)` sets a stack property.
    Set {
        /// Property name.
        name: String,
        /// Property value.
        value: u64,
    },
    /// `peerport(port)` changes the scripted peer source port for later packets.
    PeerPort {
        /// Peer source port.
        port: u16,
    },
    /// `keepalive(interval_ms)` enables keepalive on the active socket.
    KeepAlive {
        /// Interval in milliseconds.
        interval: u64,
    },
    /// Any other call, ignored by the runner.
    Other {
        /// Call name.
        name: String,
    },
}

/// A parser error with the source line number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// One-based line number.
    pub line: usize,
    /// Human-readable reason.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Parse a complete script.
pub fn parse(text: &str) -> Result<Vec<Step>, ParseError> {
    let mut steps = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut remainder = line;
        if let Some((first, rest)) = line.split_once(char::is_whitespace) {
            let rest = rest.trim();
            if !rest.is_empty() {
                match first.strip_prefix('+') {
                    Some(number) => {
                        steps.push(Step::Wait(parse_seconds(number).map_err(|message| {
                            ParseError {
                                line: index + 1,
                                message,
                            }
                        })?));
                        remainder = rest;
                    }
                    None if first.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                        steps.push(Step::Time(parse_seconds(first).map_err(|message| {
                            ParseError {
                                line: index + 1,
                                message,
                            }
                        })?));
                        remainder = rest;
                    }
                    None => {}
                }
            }
        }
        let step = parse_line(remainder).map_err(|message| ParseError {
            line: index + 1,
            message,
        })?;
        steps.push(step);
    }
    Ok(steps)
}

fn parse_line(line: &str) -> Result<Step, String> {
    if line.starts_with('<') {
        return Ok(Step::Inbound(parse_packet(line)?));
    }
    if line.starts_with('>') {
        return Ok(Step::Expect(parse_packet(line)?));
    }
    if let Some(number) = line.strip_prefix('+') {
        return parse_seconds(number).map(Step::Wait);
    }
    if line.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return parse_seconds(line).map(Step::Time);
    }
    parse_syscall(line).map(Step::Syscall)
}

fn parse_seconds(text: &str) -> Result<i64, String> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("bad timestamp {text:?}"))?;
    if value < 0.0 {
        return Err(format!("negative timestamp {text:?}"));
    }
    Ok((value * 1000.0).round() as i64)
}

fn parse_packet(line: &str) -> Result<PacketSpec, String> {
    let mut rest = line[1..].trim();
    let (packet_text, option_text) = match rest.find('<') {
        Some(start) => {
            let end = rest.rfind('>').ok_or("unterminated option list")?;
            let options = rest[start + 1..end].trim().to_string();
            rest = rest[..start].trim();
            (rest, Some(options))
        }
        None => (rest, None),
    };
    let tokens: Vec<&str> = packet_text.split_whitespace().collect();
    let flags = parse_flags(tokens.first().ok_or("missing flags")?)?;
    let range = tokens.get(1).ok_or("missing sequence range")?;
    let (seq, len, seq_any) = if *range == "*" {
        (0, 0, true)
    } else {
        let (seq, end) = parse_range(range)?;
        (seq, end.wrapping_sub(seq), false)
    };
    let mut ack = None;
    let mut win = None;
    let mut index = 2;
    while index < tokens.len() {
        match tokens[index] {
            "ack" => {
                ack = Some(
                    tokens
                        .get(index + 1)
                        .ok_or("missing ack value")?
                        .parse()
                        .map_err(|_| "bad ack value")?,
                );
                index += 2;
            }
            "win" => {
                win = Some(
                    tokens
                        .get(index + 1)
                        .ok_or("missing win value")?
                        .parse()
                        .map_err(|_| "bad win value")?,
                );
                index += 2;
            }
            other => return Err(format!("unexpected packet token {other:?}")),
        }
    }
    let options = match option_text {
        Some(text) if !text.is_empty() => parse_options(&text)?,
        _ => Vec::new(),
    };
    Ok(PacketSpec {
        flags,
        seq,
        seq_any,
        len,
        ack,
        win,
        options,
    })
}

fn parse_flags(text: &str) -> Result<u8, String> {
    let mut flags = 0u8;
    for character in text.chars() {
        flags |= match character {
            'S' => 0x02,
            'F' => 0x01,
            'R' => 0x04,
            'P' => 0x08,
            '.' => 0x10,
            'E' => 0x40,
            'C' => 0x80,
            'U' => 0x20,
            other => return Err(format!("unknown flag {other:?}")),
        };
    }
    Ok(flags)
}

fn parse_range(text: &str) -> Result<(u32, u32), String> {
    let (range, length) = text
        .split_once('(')
        .ok_or("missing length in sequence range")?;
    let length: u32 = length
        .trim_end_matches(')')
        .parse()
        .map_err(|_| "bad range length")?;
    let (start, end) = range.split_once(':').ok_or("missing colon in range")?;
    let start: u32 = start.parse().map_err(|_| "bad start sequence")?;
    let end: u32 = end.parse().map_err(|_| "bad end sequence")?;
    if end.wrapping_sub(start) != length {
        return Err(format!("range {start}:{end}({length}) is inconsistent"));
    }
    Ok((start, end))
}

fn parse_options(text: &str) -> Result<Vec<OptionSpec>, String> {
    let mut options = Vec::new();
    for raw in text.split(',') {
        let option = raw.trim();
        if option.is_empty() {
            continue;
        }
        let lower = option.to_ascii_lowercase();
        if lower == "sackok" {
            options.push(OptionSpec::SackOk);
        } else if lower == "nop" {
            options.push(OptionSpec::Nop);
        } else if lower == "eol" {
            options.push(OptionSpec::Eol);
        } else if lower == "tsany" {
            options.push(OptionSpec::TimestampsAny);
        } else if lower == "ecn" {
            // ECN is a flag, handled by the caller as E/C.
        } else if let Some(value) = lower.strip_prefix("mss ") {
            options.push(OptionSpec::Mss(
                value.trim().parse().map_err(|_| "bad mss")?,
            ));
        } else if let Some(value) = lower.strip_prefix("wscale ") {
            options.push(OptionSpec::Wscale(
                value.trim().parse().map_err(|_| "bad wscale")?,
            ));
        } else if let Some(value) = lower.strip_prefix("ts ") {
            let (val, ecr) = value
                .split_once("ecr")
                .ok_or("bad timestamp option: missing ecr")?;
            let val = val
                .trim()
                .strip_prefix("val ")
                .ok_or("bad timestamp option: missing val")?
                .parse()
                .map_err(|_| "bad tsval")?;
            options.push(OptionSpec::Timestamps {
                val,
                ecr: ecr.trim().parse().map_err(|_| "bad tsecr")?,
            });
        } else {
            let kind: u8 = lower
                .split_whitespace()
                .next()
                .ok_or("empty option")?
                .parse()
                .map_err(|_| format!("unknown option {option:?}"))?;
            options.push(OptionSpec::Unknown(kind));
        }
    }
    Ok(options)
}

fn parse_syscall(line: &str) -> Result<Syscall, String> {
    let open = line.find('(').ok_or("missing '('")?;
    let close = line.rfind(')').ok_or("missing ')'")?;
    let name = line[..open].trim();
    let args = &line[open + 1..close];
    let numbers: Vec<i64> = args
        .split(',')
        .filter_map(|arg| arg.trim().parse::<i64>().ok())
        .collect();
    let expected = line[close + 1..]
        .trim()
        .strip_prefix('=')
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<i64>().ok());
    Ok(match name {
        "socket" => Syscall::Socket,
        "bind" => Syscall::Bind {
            port: numbers.last().copied().unwrap_or(8080) as u16,
        },
        "listen" => Syscall::Listen {
            backlog: numbers.last().copied().unwrap_or(1).max(1) as usize,
        },
        "accept" => Syscall::Accept,
        "connect" => Syscall::Connect {
            port: numbers.last().copied().unwrap_or(8080) as u16,
        },
        "write" => Syscall::Write {
            len: numbers.last().copied().unwrap_or(0).max(0) as usize,
        },
        "read" => Syscall::Read {
            len: numbers.last().copied().unwrap_or(0).max(0) as usize,
            expected,
        },
        "close" => Syscall::Close,
        "setsockopt" => Syscall::SetSockOpt {
            option: numbers.get(1).and_then(|value| u64::try_from(*value).ok()),
        },
        "sysctl" => {
            let name = args
                .split(',')
                .next()
                .map(str::trim)
                .map(|value| value.trim_matches('"').to_string())
                .ok_or("sysctl without a name")?;
            let value = numbers.last().copied().unwrap_or(0).max(0) as u64;
            Syscall::Set { name, value }
        }
        "peerport" => Syscall::PeerPort {
            port: numbers.last().copied().unwrap_or(PEER_PORT as i64) as u16,
        },
        "keepalive" => Syscall::KeepAlive {
            interval: numbers.last().copied().unwrap_or(0).max(0) as u64,
        },
        other => Syscall::Other {
            name: other.to_string(),
        },
    })
}

#[cfg(test)]
#[path = "script_ut.rs"]
mod tests;
