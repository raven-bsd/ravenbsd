// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the multi-block transfer state and error paths.

use super::*;
use crate::registers::{
    ERROR_ADMA, ERROR_AUTO_CMD12, ERROR_CMD_CRC, ERROR_CURRENT_LIMIT, ERROR_DATA_TIMEOUT,
    ERROR_DMA, ErrorInterrupt, NORMAL_CMD_COMPLETE, NORMAL_TRANSFER_COMPLETE, NormalInterrupt,
};

fn make_transfer(direction: DataDirection, blocks: u16) -> MultiBlockTransfer {
    MultiBlockTransfer::new(direction, 0x100, 512, blocks).expect("transfer")
}

#[test]
fn the_request_is_validated_before_a_command_is_built() {
    assert_eq!(
        MultiBlockTransfer::new(DataDirection::Read, 0, 512, 0),
        Err(TransferError::new(TransferErrorKind::ZeroBlocks, 0, 0))
    );
    assert_eq!(
        MultiBlockTransfer::new(DataDirection::Read, 0, 512, 513),
        Err(TransferError::new(TransferErrorKind::TooManyBlocks, 0, 513))
    );
    assert_eq!(
        MultiBlockTransfer::new(DataDirection::Read, 0, 0, 1),
        Err(TransferError::new(TransferErrorKind::BadBlockSize, 0, 1))
    );
    assert_eq!(
        MultiBlockTransfer::new(DataDirection::Read, 0, 1025, 1),
        Err(TransferError::new(TransferErrorKind::BadBlockSize, 0, 1))
    );
    assert_eq!(
        MultiBlockTransfer::new(DataDirection::Read, u64::from(u32::MAX), 512, 2),
        Err(TransferError::new(TransferErrorKind::AddressTooHigh, 0, 2))
    );
    assert!(MultiBlockTransfer::new(DataDirection::Read, u64::from(u32::MAX), 512, 1).is_ok());
}

#[test]
fn single_and_multi_block_commands_are_selected_by_shape() {
    let cases = [
        (DataDirection::Read, 1, SdCommand::Cmd17),
        (DataDirection::Read, 2, SdCommand::Cmd18),
        (DataDirection::Write, 1, SdCommand::Cmd24),
        (DataDirection::Write, 2, SdCommand::Cmd25),
    ];
    for (direction, blocks, expected) in cases {
        let mut transfer = make_transfer(direction, blocks);
        let start = transfer.start().expect("start");
        assert_eq!(start.command.command(), expected);
        assert_eq!(start.command.argument(), 0x100);
        assert_eq!(start.transfer.block_count, blocks);
        assert_eq!(start.transfer.block_size, 512);
        assert_eq!(start.transfer.direction, direction);
        assert_eq!(transfer.state(), TransferState::Command);
    }
}

#[test]
fn the_full_sequence_tracks_blocks_and_bytes() {
    let mut transfer = make_transfer(DataDirection::Read, 4);
    assert_eq!(transfer.state(), TransferState::Idle);
    let _ = transfer.start().expect("start");
    transfer.command_complete().expect("command complete");
    assert_eq!(transfer.state(), TransferState::Data);
    transfer.block_complete(1).expect("block 1");
    assert_eq!(transfer.blocks_done(), 1);
    assert_eq!(transfer.remaining_blocks(), 3);
    transfer.block_complete(2).expect("blocks 2-3");
    assert_eq!(transfer.bytes_done(), 1536);
    assert_eq!(transfer.transfer_complete(), Ok(2048));
    assert_eq!(transfer.state(), TransferState::Complete);
    assert_eq!(transfer.blocks_done(), 4);
    assert_eq!(transfer.total_bytes(), 2048);
}

#[test]
fn a_dma_completion_accounts_for_unreported_blocks() {
    let mut transfer = make_transfer(DataDirection::Write, 8);
    let _ = transfer.start().expect("start");
    transfer.command_complete().expect("command complete");
    assert_eq!(transfer.transfer_complete(), Ok(4096));
    assert_eq!(transfer.blocks_done(), 8);
    assert_eq!(transfer.remaining_blocks(), 0);
}

