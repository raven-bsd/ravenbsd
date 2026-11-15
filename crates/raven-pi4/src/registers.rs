// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! EMMC2/SDHCI register map and typed register values.
//!
//! Offsets, widths and bits are the SD Host Controller simplified-specification subset that the
//! BCM2711 EMMC2 block implements. The physical base, interrupt and DMA window are device-tree
//! facts and come from [`crate::fdt`]; this module only describes the register block itself.
//!
//! Hardware-specific values that the B42 spike could not confirm (the exact capabilities contents,
//! the specification version) remain UNVERIFIED until a board log exists. The model uses
//! [`DEFAULT_BASE_CLOCK_HZ`] only when neither the mailbox nor the capabilities register supplies a
//! rate, and [`Quirks::PI4_EMMC2`] records the NetBSD quirk set.

/// Width of one register access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessWidth {
    /// One byte.
    Byte,
    /// Two bytes, little endian.
    HalfWord,
    /// Four bytes, little endian.
    Word,
}

/// SDHCI registers used by the EMMC2 model.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Register {
    /// System DMA address (`0x00`, 32-bit).
    DmaAddress = 0x00,
    /// Block size and DMA boundary (`0x04`, 16-bit).
    BlockSize = 0x04,
    /// Block count (`0x06`, 16-bit).
    BlockCount = 0x06,
    /// Command argument (`0x08`, 32-bit).
    Argument = 0x08,
    /// Transfer mode (`0x0c`, 16-bit).
    TransferMode = 0x0c,
    /// Command and response type (`0x0e`, 16-bit).
    Command = 0x0e,
    /// Response word 0, bits 31:0 (`0x10`).
    Response0 = 0x10,
    /// Response word 1, bits 63:32 (`0x14`).
    Response1 = 0x14,
    /// Response word 2, bits 95:64 (`0x18`).
    Response2 = 0x18,
    /// Response word 3, bits 127:96 (`0x1c`).
    Response3 = 0x1c,
    /// PIO data port (`0x20`, 32-bit).
    DataPort = 0x20,
    /// Present state (`0x24`, 32-bit).
    PresentState = 0x24,
    /// Host control 1: bus width, high speed and DMA select (`0x28`, 8-bit).
    HostControl = 0x28,
    /// Power control (`0x29`, 8-bit).
    PowerControl = 0x29,
    /// Block gap control (`0x2a`, 8-bit).
    BlockGapControl = 0x2a,
    /// Wake-up control (`0x2b`, 8-bit).
    WakeupControl = 0x2b,
    /// Clock control (`0x2c`, 16-bit).
    ClockControl = 0x2c,
    /// Timeout control (`0x2e`, 8-bit).
    TimeoutControl = 0x2e,
    /// Software reset (`0x2f`, 8-bit).
    SoftwareReset = 0x2f,
    /// Normal interrupt status (`0x30`, 16-bit, write one to clear).
    NormalInterruptStatus = 0x30,
    /// Error interrupt status (`0x32`, 16-bit, write one to clear).
    ErrorInterruptStatus = 0x32,
    /// Normal interrupt status enable (`0x34`, 16-bit).
    NormalInterruptStatusEnable = 0x34,
    /// Error interrupt status enable (`0x36`, 16-bit).
    ErrorInterruptStatusEnable = 0x36,
    /// Normal interrupt signal enable (`0x38`, 16-bit).
    NormalInterruptSignalEnable = 0x38,
    /// Error interrupt signal enable (`0x3a`, 16-bit).
    ErrorInterruptSignalEnable = 0x3a,
    /// Auto CMD12 error status (`0x3c`, 16-bit).
    AutoCmd12ErrorStatus = 0x3c,
    /// Host control 2 (`0x3e`, 16-bit).
    HostControl2 = 0x3e,
    /// Capabilities (`0x40`, 32-bit).
    Capabilities = 0x40,
    /// Capabilities 2 (`0x44`, 32-bit).
    Capabilities2 = 0x44,
    /// ADMA error status (`0x54`, 8-bit).
    AdmaErrorStatus = 0x54,
    /// ADMA system address (`0x58`, 32-bit).
    AdmaSystemAddress = 0x58,
    /// Slot interrupt status (`0xfc`, 16-bit).
    SlotInterruptStatus = 0xfc,
    /// Host controller version (`0xfe`, 16-bit).
    HostControllerVersion = 0xfe,
}

