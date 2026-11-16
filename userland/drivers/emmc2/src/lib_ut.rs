// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the EMMC2 driver skeleton, using a register-level fake host.

use super::*;
use raven_pi4::registers::{
    CLOCK_INTERNAL_STABLE, ERROR_CMD_TIMEOUT, NORMAL_CMD_COMPLETE, POWER_BUS, POWER_VOLTAGE_3V3,
    PRESENT_CARD_INSERTED, SOFTWARE_RESET_ALL,
};

const DMA: DmaWindow = DmaWindow {
    bus_base: 0xc000_0000,
    cpu_base: 0,
    size: 0x4000_0000,
};

const RCA: u16 = 0x1234;

struct FakeHost {
    now: u64,
    clock: Option<u32>,
    caps: u32,
    argument: u32,
    acmd41_argument: Option<u32>,
    response: [u32; 4],
    normal: u16,
    error: u16,
    clock_control: u16,
    command_count: u32,
    stall: bool,
    fail_cmd8: bool,
    legacy: bool,
    busy_rounds: u8,
    card_error_on_cmd55: bool,
    app_not_set: bool,
    reset_stuck: bool,
    clock_stuck: bool,
    present_state: u32,
    writes: [Option<u32>; 0x100],
}

impl FakeHost {
    fn new(clock: Option<u32>) -> Self {
        Self {
            now: 0,
            clock,
            caps: 0,
            argument: 0,
            acmd41_argument: None,
            response: [0; 4],
            normal: 0,
            error: 0,
            clock_control: 0,
            command_count: 0,
            stall: false,
            fail_cmd8: false,
            legacy: false,
            busy_rounds: 0,
            card_error_on_cmd55: false,
            app_not_set: false,
            reset_stuck: false,
            clock_stuck: false,
            present_state: PRESENT_CARD_INSERTED,
            writes: [None; 0x100],
        }
    }

    /// Simulate a host/controller power cycle while the card stays in the slot.
    fn power_cycle(&mut self) {
        let clock = self.clock;
        let caps = self.caps;
        let present_state = self.present_state;
        *self = Self::new(clock);
        self.caps = caps;
        self.present_state = present_state;
    }

    fn last_write(&self, register: Register) -> Option<u32> {
        self.writes[register.offset()]
    }

    fn prepare(&mut self, index: u8) {
        self.normal = NORMAL_CMD_COMPLETE;
        match index {
            0 => {}
            8 => {
                if self.fail_cmd8 {
                    self.error = ERROR_CMD_TIMEOUT;
                } else {
                    self.response[0] = self.argument;
                }
            }
            55 => {
                if self.card_error_on_cmd55 {
                    self.response[0] = 1 << 22;
                } else if self.app_not_set {
                    self.response[0] = 1 << 8;
                } else {
                    self.response[0] = (1 << 8) | (1 << 5);
                }
            }
            41 => {
                self.acmd41_argument = Some(self.argument);
                if self.busy_rounds > 0 {
                    self.busy_rounds -= 1;
                    self.response[0] = 0;
                } else {
                    let ccs = u32::from(!self.legacy) << 30;
                    self.response[0] = (1 << 31) | ccs;
                }
            }
            2 => self.response = cid_words(),
            3 => self.response[0] = (u32::from(RCA) << 16) | (1 << 8),
            9 => {
                self.response = if self.legacy {
                    csd_v1_words()
                } else {
                    csd_v2_words()
                }
            }
            7 | 16 | 6 => self.response[0] = 1 << 8,
            _ => self.response = [0; 4],
        }
        if self.stall {
            self.normal = 0;
        }
    }
}

impl Host for FakeHost {
    fn read(&mut self, register: Register) -> u32 {
        self.now = self.now.saturating_add(1_000_000);
        match register {
            Register::HostControllerVersion => 2,
            Register::Capabilities => self.caps,
            Register::SoftwareReset => u32::from(self.reset_stuck),
            Register::PresentState => self.present_state,
            Register::ClockControl => {
                if self.clock_stuck {
                    u32::from(self.clock_control)
                } else {
                    u32::from(self.clock_control | CLOCK_INTERNAL_STABLE)
                }
            }
            Register::NormalInterruptStatus => u32::from(self.normal),
            Register::ErrorInterruptStatus => u32::from(self.error),
            Register::Response0 => self.response[0],
            Register::Response1 => self.response[1],
            Register::Response2 => self.response[2],
            Register::Response3 => self.response[3],
            _ => 0,
        }
    }

    fn write(&mut self, register: Register, value: u32) {
        self.now = self.now.saturating_add(1_000_000);
        self.writes[register.offset()] = Some(value);
        match register {
            Register::Argument => self.argument = value,
            Register::Command => {
                self.command_count = self.command_count.saturating_add(1);
                self.prepare(((value >> 8) & 0x3f) as u8);
            }
            Register::ClockControl => self.clock_control = value as u16,
            Register::NormalInterruptStatus => self.normal &= !(value as u16),
            Register::ErrorInterruptStatus => self.error &= !(value as u16),
            _ => {}
        }
    }