#[test]
fn over_completion_fails_and_stays_failed() {
    let mut transfer = make_transfer(DataDirection::Read, 2);
    let _ = transfer.start().expect("start");
    transfer.command_complete().expect("command complete");
    transfer.block_complete(1).expect("one block");
    let error = transfer.block_complete(2).expect_err("over completion");
    assert_eq!(error.kind, TransferErrorKind::BlockCountMismatch);
    assert_eq!(error.blocks_done, 1);
    assert_eq!(error.blocks_remaining, 1);
    assert_eq!(transfer.state(), TransferState::Failed);
    assert_eq!(
        transfer.failure(),
        Some(TransferErrorKind::BlockCountMismatch)
    );
    assert_eq!(
        transfer.transfer_complete(),
        Err(TransferError::new(
            TransferErrorKind::BlockCountMismatch,
            1,
            1
        ))
    );
}

#[test]
fn zero_progress_and_out_of_order_events_are_rejected() {
    let mut transfer = make_transfer(DataDirection::Read, 2);
    let _ = transfer.start().expect("start");
    assert_eq!(
        transfer.block_complete(1),
        Err(TransferError::new(TransferErrorKind::UnexpectedEvent, 0, 2))
    );

    let mut transfer = make_transfer(DataDirection::Read, 2);
    let _ = transfer.start().expect("start");
    transfer.command_complete().expect("command complete");
    assert_eq!(
        transfer.block_complete(0),
        Err(TransferError::new(TransferErrorKind::InvalidProgress, 0, 2))
    );

    let mut transfer = make_transfer(DataDirection::Read, 2);
    let _ = transfer.start().expect("start");
    assert_eq!(
        transfer.transfer_complete(),
        Err(TransferError::new(TransferErrorKind::UnexpectedEvent, 0, 2))
    );
}

#[test]
fn starting_twice_is_rejected() {
    let mut transfer = make_transfer(DataDirection::Read, 2);
    let _ = transfer.start().expect("start");
    assert_eq!(
        transfer.start(),
        Err(TransferError::new(TransferErrorKind::UnexpectedEvent, 0, 2))
    );
}

#[test]
fn repeated_completion_is_already_complete_without_losing_the_state() {
    let mut transfer = make_transfer(DataDirection::Read, 1);
    let _ = transfer.start().expect("start");
    transfer.command_complete().expect("command complete");
    let _ = transfer.transfer_complete().expect("complete");
    assert_eq!(transfer.state(), TransferState::Complete);
    assert_eq!(
        transfer.transfer_complete(),
        Err(TransferError::new(TransferErrorKind::AlreadyComplete, 1, 0))
    );
    assert_eq!(transfer.state(), TransferState::Complete);
}

#[test]
fn poll_drives_command_and_transfer_completion() {
    let mut transfer = make_transfer(DataDirection::Read, 2);
    let _ = transfer.start().expect("start");
    assert_eq!(
        transfer.poll(
            NormalInterrupt::from_raw(NORMAL_CMD_COMPLETE),
            ErrorInterrupt::default()
        ),
        Ok(TransferEvent::CommandAccepted)
    );
    assert_eq!(transfer.state(), TransferState::Data);
    assert_eq!(
        transfer.poll(NormalInterrupt::from_raw(0), ErrorInterrupt::default()),
        Ok(TransferEvent::Data)
    );
    assert_eq!(
        transfer.poll(
            NormalInterrupt::from_raw(NORMAL_TRANSFER_COMPLETE),
            ErrorInterrupt::default()
        ),
        Ok(TransferEvent::Complete {
            blocks: 2,
            bytes: 1024
        })
    );
    assert_eq!(transfer.state(), TransferState::Complete);
}

#[test]
fn one_poll_can_accept_both_completion_bits() {
    let mut transfer = make_transfer(DataDirection::Read, 2);
    let _ = transfer.start().expect("start");
    assert_eq!(
        transfer.poll(
            NormalInterrupt::from_raw(NORMAL_CMD_COMPLETE | NORMAL_TRANSFER_COMPLETE),
            ErrorInterrupt::default()
        ),
        Ok(TransferEvent::Complete {
            blocks: 2,
            bytes: 1024
        })
    );
}

