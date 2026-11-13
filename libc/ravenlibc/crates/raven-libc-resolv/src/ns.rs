// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ns_initparse`, `ns_parserr`, `ns_skiprr` and the nameser helpers.

use alloc::string::String;

use crate::consts::DNS_HEADER_LEN;
use crate::name::{dn_expand, dn_skipname};

/// The question section.
pub const NS_S_QD: i32 = 0;
/// The answer section.
pub const NS_S_AN: i32 = 1;
/// The authority section.
pub const NS_S_NS: i32 = 2;
/// The additional section.
pub const NS_S_AR: i32 = 3;
/// One past the last section.
pub const NS_S_MAX: i32 = 4;

/// The `_ns_flagdata` table: `(mask, shift)`.
pub static NS_FLAGDATA: [(u16, u8); 16] = [
    (0x8000, 15),
    (0x7800, 11),
    (0x0400, 10),
    (0x0200, 9),
    (0x0100, 8),
    (0x0080, 7),
    (0x0040, 6),
    (0x0020, 5),
    (0x0010, 4),
    (0x000f, 0),
    (0x0000, 0),
    (0x0000, 0),
    (0x0000, 0),
    (0x0000, 0),
    (0x0000, 0),
    (0x0000, 0),
];

/// `ns_get16`.
#[must_use]
pub fn ns_get16(cp: &[u8]) -> Option<u16> {
    crate::be16_at(cp, 0)
}

/// `ns_get32`.
#[must_use]
pub fn ns_get32(cp: &[u8]) -> Option<u32> {
    crate::be32_at(cp, 0)
}

/// `ns_put16`.
pub fn ns_put16(value: u16, cp: &mut [u8]) -> bool {
    let bytes = value.to_be_bytes();
    match cp.get_mut(..2) {
        Some(slot) => {
            slot.copy_from_slice(&bytes);
            true
        }
        None => false,
    }
}

/// `ns_put32`.
pub fn ns_put32(value: u32, cp: &mut [u8]) -> bool {
    let bytes = value.to_be_bytes();
    match cp.get_mut(..4) {
        Some(slot) => {
            slot.copy_from_slice(&bytes);
            true
        }
        None => false,
    }
}

/// A parsed nameser message handle.
#[derive(Clone, Debug)]
pub struct NsMsg<'a> {
    msg: &'a [u8],
    id: u16,
    flags: u16,
    counts: [u16; 4],
    sections: [Option<usize>; 4],
    sect: i32,
    rrnum: i32,
    msg_ptr: Option<usize>,
}

impl<'a> NsMsg<'a> {
    /// The message ID.
    #[must_use]
    pub const fn id(&self) -> u16 {
        self.id
    }

    /// The flags word.
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.flags
    }

    /// The record count of one section.
    #[must_use]
    pub fn count(&self, section: i32) -> Option<u16> {
        match usize::try_from(section) {
            Ok(index) => self.counts.get(index).copied(),
            Err(_) => None,
        }
    }

    /// The message bytes.
    #[must_use]
    pub const fn message(&self) -> &'a [u8] {
        self.msg
    }
}

/// One parsed resource record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NsRr<'a> {
    /// The owner name.
    pub name: String,
    /// The record type.
    pub rr_type: u16,
    /// The record class.
    pub rr_class: u16,
    /// The TTL (0 for questions).
    pub ttl: u32,
    /// The rdata (empty for questions).
    pub rdata: &'a [u8],
}

/// `ns_skiprr`: the number of bytes in `count` records of `section`.
#[must_use]
pub fn ns_skiprr(msg: &[u8], ptr: usize, eom: usize, section: i32, count: i32) -> Option<usize> {
    if count < 0 || eom > msg.len() || ptr > eom {
        return None;
    }
    let mut p = ptr;
    let mut left = count;
    while left > 0 {
        left = left.saturating_sub(1);
        let name = dn_skipname(msg.get(..eom)?, p)?;
        p = p.checked_add(name)?;
        if eom.saturating_sub(p) < 4 {
            return None;
        }
        p = p.checked_add(4)?;
        if section != NS_S_QD {
            if eom.saturating_sub(p) < 6 {
                return None;
            }
            p = p.checked_add(4)?;
            let rdlen = usize::from(crate::be16_at(msg, p)?);
            p = p.checked_add(2)?;
            if rdlen > eom.saturating_sub(p) {
                return None;
            }
            p = p.checked_add(rdlen)?;
        }
    }
    p.checked_sub(ptr)
}

