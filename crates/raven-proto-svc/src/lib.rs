// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Wire format for the `svc:` control scheme.
//!
//! The `svc:` directory lists services and each `svc:/<name>` file returns one status line:
//!
//! ```text
//! <name> <state> <pid-or-handle> <restarts>\n
//! ```
//!
//! Control requests are one-byte [`Op`] values sent through the S11 `CALL` operation. This crate
//! keeps both encodings in one place so `svcd` and `svcctl` cannot drift apart. It is `no_std`,
//! allocation-free and contains no `unsafe` code.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Maximum service name length, matching the manifest parser.
pub const MAX_NAME: usize = 32;
/// Maximum encoded status line length in bytes.
pub const MAX_LINE: usize = 96;

/// A malformed control request or status line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The caller's output buffer is too small or a value is longer than [`MAX_LINE`].
    TooLong,
    /// The bytes are not one of the documented shapes.
    Malformed,
    /// The one-byte control request names no known operation.
    UnknownOpcode,
    /// The status line carries an unknown state string.
    UnknownState,
}

/// A control operation sent through `CALL` on a service file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Start a stopped service.
    Start,
    /// Stop supervision of a running service.
    Stop,
    /// Stop and start a service again.
    Restart,
}

impl Op {
    /// Wire byte for [`Op::Start`].
    pub const START: u8 = 1;
    /// Wire byte for [`Op::Stop`].
    pub const STOP: u8 = 2;
    /// Wire byte for [`Op::Restart`].
    pub const RESTART: u8 = 3;

    /// Wire byte for this operation.
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Start => Self::START,
            Self::Stop => Self::STOP,
            Self::Restart => Self::RESTART,
        }
    }

    /// Parse a wire byte.
    pub const fn from_u8(value: u8) -> Result<Self, Error> {
        match value {
            Self::START => Ok(Self::Start),
            Self::STOP => Ok(Self::Stop),
            Self::RESTART => Ok(Self::Restart),
            _ => Err(Error::UnknownOpcode),
        }
    }

    /// Encode the operation into exactly one byte.
    pub fn encode(self, out: &mut [u8]) -> Result<usize, Error> {
        if out.is_empty() {
            return Err(Error::TooLong);
        }
        out[0] = self.as_u8();
        Ok(1)
    }

    /// Decode a one-byte request.
    pub fn decode(input: &[u8]) -> Result<Self, Error> {
        match input {
            [value] => Self::from_u8(*value),
            _ => Err(Error::Malformed),
        }
    }
}

/// The externally visible state of one service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Not running and not scheduled to start.
    Stopped,
    /// The child was created but readiness has not been confirmed.
    Starting,
    /// Ready and running.
    Running,
    /// Exited and waiting for the restart delay.
    Restarting,
    /// Readiness failed or the service could not be started.
    Failed,
    /// The crash-loop limit was reached; svcd stopped restarting it.
    GaveUp,
}

impl State {
    /// Stable status-line spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Restarting => "restarting",
            Self::Failed => "failed",
            Self::GaveUp => "gave-up",
        }
    }

    /// Parse a status-line spelling.
    pub fn parse(text: &str) -> Result<Self, Error> {
        match text {
            "stopped" => Ok(Self::Stopped),
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "restarting" => Ok(Self::Restarting),
            "failed" => Ok(Self::Failed),
            "gave-up" => Ok(Self::GaveUp),
            _ => Err(Error::UnknownState),
        }
    }
}

/// One decoded `svc:/<name>` status line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status<'a> {
    /// Service name.
    pub name: &'a str,
    /// Current state.
    pub state: State,
    /// Process handle, or zero when there is no live child.
    pub handle: u32,
    /// Restart attempts since the last window reset.
    pub restarts: u32,
}

impl<'a> Status<'a> {
    /// Encode the status line, including the trailing newline.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, Error> {
        if !valid_name(self.name) {
            return Err(Error::Malformed);
        }
        let mut length = 0usize;
        length = append(out, length, self.name.as_bytes())?;
        length = append(out, length, b" ")?;
        length = append(out, length, self.state.as_str().as_bytes())?;
        length = append(out, length, b" ")?;
        length = append_u32(out, length, self.handle)?;
        length = append(out, length, b" ")?;
        length = append_u32(out, length, self.restarts)?;
        length = append(out, length, b"\n")?;
        Ok(length)
    }

    /// Decode one status line. A single trailing `\n` or `\r\n` is optional.
    pub fn decode(input: &'a [u8]) -> Result<Self, Error> {
        let line = input
            .strip_suffix(b"\r\n")
            .or_else(|| input.strip_suffix(b"\n"))
            .unwrap_or(input);
        if line.is_empty() || line.len() > MAX_LINE {
            return Err(Error::Malformed);
        }
        let mut fields = line.split(|byte| *byte == b' ');
        let name = fields.next().ok_or(Error::Malformed)?;
        let state = fields.next().ok_or(Error::Malformed)?;
        let handle = fields.next().ok_or(Error::Malformed)?;
        let restarts = fields.next().ok_or(Error::Malformed)?;
        if fields.next().is_some() {
            return Err(Error::Malformed);
        }
        let name = core::str::from_utf8(name).map_err(|_| Error::Malformed)?;
        if !valid_name(name) {
            return Err(Error::Malformed);
        }
        let state = core::str::from_utf8(state).map_err(|_| Error::Malformed)?;
        let state = State::parse(state)?;
        let handle = parse_u32(handle)?;
        let restarts = parse_u32(restarts)?;
        Ok(Self {
            name,
            state,
            handle,
            restarts,
        })
    }
}