#[test]
fn error_interrupts_are_mapped_with_command_priority() {
    assert_eq!(
        TransferErrorKind::from_error_interrupt(ErrorInterrupt::from_raw(ERROR_DATA_TIMEOUT)),
        TransferErrorKind::DataTimeout
    );
    assert_eq!(
        TransferErrorKind::from_error_interrupt(ErrorInterrupt::from_raw(ERROR_ADMA)),
        TransferErrorKind::Adma
    );
    assert_eq!(
        TransferErrorKind::from_error_interrupt(ErrorInterrupt::from_raw(ERROR_DMA)),
        TransferErrorKind::Dma
    );
    assert_eq!(
        TransferErrorKind::from_error_interrupt(ErrorInterrupt::from_raw(ERROR_AUTO_CMD12)),
        TransferErrorKind::AutoCmd12
    );
    assert_eq!(
        TransferErrorKind::from_error_interrupt(ErrorInterrupt::from_raw(ERROR_CURRENT_LIMIT)),
        TransferErrorKind::Other
    );
    assert_eq!(
        TransferErrorKind::from_error_interrupt(ErrorInterrupt::from_raw(
            ERROR_DATA_TIMEOUT | ERROR_CMD_CRC
        )),
        TransferErrorKind::CommandCrc
    );
}

#[test]
fn command_failures_are_mapped() {
    assert_eq!(
        TransferErrorKind::from_command_failure(CommandFailure::Timeout),
        TransferErrorKind::CommandTimeout
    );
    assert_eq!(
        TransferErrorKind::from_command_failure(CommandFailure::Crc),
        TransferErrorKind::CommandCrc
    );
    assert_eq!(
        TransferErrorKind::from_command_failure(CommandFailure::EndBit),
        TransferErrorKind::CommandEndBit
    );
    assert_eq!(
        TransferErrorKind::from_command_failure(CommandFailure::Index),
        TransferErrorKind::CommandIndex
    );
    assert_eq!(
        TransferErrorKind::from_command_failure(CommandFailure::DataTimeout),
        TransferErrorKind::DataTimeout
    );
    assert_eq!(
        TransferErrorKind::from_command_failure(CommandFailure::DataCrc),
        TransferErrorKind::DataCrc
    );
    assert_eq!(
        TransferErrorKind::from_command_failure(CommandFailure::Other),
        TransferErrorKind::Other
    );
}

#[test]
fn a_polled_error_stops_the_transfer_and_is_sticky() {
    let mut transfer = make_transfer(DataDirection::Write, 2);
    let _ = transfer.start().expect("start");
    let error = transfer
        .poll(
            NormalInterrupt::from_raw(0),
            ErrorInterrupt::from_raw(ERROR_DATA_TIMEOUT),
        )
        .expect_err("data timeout");
    assert_eq!(error.kind, TransferErrorKind::DataTimeout);
    assert_eq!(error.blocks_done, 0);
    assert_eq!(error.blocks_remaining, 2);
    assert_eq!(transfer.state(), TransferState::Failed);
    assert_eq!(
        transfer.poll(
            NormalInterrupt::from_raw(0),
            ErrorInterrupt::from_raw(ERROR_DATA_TIMEOUT)
        ),
        Err(error)
    );
}

#[test]
fn completion_bits_before_the_command_are_rejected() {
    let mut transfer = make_transfer(DataDirection::Read, 1);
    assert_eq!(
        transfer.poll(
            NormalInterrupt::from_raw(NORMAL_TRANSFER_COMPLETE),
            ErrorInterrupt::default()
        ),
        Err(TransferError::new(TransferErrorKind::UnexpectedEvent, 0, 1))
    );
    assert_eq!(transfer.state(), TransferState::Failed);
}

#[test]
fn polling_a_completed_transfer_reports_already_complete() {
    let mut transfer = make_transfer(DataDirection::Read, 1);
    let _ = transfer.start().expect("start");
    transfer.command_complete().expect("command complete");
    let _ = transfer.transfer_complete().expect("complete");
    assert_eq!(
        transfer.poll(NormalInterrupt::from_raw(0), ErrorInterrupt::default()),
        Err(TransferError::new(TransferErrorKind::AlreadyComplete, 1, 0))
    );
    assert_eq!(transfer.state(), TransferState::Complete);
}
