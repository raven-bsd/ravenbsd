// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded integer formatting for `net:` paths without `core::fmt`.

/// Write the decimal form of `value` into `out`; returns the written slice.
pub fn decimal(mut value: u32, out: &mut [u8]) -> Option<&[u8]> {
    let mut digits = [0u8; 10];
    let mut count = 0usize;
    loop {
        let digit = (value % 10) as u8;
        value /= 10;
        if let Some(slot) = digits.get_mut(count) {
            *slot = b'0' + digit;
        }
        count = count.saturating_add(1);
        if value == 0 {
            break;
        }
    }
    if out.len() < count {
        return None;
    }
    for index in 0..count {
        let source = digits.get(count.saturating_sub(index).saturating_sub(1))?;
        let target = out.get_mut(index)?;
        *target = *source;
    }
    out.get(..count)
}

/// Write a dotted IPv4 address into `out`; returns the written slice.
pub fn ipv4(octets: [u8; 4], out: &mut [u8]) -> Option<&[u8]> {
    let mut position = 0usize;
    for (index, octet) in octets.iter().enumerate() {
        if index != 0 {
            let slot = out.get_mut(position)?;
            *slot = b'.';
            position = position.saturating_add(1);
        }
        let used = decimal(u32::from(*octet), out.get_mut(position..)?)?;
        position = position.saturating_add(used.len());
    }
    out.get(..position)
}

#[cfg(test)]
#[path = "itoa_ut.rs"]
mod tests;
