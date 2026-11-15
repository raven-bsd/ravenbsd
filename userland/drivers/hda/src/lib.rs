// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable Intel HDA core for the QEMU `hda-output` driver (B44).
//!
//! The device driver lives in `main.rs`; this crate owns the register map, the CORB/RIRB
//! command encoding, the QEMU codec widget fields, the stream format word, the buffer
//! descriptor list encoding and the driver-ring geometry. It is `no_std`, allocates nothing and
//! never touches hardware, so the byte-level rules run under `cargo test`.
//!
//! Ported from the MIT Redox `ihdad` driver (`src/hda/common.rs`, `device.rs`, `stream.rs`);
//! the clean-room adaptation is first-party BSD-3 and records no donor import in
//! `PROVENANCE.toml` because the structure and identifiers are rewritten for this tree.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Global capability register.
pub const GCAP: usize = 0x00;
/// Global control register.
pub const GCTL: usize = 0x08;
/// Codec state change register.
pub const STATESTS: usize = 0x0e;
/// Interrupt control register.
pub const INTCTL: usize = 0x20;
/// Interrupt status register.
pub const INTSTS: usize = 0x24;
/// CORB lower base.
pub const CORBLBASE: usize = 0x40;
/// CORB upper base.
pub const CORBUBASE: usize = 0x44;
/// CORB write pointer.
pub const CORBWP: usize = 0x48;
/// CORB read pointer.
pub const CORBRP: usize = 0x4a;
/// CORB control.
pub const CORBCTL: usize = 0x4c;
/// CORB size.
pub const CORBSIZE: usize = 0x4e;
/// RIRB lower base.
pub const RIRBLBASE: usize = 0x50;
/// RIRB upper base.
pub const RIRBUBASE: usize = 0x54;
/// RIRB write pointer.
pub const RIRBWP: usize = 0x58;
/// RIRB interrupt count.
pub const RINTCNT: usize = 0x5a;
/// RIRB control.
pub const RIRBCTL: usize = 0x5c;
/// RIRB size.
pub const RIRBSIZE: usize = 0x5e;
/// Immediate command input.
pub const ICOI: usize = 0x60;
/// Immediate response input.
pub const IRII: usize = 0x64;
/// Immediate command status.
pub const ICS: usize = 0x68;
/// DMA position lower base.
pub const DPLBASE: usize = 0x70;
/// DMA position upper base.
pub const DPUBASE: usize = 0x74;
/// First stream descriptor.
pub const SD_BASE: usize = 0x80;
/// Stream descriptor stride.
pub const SD_STRIDE: usize = 0x20;

/// Stream descriptor control low.
pub const SD_CTL_LO: usize = 0x00;
/// Stream descriptor control high.
pub const SD_CTL_HI: usize = 0x02;
/// Stream descriptor status.
pub const SD_STATUS: usize = 0x03;
/// Stream descriptor link position.
pub const SD_LPIB: usize = 0x04;
/// Stream descriptor cyclic buffer length.
pub const SD_CBL: usize = 0x08;
/// Stream descriptor last valid index.
pub const SD_LVI: usize = 0x0c;
/// Stream descriptor format.
pub const SD_FORMAT: usize = 0x12;
/// Stream descriptor BDL lower base.
pub const SD_BDPL: usize = 0x18;
/// Stream descriptor BDL upper base.
pub const SD_BDPU: usize = 0x1c;

/// CRST in `GCTL`.
pub const GCTL_CRST: u32 = 1 << 0;
/// CORB run bit.
pub const CORBCTL_RUN: u8 = 1 << 1;
/// RIRB DMA enable.
pub const RIRBCTL_DMAEN: u8 = 1 << 1;
/// RIRB interrupt control.
pub const RIRBCTL_INTCTL: u8 = 1 << 0;
/// Immediate command busy.
pub const ICS_ICB: u16 = 1 << 0;
/// Immediate response valid.
pub const ICS_IRV: u16 = 1 << 1;
/// SD control stream run.
pub const SDCTL_RUN: u32 = 1 << 1;
/// SD control stream reset.
pub const SDCTL_SRST: u32 = 1 << 0;
/// SD control interrupt on completion.
pub const SDCTL_IOC: u32 = 1 << 2;
/// SD control stream number shift.
pub const SDCTL_STREAM_SHIFT: u32 = 20;

/// Bits per sample code for HDA `S16LE`.
pub const BITS_16: u16 = 1;
/// Base rate 48 kHz.
pub const BASE_48K: u16 = 0;

/// Encode one 4-bit codec / 12-bit verb command with an 8-bit payload.
#[must_use]
pub const fn cmd12(codec: u8, node: u8, verb: u16, payload: u8) -> u32 {
    ((codec as u32 & 0x0f) << 28)
        | ((node as u32 & 0xff) << 20)
        | ((verb as u32 & 0x0fff) << 8)
        | (payload as u32)
}

/// Encode one 4-bit codec / 4-bit verb command with a 16-bit payload.
#[must_use]
pub const fn cmd4(codec: u8, node: u8, verb: u16, payload: u16) -> u32 {
    ((codec as u32 & 0x0f) << 28)
        | ((node as u32 & 0xff) << 20)
        | ((verb as u32 & 0x000f) << 16)
        | (payload as u32)
}

