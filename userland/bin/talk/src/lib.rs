// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `talk` protocol and split-screen model.
//!
//! The library is transport-neutral: a caller and a listener exchange the bounded `RVTALK/1`
//! frames defined here. The target wrapper only needs a duplex byte channel and the local
//! terminal; the tests drive both ends with an in-memory loopback. No pids, `wait` or signal
//! ownership is involved (DR-46).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Protocol marker on every wire line.
pub const PROTOCOL: &str = "RVTALK/1";
/// Longest accepted wire line.
pub const MAX_FRAME: usize = 1024;
/// Longest user or host name.
pub const MAX_ADDRESS: usize = 64;
/// Longest terminal name.
pub const MAX_TERMINAL: usize = 64;
/// Longest forwarded text field.
pub const MAX_DATA: usize = 256;
/// Longest decline reason.
pub const MAX_REASON: usize = 128;
/// Width used by the split-screen helper.
pub const SCREEN_WIDTH: usize = 80;

/// Usage line shared by the host and target wrappers.
pub const USAGE: &str = "talk [-l] address [terminal]";

/// One `user[@host]` address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    /// User name.
    pub user: String,
    /// Optional host name.
    pub host: Option<String>,
}

impl Address {
    /// Render the address back to `user` or `user@host`.
    #[must_use]
    pub fn text(&self) -> String {
        match &self.host {
            Some(host) => format!("{}@{host}", self.user),
            None => self.user.clone(),
        }
    }
}

/// Parsed `talk` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Listener mode (`-l`): wait for an invitation instead of sending one.
    pub listen: bool,
    /// Caller address; absent in listener mode.
    pub address: Option<Address>,
    /// Requested recipient terminal.
    pub terminal: Option<String>,
}

/// Why a command line is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option letter is not supported.
    UnknownOption(char),
    /// The address operand is missing.
    MissingAddress,
    /// Too many operands were supplied.
    ExtraOperand,
    /// The address is empty or contains unsupported characters.
    BadAddress,
    /// The terminal name is empty or contains unsupported characters.
    BadTerminal,
}

/// One protocol frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// Caller invitation with the caller address and requested terminal.
    Invite {
        /// Caller address.
        caller: Address,
        /// Requested recipient terminal.
        terminal: Option<String>,
    },
    /// Listener accepted the invitation.
    Accept,
    /// Listener declined the invitation.
    Decline {
        /// Human-readable reason.
        reason: String,
    },
    /// Forwarded printable text.
    Data {
        /// Text bytes, escaped on the wire.
        text: String,
    },
    /// Alert the recipient's terminal.
    Alert,
    /// The sender will send no more input.
    End,
    /// The sender is terminating the session.
    Bye,
}

/// Why a frame is malformed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    /// The line is longer than [`MAX_FRAME`].
    TooLong,
    /// The protocol marker is missing.
    BadPrefix,
    /// The verb is not one of the defined frames.
    UnknownFrame,
    /// The address field is invalid.
    BadAddress,
    /// The text, terminal or percent escape is invalid.
    BadText,
}

impl ProtocolError {
    /// Stable human-readable description.
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::TooLong => "protocol line is too long",
            Self::BadPrefix => "protocol marker is missing",
            Self::UnknownFrame => "unknown protocol frame",
            Self::BadAddress => "invalid talk address",
            Self::BadText => "invalid protocol text",
        }
    }
}

/// One endpoint role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The side that sends the invitation.
    Caller,
    /// The side that receives the invitation.
    Listener,
}

/// Session state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Waiting for the invitation (listener) or acceptance (caller).
    Inviting,
    /// The listener has an invitation to accept or decline.
    Invited,
    /// Both sides accepted; text can flow.
    Active,
    /// Local input ended; the peer may still exit.
    LocalClosed,
    /// The peer ended its input; the local side may only exit.
    RemoteClosed,
    /// The session is over.
    Closed,
}

