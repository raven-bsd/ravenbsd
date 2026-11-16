// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable Raspberry Pi 4 EMMC2 (SDHCI) model.
//!
//! This crate carries every EMMC2 fact that can be checked without a board: the SDHCI register
//! map, the command and response encoding, the card-initialisation state machine, the 1-bit/4-bit
//! bus and 25/50 MHz clock plans, the device-tree facts (base, SPI, DMA window), the SDMA/ADMA2
//! descriptor model with bounce-window rules, and the multi-block transfer state.
//!
//! It is `no_std`, allocation-free and `#![forbid(unsafe_code)]`. Nothing here touches hardware:
//! [`registers`] is data, [`command`] and [`response`] are encoders/decoders, [`init`] is a typed
//! state machine driven by command outcomes, [`timing`] is arithmetic, [`fdt`] parses the fixture
//! device tree, [`dma`] validates descriptors and windows, and [`transfer`] sequences CMD17/18/24/25.
//! The board run is a separate integration step; every silicon value that a log has not confirmed
//! is marked UNVERIFIED in `docs/pi4-emmc2.md`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod command;
pub mod dma;
pub mod fdt;
pub mod init;
pub mod registers;
pub mod response;
pub mod timing;
pub mod transfer;

pub use command::{Command, CommandFailure, DataTransfer, EncodedCommand, SdCommand};
pub use dma::{
    Adma2Descriptor, Adma2Plan, Adma2Table, BounceReason, DescriptorAction, DescriptorWidth,
    DmaError, DmaMapping, SdmaPlan, map_dma, plan_adma2, plan_sdma,
};
pub use fdt::{DmaWindow, Emmc2FdtError, Emmc2Node, parse_emmc2};
pub use init::{Card, Init, InitError, InitPhase, InitStep};
pub use registers::{AccessWidth, Capabilities, Quirks, Register};
pub use response::{CardStatus, Cid, Csd, Ocr, Response, ResponseKind};
pub use timing::{BusWidth, ClockError, ClockPlan, SpecVersion, SpeedMode, Timing, TimingError};
pub use transfer::{
    MAX_BLOCKS_PER_TRANSFER, MultiBlockTransfer, TransferError, TransferErrorKind, TransferEvent,
    TransferStart, TransferState,
};
