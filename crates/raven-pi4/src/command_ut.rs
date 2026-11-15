// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for command selection and the SDHCI command/transfer-mode encoding.

use super::*;

#[test]
fn command_indices_and_application_commands_are_typed() {
    assert_eq!(SdCommand::Cmd0.number(), 0);
    assert_eq!(SdCommand::Cmd8.number(), 8);
    assert_eq!(SdCommand::Cmd55.number(), 55);
    assert_eq!(SdCommand::Acmd41.number(), 41);
    assert_eq!(SdCommand::Acmd6.number(), 6);
    assert!(!SdCommand::Cmd55.is_app());
    assert!(SdCommand::Acmd41.is_app());
    assert!(SdCommand::Cmd18.multi_block());
    assert!(!SdCommand::Cmd17.multi_block());
    assert_eq!(SdCommand::Acmd41.name(), "ACMD41");
}

#[test]
fn response_kinds_match_the_command() {
    assert_eq!(SdCommand::Cmd0.response(), ResponseKind::None);
    assert_eq!(SdCommand::Cmd2.response(), ResponseKind::R2);
    assert_eq!(SdCommand::Cmd8.response(), ResponseKind::R7);
    assert_eq!(SdCommand::Acmd41.response(), ResponseKind::R3);
    assert_eq!(SdCommand::Cmd3.response(), ResponseKind::R6);
    assert_eq!(SdCommand::Cmd7.response(), ResponseKind::R1b);
    assert_eq!(SdCommand::Cmd17.response(), ResponseKind::R1);
}

#[test]
fn no_data_commands_encode_the_index_response_and_crc_bits() {
    let encode = |command, argument| {
        Command::new(command, argument)
            .encode(None)
            .expect("command encodes")
            .command
    };
    assert_eq!(encode(SdCommand::Cmd0, 0), 0x0000);
    assert_eq!(encode(SdCommand::Cmd8, 0x1aa), 0x080a);
    assert_eq!(encode(SdCommand::Cmd2, 0), 0x0209);
    assert_eq!(encode(SdCommand::Cmd3, 0), 0x030a);
    assert_eq!(encode(SdCommand::Cmd7, 0x1234_0000), 0x070b);
    assert_eq!(encode(SdCommand::Cmd9, 0x1234_0000), 0x0909);
    assert_eq!(encode(SdCommand::Cmd55, 0), 0x370a);
    assert_eq!(encode(SdCommand::Acmd6, 2), 0x060a);
    assert_eq!(encode(SdCommand::Acmd41, 0x40ff_8000), 0x2902);
}

#[test]
fn read_commands_set_the_data_present_and_read_bits() {
    let transfer = DataTransfer::new(512, 1, DataDirection::Read).expect("valid transfer");
    let encoded = Command::new(SdCommand::Cmd17, 0x20)
        .encode(Some(transfer))
        .expect("command encodes");
    assert!(encoded.data_present());
    assert_eq!(encoded.command, 0x112a);
    assert_eq!(encoded.transfer_mode, 0x10);
    assert_eq!(encoded.block_size, 512);
    assert_eq!(encoded.block_count, 1);

    let transfer = DataTransfer::new(512, 4, DataDirection::Read)
        .expect("valid transfer")
        .with_auto_cmd12();
    let encoded = Command::new(SdCommand::Cmd18, 0x40)
        .encode(Some(transfer))
        .expect("command encodes");
    assert_eq!(encoded.command, 0x122a);
    assert_eq!(encoded.transfer_mode, 0x36);
    assert_eq!(encoded.block_count, 4);
}

#[test]
fn write_commands_encode_single_and_multi_block_modes() {
    let transfer = DataTransfer::new(512, 1, DataDirection::Write).expect("valid transfer");
    let encoded = Command::new(SdCommand::Cmd24, 0x60)
        .encode(Some(transfer))
        .expect("command encodes");
    assert_eq!(encoded.command, 0x182a);
    assert_eq!(encoded.transfer_mode, 0);

    let transfer = DataTransfer::new(512, 2, DataDirection::Write)
        .expect("valid transfer")
        .with_dma();
    let encoded = Command::new(SdCommand::Cmd25, 0x80)
        .encode(Some(transfer))
        .expect("command encodes");
    assert_eq!(encoded.command, 0x192a);
    assert_eq!(encoded.transfer_mode, 0x23);
    assert_eq!(encoded.block_count, 2);
}

#[test]
fn function_switch_reads_use_cmd6_not_acmd6() {
    let transfer = DataTransfer::new(64, 1, DataDirection::Read).expect("valid transfer");
    let encoded = Command::new(SdCommand::Cmd6, 0x00ff_fff1)
        .encode(Some(transfer))
        .expect("command encodes");
    assert_eq!(encoded.command, 0x062a);
    assert_eq!(encoded.block_size, 64);
    assert_eq!(
        Command::new(SdCommand::Acmd6, 2)
            .encode(None)
            .expect("acmd6 encodes")
            .command,
        0x060a
    );
}

#[test]
fn malformed_transfers_are_rejected_before_the_register_write() {
    assert_eq!(
        DataTransfer::new(0, 1, DataDirection::Read),
        Err(CommandError::BlockSize { value: 0 })
    );
    assert_eq!(
        DataTransfer::new(1025, 1, DataDirection::Read),
        Err(CommandError::BlockSize { value: 1025 })
    );
    assert_eq!(
        DataTransfer::new(512, 0, DataDirection::Read),
        Err(CommandError::BlockCount { value: 0 })
    );
    assert_eq!(
        DataTransfer::new(512, 513, DataDirection::Read),
        Err(CommandError::BlockCount { value: 513 })
    );
}

#[test]
fn the_transfer_direction_must_match_the_command() {
    let transfer = DataTransfer::new(512, 1, DataDirection::Write).expect("valid transfer");
    assert_eq!(
        Command::new(SdCommand::Cmd17, 0).encode(Some(transfer)),
        Err(CommandError::DirectionMismatch {
            expected: DataDirection::Read,
            got: DataDirection::Write,
        })
    );
}

#[test]
fn data_is_exactly_present_when_the_command_requires_it() {
    let transfer = DataTransfer::new(512, 1, DataDirection::Read).expect("valid transfer");
    assert_eq!(
        Command::new(SdCommand::Cmd55, 0).encode(Some(transfer)),
        Err(CommandError::DataNotExpected)
    );
    assert_eq!(
        Command::new(SdCommand::Cmd17, 0).encode(None),
        Err(CommandError::MissingData)
    );
}