/// One action for the terminal driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Send this frame to the peer.
    Send(Frame),
    /// Show the invitation on the listener's terminal.
    ShowInvitation(String),
    /// Both sides are active; draw the split screen.
    Start,
    /// Show remote text locally.
    Show(String),
    /// Ring the local terminal bell.
    Alert,
    /// Redraw the split screen (`Ctrl-L`).
    Refresh,
    /// The peer ended its input.
    RemoteEnded,
    /// The caller's invitation was declined.
    Declined(String),
    /// The session closed.
    Closed,
    /// The input has no effect in the current state.
    Ignore,
}

/// One side of a talk session.
#[derive(Clone, Debug)]
pub struct Session {
    role: Role,
    state: State,
    caller: Option<Address>,
    peer: Option<Address>,
    terminal: Option<String>,
    local_closed: bool,
    remote_closed: bool,
    exit: i32,
}

impl Session {
    /// Create the caller side.
    #[must_use]
    pub fn caller(caller: Address, terminal: Option<String>) -> Self {
        Self {
            role: Role::Caller,
            state: State::Inviting,
            caller: Some(caller),
            peer: None,
            terminal,
            local_closed: false,
            remote_closed: false,
            exit: 0,
        }
    }

    /// Create the listener side.
    #[must_use]
    pub fn listener() -> Self {
        Self {
            role: Role::Listener,
            state: State::Inviting,
            caller: None,
            peer: None,
            terminal: None,
            local_closed: false,
            remote_closed: false,
            exit: 0,
        }
    }

    /// The endpoint role.
    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    /// The current state.
    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    /// The peer address once known.
    #[must_use]
    pub fn peer(&self) -> Option<&Address> {
        self.peer.as_ref()
    }

    /// The requested recipient terminal.
    #[must_use]
    pub fn terminal(&self) -> Option<&str> {
        self.terminal.as_deref()
    }

    /// The final exit status.
    #[must_use]
    pub fn exit_status(&self) -> i32 {
        self.exit
    }

    /// Start the caller handshake by returning the invitation frame.
    pub fn start(&mut self) -> Event {
        if self.role != Role::Caller || self.state != State::Inviting {
            return Event::Ignore;
        }
        let Some(caller) = self.caller.clone() else {
            return Event::Ignore;
        };
        Event::Send(Frame::Invite {
            caller,
            terminal: self.terminal.clone(),
        })
    }

    /// Consume one frame from the peer.
    pub fn receive(&mut self, frame: Frame) -> Event {
        match frame {
            Frame::Invite { caller, terminal } => {
                if self.role != Role::Listener || self.state != State::Inviting {
                    return Event::Ignore;
                }
                self.peer = Some(caller.clone());
                self.terminal = terminal.clone();
                self.state = State::Invited;
                Event::ShowInvitation(invitation_text(&caller, terminal.as_deref()))
            }
            Frame::Accept => {
                if self.role != Role::Caller || self.state != State::Inviting {
                    return Event::Ignore;
                }
                self.state = State::Active;
                Event::Start
            }
            Frame::Decline { reason } => {
                if self.role != Role::Caller || self.state != State::Inviting {
                    return Event::Ignore;
                }
                self.state = State::Closed;
                self.exit = 1;
                let reason = sanitize_display(&reason);
                Event::Declined(reason)
            }
            Frame::Data { text } => {
                if self.state != State::Active {
                    return Event::Ignore;
                }
                let text = sanitize_display(&text);
                if text.is_empty() {
                    Event::Ignore
                } else {
                    Event::Show(text)
                }
            }
            Frame::Alert => {
                if self.state == State::Active {
                    Event::Alert
                } else {
                    Event::Ignore
                }
            }
            Frame::End => {
                if !matches!(
                    self.state,
                    State::Active | State::LocalClosed | State::RemoteClosed
                ) {
                    return Event::Ignore;
                }
                self.remote_closed = true;
                if self.local_closed {
                    self.state = State::Closed;
                } else {
                    self.state = State::RemoteClosed;
                }
                Event::RemoteEnded
            }
            Frame::Bye => {
                if self.state == State::Closed {
                    return Event::Ignore;
                }
                self.state = State::Closed;
                Event::Closed
            }
        }
    }

    /// Accept an invitation as the listener.
    pub fn accept(&mut self) -> Event {
        if self.role != Role::Listener || self.state != State::Invited {
            return Event::Ignore;
        }
        self.state = State::Active;
        Event::Send(Frame::Accept)
    }

