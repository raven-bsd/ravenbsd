// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! PWM1, CPRMAN, GPIO and DMA register facts.
//!
//! Offsets are from each block's base. The Pi 4 firmware tree maps PWM1 at CPU `0xfe20c800`,
//! CPRMAN at `0xfe101000`, GPIO at `0xfe200000` and the main DMA controller at `0xfe007000`.
//! The legacy PWM0 block at `0xfe20c000` is a different instance and must stay disabled.

/// PWM control register.
pub const CTL: usize = 0x00;
/// PWM status register.
pub const STA: usize = 0x04;
/// PWM DMA configuration register.
pub const DMAC: usize = 0x08;
/// PWM channel 1 range register.
pub const RNG1: usize = 0x10;
/// PWM channel 1 data register.
pub const DAT1: usize = 0x14;
/// PWM FIFO register.
pub const FIFO: usize = 0x18;
/// PWM channel 2 range register.
pub const RNG2: usize = 0x20;
/// PWM channel 2 data register.
pub const DAT2: usize = 0x24;

/// `CTL`: channel 2 serializer enable.
pub const CTL_MSEN2: u32 = 1 << 15;
/// `CTL`: channel 2 FIFO use.
pub const CTL_USEF2: u32 = 1 << 13;
/// `CTL`: channel 2 polarity.
pub const CTL_POLA2: u32 = 1 << 12;
/// `CTL`: channel 2 silence bit.
pub const CTL_SBIT2: u32 = 1 << 11;
/// `CTL`: channel 2 repeat last data.
pub const CTL_RPTL2: u32 = 1 << 10;
/// `CTL`: channel 2 mode.
pub const CTL_MODE2: u32 = 1 << 9;
/// `CTL`: channel 2 enable.
pub const CTL_PWEN2: u32 = 1 << 8;
/// `CTL`: channel 1 serializer enable.
pub const CTL_MSEN1: u32 = 1 << 7;
/// `CTL`: clear the FIFO (self-clearing).
pub const CTL_CLRF1: u32 = 1 << 6;
/// `CTL`: channel 1 FIFO use.
pub const CTL_USEF1: u32 = 1 << 5;
/// `CTL`: channel 1 polarity.
pub const CTL_POLA1: u32 = 1 << 4;
/// `CTL`: channel 1 silence bit.
pub const CTL_SBIT1: u32 = 1 << 3;
/// `CTL`: channel 1 repeat last data.
pub const CTL_RPTL1: u32 = 1 << 2;
/// `CTL`: channel 1 mode.
pub const CTL_MODE1: u32 = 1 << 1;
/// `CTL`: channel 1 enable.
pub const CTL_PWEN1: u32 = 1 << 0;

/// The PWM mode, FIFO-fed, two-channel control value used by the audio path.
pub const CTL_AUDIO: u32 =
    CTL_MSEN2 | CTL_USEF2 | CTL_PWEN2 | CTL_MSEN1 | CTL_CLRF1 | CTL_USEF1 | CTL_PWEN1;

/// `STA`: bus error.
pub const STA_BERR: u32 = 1 << 8;
/// `STA`: channel 1 FIFO gap.
pub const STA_GAPO1: u32 = 1 << 4;
/// `STA`: channel 1 read error.
pub const STA_RERR1: u32 = 1 << 3;
/// `STA`: channel 1 write error.
pub const STA_WERR1: u32 = 1 << 2;
/// `STA`: channel 1 FIFO empty.
pub const STA_EMPT1: u32 = 1 << 1;
/// `STA`: channel 1 FIFO full.
pub const STA_FULL1: u32 = 1 << 0;

/// Status bits cleared after a reset or error: BERR, GAPO1, RERR1 and WERR1.
pub const STA_CLEAR_ERRORS: u32 = STA_BERR | STA_GAPO1 | STA_RERR1 | STA_WERR1;

/// `DMAC`: DMA enable.
pub const DMAC_ENAB: u32 = 1 << 31;
/// `DMAC`: panic threshold mask.
pub const DMAC_PANIC_MASK: u32 = 0x0000_ff00;
/// `DMAC`: panic threshold shift.
pub const DMAC_PANIC_SHIFT: u32 = 8;
/// `DMAC`: DREQ mask.
pub const DMAC_DREQ_MASK: u32 = 0x0000_00ff;
/// The audio DMA configuration: enable, panic 14, DREQ 14.
pub const DMAC_AUDIO: u32 = DMAC_ENAB | (14 << DMAC_PANIC_SHIFT) | 14;

