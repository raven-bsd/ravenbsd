// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1989, 1993, 1994 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/setmode.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: setmode.c,v 1.22 2014/10/11 04:14:35 deraadt Exp $)
//
// This code is derived from software contributed to Berkeley by
// Dave Borman at Cray Research, Inc.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `setmode`/`getmode` (donor-map 15 T3, 02 X49).
//!
//! The donor calls `umask()` twice to read the mask; here the mask is a `getmode` argument.
//! The `BITCMD` vector keeps the donor's parse-time shape, with one flag recording whether the
//! entry's bits are `mask`-relative in C; `compress_mode` only merges entries with the same
//! flag so the deferred mask keeps the donor's result. Signals are not blocked: the crate has
//! no signal state and the shell owns process signals. A malformed spec returns `None` and the
//! shell sets the donor's errno (`EINVAL` or `ERANGE`).

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

/// `S_ISUID`.
pub const S_ISUID: u32 = 0o4000;
/// `S_ISGID`.
pub const S_ISGID: u32 = 0o2000;
/// `S_ISTXT` (the sticky bit, `S_ISVTX` on Linux).
pub const S_ISTXT: u32 = 0o1000;
/// `S_IRWXU`.
pub const S_IRWXU: u32 = 0o0700;
/// `S_IRWXG`.
pub const S_IRWXG: u32 = 0o0070;
/// `S_IRWXO`.
pub const S_IRWXO: u32 = 0o0007;
/// `S_IRUSR`.
pub const S_IRUSR: u32 = 0o0400;
/// `S_IRGRP`.
pub const S_IRGRP: u32 = 0o0040;
/// `S_IROTH`.
pub const S_IROTH: u32 = 0o0004;
/// `S_IWUSR`.
pub const S_IWUSR: u32 = 0o0200;
/// `S_IWGRP`.
pub const S_IWGRP: u32 = 0o0020;
/// `S_IWOTH`.
pub const S_IWOTH: u32 = 0o0002;
/// `S_IXUSR`.
pub const S_IXUSR: u32 = 0o0100;
/// `S_IXGRP`.
pub const S_IXGRP: u32 = 0o0010;
/// `S_IXOTH`.
pub const S_IXOTH: u32 = 0o0001;
/// `S_IFDIR` (Linux value).
pub const S_IFDIR: u32 = 0o040000;

/// `STANDARD_BITS`.
pub const STANDARD_BITS: u32 = S_ISUID | S_ISGID | S_IRWXU | S_IRWXG | S_IRWXO;

const CMD2_CLR: u8 = 0x01;
const CMD2_SET: u8 = 0x02;
const CMD2_GBITS: u8 = 0x04;
const CMD2_OBITS: u8 = 0x08;
const CMD2_UBITS: u8 = 0x10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BitCmd {
    cmd: u8,
    cmd2: u8,
    bits: u32,
    /// The bits are `mask & bits` in C; the mask arrives at `getmode`.
    masked: bool,
}

/// A parsed symbolic mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModeSet {
    commands: Vec<BitCmd>,
}

fn byte_at(spec: &[u8], p: usize) -> u8 {
    spec.get(p).copied().unwrap_or(0)
}

fn addcmd(commands: &mut Vec<BitCmd>, op: u8, who: u32, bits: u32, masked: bool) {
    match op {
        b'=' => {
            commands.push(BitCmd {
                cmd: b'-',
                cmd2: 0,
                bits: if who != 0 { who } else { STANDARD_BITS },
                masked: false,
            });
            commands.push(BitCmd {
                cmd: b'+',
                cmd2: 0,
                bits,
                masked,
            });
        }
        b'+' | b'-' | b'X' => commands.push(BitCmd {
            cmd: op,
            cmd2: 0,
            bits,
            masked,
        }),
        _ => {}
    }
}

/// `addcmd` for the donor's `u`/`g`/`o` copy commands: the operator is the `oparg`.
fn add_copy(commands: &mut Vec<BitCmd>, cmd: u8, who: u32, op: u8) {
    let (cmd2, bits, masked) = if who != 0 {
        (
            (if who & S_IRUSR != 0 { CMD2_UBITS } else { 0 })
                | (if who & S_IRGRP != 0 { CMD2_GBITS } else { 0 })
                | (if who & S_IROTH != 0 { CMD2_OBITS } else { 0 }),
            u32::MAX,
            false,
        )
    } else {
        (CMD2_UBITS | CMD2_GBITS | CMD2_OBITS, 0, true)
    };
    let cmd2 = match op {
        b'+' => cmd2 | CMD2_SET,
        b'-' => cmd2 | CMD2_CLR,
        b'=' => cmd2 | CMD2_SET | CMD2_CLR,
        _ => cmd2,
    };
    commands.push(BitCmd {
        cmd,
        cmd2,
        bits,
        masked,
    });
}