    fn now_ns(&mut self) -> u64 {
        self.now
    }

    fn clock_hz(&mut self) -> Option<u32> {
        self.clock
    }

    fn dma_window(&self) -> DmaWindow {
        DMA
    }
}

fn cid_words() -> [u32; 4] {
    [0, 0, 0, 0]
}

fn csd_v2_words() -> [u32; 4] {
    let c_size = 0xffffu32;
    let mut words = [0u32; 4];
    words[3] |= 1 << 30;
    words[1] |= (c_size & 0xffff) << 16;
    words[2] |= (c_size >> 16) & 0x3f;
    words[2] |= 9 << 16;
    words
}

fn csd_v1_words() -> [u32; 4] {
    let c_size = 0xfffu32;
    let mut words = [0u32; 4];
    words[1] |= (c_size & 0x3) << 30;
    words[1] |= 7 << 15;
    words[2] |= (c_size >> 2) & 0x3ff;
    words[2] |= 9 << 16;
    words
}

#[test]
fn probe_uses_the_mailbox_clock_and_the_model_capabilities() {
    let mut host = FakeHost::new(Some(50_000_000));
    let plan = probe(&mut host).expect("probe");
    assert_eq!(plan.version_raw, 2);
    assert_eq!(plan.spec, SpecVersion::V3);
    assert_eq!(plan.raw_capabilities, 0);
    assert!(plan.capabilities.voltage_3v3());
    assert!(plan.capabilities.high_speed_supported());
    assert!(plan.capabilities.dma_supported());
    assert_eq!(plan.clock_source, ClockSource::Mailbox);
    assert_eq!(plan.base_hz, 50_000_000);
    assert_eq!(
        plan.timing.speed(),
        raven_pi4::timing::SpeedMode::Identification
    );
    assert_eq!(plan.dma, DMA);
}

#[test]
fn probe_falls_back_to_the_documented_clock_when_the_mailbox_is_silent() {
    let mut host = FakeHost::new(None);
    let plan = probe(&mut host).expect("probe");
    assert_eq!(plan.clock_source, ClockSource::ModelDefault);
    assert_eq!(plan.base_hz, 50_000_000);
}

#[test]
fn probe_trusts_the_capabilities_only_when_the_quirk_allows_it() {
    let mut host = FakeHost::new(None);
    host.caps = (1 << 24) | (1 << 21) | (54 << 8);
    let quirks = Quirks {
        host_capabilities_valid: true,
        ..Quirks::PI4_EMMC2
    };
    let plan = probe_with_quirks(&mut host, quirks).expect("probe");
    assert_eq!(plan.clock_source, ClockSource::Capabilities);
    assert_eq!(plan.base_hz, 54_000_000);
    assert_eq!(plan.raw_capabilities, host.caps);
}

#[test]
fn probe_rejects_a_controller_without_3v3_when_capabilities_are_valid() {
    let mut host = FakeHost::new(None);
    host.caps = (1 << 21) | (1 << 22) | (50 << 8);
    let quirks = Quirks {
        host_capabilities_valid: true,
        ..Quirks::PI4_EMMC2
    };
    assert_eq!(
        probe_with_quirks(&mut host, quirks),
        Err(DriverError::UnsupportedVoltage {
            capabilities: host.caps,
        })
    );
}

#[test]
fn bring_up_resets_powers_and_programs_the_identification_clock() {
    let mut host = FakeHost::new(Some(50_000_000));
    let (_plan, timing) = bring_up(&mut host).expect("bring up");
    assert_eq!(timing.speed(), raven_pi4::timing::SpeedMode::Identification);
    assert_eq!(
        host.last_write(Register::SoftwareReset),
        Some(u32::from(SOFTWARE_RESET_ALL))
    );
    assert_eq!(
        host.last_write(Register::PowerControl),
        Some(u32::from(POWER_BUS | POWER_VOLTAGE_3V3))
    );
    assert_eq!(
        host.last_write(Register::TimeoutControl),
        Some(u32::from(TIMEOUT_CONTROL_MAX))
    );
    let clock = host
        .last_write(Register::ClockControl)
        .expect("clock write") as u16;
    assert_ne!(clock & CLOCK_SD_ENABLE, 0);
    assert_eq!(clock & 0xff00, 0x3f00);
}

