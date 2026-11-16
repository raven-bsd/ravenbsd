// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the MDIO codec and transaction machine.

use super::*;

#[test]
fn a_read_command_has_the_clause22_fields() {
    assert_eq!(read_command(1, 1), Ok(0x6086_0000));
    assert_eq!(read_command(0, 0), Ok(0x6002_0000));
    assert_eq!(read_command(31, 31), Ok(0x6FFE_0000));
}

#[test]
fn a_write_command_carries_its_data() {
    assert_eq!(write_command(1, 0, 0x8000), Ok((0x5082_0000, 0x8000)));
}

#[test]
fn address_fields_are_range_checked() {
    assert_eq!(read_command(32, 0), Err(MdioError::BadPhy(32)));
    assert_eq!(read_command(0, 32), Err(MdioError::BadRegister(32)));
    assert_eq!(write_command(32, 0, 0), Err(MdioError::BadPhy(32)));
}

#[test]
fn a_read_response_decodes_the_data() {
    assert_eq!(decode_read(0x6086_796D), Ok(0x796D));
}

#[test]
fn malformed_responses_are_rejected() {
    assert_eq!(decode_read(0x0086_796D), Err(MdioError::BadStart));
    assert_eq!(decode_read(0x7086_796D), Err(MdioError::BadOpcode));
    assert_eq!(decode_read(0x6084_796D), Err(MdioError::BadTurnaround));
}

#[test]
fn the_standard_link_bits_are_decoded() {
    let status = decode_bmsr(0x796D, 0x1000);
    assert!(status.link_up);
    assert!(status.autoneg_complete);
    assert!(status.autoneg_enabled);
    assert!(status.remote_fault);

    let down = decode_bmsr(0x0000, 0x0000);
    assert!(!down.link_up);
    assert!(!down.autoneg_complete);
    assert!(!down.autoneg_enabled);
    assert!(!down.remote_fault);
}

#[test]
fn a_read_transaction_completes() {
    let mut mdio = Mdio::new(2);
    assert_eq!(mdio.state(), MdioState::Idle);
    assert_eq!(
        mdio.transact(Transaction::Read {
            phy: 1,
            register: BMSR
        }),
        Ok(Action::Transmit {
            header: 0x6086_0000,
            write_data: None
        })
    );
    assert_eq!(mdio.state(), MdioState::Awaiting);
    assert_eq!(
        mdio.on_response(0x6086_796D),
        Ok(Action::Done(Some(0x796D)))
    );
    assert_eq!(mdio.state(), MdioState::Complete);
}

#[test]
fn a_write_transaction_completes_without_data() {
    let mut mdio = Mdio::new(0);
    assert_eq!(
        mdio.transact(Transaction::Write {
            phy: 1,
            register: BMCR,
            value: 0x8000
        }),
        Ok(Action::Transmit {
            header: 0x5082_0000,
            write_data: Some(0x8000)
        })
    );
    assert_eq!(mdio.on_response(0), Ok(Action::Done(None)));
}

#[test]
fn a_timeout_retransmits_then_faults() {
    let mut mdio = Mdio::new(1);
    let read = Transaction::Read {
        phy: 1,
        register: BMSR,
    };
    assert!(mdio.transact(read).is_ok());
    assert!(matches!(mdio.on_timeout(), Ok(Action::Transmit { .. })));
    assert_eq!(mdio.retries(), 1);
    assert_eq!(mdio.on_timeout(), Err(MdioError::Timeout));
    assert_eq!(mdio.state(), MdioState::Faulted);
    assert_eq!(mdio.on_timeout(), Err(MdioError::Idle));
}

#[test]
fn a_malformed_response_faults_the_machine() {
    let mut mdio = Mdio::new(3);
    assert!(
        mdio.transact(Transaction::Read {
            phy: 1,
            register: BMSR
        })
        .is_ok()
    );
    assert_eq!(mdio.on_response(0), Err(MdioError::BadStart));
    assert_eq!(mdio.state(), MdioState::Faulted);
    assert_eq!(mdio.on_response(0x6086_796D), Err(MdioError::Idle));
    assert_eq!(
        mdio.transact(Transaction::Read {
            phy: 1,
            register: BMSR
        }),
        Err(MdioError::Faulted)
    );
    mdio.reset();
    assert!(
        mdio.transact(Transaction::Read {
            phy: 1,
            register: BMSR
        })
        .is_ok()
    );
}

#[test]
fn only_one_transaction_is_in_flight() {
    let mut mdio = Mdio::new(0);
    assert!(
        mdio.transact(Transaction::Read {
            phy: 1,
            register: BMSR
        })
        .is_ok()
    );
    assert_eq!(
        mdio.transact(Transaction::Read {
            phy: 1,
            register: BMSR
        }),
        Err(MdioError::Busy)
    );
    mdio.reset();
    assert_eq!(mdio.state(), MdioState::Idle);
    assert_eq!(mdio.on_response(0), Err(MdioError::Idle));
}
