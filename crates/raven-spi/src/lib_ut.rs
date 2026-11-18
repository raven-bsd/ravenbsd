// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the BCM2711 SPI0 master core.

use super::*;

fn spi() -> Spi<MockRegisters> {
    Spi::new(MockRegisters::new(), CORE_CLOCK_HZ)
}

#[test]
fn constants_are_the_bcm2711_spi0_window() {
    assert_eq!(SPI0_BASE, 0xfe20_4000);
    assert_eq!(SPI0_WINDOW, 0x18);
    assert_eq!(REG_CS, 0x00);
    assert_eq!(REG_FIFO, 0x04);
    assert_eq!(REG_CLK, 0x08);
    assert_eq!(REG_DLEN, 0x0c);
    assert_eq!(REG_LTOH, 0x10);
    assert_eq!(REG_DC, 0x14);
    assert_eq!(PIN_CE0, 8);
    assert_eq!(PIN_CE1, 7);
    assert_eq!(PIN_MISO, 9);
    assert_eq!(PIN_MOSI, 10);
    assert_eq!(PIN_SCLK, 11);
    assert_eq!(PINMUX_ALT0, 4);
    assert_eq!(ChipSelect::Ce0.bits(), 0);
    assert_eq!(ChipSelect::Ce1.bits(), 1);
}

#[test]
fn divisor_is_rounded_up_and_bounded() {
    assert_eq!(divisor_for(250_000_000, 4_000_000), Ok(63));
    assert_eq!(divisor_for(250_000_000, 1_000_000), Ok(250));
    assert_eq!(divisor_for(1, 1), Ok(2));
    assert_eq!(divisor_for(0, 1), Err(Error::InvalidClock));
    assert_eq!(divisor_for(1, 0), Err(Error::InvalidClock));
    assert_eq!(divisor_for(u32::MAX, 1), Err(Error::DivisorTooLarge));
    assert_eq!(divisor_for(250_000_000, 1), Err(Error::DivisorTooLarge));
}

#[test]
fn configure_programs_clock_mode_and_idle_cs() {
    let mut spi = spi();
    assert_eq!(spi.configure(4_000_000), Ok(63));
    let writes = &spi.registers().writes;
    assert!(writes.contains(&(REG_CLK, 63)));
    assert!(writes.contains(&(REG_LTOH, 0)));
    assert!(writes.contains(&(REG_DC, 0)));
    assert!(writes.contains(&(REG_CS, 0)));
}

#[test]
fn transfer_preserves_byte_order_and_loopback() {
    let mut spi = spi();
    let tx = [0x12, 0x34, 0x56];
    let mut rx = [0u8; 3];
    spi.transfer(ChipSelect::Ce0, &tx, &mut rx).unwrap();
    assert_eq!(rx, tx);

    let writes = &spi.registers().writes;
    assert_eq!(writes.first(), Some(&(REG_CS, CS_CLEAR_RX | CS_CLEAR_TX)));
    assert!(writes.contains(&(REG_DLEN, 3)));
    assert!(writes.contains(&(REG_CS, CS_TA)));
    let fifo: std::vec::Vec<u32> = writes
        .iter()
        .filter(|(offset, _)| *offset == REG_FIFO)
        .map(|(_, value)| *value)
        .collect();
    assert_eq!(fifo, [0x12, 0x34, 0x56]);
    assert_eq!(writes.last(), Some(&(REG_CS, 0)));
}

#[test]
fn chip_select_is_encoded_for_ce0_and_ce1() {
    let mut ce0 = spi();
    ce0.transfer(ChipSelect::Ce0, &[1], &mut [0]).unwrap();
    assert!(ce0.registers().writes.contains(&(REG_CS, CS_TA)));

    let mut ce1 = spi();
    ce1.transfer(ChipSelect::Ce1, &[1], &mut [0]).unwrap();
    assert!(
        ce1.registers()
            .writes
            .contains(&(REG_CS, CS_TA | ChipSelect::Ce1.bits()))
    );
}

#[test]
fn write_only_drains_the_fifo() {
    let mut spi = spi();
    spi.write(ChipSelect::Ce0, &[0xaa, 0x55]).unwrap();
    let writes = &spi.registers().writes;
    assert!(writes.contains(&(REG_DLEN, 2)));
    assert_eq!(writes.last(), Some(&(REG_CS, 0)));
}

#[test]
fn length_mismatch_is_rejected_before_any_register_write() {
    let mut spi = spi();
    let mut rx = [0u8; 1];
    assert_eq!(
        spi.transfer(ChipSelect::Ce0, &[1, 2], &mut rx),
        Err(Error::LengthMismatch)
    );
    assert!(spi.registers().writes.is_empty());
}

#[test]
fn empty_transfer_does_not_touch_the_bus() {
    let mut spi = spi();
    spi.transfer(ChipSelect::Ce0, &[], &mut []).unwrap();
    spi.write(ChipSelect::Ce0, &[]).unwrap();
    assert!(spi.registers().writes.is_empty());
}

#[test]
fn tx_fifo_full_times_out() {
    let mut spi = spi();
    spi.registers_mut().tx_space = false;
    spi.set_timeout_polls(4);
    let mut rx = [0u8; 1];
    assert_eq!(
        spi.transfer(ChipSelect::Ce0, &[0x11], &mut rx),
        Err(Error::Timeout)
    );
}

#[test]
fn rx_fifo_empty_times_out() {
    let mut spi = spi();
    spi.registers_mut().rx_enabled = false;
    spi.set_timeout_polls(4);
    let mut rx = [0u8; 1];
    assert_eq!(
        spi.transfer(ChipSelect::Ce0, &[0x11], &mut rx),
        Err(Error::Timeout)
    );
}

#[test]
fn back_to_back_transfers_toggle_cs_each_time() {
    let mut spi = spi();
    let mut first = [0u8; 1];
    let mut second = [0u8; 1];
    spi.transfer(ChipSelect::Ce0, &[0x01], &mut first).unwrap();
    spi.transfer(ChipSelect::Ce0, &[0x02], &mut second).unwrap();
    assert_eq!(first, [0x01]);
    assert_eq!(second, [0x02]);
    let writes = &spi.registers().writes;
    let idle = writes
        .iter()
        .filter(|(offset, value)| *offset == REG_CS && *value == 0)
        .count();
    assert!(idle >= 2, "each transfer must deassert CS");
    assert_eq!(writes.last(), Some(&(REG_CS, 0)));
}