impl Register {
    /// Byte offset of the register inside the mapped EMMC2 window.
    pub const fn offset(self) -> usize {
        self as usize
    }

    /// Access width the register requires.
    pub const fn width(self) -> AccessWidth {
        match self {
            Self::BlockSize
            | Self::BlockCount
            | Self::TransferMode
            | Self::Command
            | Self::NormalInterruptStatus
            | Self::ErrorInterruptStatus
            | Self::NormalInterruptStatusEnable
            | Self::ErrorInterruptStatusEnable
            | Self::NormalInterruptSignalEnable
            | Self::ErrorInterruptSignalEnable
            | Self::AutoCmd12ErrorStatus
            | Self::HostControl2
            | Self::ClockControl
            | Self::SlotInterruptStatus
            | Self::HostControllerVersion => AccessWidth::HalfWord,
            Self::HostControl
            | Self::PowerControl
            | Self::BlockGapControl
            | Self::WakeupControl
            | Self::TimeoutControl
            | Self::SoftwareReset
            | Self::AdmaErrorStatus => AccessWidth::Byte,
            _ => AccessWidth::Word,
        }
    }

    /// Mask of the bits the register has in its access width.
    pub const fn mask(self) -> u32 {
        match self.width() {
            AccessWidth::Byte => 0xff,
            AccessWidth::HalfWord => 0xffff,
            AccessWidth::Word => 0xffff_ffff,
        }
    }
}

/// Normal interrupt status: a command completed.
pub const NORMAL_CMD_COMPLETE: u16 = 1 << 0;
/// Normal interrupt status: a data transfer completed.
pub const NORMAL_TRANSFER_COMPLETE: u16 = 1 << 1;
/// Normal interrupt status: the write buffer can accept data.
pub const NORMAL_BUFFER_WRITE_READY: u16 = 1 << 4;
/// Normal interrupt status: the read buffer has data.
pub const NORMAL_BUFFER_READ_READY: u16 = 1 << 5;
/// Normal interrupt status: the error interrupt summary bit.
pub const NORMAL_ERROR_INTERRUPT: u16 = 1 << 15;
/// Normal interrupt status bits the controller implements.
pub const NORMAL_STATUS_MASK: u16 = 0x91ff;

/// Error interrupt status: command timeout.
pub const ERROR_CMD_TIMEOUT: u16 = 1 << 0;
/// Error interrupt status: command CRC.
pub const ERROR_CMD_CRC: u16 = 1 << 1;
/// Error interrupt status: command end bit.
pub const ERROR_CMD_END_BIT: u16 = 1 << 2;
/// Error interrupt status: command index.
pub const ERROR_CMD_INDEX: u16 = 1 << 3;
/// Error interrupt status: data timeout.
pub const ERROR_DATA_TIMEOUT: u16 = 1 << 4;
/// Error interrupt status: data CRC.
pub const ERROR_DATA_CRC: u16 = 1 << 5;
/// Error interrupt status: current limit.
pub const ERROR_CURRENT_LIMIT: u16 = 1 << 7;
/// Error interrupt status: auto CMD12 failed.
pub const ERROR_AUTO_CMD12: u16 = 1 << 8;
/// Error interrupt status: ADMA failed.
pub const ERROR_ADMA: u16 = 1 << 9;
/// Error interrupt status: system DMA failed.
pub const ERROR_DMA: u16 = 1 << 12;
/// Error interrupt status bits the controller implements.
pub const ERROR_STATUS_MASK: u16 = 0x03ff;

