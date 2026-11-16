// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the SD card initialisation state machine.

use super::*;

fn command(step: InitStep) -> (SdCommand, u32) {
    match step {
        InitStep::Send(command) => (command.command(), command.argument()),
        InitStep::Ready(card) => panic!("expected a command, got ready card {card:?}"),
    }
}

fn r1() -> [u32; 4] {
    [1 << 8, 0, 0, 0]
}

fn r1_app() -> [u32; 4] {
    [(1 << 8) | (1 << 5), 0, 0, 0]
}

fn r3(ready: bool, ccs: bool) -> [u32; 4] {
    let mut raw = 0;
    if ready {
        raw |= 1 << 31;
    }
    if ccs {
        raw |= 1 << 30;
    }
    [raw, 0, 0, 0]
}

fn r6(rca: u16) -> [u32; 4] {
    [(u32::from(rca) << 16) | (1 << 8), 0, 0, 0]
}

fn cid_words() -> [u32; 4] {
    [0, 0, 0, 0]
}

fn csd_v2_words() -> [u32; 4] {
    csd_v2_words_for(0xffff)
}

fn csd_v2_words_for(c_size: u32) -> [u32; 4] {
    let mut words = [0u32; 4];
    words[3] |= 1 << 30;
    words[1] |= (c_size & 0xffff) << 16;
    words[2] |= (c_size >> 16) & 0x3f;
    words
}

fn csd_v1_words() -> [u32; 4] {
    let read_bl_len = 9u32;
    let c_size = 0xfffu32;
    let c_size_mult = 7u32;
    let mut words = [0u32; 4];
    words[1] |= (c_size & 0x3) << 30;
    words[1] |= (c_size_mult & 0x7) << 15;
    words[2] |= (c_size >> 2) & 0x3ff;
    words[2] |= (read_bl_len & 0xf) << 16;
    words
}

/// Walk to the response of CMD8 and return the step after it (CMD55).
fn after_cmd8(init: &mut Init) -> InitStep {
    let step = init.start();
    assert_eq!(command(step), (SdCommand::Cmd0, 0));
    let step = init.advance(Ok([0; 4])).expect("cmd0 accepted");
    assert_eq!(command(step), (SdCommand::Cmd8, CMD8_ARGUMENT));
    init.advance(Ok([0x1aa, 0, 0, 0])).expect("cmd8 accepted")
}

/// Walk from CMD55 to CMD2 with a ready SDHC card.
fn to_cmd2(init: &mut Init) -> InitStep {
    let step = after_cmd8(init);
    assert_eq!(command(step), (SdCommand::Cmd55, 0));
    let step = init.advance(Ok(r1_app())).expect("cmd55 accepted");
    assert_eq!(command(step), (SdCommand::Acmd41, ACMD41_HCS_ARGUMENT));
    init.advance(Ok(r3(true, true))).expect("acmd41 ready")
}

#[test]
fn the_full_sdhc_sequence_reaches_a_ready_card() {
    let mut init = Init::new();
    assert_eq!(init.phase(), InitPhase::Idle);
    let step = to_cmd2(&mut init);
    assert_eq!(command(step), (SdCommand::Cmd2, 0));
    assert_eq!(init.phase(), InitPhase::Cid);

    let step = init.advance(Ok(cid_words())).expect("cid accepted");
    assert_eq!(command(step), (SdCommand::Cmd3, 0));

    let step = init.advance(Ok(r6(0x1234))).expect("rca accepted");
    assert_eq!(command(step), (SdCommand::Cmd9, 0x1234));
    assert_eq!(init.phase(), InitPhase::Csd);

    let step = init.advance(Ok(csd_v2_words())).expect("csd accepted");
    assert_eq!(command(step), (SdCommand::Cmd7, 0x1234));

    let step = init.advance(Ok(r1())).expect("card selected");
    assert_eq!(command(step), (SdCommand::Cmd55, 0x1234));
    assert_eq!(init.phase(), InitPhase::BusWidth);

    let step = init.advance(Ok(r1_app())).expect("app cmd accepted");
    assert_eq!(command(step), (SdCommand::Acmd6, 2));

    let step = init.advance(Ok(r1())).expect("bus width accepted");
    assert_eq!(command(step), (SdCommand::Cmd16, BLOCK_LEN));

    let card = match init.advance(Ok(r1())).expect("block length accepted") {
        InitStep::Ready(card) => card,
        other => panic!("expected ready, got {other:?}"),
    };
    assert_eq!(card.rca, 0x1234);
    assert!(card.high_capacity);
    assert!(!card.legacy);
    assert_eq!(card.capacity_bytes, 32 * 1024 * 1024 * 1024);
    assert_eq!(card.cid, Cid::from_raw(crate::response::RawLong::default()));
    assert_eq!(init.phase(), InitPhase::Ready);
}

