// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for MDIO encoding and the busy-poll budget.

use super::*;
use crate::regs::MDIO_CMD;

#[test]
fn a_read_command_encodes_the_phy_and_register() {
    let command = MdioCommand::read(1, 0x02).expect("read");
    let value = command.register_value();
    assert_eq!(value & MDIO_START_BUSY, MDIO_START_BUSY);
    assert_eq!(value & MDIO_READ, MDIO_READ);
    assert_eq!(value & MDIO_WRITE, 0);
    assert_eq!((value & MDIO_PMD_MASK) >> MDIO_PMD_SHIFT, 1);
    assert_eq!((value & MDIO_REG_MASK) >> MDIO_REG_SHIFT, 2);
    assert_eq!(value & MDIO_DATA_MASK, 0);
    assert!(command.is_read());
    assert!(!command.is_write());
    assert_eq!(command.phy(), 1);
    assert_eq!(command.register(), 2);
}

#[test]
fn a_write_command_carries_the_data_and_the_write_bit() {
    let command = MdioCommand::write(0x1f, 0x1f, 0xbeef).expect("write");
    let value = command.register_value();
    assert_eq!(value & MDIO_WRITE, MDIO_WRITE);
    assert_eq!(value & MDIO_READ, 0);
    assert_eq!(value & MDIO_DATA_MASK, 0xbeef);
    assert!(command.is_write());
    assert!(!command.is_read());
    assert_eq!(command.write_value(), Some(0xbeef));
}

#[test]
fn out_of_range_phy_addresses_are_rejected() {
    assert_eq!(
        MdioCommand::read(MAX_PHY_ADDRESS + 1, 0),
        Err(MdioError::PhyAddress(MAX_PHY_ADDRESS + 1))
    );
    assert_eq!(MdioCommand::write(32, 0, 0), Err(MdioError::PhyAddress(32)));
}

#[test]
fn out_of_range_register_numbers_are_rejected() {
    assert_eq!(
        MdioCommand::read(0, MAX_MDIO_REGISTER + 1),
        Err(MdioError::Register(MAX_MDIO_REGISTER + 1))
    );
}

#[test]
fn the_pi4_phy_address_is_one_and_register_two_reads_the_id() {
    let command = MdioCommand::read(1, 2).expect("read");
    assert_eq!(command.register_value(), 0x2822_0000);
    assert_eq!(MDIO_CMD, 0xe14);
}

#[test]
fn a_read_completes_on_the_first_clear_busy_poll() {
    let mut transaction = MdioTransaction::start_read(1, 2).expect("start");
    assert_eq!(transaction.state(), MdioState::Waiting);
    assert_eq!(transaction.polls_left(), MDIO_BUSY_POLLS);

    let status = transaction.observe(0x0000_600d);
    assert_eq!(status, MdioState::ReadComplete(0x600d));
    assert_eq!(transaction.state(), MdioState::ReadComplete(0x600d));
}

#[test]
fn a_write_completes_without_a_read_value() {
    let mut transaction = MdioTransaction::start_write(1, 0x18, 0x8007).expect("start");
    assert_eq!(transaction.observe(0), MdioState::WriteComplete);
}

#[test]
fn a_busy_read_consumes_the_budget_and_then_times_out() {
    let mut transaction = MdioTransaction::start_read(1, 1).expect("start");
    for _ in 0..MDIO_BUSY_POLLS {
        assert_eq!(transaction.observe(MDIO_START_BUSY), MdioState::Waiting);
    }
    assert_eq!(transaction.polls_left(), 0);
    assert_eq!(transaction.observe(MDIO_START_BUSY), MdioState::TimedOut);
    assert_eq!(transaction.state(), MdioState::TimedOut);
}

#[test]
fn a_transaction_that_clears_before_the_budget_still_completes() {
    let mut transaction = MdioTransaction::start_read(1, 3).expect("start");
    for _ in 0..(MDIO_BUSY_POLLS - 1) {
        assert_eq!(transaction.observe(MDIO_START_BUSY), MdioState::Waiting);
    }
    assert_eq!(transaction.polls_left(), 1);
    assert_eq!(
        transaction.observe(0x0000_84a0),
        MdioState::ReadComplete(0x84a0)
    );
}

#[test]
fn observing_after_completion_does_not_change_the_result() {
    let mut transaction = MdioTransaction::start_read(1, 2).expect("start");
    assert_eq!(
        transaction.observe(0x0000_600d),
        MdioState::ReadComplete(0x600d)
    );
    assert_eq!(
        transaction.observe(MDIO_START_BUSY),
        MdioState::ReadComplete(0x600d)
    );
}

#[test]
fn the_poll_budget_is_ten_milliseconds() {
    assert_eq!(MDIO_POLL_INTERVAL_US, 10);
    assert_eq!(MDIO_BUSY_POLLS, 1000);
    assert_eq!(MDIO_MAX_WAIT_US, 10_000);
}