/// Present state: the command line is inhibited.
pub const PRESENT_CMD_INHIBIT: u32 = 1 << 0;
/// Present state: the data line is inhibited.
pub const PRESENT_DAT_INHIBIT: u32 = 1 << 1;
/// Present state: the read buffer can be drained.
pub const PRESENT_BUFFER_READ_ENABLE: u32 = 1 << 11;
/// Present state: the write buffer can be filled.
pub const PRESENT_BUFFER_WRITE_ENABLE: u32 = 1 << 10;

/// Software reset: reset the whole host controller.
pub const SOFTWARE_RESET_ALL: u8 = 1 << 0;
/// Software reset: reset the command circuit.
pub const SOFTWARE_RESET_CMD: u8 = 1 << 1;
/// Software reset: reset the data circuit.
pub const SOFTWARE_RESET_DAT: u8 = 1 << 2;

/// Host control: 4-bit data bus.
pub const HOST_CONTROL_4BIT: u8 = 1 << 1;
/// Host control: high-speed timing.
pub const HOST_CONTROL_HIGH_SPEED: u8 = 1 << 2;
/// Host control: DMA select shift.
pub const HOST_CONTROL_DMA_SHIFT: u8 = 3;
/// Host control: DMA select mask.
pub const HOST_CONTROL_DMA_MASK: u8 = 0x3 << HOST_CONTROL_DMA_SHIFT;

/// Power control: apply bus power.
pub const POWER_BUS: u8 = 1 << 0;
/// Power control: voltage field shift.
pub const POWER_VOLTAGE_SHIFT: u8 = 1;
/// Power control: voltage field mask.
pub const POWER_VOLTAGE_MASK: u8 = 0x7 << POWER_VOLTAGE_SHIFT;
/// Power control: 3.3 V voltage code.
pub const POWER_VOLTAGE_3V3: u8 = 0x7 << POWER_VOLTAGE_SHIFT;

/// Clock control: enable the internal clock.
pub const CLOCK_INTERNAL_ENABLE: u16 = 1 << 0;
/// Clock control: the internal clock is stable.
pub const CLOCK_INTERNAL_STABLE: u16 = 1 << 1;
/// Clock control: drive the SD clock.
pub const CLOCK_SD_ENABLE: u16 = 1 << 2;
/// Clock control: low eight bits of the frequency select field.
pub const CLOCK_DIVIDER_LOW_SHIFT: u16 = 8;
/// Clock control: high two bits of the frequency select field.
pub const CLOCK_DIVIDER_HIGH_SHIFT: u16 = 6;

/// Default base clock used when neither the mailbox nor the capabilities register knows it.
///
/// NetBSD's `bcm2835_emmc.c` uses 50 MHz as the fallback; the mailbox clock id 12 is the
/// authoritative source at run time. UNVERIFIED on silicon.
pub const DEFAULT_BASE_CLOCK_HZ: u32 = 50_000_000;

/// VideoCore mailbox clock id for the EMMC2 clock (`VCPROP_CLK_EMMC2`).
pub const CLOCK_ID_EMMC2: u32 = 12;

/// Size of the EMMC2 register window in the Pi 4 device tree.
pub const EMMC2_WINDOW_BYTES: u64 = 0x100;

/// Controller quirks the EMMC2 integration must respect.
///
/// The values mirror the NetBSD flags in `bcm2835_emmc.c` and `bcm2838_emmc2_acpi.c`: the
/// controller reports no usable capabilities, does not implement power state 0, has no 1.8 V
/// signalling, needs 32-bit register access and does not honour the standard high-speed bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quirks {
    /// The capabilities register contents are usable as-is.
    pub host_capabilities_valid: bool,
    /// Do not rely on the standard high-speed host-control bit.
    pub no_high_speed_bit: bool,
    /// Power state 0 is not implemented.
    pub no_power_0: bool,
    /// 1.8 V signalling is not available.
    pub no_1v8: bool,
    /// Registers must be accessed as 32-bit values.
    pub thirty_two_bit_access: bool,
}

