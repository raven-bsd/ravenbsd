// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parser for the small packetdrill-compatible script subset used here.
//!
//! Lines are `<time> <command>` where a command is either a TCP packet (`<` incoming, `>`
//! expected outgoing) or a limited socket syscall. Times are relative seconds (`+0`, `+.1`).

use anyhow::{Context, Result, bail};

use crate::frame::{ACK, FIN, PSH, RST, SYN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PacketSpec {
    pub flags: u8,
    pub seq_start: u32,
    pub seq_len: usize,
    pub ack: Option<u32>,
    pub window: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Syscall {
    Socket {
        fd: u32,
        expected: isize,
    },
    Bind {
        expected: isize,
    },
    Listen {
        fd: u32,
        backlog: usize,
        expected: isize,
    },
    Accept {
        fd: u32,
        new_fd: u32,
        expected: isize,
    },
    Connect {
        fd: u32,
        expected: isize,
    },
    Read {
        fd: u32,
        len: usize,
        expected: isize,
        error: Option<String>,
    },
    Write {
        fd: u32,
        len: usize,
        expected: isize,
        error: Option<String>,
    },
    Close {
        fd: u32,
        expected: isize,
    },
    Ignore {
        expected: isize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventKind {
    Packet {
        direction: Direction,
        spec: PacketSpec,
    },
    Syscall(Syscall),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub delay_ms: i64,
    pub kind: EventKind,
}

#[derive(Clone, Debug)]
pub struct Script {
    pub name: String,
    pub events: Vec<Event>,
}

pub fn parse(name: &str, text: &str) -> Result<Script> {
    let mut events = Vec::new();
    let mut now_ms = 0i64;
    for (index, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() || line.starts_with("--") {
            continue;
        }
        let (time, command) = line
            .split_once(char::is_whitespace)
            .with_context(|| format!("{}:{}: missing command", name, index + 1))?;
        let (relative, value_ms) = parse_time(time)
            .with_context(|| format!("{}:{}: invalid timestamp `{time}`", name, index + 1))?;
        let delay_ms = if relative {
            value_ms
        } else {
            value_ms - now_ms
        };
        if delay_ms < 0 {
            bail!(
                "{}:{}: absolute timestamp `{time}` moves backwards",
                name,
                index + 1
            );
        }
        now_ms += delay_ms;
        let command = command.trim();
        let kind = if let Some(rest) = command.strip_prefix('<') {
            EventKind::Packet {
                direction: Direction::Incoming,
                spec: parse_packet(rest.trim())
                    .with_context(|| format!("{}:{}: bad incoming packet", name, index + 1))?,
            }
        } else if let Some(rest) = command.strip_prefix('>') {
            EventKind::Packet {
                direction: Direction::Outgoing,
                spec: parse_packet(rest.trim())
                    .with_context(|| format!("{}:{}: bad outgoing packet", name, index + 1))?,
            }
        } else {
            EventKind::Syscall(parse_syscall(command).with_context(|| {
                format!("{}:{}: unsupported syscall `{command}`", name, index + 1)
            })?)
        };
        events.push(Event { delay_ms, kind });
    }
    if events.is_empty() {
        bail!("{name}: script has no events");
    }
    Ok(Script {
        name: name.to_string(),
        events,
    })
}

fn strip_comment(line: &str) -> &str {
    line.split_once("//").map(|(line, _)| line).unwrap_or(line)
}

fn parse_time(text: &str) -> Result<(bool, i64)> {
    let (relative, text) = match text.strip_prefix('+') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let text = text.split("...").next().unwrap_or(text);
    let seconds: f64 = text.parse()?;
    if !seconds.is_finite() || seconds < 0.0 {
        bail!("negative or non-finite time");
    }
    Ok((relative, (seconds * 1000.0).round() as i64))
}

fn parse_packet(text: &str) -> Result<PacketSpec> {
    let mut tokens = text.split_whitespace();
    let flags_text = tokens.next().context("missing TCP flags")?;
    let flags = parse_flags(flags_text)?;
    let sequence = tokens.next().context("missing sequence range")?;
    let (seq_start, seq_len) = parse_sequence(sequence)?;
    let mut ack = None;
    let mut window = 65535;
    let mut tokens = tokens.peekable();
    while let Some(token) = tokens.next() {
        match token {
            "ack" => {
                let value = tokens.next().context("missing ack value")?;
                ack = Some(value.parse().context("bad ack value")?);
            }
            "win" => {
                let value = tokens.next().context("missing window value")?;
                window = value.parse().context("bad window value")?;
            }
            _ if token.starts_with('<') => break,
            _ => {}
        }
    }
    Ok(PacketSpec {
        flags,
        seq_start,
        seq_len,
        ack,
        window,
    })
}

fn parse_flags(text: &str) -> Result<u8> {
    let mut flags = 0;
    for character in text.chars() {
        match character {
            'S' => flags |= SYN,
            '.' => flags |= ACK,
            'P' => flags |= PSH,
            'F' => flags |= FIN,
            'R' => flags |= RST,
            _ => bail!("unknown TCP flag `{character}`"),
        }
    }
    Ok(flags)
}

fn parse_sequence(text: &str) -> Result<(u32, usize)> {
    let (start, end) = text.split_once(':').context("missing `:` in sequence")?;
    let start: u32 = start.parse().context("bad sequence start")?;
    let (end, length) = match end.split_once('(') {
        Some((end, length)) => (
            end.parse::<u32>().context("bad sequence end")?,
            Some(
                length
                    .strip_suffix(')')
                    .context("unterminated sequence length")?
                    .parse::<usize>()
                    .context("bad sequence length")?,
            ),
        ),
        None => (end.parse().context("bad sequence end")?, None),
    };
    let derived = end.saturating_sub(start) as usize;
    let length = length.unwrap_or(derived);
    if length != derived {
        bail!("sequence length does not match range");
    }
    Ok((start, length))
}

fn parse_syscall(text: &str) -> Result<Syscall> {
    let (call, result) = text
        .split_once('=')
        .context("missing `= <expected result>`")?;
    let call = call.trim();
    let name = call
        .split_once('(')
        .map(|(name, _)| name.trim())
        .context("missing `(`")?;
    let args_text = call
        .strip_suffix(')')
        .and_then(|call| call.split_once('('))
        .map(|(_, args)| args)
        .context("missing `)`")?;
    let args: Vec<&str> = args_text.split(',').map(str::trim).collect();
    let (expected, error) = parse_expected(result.trim())?;

    Ok(match name {
        "socket" => Syscall::Socket {
            fd: expected.max(0) as u32,
            expected,
        },
        "bind" | "setsockopt" => Syscall::Ignore { expected },
        "listen" => Syscall::Listen {
            fd: arg_u32(&args, 0)?,
            backlog: arg_usize(&args, 1)?,
            expected,
        },
        "accept" => Syscall::Accept {
            fd: arg_u32(&args, 0)?,
            new_fd: expected.max(0) as u32,
            expected,
        },
        "connect" => Syscall::Connect {
            fd: arg_u32(&args, 0)?,
            expected,
        },
        "read" => Syscall::Read {
            fd: arg_u32(&args, 0)?,
            len: arg_last_usize(&args)?,
            expected,
            error,
        },
        "write" => Syscall::Write {
            fd: arg_u32(&args, 0)?,
            len: arg_last_usize(&args)?,
            expected,
            error,
        },
        "close" => Syscall::Close {
            fd: arg_u32(&args, 0)?,
            expected,
        },
        other => bail!("unknown syscall `{other}`"),
    })
}

fn parse_expected(text: &str) -> Result<(isize, Option<String>)> {
    let mut words = text.split_whitespace();
    let value: isize = words.next().context("missing result")?.parse()?;
    Ok((value, words.next().map(str::to_string)))
}

fn arg_u32(args: &[&str], index: usize) -> Result<u32> {
    args.get(index)
        .context("missing argument")?
        .parse()
        .context("bad integer argument")
}

fn arg_usize(args: &[&str], index: usize) -> Result<usize> {
    args.get(index)
        .context("missing argument")?
        .parse()
        .context("bad integer argument")
}

fn arg_last_usize(args: &[&str]) -> Result<usize> {
    args.last()
        .context("missing length argument")?
        .parse()
        .context("bad length argument")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_and_relative_times_accumulate() {
        let script = parse(
            "x",
            "0.100 socket(..., SOCK_STREAM, IPPROTO_TCP) = 3\n\
             +0.050 listen(3, 1) = 0\n\
             0.300 accept(3, ..., ...) = 4\n",
        )
        .unwrap();
        assert_eq!(script.events[0].delay_ms, 100);
        assert_eq!(script.events[1].delay_ms, 50);
        assert_eq!(script.events[2].delay_ms, 150);
    }

    #[test]
    fn absolute_time_moving_backwards_is_rejected() {
        assert!(
            parse(
                "x",
                "0.300 socket(..., SOCK_STREAM, IPPROTO_TCP) = 3\n\
                 0.100 listen(3, 1) = 0\n",
            )
            .is_err()
        );
    }

    #[test]
    fn parses_handshake_events() {
        let script = parse(
            "x",
            "0 socket(..., SOCK_STREAM, IPPROTO_TCP) = 3\n\
             +0 listen(3, 1) = 0\n\
             +.1 < S 0:0(0) win 65535\n\
             +0 > S. 0:0(0) ack 1 <mss 1460>\n\
             +0 < . 1:1(0) ack 1 win 65535\n\
             +0 accept(3, ..., ...) = 4\n",
        )
        .unwrap();
        assert_eq!(script.events.len(), 6);
        assert_eq!(script.events[2].delay_ms, 100);
        assert_eq!(
            script.events[2].kind,
            EventKind::Packet {
                direction: Direction::Incoming,
                spec: PacketSpec {
                    flags: SYN,
                    seq_start: 0,
                    seq_len: 0,
                    ack: None,
                    window: 65535,
                },
            }
        );
    }
}
