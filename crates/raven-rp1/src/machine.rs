// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RP1 enumeration and firmware-load state machine.
//!
//! The machine is a pure transition table: the caller performs an [`Action`] (read config space,
//! wait for the boot ROM, send a chunk, ask for verification, start the firmware) and feeds the
//! observed [`Event`] back. Every transition is explicit and every protocol violation is a named
//! [`Fault`]; an event that does not belong to the current state is an error that leaves the state
//! untouched.
//!
//! ```text
//! Idle --start--> Identifying --Identified--> Enumerated --BootRomReady--> Loading
//! Loading --Ack--> Loading/Verifying --VerifyPassed--> Ready
//! any working state --Malformed/Timeouts/Reject limit/VerifyFailed--> Faulted
//! Faulted --reset--> Idle
//! ```

use core::fmt;

use crate::firmware::FirmwareImage;
use crate::frame::{FrameError, Layout};
use crate::transfer::{Chunk, Transfer, TransferError, TransferPlan};

/// The RP1 vendor/device identity read from config space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Vendor ID.
    pub vendor: u16,
    /// Device ID.
    pub device: u16,
}

/// Which identity the bring-up accepts.
///
/// The real RP1 IDs are UNVERIFIED until a board log exists; [`ExpectedIdentity::UNVERIFIED`]
/// accepts any responding function. A bench profile supplies concrete IDs for the strict test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedIdentity {
    /// Accepted vendor ID, or `None` for any.
    pub vendor: Option<u16>,
    /// Accepted device ID, or `None` for any.
    pub device: Option<u16>,
}

impl ExpectedIdentity {
    /// Accept any responding function.
    pub const UNVERIFIED: Self = Self {
        vendor: None,
        device: None,
    };

    /// Require both IDs.
    #[must_use]
    pub const fn exact(vendor: u16, device: u16) -> Self {
        Self {
            vendor: Some(vendor),
            device: Some(device),
        }
    }

    /// True when `identity` satisfies every constraint that is present.
    #[must_use]
    pub const fn matches(&self, identity: Identity) -> bool {
        let vendor = match self.vendor {
            Some(vendor) => vendor == identity.vendor,
            None => true,
        };
        let device = match self.device {
            Some(device) => device == identity.device,
            None => true,
        };
        vendor && device
    }
}

/// Retry budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// A chunk may be sent again this many times after a rejection.
    pub max_retries: u16,
    /// The current step may time out this many times.
    pub max_timeouts: u16,
}

impl Limits {
    /// The model defaults: three retries, two timeouts.
    pub const DEFAULT: Self = Self {
        max_retries: 3,
        max_timeouts: 2,
    };
}

/// Why a machine configuration is inconsistent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// The declared firmware size differs from the transfer plan's image length.
    FirmwareSizeMismatch {
        /// Size in the firmware record.
        firmware: u64,
        /// Image length in the plan.
        plan: u32,
    },
    /// The mailbox layout is unusable.
    Frame(FrameError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FirmwareSizeMismatch { firmware, plan } => write!(
                f,
                "firmware record says {firmware} bytes, the transfer plan says {plan}"
            ),
            Self::Frame(error) => write!(f, "mailbox layout: {error}"),
        }
    }
}

impl core::error::Error for ConfigError {}

/// Everything the machine needs to run one load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    expected: ExpectedIdentity,
    firmware: FirmwareImage,
    plan: TransferPlan,
    limits: Limits,
    frames: Layout,
}

impl Config {
    /// Assemble a configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the record size and the plan disagree, or when the frame
    /// layout is unusable. An unrecorded firmware image is accepted here so [`Machine::start`]
    /// can report it as a named fault.
    pub fn new(
        expected: ExpectedIdentity,
        firmware: FirmwareImage,
        plan: TransferPlan,
        limits: Limits,
        frames: Layout,
    ) -> Result<Self, ConfigError> {
        if let Some(size) = firmware.size
            && size != u64::from(plan.image_len())
        {
            return Err(ConfigError::FirmwareSizeMismatch {
                firmware: size,
                plan: plan.image_len(),
            });
        }
        frames.validate().map_err(ConfigError::Frame)?;
        Ok(Self {
            expected,
            firmware,
            plan,
            limits,
            frames,
        })
    }