/// `ns_initparse`.
#[must_use]
pub fn ns_initparse(msg: &[u8]) -> Option<NsMsg<'_>> {
    if msg.len() < DNS_HEADER_LEN {
        return None;
    }
    let id = crate::be16_at(msg, 0)?;
    let flags = crate::be16_at(msg, 2)?;
    let mut counts = [0u16; 4];
    for index in 0..4usize {
        let at = 4usize.saturating_add(index.saturating_mul(2));
        let value = crate::be16_at(msg, at)?;
        if let Some(slot) = counts.get_mut(index) {
            *slot = value;
        }
    }
    let eom = msg.len();
    let mut p = DNS_HEADER_LEN;
    let mut sections = [None; 4];
    for index in 0..4usize {
        let count = counts.get(index).copied().unwrap_or(0);
        if count == 0 {
            continue;
        }
        if let Some(slot) = sections.get_mut(index) {
            *slot = Some(p);
        }
        let section = i32::try_from(index).ok()?;
        let skipped = ns_skiprr(msg, p, eom, section, i32::from(count))?;
        p = p.checked_add(skipped)?;
    }
    if p != eom {
        return None;
    }
    Some(NsMsg {
        msg,
        id,
        flags,
        counts,
        sections,
        sect: NS_S_MAX,
        rrnum: -1,
        msg_ptr: None,
    })
}

/// `ns_parserr`.
pub fn ns_parserr<'a>(handle: &mut NsMsg<'a>, section: i32, rrnum: i32) -> Option<NsRr<'a>> {
    let section_index = usize::try_from(section).ok()?;
    if !(NS_S_QD..NS_S_MAX).contains(&section) {
        return None;
    }
    let count = *handle.counts.get(section_index)?;
    if section != handle.sect {
        handle.sect = section;
        handle.rrnum = 0;
        handle.msg_ptr = *handle.sections.get(section_index)?;
    }
    let number = if rrnum == -1 { handle.rrnum } else { rrnum };
    if number < 0 || number >= i32::from(count) {
        return None;
    }
    if number < handle.rrnum {
        handle.rrnum = 0;
        handle.msg_ptr = *handle.sections.get(section_index)?;
    }
    if number > handle.rrnum {
        let current = handle.msg_ptr?;
        let skipped = ns_skiprr(
            handle.msg,
            current,
            handle.msg.len(),
            section,
            number.saturating_sub(handle.rrnum),
        )?;
        handle.msg_ptr = Some(current.checked_add(skipped)?);
        handle.rrnum = number;
    }
    let mut ptr = handle.msg_ptr?;
    let mut name_buffer = [0u8; 256];
    let (consumed, name_len) = dn_expand(handle.msg, ptr, &mut name_buffer)?;
    ptr = ptr.checked_add(consumed)?;
    let name = String::from(core::str::from_utf8(name_buffer.get(..name_len)?).ok()?);
    let rr_type = crate::be16_at(handle.msg, ptr)?;
    let rr_class = crate::be16_at(handle.msg, ptr.checked_add(2)?)?;
    ptr = ptr.checked_add(4)?;
    let (ttl, rdata) = if section != NS_S_QD {
        if handle.msg.len().saturating_sub(ptr) < 6 {
            return None;
        }
        let ttl = crate::be32_at(handle.msg, ptr)?;
        let rdlength = usize::from(crate::be16_at(handle.msg, ptr.checked_add(4)?)?);
        ptr = ptr.checked_add(6)?;
        let end = ptr.checked_add(rdlength)?;
        if end > handle.msg.len() {
            return None;
        }
        (ttl, handle.msg.get(ptr..end)?)
    } else {
        (0, &[][..])
    };
    ptr = ptr.checked_add(rdata.len())?;
    handle.msg_ptr = Some(ptr);
    handle.rrnum = number.saturating_add(1);
    if handle.rrnum > i32::from(count) {
        handle.sect = section.saturating_add(1);
        if handle.sect == NS_S_MAX {
            handle.rrnum = -1;
            handle.msg_ptr = None;
        } else {
            handle.rrnum = 0;
        }
    }
    Some(NsRr {
        name,
        rr_type,
        rr_class,
        ttl,
        rdata,
    })
}

/// `ns_name_uncompress`.
pub fn ns_name_uncompress(
    msg: &[u8],
    eom: usize,
    src: usize,
    dst: &mut [u8; 256],
) -> Option<(usize, usize)> {
    if eom > msg.len() {
        return None;
    }
    dn_expand(msg.get(..eom)?, src, dst)
}

#[cfg(test)]
#[path = "ns_ut.rs"]
mod tests;