    /// Decline an invitation as the listener.
    pub fn decline(&mut self, reason: &str) -> Event {
        if self.role != Role::Listener || self.state != State::Invited {
            return Event::Ignore;
        }
        self.state = State::Closed;
        self.exit = 1;
        Event::Send(Frame::Decline {
            reason: reason.to_string(),
        })
    }

    /// Forward one line of local input, filtering to printable text.
    pub fn local_input(&mut self, text: &str) -> Event {
        if self.state != State::Active {
            return Event::Ignore;
        }
        let text = sanitize_display(text);
        if text.is_empty() {
            Event::Ignore
        } else {
            Event::Send(Frame::Data { text })
        }
    }

    /// Handle one raw local character.
    pub fn local_char(&mut self, character: char) -> Event {
        if self.state == State::RemoteClosed {
            return if character == '\x03' {
                self.escape()
            } else {
                Event::Ignore
            };
        }
        if self.state != State::Active {
            return Event::Ignore;
        }
        match character {
            '\x03' => self.escape(),
            '\x04' => self.local_eof(),
            '\x0c' => Event::Refresh,
            '\x07' => Event::Send(Frame::Alert),
            printable if printable == ' ' || printable.is_ascii_graphic() => {
                Event::Send(Frame::Data {
                    text: printable.to_string(),
                })
            }
            _ => Event::Ignore,
        }
    }

    /// Close the local input side and notify the peer.
    pub fn local_eof(&mut self) -> Event {
        if self.state != State::Active {
            return Event::Ignore;
        }
        self.local_closed = true;
        if self.remote_closed {
            self.state = State::Closed;
        } else {
            self.state = State::LocalClosed;
        }
        Event::Send(Frame::End)
    }

    /// Terminate the session now (`Ctrl-C`).
    pub fn escape(&mut self) -> Event {
        if self.state == State::Closed {
            return Event::Ignore;
        }
        self.state = State::Closed;
        self.exit = 0;
        Event::Send(Frame::Bye)
    }
}

/// POSIX-style invitation shown on the listener's terminal.
#[must_use]
pub fn invitation_text(caller: &Address, terminal: Option<&str>) -> String {
    let location = match terminal {
        Some(terminal) => format!(" on {terminal}"),
        None => String::new(),
    };
    format!(
        "Message from {}{location}\r\ntalk: connection requested by {}\r\ntalk: respond with: talk {}\r\n",
        caller.text(),
        caller.text(),
        caller.text()
    )
}

/// Split-screen preamble with a labelled local and remote pane.
#[must_use]
pub fn split_screen(local: &str, remote: &str) -> String {
    let rule = "-".repeat(SCREEN_WIDTH);
    format!("\x1b[2J\x1b[H== {local} ==\r\n{rule}\r\n== {remote} ==\r\n{rule}\r\n")
}

/// Redraw escape (`Ctrl-L`).
#[must_use]
pub fn refresh_screen() -> &'static str {
    "\x1b[2J\x1b[H"
}

/// Keep only printable ASCII and spaces from remote text.
#[must_use]
pub fn sanitize_display(text: &str) -> String {
    text.chars()
        .filter(|character| *character == ' ' || character.is_ascii_graphic())
        .collect()
}

