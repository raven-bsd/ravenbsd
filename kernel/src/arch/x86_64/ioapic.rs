// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IOAPIC redirection table programming.

#![allow(dead_code)] // route/mask are the S07 driver and S21 device-irq API.

use core::fmt;

use raven_rmm::MapError;
use spin::Mutex;

use crate::arch::x86_64::acpi;
use crate::memory::mmio;

/// IOAPIC register select offset.
const IOREGSEL: usize = 0x00;
/// IOAPIC data window offset.
const IOWIN: usize = 0x10;

#[derive(Clone, Copy)]
struct State {
    base: usize,
    gsi_base: u32,
    max_entry: u8,
}

static IOAPIC: Mutex<Option<State>> = Mutex::new(None);

/// Why an IOAPIC operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoApicError {
    /// No IOAPIC was mapped.
    NotMapped,
    /// The GSI is outside this IOAPIC's redirection table.
    OutOfRange,
    /// The MMIO page could not be mapped.
    Map(MapError),
}

impl fmt::Display for IoApicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoApicError::NotMapped => f.write_str("no IOAPIC mapped"),
            IoApicError::OutOfRange => f.write_str("GSI outside the IOAPIC table"),
            IoApicError::Map(error) => write!(f, "IOAPIC MMIO map failed: {error}"),
        }
    }
}

/// Map the first MADT IOAPIC and mask every redirection entry.
pub fn init() -> Result<(), IoApicError> {
    let info = acpi::info();
    let Some(first) = info.io_apics.first() else {
        return Ok(());
    };
    let base = mmio::map_mmio(first.address, 0x1000).map_err(IoApicError::Map)? as usize;
    let version = read_reg(base, 1);
    let max_entry = ((version >> 16) & 0xFF) as u8;
    let state = State {
        base,
        gsi_base: first.gsi_base,
        max_entry,
    };
    mask_all_state(&state);
    *IOAPIC.lock() = Some(state);
    Ok(())
}

/// Translate an ISA IRQ through the MADT overrides.
pub fn gsi_for_isa(irq: u8) -> u32 {
    acpi::info().gsi_for_isa(irq)
}

/// Route a GSI to `vector` on `destination`'s local APIC.
pub fn route(
    gsi: u32,
    vector: u8,
    destination: u32,
    active_low: bool,
    level: bool,
) -> Result<(), IoApicError> {
    let guard = IOAPIC.lock();
    let state = guard.as_ref().ok_or(IoApicError::NotMapped)?;
    let index = gsi
        .checked_sub(state.gsi_base)
        .ok_or(IoApicError::OutOfRange)?;
    if index > u32::from(state.max_entry) {
        return Err(IoApicError::OutOfRange);
    }

    let mut low = u32::from(vector);
    if active_low {
        low |= 1 << 13;
    }
    if level {
        low |= 1 << 15;
    }
    let high = (destination & 0xFF) << 24;
    write_redirection(state, index as u8, low, high);
    Ok(())
}

/// Mask one GSI.
pub fn mask(gsi: u32) -> Result<(), IoApicError> {
    let guard = IOAPIC.lock();
    let state = guard.as_ref().ok_or(IoApicError::NotMapped)?;
    let index = gsi
        .checked_sub(state.gsi_base)
        .ok_or(IoApicError::OutOfRange)?;
    if index > u32::from(state.max_entry) {
        return Err(IoApicError::OutOfRange);
    }
    let mut entry = read_redirection(state, index as u8);
    entry |= 1 << 16;
    write_redirection(state, index as u8, entry as u32, (entry >> 32) as u32);
    Ok(())
}

/// Mask every redirection entry of the mapped IOAPIC.
pub fn mask_all() {
    let guard = IOAPIC.lock();
    if let Some(state) = guard.as_ref() {
        mask_all_state(state);
    }
}

fn mask_all_state(state: &State) {
    let mut index = 0u8;
    loop {
        let mut entry = read_redirection(state, index);
        entry |= 1 << 16;
        write_redirection(state, index, entry as u32, (entry >> 32) as u32);
        if index >= state.max_entry {
            break;
        }
        index = index.saturating_add(1);
    }
}

fn read_reg(base: usize, register: u8) -> u32 {
    let select = base.saturating_add(IOREGSEL) as *mut u32;
    let window = base.saturating_add(IOWIN) as *const u32;
    // SAFETY: the IOAPIC page is mapped uncached; the register numbers are architectural and
    // the MMIO window is at the documented offset.
    unsafe {
        core::ptr::write_volatile(select, u32::from(register));
        core::ptr::read_volatile(window)
    }
}

fn write_reg(base: usize, register: u8, value: u32) {
    let select = base.saturating_add(IOREGSEL) as *mut u32;
    let window = base.saturating_add(IOWIN) as *mut u32;
    // SAFETY: same invariant as `read_reg`.
    unsafe {
        core::ptr::write_volatile(select, u32::from(register));
        core::ptr::write_volatile(window, value);
    }
}

fn read_redirection(state: &State, index: u8) -> u64 {
    let low = read_reg(state.base, 0x10u8.saturating_add(index.saturating_mul(2)));
    let high = read_reg(
        state.base,
        0x10u8
            .saturating_add(index.saturating_mul(2))
            .saturating_add(1),
    );
    u64::from(low) | (u64::from(high) << 32)
}

fn write_redirection(state: &State, index: u8, low: u32, high: u32) {
    write_reg(
        state.base,
        0x10u8.saturating_add(index.saturating_mul(2)),
        low,
    );
    write_reg(
        state.base,
        0x10u8
            .saturating_add(index.saturating_mul(2))
            .saturating_add(1),
        high,
    );
}
