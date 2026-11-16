// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the PWM/PWM1/CPRMAN/GPIO register facts.

use super::*;

#[test]
fn the_audio_control_word_enables_both_pwm_channels() {
    assert_eq!(CTL_AUDIO, 0xa1e1);
    assert_eq!(CTL_AUDIO & CTL_MSEN2, CTL_MSEN2);
    assert_eq!(CTL_AUDIO & CTL_USEF2, CTL_USEF2);
    assert_eq!(CTL_AUDIO & CTL_PWEN2, CTL_PWEN2);
    assert_eq!(CTL_AUDIO & CTL_MSEN1, CTL_MSEN1);
    assert_eq!(CTL_AUDIO & CTL_CLRF1, CTL_CLRF1);
    assert_eq!(CTL_AUDIO & CTL_USEF1, CTL_USEF1);
    assert_eq!(CTL_AUDIO & CTL_PWEN1, CTL_PWEN1);
}

#[test]
fn the_dma_control_word_is_the_audio_value() {
    assert_eq!(DMAC_AUDIO, 0x8000_0e0e);
    assert_eq!(DMAC_AUDIO & DMAC_ENAB, DMAC_ENAB);
    assert_eq!((DMAC_AUDIO & DMAC_PANIC_MASK) >> DMAC_PANIC_SHIFT, 14);
    assert_eq!(DMAC_AUDIO & DMAC_DREQ_MASK, 14);
}

#[test]
fn the_error_clear_bits_match_the_status_layout() {
    assert_eq!(STA_CLEAR_ERRORS, 0x11c);
    assert_eq!(STA_CLEAR_ERRORS & STA_BERR, STA_BERR);
    assert_eq!(STA_CLEAR_ERRORS & STA_GAPO1, STA_GAPO1);
    assert_eq!(STA_CLEAR_ERRORS & STA_RERR1, STA_RERR1);
    assert_eq!(STA_CLEAR_ERRORS & STA_WERR1, STA_WERR1);
}

#[test]
fn the_pwm_register_offsets_are_stable() {
    assert_eq!(CTL, 0x00);
    assert_eq!(STA, 0x04);
    assert_eq!(DMAC, 0x08);
    assert_eq!(RNG1, 0x10);
    assert_eq!(DAT1, 0x14);
    assert_eq!(FIFO, 0x18);
    assert_eq!(RNG2, 0x20);
    assert_eq!(DAT2, 0x24);
}

#[test]
fn the_board_bases_match_the_firmware_tree() {
    assert_eq!(PWM1_BASE, 0xfe20_c800);
    assert_eq!(PWM1_SIZE, 0x28);
    assert_eq!(PWM_FIFO_BUS, 0x7e20_c818);
    assert_eq!(PWM0_BASE, 0xfe20_c000);
    assert_eq!(CPRMAN_BASE, 0xfe10_1000);
    assert_eq!(GPIO_BASE, 0xfe20_0000);
}

#[test]
fn the_gpio_field_shifts_are_three_and_two_bits() {
    assert_eq!(gpio_fsel_shift(40), Some(0));
    assert_eq!(gpio_fsel_shift(41), Some(3));
    assert_eq!(gpio_fsel_shift(49), Some(27));
    assert_eq!(gpio_fsel_shift(39), None);
    assert_eq!(gpio_pull_shift(40), Some(16));
    assert_eq!(gpio_pull_shift(41), Some(18));
    assert_eq!(gpio_pull_shift(47), Some(30));
    assert_eq!(gpio_pull_shift(48), None);
}