impl Quirks {
    /// The quirk set the B42 spike records for `brcm,bcm2711-emmc2`.
    pub const PI4_EMMC2: Self = Self {
        host_capabilities_valid: false,
        no_high_speed_bit: true,
        no_power_0: true,
        no_1v8: true,
        thirty_two_bit_access: true,
    };
}

/// Capabilities register (`0x40`) decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities(u32);

impl Capabilities {
    /// Decode a raw capabilities word.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw capabilities word.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// 3.3 V is supported (bit 24).
    pub const fn voltage_3v3(self) -> bool {
        self.0 & (1 << 24) != 0
    }

    /// 1.8 V is supported (bit 26).
    pub const fn voltage_1v8(self) -> bool {
        self.0 & (1 << 26) != 0
    }

    /// The controller has a DMA engine (bit 22).
    pub const fn dma_supported(self) -> bool {
        self.0 & (1 << 22) != 0
    }

    /// High-speed timing is supported (bit 21).
    pub const fn high_speed_supported(self) -> bool {
        self.0 & (1 << 21) != 0
    }

    /// ADMA2 is supported (bit 19).
    pub const fn adma2_supported(self) -> bool {
        self.0 & (1 << 19) != 0
    }

    /// An 8-bit bus is supported (bit 18).
    pub const fn eight_bit_supported(self) -> bool {
        self.0 & (1 << 18) != 0
    }

    /// Maximum block length in bytes (`512 << bits 17:16`).
    pub const fn max_block_len(self) -> u32 {
        512 << ((self.0 >> 16) & 0x3)
    }

    /// Base clock in Hz from the capabilities field (bits 15:8, MHz).
    pub const fn base_clock_hz(self) -> u32 {
        ((self.0 >> 8) & 0xff).wrapping_mul(1_000_000)
    }
}

/// Normal interrupt status decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NormalInterrupt(u16);

impl NormalInterrupt {
    /// Decode a raw status word.
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw & NORMAL_STATUS_MASK)
    }

    /// The raw status word.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// A command completed.
    pub const fn command_complete(self) -> bool {
        self.0 & NORMAL_CMD_COMPLETE != 0
    }

    /// A data transfer completed.
    pub const fn transfer_complete(self) -> bool {
        self.0 & NORMAL_TRANSFER_COMPLETE != 0
    }

    /// The error interrupt summary is set.
    pub const fn error_interrupt(self) -> bool {
        self.0 & NORMAL_ERROR_INTERRUPT != 0
    }
}

/// Error interrupt status decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ErrorInterrupt(u16);

impl ErrorInterrupt {
    /// Decode a raw status word.
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw & ERROR_STATUS_MASK)
    }

    /// The raw status word.
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// True when any implemented error bit is set.
    pub const fn any(self) -> bool {
        self.0 != 0
    }
}

/// Present state decoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PresentState(u32);

impl PresentState {
    /// Decode a raw present-state word.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw present-state word.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The command line is inhibited.
    pub const fn cmd_inhibit(self) -> bool {
        self.0 & PRESENT_CMD_INHIBIT != 0
    }

    /// The data line is inhibited.
    pub const fn dat_inhibit(self) -> bool {
        self.0 & PRESENT_DAT_INHIBIT != 0
    }

    /// The read buffer can be drained.
    pub const fn buffer_read_ready(self) -> bool {
        self.0 & PRESENT_BUFFER_READ_ENABLE != 0
    }

    /// The write buffer can be filled.
    pub const fn buffer_write_ready(self) -> bool {
        self.0 & PRESENT_BUFFER_WRITE_ENABLE != 0
    }
}

#[cfg(test)]
#[path = "registers_ut.rs"]
mod tests;
