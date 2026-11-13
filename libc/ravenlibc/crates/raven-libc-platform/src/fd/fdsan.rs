// SPDX-License-Identifier: BSD-2-Clause
/*
 * Copyright (C) 2018 The Android Open Source Project
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *  * Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *  * Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 * "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 * LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 * FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
 * COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 * INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
 * BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
 * OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
 * AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
 * OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from bionic:libc/bionic/fdsan.cpp,
// bionic:libc/private/bionic_fdsan.h and bionic:libc/include/android/fdsan.h @
// 731631f300090436d7f5df80d50b6275c8c60a93; the owner-tag encoding, the
// error-level policy and the close/exchange mismatch detection are kept; the
// Android property read is replaced by `RAVEN_FDSAN`, async logging by the
// `DEBUG_WRITE` seam, mmap by the fd table's lazily allocated overflow entries,
// and the default level is `warn` instead of `fatal` (H-9).

//! Descriptor-use debugging (`fdsan`), transpiled from bionic.
//!
//! An fd table entry carries an owner tag. A component that owns a descriptor records its type
//! and a value; `close` with the expected tag detects a double close or an ownership mismatch.
//! The error level comes from `RAVEN_FDSAN` (`off`, `warn`, `warn_once`, `fatal`; default
//! `warn`). The tag array is not a second fd table: it is the `tag` field of the fd entry, with
//! the same 128-inline/overflow shape.

use alloc::format;
use alloc::string::String;
use core::cell::Cell;

/// The owner type stored in the top byte of a tag (bionic numbering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum OwnerType {
    /// A generic native owner.
    Generic = 0,
    /// A `FILE *`.
    File = 1,
    /// A `DIR *`.
    Dir = 2,
    /// A C++ `unique_fd`.
    UniqueFd = 3,
    /// A libc-internal owner.
    Libc = 4,
    /// A socket implementation.
    Socket = 5,
    /// An archive or zip owner.
    ZipArchive = 6,
    /// A native handle.
    NativeHandle = 7,
    /// A Rust `std`-style file.
    RustFile = 8,
    /// An unknown generic owner (the sign-extended pointer form).
    GenericFf = 0xff,
}

impl OwnerType {
    /// The bionic spelling used in diagnostics.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Generic => "native object of unknown type",
            Self::File => "FILE*",
            Self::Dir => "DIR*",
            Self::UniqueFd => "unique_fd",
            Self::Libc => "libc",
            Self::Socket => "SocketImpl",
            Self::ZipArchive => "ZipArchive",
            Self::NativeHandle => "native_handle_t",
            Self::RustFile => "Rust File",
            Self::GenericFf => "Java object of unknown type",
        }
    }
}

/// Build an owner tag: the type in the top byte and the value in the low 56 bits.
pub fn tag(owner: OwnerType, value: u64) -> u64 {
    if value == 0 {
        return 0;
    }
    ((owner as u64) << 56) | (value & ((1u64 << 56) - 1))
}

/// The owner type of a tag.
pub const fn tag_type(tag: u64) -> u8 {
    (tag >> 56) as u8
}

/// The value of a tag, sign-extended exactly as bionic does.
pub const fn tag_value(tag: u64) -> u64 {
    ((tag << 8) as i64 >> 8) as u64
}

/// The configured fdsan error level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorLevel {
    /// No checks.
    Disabled,
    /// Report the first mismatch, then disable.
    WarnOnce,
    /// Report every mismatch.
    WarnAlways,
    /// Report and terminate the process.
    Fatal,
}

impl ErrorLevel {
    /// Parse `RAVEN_FDSAN`; an absent or unknown value is `warn` (H-9).
    pub fn from_env(value: Option<&[u8]>) -> Self {
        match value {
            Some(b"off") | Some(b"0") => Self::Disabled,
            Some(b"warn") => Self::WarnAlways,
            Some(b"warn_once") => Self::WarnOnce,
            Some(b"fatal") | Some(b"1") => Self::Fatal,
            _ => Self::WarnAlways,
        }
    }
}

/// What the close path does after a detected mismatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing to report.
    None,
    /// Emit the message and continue.
    Warn,
    /// Emit the message, then terminate the process.
    Abort,
}

/// One kind of ownership violation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// The tag the caller expected differs from the actual tag.
    WrongOwner {
        /// Tag the caller passed to `close`.
        expected: u64,
        /// Tag in the fd entry.
        actual: u64,
    },
    /// The caller expected an owner but the fd is unowned.
    ExpectedOwned {
        /// Tag the caller passed.
        expected: u64,
    },
    /// The caller expected no owner but the fd is owned.
    ExpectedUnowned {
        /// Tag in the fd entry.
        actual: u64,
    },
    /// A `close` with a non-zero tag found the fd already closed.
    DoubleClose,
}

impl Mismatch {
    /// The diagnostic line, without the "fdsan: " prefix.
    pub fn message(self) -> String {
        match self {
            Self::WrongOwner { expected, actual } => format!(
                "attempted to close file descriptor, expected to be owned by {} 0x{:x}, actually owned by {} 0x{:x}",
                OwnerType::describe_type(tag_type(expected)),
                tag_value(expected),
                OwnerType::describe_type(tag_type(actual)),
                tag_value(actual)
            ),
            Self::ExpectedOwned { expected } => format!(
                "attempted to close file descriptor, expected to be owned by {} 0x{:x}, actually unowned",
                OwnerType::describe_type(tag_type(expected)),
                tag_value(expected)
            ),
            Self::ExpectedUnowned { actual } => format!(
                "attempted to close file descriptor, expected to be unowned, actually owned by {} 0x{:x}",
                OwnerType::describe_type(tag_type(actual)),
                tag_value(actual)
            ),
            Self::DoubleClose => "double-close of file descriptor detected".into(),
        }
    }
}

impl OwnerType {
    fn describe_type(raw: u8) -> &'static str {
        match raw {
            0 => Self::Generic.describe(),
            1 => Self::File.describe(),
            2 => Self::Dir.describe(),
            3 => Self::UniqueFd.describe(),
            4 => Self::Libc.describe(),
            5 => Self::Socket.describe(),
            6 => Self::ZipArchive.describe(),
            7 => Self::NativeHandle.describe(),
            8 => Self::RustFile.describe(),
            _ => Self::GenericFf.describe(),
        }
    }
}

/// The per-process fdsan state: the error level and the warn-once latch.
pub struct FdsanState {
    level: Cell<ErrorLevel>,
    reported: Cell<bool>,
}

impl Default for FdsanState {
    fn default() -> Self {
        Self::new(ErrorLevel::WarnAlways)
    }
}

impl FdsanState {
    /// A state with `level`.
    pub const fn new(level: ErrorLevel) -> Self {
        Self {
            level: Cell::new(level),
            reported: Cell::new(false),
        }
    }

    /// A state initialised from `RAVEN_FDSAN`.
    pub fn from_env(value: Option<&[u8]>) -> Self {
        Self::new(ErrorLevel::from_env(value))
    }

    /// The current level.
    pub fn level(&self) -> ErrorLevel {
        self.level.get()
    }

    /// Replace the level and return the old one.
    pub fn set_level(&self, level: ErrorLevel) -> ErrorLevel {
        self.level.replace(level)
    }

    /// Decide what to do about a mismatch, honouring `warn_once`.
    pub fn classify(&self, _mismatch: Mismatch) -> Action {
        match self.level.get() {
            ErrorLevel::Disabled => Action::None,
            ErrorLevel::WarnOnce if self.reported.get() => Action::None,
            ErrorLevel::WarnOnce => {
                self.reported.set(true);
                Action::Warn
            }
            ErrorLevel::WarnAlways => Action::Warn,
            ErrorLevel::Fatal => Action::Abort,
        }
    }
}

#[cfg(test)]
#[path = "fdsan_ut.rs"]
mod tests;
