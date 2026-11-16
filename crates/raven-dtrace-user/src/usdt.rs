// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USDT nop-site marker and `.note.stapsdt` parsing.
//!
//! A USDT probe site is a one-instruction marker plus metadata: the marker is an architectural
//! `nop`, the metadata is an ELF note naming the provider, the probe and its arguments. The
//! kernel provider has no user-probe enabler yet, so this module provides the marker, the
//! descriptor table and the note parser the consumer needs; firing a user probe is a follow-up.

/// One statically declared USDT marker.
pub struct UsdtProbe {
    /// Provider name.
    pub provider: &'static str,
    /// Probe name.
    pub name: &'static str,
    /// The marker entry point; its first instruction is the nop site.
    pub site: unsafe extern "C" fn(),
    /// Argument names, in argument-register order.
    pub args: &'static [&'static str],
}

impl UsdtProbe {
    /// The marker site address.
    #[must_use]
    pub fn site_address(&self) -> usize {
        self.site as *const () as usize
    }
}

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .balign 16
    .global raven_usdt_nop_site
    .type raven_usdt_nop_site, @function
raven_usdt_nop_site:
    nop
    ret
    .size raven_usdt_nop_site, . - raven_usdt_nop_site
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .balign 16
    .global raven_usdt_nop_site
    .type raven_usdt_nop_site, %function
raven_usdt_nop_site:
    nop
    ret
    .size raven_usdt_nop_site, . - raven_usdt_nop_site
"#
);

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
unsafe extern "C" {
    /// The one-instruction marker; the consumer patches it when a kernel enabler exists.
    fn raven_usdt_nop_site();
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
static USDT_SITES: [UsdtProbe; 1] = [UsdtProbe {
    provider: "raven",
    name: "usdt-test",
    site: raven_usdt_nop_site,
    args: &["arg0"],
}];

/// The static USDT marker table.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub static USDT_PROBES: &[UsdtProbe] = &USDT_SITES;

/// The marker instruction byte(s) on this architecture: x86-64 `nop` is `0x90`, aarch64 `nop` is
/// `0xd503201f`.
pub const fn nop_opcode() -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        0x90
    }
    #[cfg(target_arch = "aarch64")]
    {
        0xd503_201f
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        0
    }
}

/// Address of the marker instruction.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[must_use]
pub fn site_address() -> usize {
    raven_usdt_nop_site as *const () as usize
}

/// Read the marker instruction at the site.
#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
#[must_use]
pub fn site_opcode() -> u32 {
    let address = site_address();
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: the address is the first byte of the marker function, mapped executable and
        // readable in the current process.
        u32::from(unsafe { core::ptr::read_volatile(address as *const u8) })
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: the address is the first instruction of the marker function, mapped executable
        // and readable in the current process.
        unsafe { core::ptr::read_volatile(address as *const u32) }
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[must_use]
pub fn site_address() -> usize {
    0
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[must_use]
pub fn site_opcode() -> u32 {
    0
}

/// One decoded `.note.stapsdt` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsdtNote<'a> {
    /// Virtual address of the marker.
    pub address: u64,
    /// Load base the address is relative to.
    pub base: u64,
    /// Semaphore address (zero when the probe has no semaphore).
    pub semaphore: u64,
    /// Provider name.
    pub provider: &'a str,
    /// Probe name.
    pub name: &'a str,
    /// Argument description string.
    pub args: &'a str,
}

/// Why a note could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteError {
    /// The note or descriptor is truncated.
    TooShort,
    /// The note name is not `stapsdt`.
    BadName,
    /// The note type is not 3.
    BadType,
    /// A descriptor string is missing its NUL terminator or is not UTF-8.
    BadString,
}

/// Parse the first `.note.stapsdt` note in `data`.
///
/// The ELF note layout is `namesz`, `descsz`, `type` (three little-endian `u32`s), a 4-byte
/// aligned name, then a 4-byte aligned descriptor. The descriptor is three 64-bit words (address,
/// base, semaphore) followed by the NUL-terminated provider, probe and argument strings.
///
/// # Errors
///
/// [`NoteError`] for a truncated note, a non-`stapsdt` name, a non-3 type or a malformed string.
pub fn parse_note(data: &[u8]) -> Result<UsdtNote<'_>, NoteError> {
    let namesz = read_u32(data, 0)? as usize;
    let descsz = read_u32(data, 4)? as usize;
    let note_type = read_u32(data, 8)?;
    if note_type != 3 {
        return Err(NoteError::BadType);
    }
    let name_end = 12usize.checked_add(namesz).ok_or(NoteError::TooShort)?;
    let name = data
        .get(12..name_end)
        .ok_or(NoteError::TooShort)?
        .strip_suffix(&[0])
        .unwrap_or(&[]);
    if name != b"stapsdt" {
        return Err(NoteError::BadName);
    }
    let desc_start = align4(name_end)?;
    let desc_end = desc_start.checked_add(descsz).ok_or(NoteError::TooShort)?;
    let desc = data.get(desc_start..desc_end).ok_or(NoteError::TooShort)?;
    if desc.len() < 24 {
        return Err(NoteError::TooShort);
    }
    let address = read_u64(desc, 0)?;
    let base = read_u64(desc, 8)?;
    let semaphore = read_u64(desc, 16)?;
    let mut rest = desc.get(24..).ok_or(NoteError::TooShort)?;
    let provider = take_str(&mut rest)?;
    let probe = take_str(&mut rest)?;
    let args = take_str(&mut rest)?;
    Ok(UsdtNote {
        address,
        base,
        semaphore,
        provider,
        name: probe,
        args,
    })
}

fn align4(value: usize) -> Result<usize, NoteError> {
    value
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or(NoteError::TooShort)
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, NoteError> {
    let bytes = data
        .get(offset..offset.saturating_add(4))
        .ok_or(NoteError::TooShort)?;
    let mut word = [0u8; 4];
    if let (Some(target), Some(source)) = (word.get_mut(..), bytes.get(..4)) {
        target.copy_from_slice(source);
    }
    Ok(u32::from_le_bytes(word))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, NoteError> {
    let bytes = data
        .get(offset..offset.saturating_add(8))
        .ok_or(NoteError::TooShort)?;
    let mut word = [0u8; 8];
    if let (Some(target), Some(source)) = (word.get_mut(..), bytes.get(..8)) {
        target.copy_from_slice(source);
    }
    Ok(u64::from_le_bytes(word))
}

fn take_str<'a>(rest: &mut &'a [u8]) -> Result<&'a str, NoteError> {
    let end = rest
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(NoteError::BadString)?;
    let bytes = rest.get(..end).ok_or(NoteError::BadString)?;
    *rest = rest
        .get(end.saturating_add(1)..)
        .ok_or(NoteError::BadString)?;
    core::str::from_utf8(bytes).map_err(|_| NoteError::BadString)
}

#[cfg(test)]
#[path = "usdt_ut.rs"]
mod tests;
