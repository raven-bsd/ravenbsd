// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Versioned wire format between the kernel Linux-personality trap and `linuxd`.
//!
//! The kernel does not interpret Linux numbers: it captures the raw register
//! arguments, wraps them in [`LinuxSyscallV1`] and waits for [`LinuxReplyV1`].
//! The format is fixed little-endian with no padding.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// Protocol version.
pub const VERSION: u8 = 1;
/// Linux syscall request version 1.
pub const SYSCALL_VERSION: u8 = VERSION;
/// Reply version 1.
pub const REPLY_VERSION: u8 = VERSION;

/// x86-64 architecture id.
pub const ARCH_X86_64: u8 = 1;
/// AArch64 architecture id.
pub const ARCH_AARCH64: u8 = 2;
/// RISC-V 64 architecture id.
pub const ARCH_RISCV64: u8 = 3;

/// Encoded request length: version, arch, number and six arguments.
pub const SYSCALL_LEN: usize = 2 + 8 + 6 * 8;
/// Encoded reply length: version and signed value.
pub const REPLY_LEN: usize = 1 + 8;

/// Why a message was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtoError {
    /// The destination is too small or the input ends early.
    TooShort,
    /// The input has bytes after the message.
    TrailingBytes,
    /// The version is not [`VERSION`].
    BadVersion,
    /// The architecture id is unknown.
    BadArch,
}

/// One captured Linux syscall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinuxSyscallV1 {
    /// Protocol version.
    pub version: u8,
    /// Architecture id (`ARCH_*`).
    pub arch: u8,
    /// Linux syscall number.
    pub nr: u64,
    /// Six raw argument words.
    pub args: [u64; 6],
}

impl LinuxSyscallV1 {
    /// Build a version-1 request.
    pub fn new(arch: u8, nr: u64, args: [u64; 6]) -> Self {
        Self {
            version: VERSION,
            arch,
            nr,
            args,
        }
    }

    /// Encode into `out`; returns [`SYSCALL_LEN`].
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if self.version != VERSION {
            return Err(ProtoError::BadVersion);
        }
        if !matches!(self.arch, ARCH_X86_64 | ARCH_AARCH64 | ARCH_RISCV64) {
            return Err(ProtoError::BadArch);
        }
        let bytes = out.get_mut(..SYSCALL_LEN).ok_or(ProtoError::TooShort)?;
        bytes[0] = self.version;
        bytes[1] = self.arch;
        bytes[2..10].copy_from_slice(&self.nr.to_le_bytes());
        let mut index = 0usize;
        while index < 6 {
            let offset = 10 + index * 8;
            let slot = bytes
                .get_mut(offset..offset + 8)
                .ok_or(ProtoError::TooShort)?;
            slot.copy_from_slice(&self.args[index].to_le_bytes());
            index += 1;
        }
        Ok(SYSCALL_LEN)
    }

    /// Decode one complete request.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() < SYSCALL_LEN {
            return Err(ProtoError::TooShort);
        }
        if input.len() > SYSCALL_LEN {
            return Err(ProtoError::TrailingBytes);
        }
        if input[0] != VERSION {
            return Err(ProtoError::BadVersion);
        }
        if !matches!(input[1], ARCH_X86_64 | ARCH_AARCH64 | ARCH_RISCV64) {
            return Err(ProtoError::BadArch);
        }
        let nr = u64::from_le_bytes(input[2..10].try_into().map_err(|_| ProtoError::TooShort)?);
        let mut args = [0u64; 6];
        let mut index = 0usize;
        while index < 6 {
            let offset = 10 + index * 8;
            args[index] = u64::from_le_bytes(
                input
                    .get(offset..offset + 8)
                    .ok_or(ProtoError::TooShort)?
                    .try_into()
                    .map_err(|_| ProtoError::TooShort)?,
            );
            index += 1;
        }
        Ok(Self {
            version: input[0],
            arch: input[1],
            nr,
            args,
        })
    }
}

/// One reply value returned to the Linux process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinuxReplyV1 {
    /// Protocol version.
    pub version: u8,
    /// Signed value written to `rax`/`x0`.
    pub value: i64,
}

impl LinuxReplyV1 {
    /// Build a version-1 reply.
    pub const fn new(value: i64) -> Self {
        Self {
            version: VERSION,
            value,
        }
    }

    /// Encode into `out`; returns [`REPLY_LEN`].
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        if self.version != VERSION {
            return Err(ProtoError::BadVersion);
        }
        let bytes = out.get_mut(..REPLY_LEN).ok_or(ProtoError::TooShort)?;
        bytes[0] = self.version;
        bytes[1..9].copy_from_slice(&self.value.to_le_bytes());
        Ok(REPLY_LEN)
    }

    /// Decode one complete reply.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        if input.len() < REPLY_LEN {
            return Err(ProtoError::TooShort);
        }
        if input.len() > REPLY_LEN {
            return Err(ProtoError::TrailingBytes);
        }
        if input[0] != VERSION {
            return Err(ProtoError::BadVersion);
        }
        let value = i64::from_le_bytes(input[1..9].try_into().map_err(|_| ProtoError::TooShort)?);
        Ok(Self {
            version: input[0],
            value,
        })
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