/// Run the built-in local loopback and return a readable transcript.
///
/// This exercises the same encoded frames and state machine as a real peer, but both endpoints
/// are local. It is the documented local diagnostic, not a substitute for a peer terminal.
pub fn loopback_transcript() -> Result<Vec<String>, ProtocolError> {
    let mut caller = Session::caller(
        Address {
            user: "local".to_string(),
            host: None,
        },
        None,
    );
    let mut listener = Session::listener();
    let mut lines = Vec::new();

    let Event::Send(invite) = caller.start() else {
        return Err(ProtocolError::UnknownFrame);
    };
    let Event::ShowInvitation(invitation) = listener.receive(wire(invite)?) else {
        return Err(ProtocolError::UnknownFrame);
    };
    lines.push(format!(
        "invitation: {}",
        invitation.lines().next().unwrap_or("")
    ));

    let Event::Send(accept) = listener.accept() else {
        return Err(ProtocolError::UnknownFrame);
    };
    if caller.receive(wire(accept)?) != Event::Start {
        return Err(ProtocolError::UnknownFrame);
    }
    lines.push("screen: split".to_string());

    let Event::Send(data) = caller.local_input("hello") else {
        return Err(ProtocolError::UnknownFrame);
    };
    let Event::Show(text) = listener.receive(wire(data)?) else {
        return Err(ProtocolError::UnknownFrame);
    };
    lines.push("caller> hello".to_string());
    lines.push(format!("peer< {text}"));

    let Event::Send(data) = listener.local_input("hi") else {
        return Err(ProtocolError::UnknownFrame);
    };
    let Event::Show(text) = caller.receive(wire(data)?) else {
        return Err(ProtocolError::UnknownFrame);
    };
    lines.push("peer> hi".to_string());
    lines.push(format!("caller< {text}"));

    let Event::Send(end) = caller.local_eof() else {
        return Err(ProtocolError::UnknownFrame);
    };
    if listener.receive(wire(end)?) != Event::RemoteEnded {
        return Err(ProtocolError::UnknownFrame);
    }
    lines.push("caller: eof".to_string());
    lines.push("peer: remote ended".to_string());

    let Event::Send(bye) = listener.escape() else {
        return Err(ProtocolError::UnknownFrame);
    };
    if caller.receive(wire(bye)?) != Event::Closed {
        return Err(ProtocolError::UnknownFrame);
    }
    lines.push("peer: bye".to_string());
    lines.push(format!(
        "local loopback closed (caller={} peer={})",
        caller.exit_status(),
        listener.exit_status()
    ));
    Ok(lines)
}

fn wire(frame: Frame) -> Result<Frame, ProtocolError> {
    decode_frame(&encode_frame(&frame)?)
}

/// Parse `talk [-l] [address [terminal]]`.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut listen = false;
    let mut options = true;
    let mut operands: Vec<String> = Vec::new();
    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument.len() > 1 && argument.starts_with('-') {
            match argument.as_str() {
                "-l" | "--listen" => listen = true,
                _ => {
                    return Err(UsageError::UnknownOption(
                        argument.chars().nth(1).unwrap_or('-'),
                    ));
                }
            }
            continue;
        }
        operands.push(argument);
    }
    if listen {
        let terminal = match operands.as_slice() {
            [] => None,
            [terminal] => Some(validate_terminal(terminal)?),
            _ => return Err(UsageError::ExtraOperand),
        };
        return Ok(Options {
            listen: true,
            address: None,
            terminal,
        });
    }
    match operands.as_slice() {
        [] => Err(UsageError::MissingAddress),
        [address] => Ok(Options {
            listen: false,
            address: Some(parse_address(address)?),
            terminal: None,
        }),
        [address, terminal] => Ok(Options {
            listen: false,
            address: Some(parse_address(address)?),
            terminal: Some(validate_terminal(terminal)?),
        }),
        _ => Err(UsageError::ExtraOperand),
    }
}

/// Parse one `user[@host]` address.
pub fn parse_address(text: &str) -> Result<Address, UsageError> {
    let mut parts = text.split('@');
    let user = parts.next().unwrap_or("");
    let host = parts.next();
    if parts.next().is_some() || !valid_name(user, MAX_ADDRESS) {
        return Err(UsageError::BadAddress);
    }
    let host = match host {
        None => None,
        Some(host) if valid_name(host, MAX_ADDRESS) => Some(host.to_string()),
        Some(_) => return Err(UsageError::BadAddress),
    };
    Ok(Address {
        user: user.to_string(),
        host,
    })
}