#[test]
fn init_card_selects_a_32_gib_card_and_programs_the_default_timing() {
    let mut host = FakeHost::new(Some(50_000_000));
    let ready = init_card(&mut host).expect("card init");
    assert_eq!(ready.card.rca, RCA);
    assert!(ready.card.high_capacity);
    assert!(!ready.card.legacy);
    assert_eq!(ready.card.capacity_bytes, 32 * 1024 * 1024 * 1024);
    assert_eq!(ready.timing.speed(), raven_pi4::timing::SpeedMode::Default);
    assert_eq!(ready.timing.width(), BusWidth::Bit4);
    assert_eq!(host.last_write(Register::HostControl), Some(0x02));
    let clock = host
        .last_write(Register::ClockControl)
        .expect("clock write") as u16;
    assert_ne!(clock & CLOCK_SD_ENABLE, 0);
    assert_eq!(clock & 0xff00, 0x0100);
    assert_eq!(host.command_count, 11);
}

#[test]
fn init_card_retries_a_busy_acmd41() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.busy_rounds = 2;
    let ready = init_card(&mut host).expect("card init");
    assert_eq!(ready.card.rca, RCA);
    assert_eq!(host.command_count, 15);
    assert_eq!(
        host.acmd41_argument,
        Some(raven_pi4::init::ACMD41_HCS_ARGUMENT)
    );
}

#[test]
fn init_card_uses_the_v1_path_after_a_cmd8_timeout() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.fail_cmd8 = true;
    host.legacy = true;
    let ready = init_card(&mut host).expect("legacy card init");
    assert!(ready.card.legacy);
    assert!(!ready.card.high_capacity);
    assert_eq!(ready.card.capacity_bytes, 1024 * 1024 * 1024);
    assert_eq!(
        host.acmd41_argument,
        Some(raven_pi4::init::ACMD41_LEGACY_ARGUMENT)
    );
}

#[test]
fn init_card_reports_a_command_timeout() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.stall = true;
    assert_eq!(
        init_card(&mut host),
        Err(DriverError::Init(InitError::CommandFailed {
            command: raven_pi4::command::SdCommand::Cmd0,
            failure: CommandFailure::Timeout,
        }))
    );
}

#[test]
fn init_card_reports_a_card_error() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.card_error_on_cmd55 = true;
    let status = raven_pi4::response::CardStatus::from_raw(1 << 22);
    assert_eq!(
        init_card(&mut host),
        Err(DriverError::Init(InitError::CardError {
            command: raven_pi4::command::SdCommand::Cmd55,
            status,
        }))
    );
}

#[test]
fn init_card_rejects_a_card_that_does_not_accept_app_cmd() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.app_not_set = true;
    assert_eq!(
        init_card(&mut host),
        Err(DriverError::Init(InitError::AppCommandNotAccepted {
            command: raven_pi4::command::SdCommand::Cmd55,
        }))
    );
}

#[test]
fn a_stuck_reset_is_a_timeout() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.reset_stuck = true;
    assert_eq!(bring_up(&mut host), Err(DriverError::ResetTimeout));
}

#[test]
fn a_clock_that_never_stabilises_is_a_timeout() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.clock_stuck = true;
    assert_eq!(bring_up(&mut host), Err(DriverError::ClockStableTimeout));
}

#[test]
fn card_insertion_is_reported_from_the_present_state() {
    let mut host = FakeHost::new(Some(50_000_000));
    assert!(card_inserted(&mut host));
    host.present_state = 0;
    assert!(!card_inserted(&mut host));
    host.present_state = PRESENT_CARD_INSERTED;
    assert!(card_inserted(&mut host));
}

#[test]
fn a_removed_card_is_refused_before_initialisation() {
    let mut host = FakeHost::new(Some(50_000_000));
    host.present_state = 0;
    assert_eq!(init_card_checked(&mut host), Err(DriverError::NoCard));
    assert_eq!(host.command_count, 0);
}

#[test]
fn a_removed_then_reinserted_card_initialises_again() {
    let mut host = FakeHost::new(Some(50_000_000));
    let first = init_card_checked(&mut host).expect("first init");
    host.present_state = 0;
    assert_eq!(init_card_checked(&mut host), Err(DriverError::NoCard));
    host.present_state = PRESENT_CARD_INSERTED;
    let second = init_card_checked(&mut host).expect("reinserted card");
    assert_eq!(second.card, first.card);
    assert_eq!(second.timing, first.timing);
}

#[test]
fn a_power_cycle_reinitialises_the_card() {
    let mut host = FakeHost::new(Some(50_000_000));
    let first = init_card_checked(&mut host).expect("first init");
    let commands = host.command_count;
    host.power_cycle();
    assert_eq!(host.command_count, 0);
    assert!(host.last_write(Register::SoftwareReset).is_none());
    let second = init_card_checked(&mut host).expect("second init");
    assert_eq!(second.card, first.card);
    assert_eq!(second.plan, first.plan);
    assert_eq!(second.timing, first.timing);
    assert_eq!(host.command_count, commands);
    assert_eq!(
        host.last_write(Register::PowerControl),
        Some(u32::from(POWER_BUS | POWER_VOLTAGE_3V3))
    );
}
