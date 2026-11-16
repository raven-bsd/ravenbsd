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

/// Magic at the start of a frame payload carried beside a trap request or reply.
pub const FRAME_MAGIC: [u8; 4] = *b"LFRM";
/// Version of the frame payload. The syscall header stays version 1 so an older server can
/// ignore the frame; the frame envelope has its own version.
pub const FRAME_VERSION: u8 = 1;
/// Number of general-purpose registers in a frame.
pub const FRAME_GPRS: usize = 15;
/// Frame header: magic, version and two reserved bytes.
pub const FRAME_HEADER: usize = 8;
/// Frame length: header plus RIP, RSP, RFLAGS, FS base and [`FRAME_GPRS`] registers.
pub const FRAME_LEN: usize = FRAME_HEADER + (4 + FRAME_GPRS) * 8;

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
    /// The frame magic, version or reserved bytes are wrong.
    BadFrame,
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

/// The saved user frame carried beside a trap request or reply.
///
/// The kernel writes this into the scheme packet's `path` field (256 bytes) because the packet's
/// inline `data` is only 64 bytes and the version-1 syscall request already uses 58. The server
/// reads the first [`FRAME_LEN`] bytes. The reply may carry the same envelope to resume the
/// process at a chosen RIP/RSP; the 9-byte [`LinuxReplyV1`] stays in `data`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinuxFrame {
    /// Instruction pointer.
    pub rip: u64,
    /// Stack pointer.
    pub rsp: u64,
    /// Saved RFLAGS.
    pub rflags: u64,
    /// User FS/TPIDR base at trap time.
    pub fs_base: u64,
    /// General registers: RAX, RBX, RCX, RDX, RSI, RDI, RBP, R8-R15.
    pub gprs: [u64; FRAME_GPRS],
}

impl LinuxFrame {
    /// Encode into the first [`FRAME_LEN`] bytes of `out`; returns [`FRAME_LEN`].
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, ProtoError> {
        let bytes = out.get_mut(..FRAME_LEN).ok_or(ProtoError::TooShort)?;
        bytes[..4].copy_from_slice(&FRAME_MAGIC);
        bytes[4] = FRAME_VERSION;
        bytes[5] = 0;
        bytes[6] = 0;
        bytes[7] = 0;
        for (index, value) in self.words().iter().enumerate() {
            let offset = FRAME_HEADER + index * 8;
            let slot = bytes
                .get_mut(offset..offset + 8)
                .ok_or(ProtoError::TooShort)?;
            slot.copy_from_slice(&value.to_le_bytes());
        }
        Ok(FRAME_LEN)
    }

    /// Decode one frame from a carrier that may be longer than [`FRAME_LEN`] (the packet path is
    /// zero-padded); the first [`FRAME_LEN`] bytes are the envelope.
    pub fn decode(input: &[u8]) -> Result<Self, ProtoError> {
        let bytes = input.get(..FRAME_LEN).ok_or(ProtoError::TooShort)?;
        if bytes.get(..4) != Some(FRAME_MAGIC.as_slice()) || bytes[4] != FRAME_VERSION {
            return Err(ProtoError::BadFrame);
        }
        if bytes[5] != 0 || bytes[6] != 0 || bytes[7] != 0 {
            return Err(ProtoError::BadFrame);
        }
        let mut words = [0u64; 4 + FRAME_GPRS];
        let mut index = 0usize;
        while index < words.len() {
            let offset = FRAME_HEADER + index * 8;
            words[index] = u64::from_le_bytes(
                bytes
                    .get(offset..offset + 8)
                    .ok_or(ProtoError::TooShort)?
                    .try_into()
                    .map_err(|_| ProtoError::TooShort)?,
            );
            index += 1;
        }
        let mut gprs = [0u64; FRAME_GPRS];
        gprs.copy_from_slice(&words[4..]);
        Ok(Self {
            rip: words[0],
            rsp: words[1],
            rflags: words[2],
            fs_base: words[3],
            gprs,
        })
    }

    fn words(&self) -> [u64; 4 + FRAME_GPRS] {
        let mut words = [0u64; 4 + FRAME_GPRS];
        words[0] = self.rip;
        words[1] = self.rsp;
        words[2] = self.rflags;
        words[3] = self.fs_base;
        words[4..].copy_from_slice(&self.gprs);
        words
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