#[test]
fn an_sdxc_card_reaches_ready_with_two_tib_capacity() {
    let mut init = Init::new();
    let step = to_cmd2(&mut init);
    assert_eq!(command(step), (SdCommand::Cmd2, 0));

    let step = init.advance(Ok(cid_words())).expect("cid accepted");
    assert_eq!(command(step), (SdCommand::Cmd3, 0));
    let step = init.advance(Ok(r6(0x4567))).expect("rca accepted");
    assert_eq!(command(step), (SdCommand::Cmd9, 0x4567));
    let step = init
        .advance(Ok(csd_v2_words_for(0x3f_ffff)))
        .expect("sdxc csd accepted");
    assert_eq!(command(step), (SdCommand::Cmd7, 0x4567));
    let step = init.advance(Ok(r1())).expect("card selected");
    assert_eq!(command(step), (SdCommand::Cmd55, 0x4567));
    let step = init.advance(Ok(r1_app())).expect("app cmd accepted");
    assert_eq!(command(step), (SdCommand::Acmd6, 2));
    let step = init.advance(Ok(r1())).expect("bus width accepted");
    assert_eq!(command(step), (SdCommand::Cmd16, BLOCK_LEN));

    let card = match init.advance(Ok(r1())).expect("block length accepted") {
        InitStep::Ready(card) => card,
        other => panic!("expected ready, got {other:?}"),
    };
    assert_eq!(card.rca, 0x4567);
    assert!(card.high_capacity);
    assert!(!card.legacy);
    assert_eq!(card.capacity_bytes, 2 * 1024 * 1024 * 1024 * 1024);
    assert!(card.capacity_bytes > 32 * 1024 * 1024 * 1024);
}

#[test]
fn a_busy_acmd41_retries_cmd55_and_still_finishes() {
    let mut init = Init::new();
    let step = after_cmd8(&mut init);
    assert_eq!(command(step), (SdCommand::Cmd55, 0));
    let step = init.advance(Ok(r1_app())).expect("cmd55 accepted");
    assert_eq!(command(step), (SdCommand::Acmd41, ACMD41_HCS_ARGUMENT));

    let step = init
        .advance(Ok(r3(false, false)))
        .expect("busy is not an error");
    assert_eq!(init.op_cond_attempts(), 1);
    assert_eq!(command(step), (SdCommand::Cmd55, 0));

    let step = init.advance(Ok(r1_app())).expect("cmd55 accepted");
    assert_eq!(command(step), (SdCommand::Acmd41, ACMD41_HCS_ARGUMENT));
    let step = init.advance(Ok(r3(true, false))).expect("acmd41 ready");
    assert_eq!(command(step), (SdCommand::Cmd2, 0));
}

#[test]
fn acmd41_gives_up_after_the_bounded_number_of_rounds() {
    let mut init = Init::new();
    let step = after_cmd8(&mut init);
    assert_eq!(command(step), (SdCommand::Cmd55, 0));
    let mut step = init.advance(Ok(r1_app())).expect("cmd55 accepted");
    assert_eq!(command(step), (SdCommand::Acmd41, ACMD41_HCS_ARGUMENT));

    loop {
        match init.advance(Ok(r3(false, false))) {
            Ok(next) => {
                assert_eq!(command(next), (SdCommand::Cmd55, 0));
                step = init.advance(Ok(r1_app())).expect("cmd55 accepted");
                assert_eq!(command(step), (SdCommand::Acmd41, ACMD41_HCS_ARGUMENT));
            }
            Err(error) => {
                assert_eq!(
                    error,
                    InitError::Acmd41Timeout {
                        attempts: MAX_OP_COND_ATTEMPTS
                    }
                );
                break;
            }
        }
    }
}

#[test]
fn a_cmd8_timeout_uses_the_v1_card_path() {
    let mut init = Init::new();
    let step = init.start();
    assert_eq!(command(step), (SdCommand::Cmd0, 0));
    let step = init.advance(Ok([0; 4])).expect("cmd0 accepted");
    assert_eq!(command(step), (SdCommand::Cmd8, CMD8_ARGUMENT));

    let step = init
        .advance(Err(CommandFailure::Timeout))
        .expect("cmd8 timeout selects the legacy path");
    assert!(init.legacy());
    assert_eq!(command(step), (SdCommand::Cmd55, 0));

    let step = init.advance(Ok(r1_app())).expect("cmd55 accepted");
    assert_eq!(command(step), (SdCommand::Acmd41, ACMD41_LEGACY_ARGUMENT));

    let step = init
        .advance(Ok(r3(true, true)))
        .expect("legacy acmd41 ready");
    assert_eq!(command(step), (SdCommand::Cmd2, 0));

    let step = init.advance(Ok(cid_words())).expect("cid accepted");
    assert_eq!(command(step), (SdCommand::Cmd3, 0));
    let step = init.advance(Ok(r6(0x2222))).expect("rca accepted");
    assert_eq!(command(step), (SdCommand::Cmd9, 0x2222));
    let step = init.advance(Ok(csd_v1_words())).expect("csd accepted");
    assert_eq!(command(step), (SdCommand::Cmd7, 0x2222));
    let step = init.advance(Ok(r1())).expect("card selected");
    assert_eq!(command(step), (SdCommand::Cmd55, 0x2222));
    let step = init.advance(Ok(r1_app())).expect("app cmd accepted");
    assert_eq!(command(step), (SdCommand::Acmd6, 2));
    let step = init.advance(Ok(r1())).expect("bus width accepted");
    assert_eq!(command(step), (SdCommand::Cmd16, BLOCK_LEN));
    let card = match init.advance(Ok(r1())).expect("block length accepted") {
        InitStep::Ready(card) => card,
        other => panic!("expected ready, got {other:?}"),
    };
    assert!(card.legacy);
    assert!(!card.high_capacity);
    assert_eq!(card.capacity_bytes, 1024 * 1024 * 1024);
}