/// Add one plain `+`/`-`/`X` command with the donor's `(who ? who : mask) & oparg` rule.
fn add_plain(commands: &mut Vec<BitCmd>, op: u8, who: u32, oparg: u32) {
    if who != 0 {
        addcmd(commands, op, who, who & oparg, false);
    } else {
        addcmd(commands, op, 0, oparg, true);
    }
}

/// `setmode`: parse a symbolic or octal mode.
#[must_use]
pub fn setmode(spec: &[u8]) -> Option<ModeSet> {
    if spec.is_empty() {
        return None;
    }
    let mut commands = Vec::new();
    let mut p = 0usize;

    if byte_at(spec, 0).is_ascii_digit() {
        let mut value: u32 = 0;
        let mut index = 0usize;
        let mut overflow = false;
        while index < spec.len() && (b'0'..=b'7').contains(&spec[index]) {
            value = match value
                .checked_mul(8)
                .and_then(|current| current.checked_add(u32::from(spec[index].wrapping_sub(b'0'))))
            {
                Some(next) => next,
                None => {
                    overflow = true;
                    u32::MAX
                }
            };
            index = index.wrapping_add(1);
        }
        if index != spec.len() || overflow || value & !(STANDARD_BITS | S_ISTXT) != 0 {
            return None;
        }
        // The donor's ABSOLUTE case is `ADDCMD('=', STANDARD_BITS|S_ISTXT, perm, mask)`.
        addcmd(&mut commands, b'=', STANDARD_BITS | S_ISTXT, value, false);
        commands.push(BitCmd {
            cmd: 0,
            cmd2: 0,
            bits: 0,
            masked: false,
        });
        return Some(ModeSet { commands });
    }

    let mut who: u32 = 0;
    let mut reset_who = true;
    loop {
        if reset_who {
            who = 0;
            reset_who = false;
        }
        loop {
            match byte_at(spec, p) {
                b'a' => who |= STANDARD_BITS,
                b'u' => who |= S_ISUID | S_IRWXU,
                b'g' => who |= S_ISGID | S_IRWXG,
                b'o' => who |= S_IRWXO,
                _ => break,
            }
            p = p.wrapping_add(1);
        }

        let op = byte_at(spec, p);
        if op != b'+' && op != b'-' && op != b'=' {
            return None;
        }
        p = p.wrapping_add(1);
        let mut equalopdone = false;
        who &= !S_ISTXT;
        let mut perm: u32 = 0;
        let mut perm_xbits: u32 = 0;
        loop {
            let c = byte_at(spec, p);
            match c {
                b'r' => perm |= S_IRUSR | S_IRGRP | S_IROTH,
                b's' => {
                    if who == 0 || who & !S_IRWXO != 0 {
                        perm |= S_ISUID | S_ISGID;
                    }
                }
                b't' => {
                    if who == 0 || who & !S_IRWXO != 0 {
                        who |= S_ISTXT;
                        perm |= S_ISTXT;
                    }
                }
                b'w' => perm |= S_IWUSR | S_IWGRP | S_IWOTH,
                b'X' => perm_xbits = S_IXUSR | S_IXGRP | S_IXOTH,
                b'x' => perm |= S_IXUSR | S_IXGRP | S_IXOTH,
                b'u' | b'g' | b'o' => {
                    if perm != 0 {
                        add_plain(&mut commands, op, who, perm);
                        perm = 0;
                    }
                    if op == b'=' {
                        equalopdone = true;
                    }
                    if op == b'+' && perm_xbits != 0 {
                        add_plain(&mut commands, b'X', who, perm_xbits);
                        perm_xbits = 0;
                    }
                    add_copy(&mut commands, c, who, op);
                    p = p.wrapping_add(1);
                }
                _ => {
                    if perm != 0 || (op == b'=' && !equalopdone) {
                        add_plain(&mut commands, op, who, perm);
                    }
                    if perm_xbits != 0 {
                        add_plain(&mut commands, b'X', who, perm_xbits);
                    }
                    break;
                }
            }
            if !matches!(c, b'u' | b'g' | b'o') {
                p = p.wrapping_add(1);
            }
        }

        if p >= spec.len() {
            break;
        }
        if spec[p] != b',' {
            // A missing comma continues the clause with the same `who` (the donor's
            // `goto getop` skips the `who = 0` initializer).
            continue;
        }
        p = p.wrapping_add(1);
        reset_who = true;
    }
    commands.push(BitCmd {
        cmd: 0,
        cmd2: 0,
        bits: 0,
        masked: false,
    });
    compress_mode(&mut commands);
    Some(ModeSet { commands })
}