/// Encode the HDA stream format word for 48 kHz, 16-bit, `channels` channels.
#[must_use]
pub const fn format_48k(channels: u8) -> u16 {
    BASE_48K | ((BITS_16 & 0x7) << 4) | ((channels.saturating_sub(1) as u16) & 0x0f)
}

/// QEMU `intel-hda` (ICH6) vendor id.
pub const QEMU_HDA_VENDOR: u16 = 0x8086;
/// QEMU `intel-hda` (ICH6) device id.
pub const QEMU_HDA_DEVICE: u16 = 0x2668;
/// PCI base class for a multimedia device.
pub const PCI_CLASS_MULTIMEDIA: u8 = 0x04;
/// PCI subclass for an audio device (HDA is class 0x0403).
pub const PCI_SUBCLASS_AUDIO: u8 = 0x03;
/// Widget type value of an audio output converter.
pub const WIDGET_AUDIO_OUTPUT: u8 = 0x0;
/// Widget type value of a pin complex.
pub const WIDGET_PIN_COMPLEX: u8 = 0x4;

/// Widget type from a `GET_PARAMETER` capabilities word (bits 23:20).
#[must_use]
pub const fn widget_type(capabilities: u32) -> u8 {
    ((capabilities >> 20) & 0x0f) as u8
}

/// Whether the widget has an output amplifier (capabilities bit 2).
#[must_use]
pub const fn widget_has_output_amp(capabilities: u32) -> bool {
    capabilities & (1 << 2) != 0
}

/// Whether the widget has an input amplifier (capabilities bit 1).
#[must_use]
pub const fn widget_has_input_amp(capabilities: u32) -> bool {
    capabilities & (1 << 1) != 0
}

/// Whether a pin's default configuration names an output device.
///
/// The default-device field is bits 23:20; values 0..=6 are line out, speaker, headphone,
/// CD, S/PDIF, digital other out and modem line side.
#[must_use]
pub const fn config_is_output(config_default: u32) -> bool {
    ((config_default >> 20) & 0x0f) <= 6
}

/// Build a stream-descriptor control word.
///
/// `stream_tag` occupies bits 23:20, `ioc` is bit 2 and `run` is bit 1.
#[must_use]
pub const fn stream_control(stream_tag: u8, run: bool, ioc: bool) -> u32 {
    ((stream_tag as u32 & 0x0f) << 20) | ((ioc as u32) << 2) | ((run as u32) << 1)
}

/// Offset of output stream `index` after the input stream descriptors.
#[must_use]
pub const fn stream_descriptor_offset(input_streams: u8, index: u8) -> usize {
    SD_BASE + (input_streams as usize + index as usize) * SD_STRIDE
}

/// Frames advanced between two link-position readings, accounting for the cyclic wrap.
///
/// `previous` and `current` are byte offsets inside a cyclic buffer of `cbl_bytes`; a zero
/// `frame_bytes` yields zero.
#[must_use]
pub const fn lpib_delta_frames(
    previous: u32,
    current: u32,
    cbl_bytes: u32,
    frame_bytes: u16,
) -> u32 {
    if frame_bytes == 0 {
        return 0;
    }
    let delta = if current >= previous {
        current - previous
    } else {
        cbl_bytes.saturating_sub(previous).saturating_add(current)
    };
    delta / frame_bytes as u32
}

/// The 48 kHz stereo S16LE ring used by the target driver: 480 frames per period, 17 periods.
///
/// `region_len()` is exactly 32 KiB (128-byte header plus 32,640 bytes of PCM), so the driver
/// can use the first half of its 64 KiB DMA buffer and keep CORB/RIRB/BDL in the second half.
#[must_use]
pub const fn ring_geometry_48k() -> RingGeometry {
    RingGeometry {
        period_frames: 480,
        period_count: 17,
        frame_bytes: 4,
    }
}

/// One buffer descriptor list entry (16 bytes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BdlEntry {
    /// Device-visible buffer address.
    pub address: u64,
    /// Buffer length in bytes.
    pub length: u32,
    /// Interrupt on completion.
    pub ioc: bool,
}

impl BdlEntry {
    /// Encode the entry into `out[offset..offset+16]`.
    pub fn write(&self, out: &mut [u8], offset: usize) -> bool {
        let Some(entry) = out.get_mut(offset..offset.saturating_add(16)) else {
            return false;
        };
        entry[0..8].copy_from_slice(&self.address.to_le_bytes());
        entry[8..12].copy_from_slice(&self.length.to_le_bytes());
        entry[12..16].fill(0);
        if self.ioc {
            entry[12] = 1;
        }
        true
    }
}

/// Validated driver-ring geometry for the HDA sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingGeometry {
    /// Frames per period.
    pub period_frames: u32,
    /// Number of periods.
    pub period_count: u16,
    /// Bytes per frame.
    pub frame_bytes: u16,
}

impl RingGeometry {
    /// The exact ring region length (header alignment plus PCM).
    #[must_use]
    pub const fn region_len(&self) -> usize {
        let data =
            self.period_frames as usize * self.period_count as usize * self.frame_bytes as usize;
        128 + data
    }

    /// The cyclic buffer length in bytes.
    #[must_use]
    pub const fn cbl_bytes(&self) -> u32 {
        self.period_frames * self.period_count as u32 * self.frame_bytes as u32
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