/// A service name is `[a-z0-9_-]{1,32}`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
}

fn append(out: &mut [u8], position: usize, bytes: &[u8]) -> Result<usize, Error> {
    let end = position.checked_add(bytes.len()).ok_or(Error::TooLong)?;
    let target = out.get_mut(position..end).ok_or(Error::TooLong)?;
    target.copy_from_slice(bytes);
    Ok(end)
}

fn append_u32(out: &mut [u8], position: usize, value: u32) -> Result<usize, Error> {
    let mut digits = [0u8; 10];
    let mut index = digits.len();
    let mut remaining = value;
    loop {
        index -= 1;
        digits[index] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    append(out, position, &digits[index..])
}

fn parse_u32(bytes: &[u8]) -> Result<u32, Error> {
    if bytes.is_empty() || bytes.len() > 10 {
        return Err(Error::Malformed);
    }
    let mut value = 0u32;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return Err(Error::Malformed);
        }
        value = value
            .checked_mul(10)
            .and_then(|value| value.checked_add(u32::from(byte - b'0')))
            .ok_or(Error::Malformed)?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_golden_bytes() {
        let mut bytes = [0u8; 1];
        assert_eq!(Op::Start.encode(&mut bytes), Ok(1));
        assert_eq!(bytes, [1]);
        assert_eq!(Op::Stop.encode(&mut bytes), Ok(1));
        assert_eq!(bytes, [2]);
        assert_eq!(Op::Restart.encode(&mut bytes), Ok(1));
        assert_eq!(bytes, [3]);
    }

    #[test]
    fn op_roundtrip() {
        for op in [Op::Start, Op::Stop, Op::Restart] {
            let mut bytes = [0u8; 1];
            op.encode(&mut bytes).unwrap();
            assert_eq!(Op::decode(&bytes), Ok(op));
        }
    }

    #[test]
    fn op_rejects_bad_shapes() {
        assert_eq!(Op::decode(&[]), Err(Error::Malformed));
        assert_eq!(Op::decode(&[1, 2]), Err(Error::Malformed));
        assert_eq!(Op::decode(&[0]), Err(Error::UnknownOpcode));
        assert_eq!(Op::decode(&[4]), Err(Error::UnknownOpcode));
        assert_eq!(Op::Start.encode(&mut []), Err(Error::TooLong));
    }

    #[test]
    fn state_spellings_roundtrip() {
        for state in [
            State::Stopped,
            State::Starting,
            State::Running,
            State::Restarting,
            State::Failed,
            State::GaveUp,
        ] {
            assert_eq!(State::parse(state.as_str()), Ok(state));
        }
        assert_eq!(State::parse("RUNNING"), Err(Error::UnknownState));
    }

    #[test]
    fn status_golden_line() {
        let status = Status {
            name: "logd",
            state: State::Running,
            handle: 7,
            restarts: 0,
        };
        let mut line = [0u8; MAX_LINE];
        let length = status.encode(&mut line).unwrap();
        assert_eq!(&line[..length], b"logd running 7 0\n");
    }

    #[test]
    fn status_roundtrip() {
        let status = Status {
            name: "a",
            state: State::GaveUp,
            handle: 0,
            restarts: 3,
        };
        let mut line = [0u8; MAX_LINE];
        let length = status.encode(&mut line).unwrap();
        assert_eq!(Status::decode(&line[..length]), Ok(status));
        assert_eq!(
            Status::decode(b"a gave-up 0 3"),
            Ok(Status {
                name: "a",
                state: State::GaveUp,
                handle: 0,
                restarts: 3
            })
        );
        assert_eq!(
            Status::decode(b"a stopped 0 0\r\n").unwrap().state,
            State::Stopped
        );
    }

    #[test]
    fn status_rejects_bad_names_and_numbers() {
        assert_eq!(Status::decode(b"Bad running 1 0"), Err(Error::Malformed));
        assert_eq!(Status::decode(b"a running x 0"), Err(Error::Malformed));
        assert_eq!(Status::decode(b"a running 1 -1"), Err(Error::Malformed));
        assert_eq!(Status::decode(b"a running 1"), Err(Error::Malformed));
        assert_eq!(
            Status::decode(b"a running 1 0 extra"),
            Err(Error::Malformed)
        );
        assert_eq!(Status::decode(b""), Err(Error::Malformed));
    }

    #[test]
    fn status_rejects_unknown_state() {
        assert_eq!(Status::decode(b"a unknown 1 0"), Err(Error::UnknownState));
    }

    #[test]
    fn status_encode_needs_room() {
        let status = Status {
            name: "logd",
            state: State::Running,
            handle: 7,
            restarts: 0,
        };
        let mut line = [0u8; 3];
        assert_eq!(status.encode(&mut line), Err(Error::TooLong));
    }

    #[test]
    fn name_validation() {
        assert!(valid_name("a"));
        assert!(valid_name("a-b_c9"));
        assert!(!valid_name(""));
        assert!(!valid_name("Upper"));
        assert!(!valid_name("with space"));
        assert!(!valid_name(&"x".repeat(MAX_NAME + 1)));
        assert!(valid_name(&"x".repeat(MAX_NAME)));
    }

    #[test]
    fn maximum_numbers_encode() {
        let status = Status {
            name: "a",
            state: State::Restarting,
            handle: u32::MAX,
            restarts: u32::MAX,
        };
        let mut line = [0u8; MAX_LINE];
        let length = status.encode(&mut line).unwrap();
        assert_eq!(Status::decode(&line[..length]), Ok(status));
    }
}