    /// Accepted identity.
    #[must_use]
    pub const fn expected(&self) -> ExpectedIdentity {
        self.expected
    }

    /// Firmware record.
    #[must_use]
    pub const fn firmware(&self) -> FirmwareImage {
        self.firmware
    }

    /// Transfer plan.
    #[must_use]
    pub const fn plan(&self) -> TransferPlan {
        self.plan
    }

    /// Retry budgets.
    #[must_use]
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Mailbox layout.
    #[must_use]
    pub const fn frames(&self) -> Layout {
        self.frames
    }
}

/// Where the machine is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Nothing started.
    Idle,
    /// Config space is being read.
    Identifying,
    /// The function answered and the boot ROM is awaited.
    Enumerated,
    /// Chunks are being transferred.
    Loading,
    /// The firmware is verifying the image.
    Verifying,
    /// The firmware is running.
    Ready,
    /// A named fault stopped the machine; only a reset leaves this state.
    Faulted,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Idle => "idle",
            Self::Identifying => "identifying",
            Self::Enumerated => "enumerated",
            Self::Loading => "loading",
            Self::Verifying => "verifying",
            Self::Ready => "ready",
            Self::Faulted => "faulted",
        };
        f.write_str(name)
    }
}

/// What the caller observed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Config space returned an identity.
    Identified(Identity),
    /// The boot ROM announced it is ready for the image.
    BootRomReady,
    /// The firmware acknowledged chunk `sequence`.
    Ack {
        /// Chunk index echoed by the firmware.
        sequence: u16,
    },
    /// The firmware rejected chunk `sequence`.
    Reject {
        /// Chunk index echoed by the firmware.
        sequence: u16,
    },
    /// The verify step reported success.
    VerifyPassed,
    /// The verify step reported a mismatch.
    VerifyFailed,
    /// The previous action did not complete in time.
    Timeout,
    /// A response could not be decoded.
    Malformed,
}

/// What the caller must do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Read the vendor/device pair from config space.
    ReadIdentity,
    /// Poll the boot ROM until [`Event::BootRomReady`].
    AwaitBootRom,
    /// Send this chunk.
    SendChunk(Chunk),
    /// Send this chunk again after a rejection or timeout.
    RetryChunk(Chunk),
    /// Ask the firmware to verify the image.
    RequestVerify,
    /// Start the transferred image.
    StartFirmware,
    /// Stop; the fault is final until a reset.
    Fault(Fault),
}

/// A named terminal fault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The firmware record is incomplete; the load cannot start.
    FirmwareUnrecorded,
    /// The function does not match the expected identity.
    IdentityMismatch {
        /// The constraints.
        expected: ExpectedIdentity,
        /// What the bus returned.
        observed: Identity,
    },
    /// An acknowledgement named a chunk that was not in flight.
    UnexpectedAck {
        /// Chunk in flight.
        expected: u32,
        /// Chunk named by the firmware.
        observed: u16,
    },
    /// A rejection named a chunk that was not in flight.
    UnexpectedReject {
        /// Chunk in flight.
        expected: u32,
        /// Chunk named by the firmware.
        observed: u16,
    },
    /// A chunk exhausted its retry budget.
    ChunkRejected {
        /// Chunk index.
        index: u32,
        /// Failed attempts.
        attempts: u16,
    },
    /// A step exhausted its timeout budget.
    Timeout {
        /// The step that timed out.
        state: State,
    },
    /// The firmware reported a verification failure.
    VerifyFailed,
    /// A response could not be decoded.
    MalformedResponse,
}

