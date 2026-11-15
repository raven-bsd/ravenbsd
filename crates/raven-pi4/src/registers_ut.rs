// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the SDHCI register map and register-value decoding.

use super::*;

#[test]
fn register_offsets_and_widths_match_the_sdhci_map() {
    assert_eq!(Register::DmaAddress.offset(), 0x00);
    assert_eq!(Register::BlockSize.offset(), 0x04);
    assert_eq!(Register::BlockCount.offset(), 0x06);
    assert_eq!(Register::Argument.offset(), 0x08);
    assert_eq!(Register::TransferMode.offset(), 0x0c);
    assert_eq!(Register::Command.offset(), 0x0e);
    assert_eq!(Register::Response0.offset(), 0x10);
    assert_eq!(Register::Response3.offset(), 0x1c);
    assert_eq!(Register::DataPort.offset(), 0x20);
    assert_eq!(Register::PresentState.offset(), 0x24);
    assert_eq!(Register::HostControl.offset(), 0x28);
    assert_eq!(Register::PowerControl.offset(), 0x29);
    assert_eq!(Register::ClockControl.offset(), 0x2c);
    assert_eq!(Register::TimeoutControl.offset(), 0x2e);
    assert_eq!(Register::SoftwareReset.offset(), 0x2f);
    assert_eq!(Register::NormalInterruptStatus.offset(), 0x30);
    assert_eq!(Register::ErrorInterruptStatus.offset(), 0x32);
    assert_eq!(Register::Capabilities.offset(), 0x40);
    assert_eq!(Register::Capabilities2.offset(), 0x44);
    assert_eq!(Register::AdmaSystemAddress.offset(), 0x58);
    assert_eq!(Register::HostControllerVersion.offset(), 0xfe);
}

#[test]
fn register_widths_match_the_access_contract() {
    assert_eq!(Register::Command.width(), AccessWidth::HalfWord);
    assert_eq!(Register::ClockControl.width(), AccessWidth::HalfWord);
    assert_eq!(Register::HostControl.width(), AccessWidth::Byte);
    assert_eq!(Register::SoftwareReset.width(), AccessWidth::Byte);
    assert_eq!(Register::Capabilities.width(), AccessWidth::Word);
    assert_eq!(Register::Response2.width(), AccessWidth::Word);
    assert_eq!(Register::Command.mask(), 0xffff);
    assert_eq!(Register::PowerControl.mask(), 0xff);
    assert_eq!(Register::Argument.mask(), 0xffff_ffff);
}

#[test]
fn capabilities_decode_the_pi4_model_word() {
    let raw = (1 << 24) | (1 << 21) | (1 << 22) | (1 << 16) | (50 << 8);
    let caps = Capabilities::from_raw(raw);
    assert_eq!(caps.raw(), raw);
    assert!(caps.voltage_3v3());
    assert!(caps.high_speed_supported());
    assert!(caps.dma_supported());
    assert!(!caps.voltage_1v8());
    assert!(!caps.adma2_supported());
    assert!(!caps.eight_bit_supported());
    assert_eq!(caps.max_block_len(), 1024);
    assert_eq!(caps.base_clock_hz(), 50_000_000);
}

#[test]
fn capabilities_report_missing_bits_as_absent() {
    let caps = Capabilities::from_raw(0);
    assert!(!caps.voltage_3v3());
    assert!(!caps.high_speed_supported());
    assert!(!caps.dma_supported());
    assert_eq!(caps.max_block_len(), 512);
    assert_eq!(caps.base_clock_hz(), 0);
}

#[test]
fn normal_and_error_interrupts_decode() {
    let normal = NormalInterrupt::from_raw(NORMAL_CMD_COMPLETE | NORMAL_ERROR_INTERRUPT);
    assert!(normal.command_complete());
    assert!(normal.error_interrupt());
    assert!(!normal.transfer_complete());
    assert_eq!(normal.raw(), NORMAL_CMD_COMPLETE | NORMAL_ERROR_INTERRUPT);

    let error = ErrorInterrupt::from_raw(ERROR_CMD_TIMEOUT | 0x8000);
    assert!(error.any());
    assert_eq!(error.raw(), ERROR_CMD_TIMEOUT);
    assert!(!ErrorInterrupt::from_raw(0).any());
}

#[test]
fn present_state_exposes_the_inhibit_and_buffer_bits() {
    let state = PresentState::from_raw(
        PRESENT_CMD_INHIBIT | PRESENT_DAT_INHIBIT | PRESENT_BUFFER_READ_ENABLE,
    );
    assert!(state.cmd_inhibit());
    assert!(state.dat_inhibit());
    assert!(state.buffer_read_ready());
    assert!(!state.buffer_write_ready());
    assert_eq!(state.raw(), 0x803);
}

#[test]
fn pi4_quirks_record_the_hostcaps_and_speed_constraints() {
    let quirks = Quirks::PI4_EMMC2;
    assert!(!quirks.host_capabilities_valid);
    assert!(quirks.no_high_speed_bit);
    assert!(quirks.no_power_0);
    assert!(quirks.no_1v8);
    assert!(quirks.thirty_two_bit_access);
    assert_eq!(DEFAULT_BASE_CLOCK_HZ, 50_000_000);
    assert_eq!(CLOCK_ID_EMMC2, 12);
    assert_eq!(EMMC2_WINDOW_BYTES, 0x100);
}