#[test]
fn a_non_timeout_cmd8_failure_is_not_a_legacy_card() {
    let mut init = Init::new();
    let _ = init.start();
    let step = init.advance(Ok([0; 4])).expect("cmd0 accepted");
    assert_eq!(command(step), (SdCommand::Cmd8, CMD8_ARGUMENT));
    assert_eq!(
        init.advance(Err(CommandFailure::Crc)),
        Err(InitError::CommandFailed {
            command: SdCommand::Cmd8,
            failure: CommandFailure::Crc,
        })
    );
    assert!(!init.legacy());
}

#[test]
fn cmd8_echo_and_voltage_mismatches_are_typed_errors() {
    let mut init = Init::new();
    let _ = init.start();
    let _ = init.advance(Ok([0; 4])).expect("cmd0 accepted");
    assert_eq!(
        init.advance(Ok([0x0aa, 0, 0, 0])),
        Err(InitError::UnsupportedVoltage { accepted: 0 })
    );

    let mut init = Init::new();
    let _ = init.start();
    let _ = init.advance(Ok([0; 4])).expect("cmd0 accepted");
    assert_eq!(
        init.advance(Ok([0x1ab, 0, 0, 0])),
        Err(InitError::EchoMismatch { pattern: 0xab })
    );
}

#[test]
fn a_card_error_status_aborts_the_sequence() {
    let mut init = Init::new();
    let _ = init.start();
    let _ = init.advance(Ok([0; 4])).expect("cmd0 accepted");
    let _ = init.advance(Ok([0x1aa, 0, 0, 0])).expect("cmd8 accepted");
    let status = CardStatus::from_raw(1 << 22);
    assert_eq!(
        init.advance(Ok([1 << 22, 0, 0, 0])),
        Err(InitError::CardError {
            command: SdCommand::Cmd55,
            status,
        })
    );
}

#[test]
fn cmd55_without_app_cmd_is_rejected() {
    let mut init = Init::new();
    let _ = init.start();
    let _ = init.advance(Ok([0; 4])).expect("cmd0 accepted");
    let _ = init.advance(Ok([0x1aa, 0, 0, 0])).expect("cmd8 accepted");
    assert_eq!(
        init.advance(Ok(r1())),
        Err(InitError::AppCommandNotAccepted {
            command: SdCommand::Cmd55,
        })
    );
}

#[test]
fn rca_zero_is_rejected() {
    let mut init = Init::new();
    let _ = to_cmd2(&mut init);
    let _ = init.advance(Ok(cid_words())).expect("cid accepted");
    assert_eq!(init.advance(Ok(r6(0))), Err(InitError::NoRelativeAddress));
}

#[test]
fn a_reserved_csd_structure_is_rejected() {
    let mut init = Init::new();
    let _ = to_cmd2(&mut init);
    let _ = init.advance(Ok(cid_words())).expect("cid accepted");
    let _ = init.advance(Ok(r6(0x1234))).expect("rca accepted");
    let mut reserved = [0u32; 4];
    reserved[3] = 2 << 30;
    assert_eq!(init.advance(Ok(reserved)), Err(InitError::InvalidCapacity));
}

#[test]
fn advancing_a_ready_machine_is_out_of_order() {
    let mut init = Init::new();
    let _ = to_cmd2(&mut init);
    let _ = init.advance(Ok(cid_words())).expect("cid accepted");
    let _ = init.advance(Ok(r6(0x1234))).expect("rca accepted");
    let _ = init.advance(Ok(csd_v2_words())).expect("csd accepted");
    let _ = init.advance(Ok(r1())).expect("card selected");
    let _ = init.advance(Ok(r1_app())).expect("app cmd accepted");
    let _ = init.advance(Ok(r1())).expect("bus width accepted");
    let _ = init.advance(Ok(r1())).expect("block length accepted");
    assert_eq!(init.advance(Ok(r1())), Err(InitError::OutOfOrder));
}
