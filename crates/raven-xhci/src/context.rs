// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Input-context encoding for Address Device and Configure Endpoint.
//!
//! The structures follow the xHCI 1.1 input context: a 32-byte input control context followed by
//! 32 device contexts whose size is 32 or 64 bytes (`HCCPARAMS1.CSZ`). Only slot 1, EP0 (DCI 1)
//! and the interrupt IN endpoint (DCI 3) are used by the USB keyboard path. The encoding is pure
//! and host-tested; the controller writes it into DMA.

use crate::error::XhciError;

/// Largest input context: 32-byte control context plus 32 64-byte contexts.
pub const MAX_INPUT_CONTEXT: usize = 32 + 32 * 64;

/// Device context size from `HCCPARAMS1.CSZ`.
#[must_use]
pub const fn context_size(csz: bool) -> usize {
    if csz { 64 } else { 32 }
}

/// Input context size for `context_size`.
#[must_use]
pub const fn input_context_size(context_size: usize) -> usize {
    32 + 32 * context_size
}

fn put_u32(out: &mut [u8], offset: usize, value: u32) -> Result<(), XhciError> {
    let end = offset.checked_add(4).ok_or(XhciError::Length)?;
    let slot = out.get_mut(offset..end).ok_or(XhciError::Length)?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn put_u64(out: &mut [u8], offset: usize, value: u64) -> Result<(), XhciError> {
    let end = offset.checked_add(8).ok_or(XhciError::Length)?;
    let slot = out.get_mut(offset..end).ok_or(XhciError::Length)?;
    slot.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Encode the Address Device input context.
///
/// `ep0_bus` is the device-visible address of the EP0 transfer ring; `max_packet` is the initial
/// control endpoint packet size (8 for a full-speed device until its descriptor is read).
///
/// # Errors
///
/// Returns [`XhciError::Length`] for an invalid context size or a short output buffer.
pub fn encode_address_input(
    context_size: usize,
    port: u8,
    speed: u8,
    ep0_bus: u64,
    max_packet: u16,
    out: &mut [u8],
) -> Result<(), XhciError> {
    encode_slot_and_ep0(context_size, port, speed, ep0_bus, max_packet, 1, 0x3, out)
}

/// Encode the Configure Endpoint input context for the interrupt IN endpoint (DCI 3).
///
/// # Errors
///
/// Returns [`XhciError::Length`] for an invalid context size or a short output buffer.
// The eight inputs are the two bus addresses, the slot/port/speed, packet/interval and output;
// grouping them in a struct would only move the same arguments to a temporary.
#[allow(clippy::too_many_arguments)]
pub fn encode_configure_input(
    context_size: usize,
    port: u8,
    speed: u8,
    ep0_bus: u64,
    ep1_bus: u64,
    max_packet: u16,
    interval: u8,
    out: &mut [u8],
) -> Result<(), XhciError> {
    encode_slot_and_ep0(context_size, port, speed, ep0_bus, max_packet, 3, 0x9, out)?;
    let ep1 = 32 + 3 * context_size;
    put_u32(out, ep1, u32::from(interval) << 16)?;
    // CErr 3, EP Type 7 (Interrupt IN), max packet size in the upper half.
    put_u32(
        out,
        ep1 + 4,
        (3 << 1) | (7 << 3) | (u32::from(max_packet) << 16),
    )?;
    put_u64(out, ep1 + 8, ep1_bus | 1)?;
    put_u32(out, ep1 + 16, 8)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_slot_and_ep0(
    context_size: usize,
    port: u8,
    speed: u8,
    ep0_bus: u64,
    max_packet: u16,
    entries: u32,
    add_flags: u32,
    out: &mut [u8],
) -> Result<(), XhciError> {
    if context_size != 32 && context_size != 64 {
        return Err(XhciError::Length);
    }
    let needed = 32 + 32 * context_size;
    if out.len() < needed {
        return Err(XhciError::Length);
    }
    out.fill(0);
    put_u32(out, 4, add_flags)?;

    let slot = 32;
    put_u32(out, slot, (entries << 27) | (u32::from(speed & 0x0f) << 20))?;
    put_u32(out, slot + 4, u32::from(port) << 16)?;

    let ep0 = 32 + context_size;
    put_u32(
        out,
        ep0 + 4,
        (3 << 1) | (4 << 3) | (u32::from(max_packet) << 16),
    )?;
    put_u64(out, ep0 + 8, ep0_bus | 1)?;
    put_u32(out, ep0 + 16, 8)?;
    Ok(())
}

#[cfg(test)]
#[path = "context_ut.rs"]
mod tests;