/// GPIO function select register 4 (pins 40..49).
pub const GPIO_GPFSEL4: usize = 0x10;
/// BCM2711 GPIO pull-up/down register 2 (pins 32..47).
pub const GPIO_GPPUPPDN2: usize = 0xec;
/// GPIO alternate function 0.
pub const GPIO_ALT0: u32 = 4;

/// CPRMAN PWM control register.
pub const CM_PWMCTL: usize = 0xa0;
/// CPRMAN PWM divider register.
pub const CM_PWMDIV: usize = 0xa4;

/// CPRMAN register password.
pub const CM_PASSWD: u32 = 0x5a00_0000;
/// `CM_PWMCTL`: busy.
pub const CM_CTL_BUSY: u32 = 1 << 7;
/// `CM_PWMCTL`: enable.
pub const CM_CTL_ENAB: u32 = 1 << 4;
/// `CM_PWMCTL`: source mask.
pub const CM_CTL_SRC_MASK: u32 = 0x0f;
/// PLLD clock source.
pub const CM_CTL_SRC_PLLD: u32 = 6;
/// `CM_PWMDIV`: integer divider mask.
pub const CM_DIV_DIVI_MASK: u32 = 0x000f_f000;
/// `CM_PWMDIV`: integer divider shift.
pub const CM_DIV_DIVI_SHIFT: u32 = 12;
/// `CM_PWMDIV`: fractional divider mask.
pub const CM_DIV_DIVF_MASK: u32 = 0x0000_0fff;

/// Main DMA controller base offset used by the driver tables.
pub const DMA_BASE: u64 = 0xfe00_7000;
/// DMA channel register stride.
pub const DMA_CHANNEL_STRIDE: usize = 0x100;
/// Number of channels on the main DMA controller.
pub const DMA_CHANNELS: u8 = 15;
/// Global DMA enable register.
pub const DMA_ENABLE: usize = 0xff0;
/// Global DMA interrupt-status register.
pub const DMA_INT_STATUS: usize = 0xfe0;
/// Channel CS: reset.
pub const DMA_CS_RESET: u32 = 1 << 31;
/// Channel CS: abort.
pub const DMA_CS_ABORT: u32 = 1 << 30;
/// Channel CS: interrupt.
pub const DMA_CS_INT: u32 = 1 << 2;
/// Channel CS: end.
pub const DMA_CS_END: u32 = 1 << 1;
/// Channel CS: active.
pub const DMA_CS_ACTIVE: u32 = 1 << 0;
/// Channel CS: error.
pub const DMA_CS_ERROR: u32 = 1 << 8;
/// Channel CS: interrupt/end clear mask.
pub const DMA_CS_INTMASK: u32 = DMA_CS_INT | DMA_CS_END;

/// PWM1 CPU physical base (from the live firmware tree).
pub const PWM1_BASE: u64 = 0xfe20_c800;
/// PWM1 register window length.
pub const PWM1_SIZE: u64 = 0x28;
/// PWM FIFO bus address used by the DMA destination field.
pub const PWM_FIFO_BUS: u64 = 0x7e20_c818;
/// CPRMAN CPU physical base.
pub const CPRMAN_BASE: u64 = 0xfe10_1000;
/// GPIO CPU physical base.
pub const GPIO_BASE: u64 = 0xfe20_0000;
/// Legacy PWM0 base; it must not be used by this driver.
pub const PWM0_BASE: u64 = 0xfe20_c000;

/// GPIO function-select bits for one pin inside `GPFSEL4`.
#[must_use]
pub const fn gpio_fsel_shift(pin: u8) -> Option<u32> {
    if pin < 40 || pin > 49 {
        return None;
    }
    Some((pin - 40) as u32 * 3)
}

/// GPIO pull field shift for one pin inside `GPPUPPDN2`.
#[must_use]
pub const fn gpio_pull_shift(pin: u8) -> Option<u32> {
    if pin < 32 || pin > 47 {
        return None;
    }
    Some((pin - 32) as u32 * 2)
}

#[cfg(test)]
#[path = "registers_ut.rs"]
mod tests;