fn compress_mode(commands: &mut Vec<BitCmd>) {
    let mut output: Vec<BitCmd> = Vec::new();
    let mut index = 0usize;
    loop {
        while commands.get(index).is_some_and(|command| {
            command.cmd != b'+' && command.cmd != b'-' && command.cmd != b'X'
        }) {
            let command = *commands.get(index).unwrap_or(&BitCmd {
                cmd: 0,
                cmd2: 0,
                bits: 0,
                masked: false,
            });
            output.push(command);
            index = index.wrapping_add(1);
            if command.cmd == 0 {
                *commands = output;
                return;
            }
        }
        let masked = commands.get(index).map(|command| command.masked);
        let mut setbits: u32 = 0;
        let mut clrbits: u32 = 0;
        let mut xbits: u32 = 0;
        while let Some(command) = commands.get(index) {
            if Some(command.masked) != masked {
                break;
            }
            match command.cmd {
                b'-' => {
                    clrbits |= command.bits;
                    setbits &= !command.bits;
                    xbits &= !command.bits;
                }
                b'+' => {
                    setbits |= command.bits;
                    clrbits &= !command.bits;
                    xbits &= !command.bits;
                }
                b'X' => xbits |= command.bits & !setbits,
                _ => break,
            }
            index = index.wrapping_add(1);
        }
        let masked = masked.unwrap_or(false);
        if clrbits != 0 {
            output.push(BitCmd {
                cmd: b'-',
                cmd2: 0,
                bits: clrbits,
                masked,
            });
        }
        if setbits != 0 {
            output.push(BitCmd {
                cmd: b'+',
                cmd2: 0,
                bits: setbits,
                masked,
            });
        }
        if xbits != 0 {
            output.push(BitCmd {
                cmd: b'X',
                cmd2: 0,
                bits: xbits,
                masked,
            });
        }
    }
}

impl ModeSet {
    /// `getmode`: apply the parsed operations to `omode` with the given `umask`.
    #[must_use]
    pub fn getmode(&self, omode: u32, umask: u32) -> u32 {
        let mask = !umask & 0o7777;
        let mut newmode = omode;
        let mut value: u32;
        for command in &self.commands {
            match command.cmd {
                b'u' => {
                    value = (newmode & S_IRWXU) >> 6;
                    newmode = apply_copy(newmode, command, value, mask);
                }
                b'g' => {
                    value = (newmode & S_IRWXG) >> 3;
                    newmode = apply_copy(newmode, command, value, mask);
                }
                b'o' => {
                    value = newmode & S_IRWXO;
                    newmode = apply_copy(newmode, command, value, mask);
                }
                b'+' => {
                    newmode |= if command.masked {
                        command.bits & mask
                    } else {
                        command.bits
                    }
                }
                b'-' => {
                    newmode &= !(if command.masked {
                        command.bits & mask
                    } else {
                        command.bits
                    })
                }
                b'X' => {
                    if omode & (S_IFDIR | S_IXUSR | S_IXGRP | S_IXOTH) != 0 {
                        newmode |= if command.masked {
                            command.bits & mask
                        } else {
                            command.bits
                        };
                    }
                }
                _ => return newmode,
            }
        }
        newmode
    }
}

fn apply_copy(newmode: u32, command: &BitCmd, value: u32, mask: u32) -> u32 {
    let bits = if command.masked { mask } else { command.bits };
    let mut mode = newmode;
    if command.cmd2 & CMD2_CLR != 0 {
        let clrval = if command.cmd2 & CMD2_SET != 0 {
            S_IRWXO
        } else {
            value
        };
        if command.cmd2 & CMD2_UBITS != 0 {
            mode &= !((clrval << 6) & bits);
        }
        if command.cmd2 & CMD2_GBITS != 0 {
            mode &= !((clrval << 3) & bits);
        }
        if command.cmd2 & CMD2_OBITS != 0 {
            mode &= !(clrval & bits);
        }
    }
    if command.cmd2 & CMD2_SET != 0 {
        if command.cmd2 & CMD2_UBITS != 0 {
            mode |= (value << 6) & bits;
        }
        if command.cmd2 & CMD2_GBITS != 0 {
            mode |= (value << 3) & bits;
        }
        if command.cmd2 & CMD2_OBITS != 0 {
            mode |= value & bits;
        }
    }
    mode
}

#[cfg(test)]
#[path = "setmode_ut.rs"]
mod tests;