/// Encode one frame for the wire.
pub fn encode_frame(frame: &Frame) -> Result<String, ProtocolError> {
    let line = match frame {
        Frame::Invite { caller, terminal } => {
            let mut line = format!("{PROTOCOL} INVITE {}", caller.text());
            if let Some(terminal) = terminal {
                line.push(' ');
                line.push_str(terminal);
            }
            line
        }
        Frame::Accept => format!("{PROTOCOL} ACCEPT"),
        Frame::Decline { reason } => {
            let reason = escape_text(reason);
            if reason.is_empty() {
                format!("{PROTOCOL} DECLINE")
            } else {
                format!("{PROTOCOL} DECLINE {reason}")
            }
        }
        Frame::Data { text } => {
            let text = escape_text(text);
            if text.is_empty() {
                format!("{PROTOCOL} DATA")
            } else {
                format!("{PROTOCOL} DATA {text}")
            }
        }
        Frame::Alert => format!("{PROTOCOL} ALERT"),
        Frame::End => format!("{PROTOCOL} END"),
        Frame::Bye => format!("{PROTOCOL} BYE"),
    };
    if line.len() > MAX_FRAME {
        return Err(ProtocolError::TooLong);
    }
    Ok(line)
}

/// Decode one wire line.
pub fn decode_frame(line: &str) -> Result<Frame, ProtocolError> {
    let line = line.trim_end_matches(['\n', '\r']);
    if line.is_empty() {
        return Err(ProtocolError::UnknownFrame);
    }
    if line.len() > MAX_FRAME {
        return Err(ProtocolError::TooLong);
    }
    let rest = line
        .strip_prefix(PROTOCOL)
        .ok_or(ProtocolError::BadPrefix)?;
    let rest = rest.strip_prefix(' ').ok_or(ProtocolError::BadPrefix)?;
    let (verb, tail) = rest.split_once(' ').unwrap_or((rest, ""));
    match verb {
        "INVITE" => {
            let (address, terminal) = tail.split_once(' ').unwrap_or((tail, ""));
            if address.is_empty() {
                return Err(ProtocolError::BadAddress);
            }
            let caller = parse_address(address).map_err(|_| ProtocolError::BadAddress)?;
            let terminal = if terminal.is_empty() {
                None
            } else {
                Some(validate_terminal(terminal).map_err(|_| ProtocolError::BadText)?)
            };
            Ok(Frame::Invite { caller, terminal })
        }
        "ACCEPT" if tail.is_empty() => Ok(Frame::Accept),
        "DECLINE" => Ok(Frame::Decline {
            reason: unescape_text(tail, MAX_REASON)?,
        }),
        "DATA" => Ok(Frame::Data {
            text: unescape_text(tail, MAX_DATA)?,
        }),
        "ALERT" if tail.is_empty() => Ok(Frame::Alert),
        "END" if tail.is_empty() => Ok(Frame::End),
        "BYE" if tail.is_empty() => Ok(Frame::Bye),
        _ => Err(ProtocolError::UnknownFrame),
    }
}

fn valid_name(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn validate_terminal(text: &str) -> Result<String, UsageError> {
    if text.is_empty() || text.len() > MAX_TERMINAL {
        return Err(UsageError::BadTerminal);
    }
    if !text.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'_' | b'-' | b':')
    }) {
        return Err(UsageError::BadTerminal);
    }
    Ok(text.to_string())
}

fn escape_text(text: &str) -> String {
    let mut escaped = String::new();
    for byte in text.bytes() {
        if byte == b'%' || !(0x20..=0x7e).contains(&byte) {
            escaped.push('%');
            escaped.push(hex_digit(byte >> 4));
            escaped.push(hex_digit(byte & 0x0f));
        } else {
            escaped.push(char::from(byte));
        }
    }
    escaped
}

fn unescape_text(text: &str, max: usize) -> Result<String, ProtocolError> {
    let bytes = text.as_bytes();
    let mut decoded: Vec<u8> = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1).ok_or(ProtocolError::BadText)?)?;
            let low = hex_value(*bytes.get(index + 2).ok_or(ProtocolError::BadText)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
        if decoded.len() > max {
            return Err(ProtocolError::TooLong);
        }
    }
    String::from_utf8(decoded).map_err(|_| ProtocolError::BadText)
}

fn hex_digit(value: u8) -> char {
    char::from(if value < 10 {
        b'0' + value
    } else {
        b'a' + (value - 10)
    })
}

fn hex_value(byte: u8) -> Result<u8, ProtocolError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ProtocolError::BadText),
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
