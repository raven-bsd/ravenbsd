// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Historical `.Z` LZW codec (block mode, 9..16-bit codes).
//!
//! The stream starts with `1f 9d` and a flags byte (`maxbits | 0x80`).
//! Codes are packed least-significant-bit first and grow from 9 bits up to
//! the header's maximum. The clear code is 256; the first free code is 257.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// `.Z` magic.
pub const MAGIC: [u8; 2] = [0x1f, 0x9d];
/// Clear code.
pub const CLEAR: u16 = 256;
/// First free code in block mode.
pub const FIRST: u16 = 257;

/// Why a stream is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The two-byte magic is missing.
    BadMagic,
    /// The flags byte is not a supported block-mode header.
    BadFlags,
    /// A code is outside the dictionary.
    Corrupt,
    /// A dictionary code points before the first byte.
    Truncated,
}

/// One parsed `compress` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Write to standard output and keep input files.
    pub stdout: bool,
    /// Decompress instead of compress.
    pub decompress: bool,
    /// Maximum code width (9..16).
    pub bits: u8,
    /// Input files.
    pub files: Vec<String>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown option was supplied.
    UnknownOption,
    /// `-b` is outside 9..16.
    BadBits,
    /// `-b` has no value.
    MissingBits,
    /// `-c`/`-d` needs at least one file in this image.
    MissingFile,
}

/// Parse `compress [-c] [-d] [-b bits] [file ...]`.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option, a bad/missing bit width or no
/// input file.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut options = Options {
        stdout: false,
        decompress: false,
        bits: 16,
        files: Vec::new(),
    };
    let mut args = args.into_iter();
    let mut options_on = true;
    while let Some(argument) = args.next() {
        if options_on && argument == "--" {
            options_on = false;
            continue;
        }
        if options_on && argument.starts_with('-') && argument.len() > 1 {
            let mut chars = argument.chars().skip(1);
            while let Some(flag) = chars.next() {
                match flag {
                    'c' => options.stdout = true,
                    'd' => options.decompress = true,
                    'b' => {
                        let value: String = chars.collect();
                        let value = if value.is_empty() {
                            args.next().ok_or(UsageError::MissingBits)?
                        } else {
                            value
                        };
                        options.bits = value.parse().map_err(|_| UsageError::BadBits)?;
                        if !(9..=16).contains(&options.bits) {
                            return Err(UsageError::BadBits);
                        }
                        break;
                    }
                    _ => return Err(UsageError::UnknownOption),
                }
            }
            continue;
        }
        options.files.push(argument);
    }
    if options.files.is_empty() {
        return Err(UsageError::MissingFile);
    }
    Ok(options)
}

/// Compress `input` into a `.Z` stream.
#[must_use]
pub fn encode(input: &[u8]) -> Vec<u8> {
    encode_bits(input, 16)
}

/// Compress `input` with an explicit maximum code width.
#[must_use]
pub fn encode_bits(input: &[u8], maxbits: u8) -> Vec<u8> {
    let maxbits = maxbits.clamp(9, 16);
    let mut out = vec![MAGIC[0], MAGIC[1], 0x80 | maxbits];
    let mut writer = BitWriter::new();
    let mut dict: BTreeMap<(u16, u8), u16> = BTreeMap::new();
    let mut width = 9u8;
    let mut next = u32::from(FIRST);
    let Some((&first, rest)) = input.split_first() else {
        writer.finish(&mut out);
        return out;
    };
    let mut current = u16::from(first);
    for &byte in rest {
        if let Some(&code) = dict.get(&(current, byte)) {
            current = code;
            continue;
        }
        writer.write(current, width);
        if next > u32::from(max_code(width)) && width < maxbits {
            width += 1;
        }
        if next < (1u32 << maxbits) {
            dict.insert((current, byte), next as u16);
            next += 1;
        }
        if next >= (1u32 << maxbits) {
            writer.write(CLEAR, width);
            dict.clear();
            width = 9;
            next = u32::from(FIRST);
        }
        current = u16::from(byte);
    }
    writer.write(current, width);
    writer.finish(&mut out);
    out
}