/// An event that does not belong to the current state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MachineError {
    /// `start` was called on a machine that is not idle.
    AlreadyStarted {
        /// Current state.
        state: State,
    },
    /// The event is not valid in this state; nothing changed.
    UnexpectedEvent {
        /// Current state.
        state: State,
        /// The rejected event.
        event: Event,
    },
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyStarted { state } => write!(f, "machine already started ({state})"),
            Self::UnexpectedEvent { state, event } => {
                write!(f, "event {event:?} is not valid while {state}")
            }
        }
    }
}

impl core::error::Error for MachineError {}

/// The enumeration and firmware-load machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Machine {
    config: Config,
    state: State,
    transfer: Transfer,
    timeouts: u16,
    fault: Option<Fault>,
}

impl Machine {
    /// Create an idle machine.
    #[must_use]
    pub const fn new(config: Config) -> Self {
        let plan = config.plan();
        Self {
            config,
            state: State::Idle,
            transfer: Transfer::new(plan),
            timeouts: 0,
            fault: None,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// The terminal fault, when the machine is faulted.
    #[must_use]
    pub const fn fault(&self) -> Option<Fault> {
        self.fault
    }

    /// The transfer state.
    #[must_use]
    pub const fn transfer(&self) -> Transfer {
        self.transfer
    }

    /// The configuration.
    #[must_use]
    pub const fn config(&self) -> Config {
        self.config
    }

    /// Begin: check the firmware record and ask for the identity.
    ///
    /// # Errors
    ///
    /// Returns [`MachineError::AlreadyStarted`] when the machine is not idle. An unrecorded
    /// firmware image is a [`Fault::FirmwareUnrecorded`], not an error.
    pub fn start(&mut self) -> Result<Action, MachineError> {
        if self.state != State::Idle {
            return Err(MachineError::AlreadyStarted { state: self.state });
        }
        if !self.config.firmware().is_recorded() {
            return self.fail(Fault::FirmwareUnrecorded);
        }
        self.state = State::Identifying;
        self.timeouts = 0;
        Ok(Action::ReadIdentity)
    }

    /// Feed one observation and return the next action.
    ///
    /// # Errors
    ///
    /// Returns [`MachineError::UnexpectedEvent`] for an event that does not belong to the
    /// current state. Terminal protocol failures are [`Action::Fault`] values.
    pub fn on(&mut self, event: Event) -> Result<Action, MachineError> {
        match event {
            Event::Identified(identity) => self.identified(identity),
            Event::BootRomReady => self.boot_rom_ready(),
            Event::Ack { sequence } => self.acknowledged(sequence),
            Event::Reject { sequence } => self.rejected(sequence),
            Event::VerifyPassed => self.verified(),
            Event::VerifyFailed => self.verify_failed(),
            Event::Timeout => self.timed_out(),
            Event::Malformed => self.malformed(),
        }
    }

    /// Return to idle after a fault (or at any time). The transfer restarts at chunk zero.
    pub fn reset(&mut self) {
        let plan = self.config.plan();
        self.state = State::Idle;
        self.transfer = Transfer::new(plan);
        self.timeouts = 0;
        self.fault = None;
    }

    fn identified(&mut self, identity: Identity) -> Result<Action, MachineError> {
        self.require(State::Identifying, Event::Identified(identity))?;
        if !self.config.expected().matches(identity) {
            return self.fail(Fault::IdentityMismatch {
                expected: self.config.expected(),
                observed: identity,
            });
        }
        self.state = State::Enumerated;
        self.timeouts = 0;
        Ok(Action::AwaitBootRom)
    }

    fn boot_rom_ready(&mut self) -> Result<Action, MachineError> {
        self.require(State::Enumerated, Event::BootRomReady)?;
        self.state = State::Loading;
        self.timeouts = 0;
        match self.transfer.current() {
            Some(chunk) => Ok(Action::SendChunk(chunk)),
            None => {
                self.state = State::Verifying;
                Ok(Action::RequestVerify)
            }
        }
    }

    fn acknowledged(&mut self, sequence: u16) -> Result<Action, MachineError> {
        self.require(State::Loading, Event::Ack { sequence })?;
        if u32::from(sequence) != self.transfer.next_index() {
            return self.fail(Fault::UnexpectedAck {
                expected: self.transfer.next_index(),
                observed: sequence,
            });
        }
        match self.transfer.acked(u32::from(sequence)) {
            Ok(Some(chunk)) => {
                self.timeouts = 0;
                Ok(Action::SendChunk(chunk))
            }
            Ok(None) => {
                self.state = State::Verifying;
                self.timeouts = 0;
                Ok(Action::RequestVerify)
            }
            Err(_) => self.fail(Fault::UnexpectedAck {
                expected: self.transfer.next_index(),
                observed: sequence,
            }),
        }
    }

    fn rejected(&mut self, sequence: u16) -> Result<Action, MachineError> {
        self.require(State::Loading, Event::Reject { sequence })?;
        if u32::from(sequence) != self.transfer.next_index() {
            return self.fail(Fault::UnexpectedReject {
                expected: self.transfer.next_index(),
                observed: sequence,
            });
        }
        let limit = self.config.limits().max_retries;
        match self.transfer.retry(u32::from(sequence), limit) {
            Ok(chunk) => {
                self.timeouts = 0;
                Ok(Action::RetryChunk(chunk))
            }
            Err(TransferError::RetriesExhausted { index, attempts }) => {
                self.fail(Fault::ChunkRejected { index, attempts })
            }
            Err(_) => Err(MachineError::UnexpectedEvent {
                state: self.state,
                event: Event::Reject { sequence },
            }),
        }
    }

    fn verified(&mut self) -> Result<Action, MachineError> {
        self.require(State::Verifying, Event::VerifyPassed)?;
        self.state = State::Ready;
        Ok(Action::StartFirmware)
    }

    fn verify_failed(&mut self) -> Result<Action, MachineError> {
        self.require(State::Verifying, Event::VerifyFailed)?;
        self.fail(Fault::VerifyFailed)
    }

    fn timed_out(&mut self) -> Result<Action, MachineError> {
        match self.state {
            State::Identifying | State::Enumerated | State::Loading | State::Verifying => {}
            State::Idle | State::Ready | State::Faulted => {
                return Err(MachineError::UnexpectedEvent {
                    state: self.state,
                    event: Event::Timeout,
                });
            }
        }
        self.timeouts = self.timeouts.saturating_add(1);
        if self.timeouts > self.config.limits().max_timeouts {
            return self.fail(Fault::Timeout { state: self.state });
        }
        match self.state {
            State::Identifying => Ok(Action::ReadIdentity),
            State::Enumerated => Ok(Action::AwaitBootRom),
            State::Loading => match self.transfer.current() {
                Some(chunk) => Ok(Action::RetryChunk(chunk)),
                None => self.fail(Fault::Timeout {
                    state: State::Loading,
                }),
            },
            State::Verifying => Ok(Action::RequestVerify),
            State::Idle | State::Ready | State::Faulted => Err(MachineError::UnexpectedEvent {
                state: self.state,
                event: Event::Timeout,
            }),
        }
    }

    fn malformed(&mut self) -> Result<Action, MachineError> {
        match self.state {
            State::Identifying | State::Enumerated | State::Loading | State::Verifying => {
                self.fail(Fault::MalformedResponse)
            }
            State::Idle | State::Ready | State::Faulted => Err(MachineError::UnexpectedEvent {
                state: self.state,
                event: Event::Malformed,
            }),
        }
    }

    fn require(&self, expected: State, event: Event) -> Result<(), MachineError> {
        if self.state == expected {
            Ok(())
        } else {
            Err(MachineError::UnexpectedEvent {
                state: self.state,
                event,
            })
        }
    }

    fn fail(&mut self, fault: Fault) -> Result<Action, MachineError> {
        self.state = State::Faulted;
        self.fault = Some(fault);
        Ok(Action::Fault(fault))
    }
}

#[cfg(test)]
#[path = "machine_ut.rs"]
mod tests;