/// Decompress a `.Z` stream.
///
/// # Errors
///
/// Returns [`Error`] for a bad magic/flags byte, an out-of-range code or a
/// dictionary reference that does not exist.
pub fn decode(input: &[u8]) -> Result<Vec<u8>, Error> {
    if input.len() < 3 {
        return Err(Error::Truncated);
    }
    if input[0] != MAGIC[0] || input[1] != MAGIC[1] {
        return Err(Error::BadMagic);
    }
    let flags = input[2];
    let maxbits = flags & 0x1f;
    if flags & 0x80 == 0 || !(9..=16).contains(&maxbits) {
        return Err(Error::BadFlags);
    }
    let mut reader = BitReader::new(&input[3..]);
    let mut width = 9u8;
    let mut next = u32::from(FIRST);
    let mut prefix = vec![0u16; 1usize << maxbits];
    let mut suffix = vec![0u8; 1usize << maxbits];
    for value in 0..256usize {
        suffix[value] = value as u8;
        prefix[value] = u16::MAX;
    }
    let mut output = Vec::new();
    let mut old: Option<u16> = None;
    while let Some(code) = reader.read(width) {
        if code == CLEAR {
            width = 9;
            next = u32::from(FIRST);
            old = None;
            continue;
        }
        if code < 256 {
            let first = code as u8;
            output.push(first);
            if let Some(previous) = old {
                add(&mut prefix, &mut suffix, &mut next, previous, first)?;
            }
            old = Some(code);
        } else if u32::from(code) < next {
            let (sequence, first) = expand(&prefix, &suffix, code)?;
            output.extend_from_slice(&sequence);
            if let Some(previous) = old {
                add(&mut prefix, &mut suffix, &mut next, previous, first)?;
            }
            old = Some(code);
        } else if u32::from(code) == next {
            let previous = old.ok_or(Error::Corrupt)?;
            let (mut sequence, first) = expand(&prefix, &suffix, previous)?;
            sequence.push(first);
            output.extend_from_slice(&sequence);
            add(&mut prefix, &mut suffix, &mut next, previous, first)?;
            old = Some(code);
        } else {
            return Err(Error::Corrupt);
        }
        if next > u32::from(max_code(width)) && width < maxbits {
            width += 1;
        }
    }
    Ok(output)
}

fn add(
    prefix: &mut [u16],
    suffix: &mut [u8],
    next: &mut u32,
    previous: u16,
    byte: u8,
) -> Result<(), Error> {
    let index = *next as usize;
    if index >= prefix.len() || index >= suffix.len() {
        return Err(Error::Corrupt);
    }
    prefix[index] = previous;
    suffix[index] = byte;
    *next += 1;
    Ok(())
}

fn expand(prefix: &[u16], suffix: &[u8], code: u16) -> Result<(Vec<u8>, u8), Error> {
    let mut stack = Vec::new();
    let mut current = code;
    for _ in 0..=u16::MAX {
        if current < 256 {
            stack.push(current as u8);
            break;
        }
        let index = usize::from(current);
        if index >= prefix.len() || index >= suffix.len() {
            return Err(Error::Corrupt);
        }
        stack.push(suffix[index]);
        let previous = prefix[index];
        if previous == u16::MAX {
            return Err(Error::Truncated);
        }
        current = previous;
    }
    let first = *stack.last().ok_or(Error::Corrupt)?;
    stack.reverse();
    Ok((stack, first))
}

const fn max_code(width: u8) -> u16 {
    (1u16 << width) - 1
}

struct BitWriter {
    buffer: u32,
    bits: u8,
    bytes: Vec<u8>,
}

impl BitWriter {
    const fn new() -> Self {
        Self {
            buffer: 0,
            bits: 0,
            bytes: Vec::new(),
        }
    }

    fn write(&mut self, code: u16, width: u8) {
        self.buffer |= u32::from(code) << self.bits;
        self.bits += width;
        while self.bits >= 8 {
            self.bits -= 8;
            self.bytes.push((self.buffer & 0xff) as u8);
            self.buffer >>= 8;
        }
    }

    fn finish(&mut self, out: &mut Vec<u8>) {
        if self.bits > 0 {
            self.bytes.push((self.buffer & 0xff) as u8);
        }
        out.extend_from_slice(&self.bytes);
        self.bytes.clear();
        self.buffer = 0;
        self.bits = 0;
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> BitReader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn read(&mut self, width: u8) -> Option<u16> {
        let end = self.bit.checked_add(usize::from(width))?;
        if end > self.data.len().saturating_mul(8) {
            return None;
        }
        let mut value = 0u32;
        for index in 0..usize::from(width) {
            let position = self.bit + index;
            let byte = self.data.get(position / 8)?;
            let bit = (byte >> (position % 8)) & 1;
            value |= u32::from(bit) << index;
        }
        self.bit = end;
        Some(value as u16)
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
