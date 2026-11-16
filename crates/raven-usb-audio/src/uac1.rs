// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB Audio Class 1.0 descriptors and the entity graph.
//!
//! The descriptor shapes are the public USB Audio Class 1.0 definitions: an AudioControl
//! interface with an AC header, an input terminal (USB streaming), a feature unit and an output
//! terminal (speaker), and an AudioStreaming interface with the AS general/format-type-I
//! descriptors and the isochronous endpoints. The parser checks every length and reference; the
//! encoder writes a complete configuration set so the tests can round-trip it.

use core::fmt;

use crate::usb::{
    ConfigDescriptor, DescriptorError, DeviceDescriptor, Direction, EndpointDescriptor,
    InterfaceDescriptor, TYPE_ENDPOINT, TYPE_INTERFACE, TransferType, UsageType,
};

/// Class-specific interface descriptor type.
pub const CS_INTERFACE: u8 = 0x24;
/// Class-specific endpoint descriptor type.
pub const CS_ENDPOINT: u8 = 0x25;
/// Audio interface class.
pub const AUDIO_CLASS: u8 = 0x01;
/// AudioControl interface subclass.
pub const SUBCLASS_AUDIOCONTROL: u8 = 0x01;
/// AudioStreaming interface subclass.
pub const SUBCLASS_AUDIOSTREAMING: u8 = 0x02;

/// AC header subtype.
pub const AC_HEADER: u8 = 0x01;
/// Input terminal subtype.
pub const INPUT_TERMINAL: u8 = 0x02;
/// Output terminal subtype.
pub const OUTPUT_TERMINAL: u8 = 0x03;
/// Feature unit subtype.
pub const FEATURE_UNIT: u8 = 0x06;
/// AS general subtype.
pub const AS_GENERAL: u8 = 0x01;
/// Format type I subtype.
pub const FORMAT_TYPE_I: u8 = 0x02;

/// USB streaming terminal type.
pub const TERMINAL_USB_STREAMING: u16 = 0x0101;
/// Speaker terminal type.
pub const TERMINAL_SPEAKER: u16 = 0x0301;

/// Largest number of discrete sample rates the model stores.
pub const MAX_RATES: usize = 8;
/// Largest feature-unit control array.
pub const MAX_CONTROLS: usize = 12;

/// Why a UAC1 descriptor set cannot be decoded or encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Uac1Error {
    /// A standard descriptor is malformed.
    Descriptor(DescriptorError),
    /// A required descriptor is absent.
    Missing(&'static str),
    /// A descriptor appears twice or out of order.
    Unexpected(&'static str),
    /// A field value is invalid.
    BadValue(&'static str),
    /// A terminal/unit reference does not resolve.
    BadEntity(&'static str),
    /// A class descriptor is shorter than its fixed fields.
    Truncated(&'static str),
    /// The output buffer is too small.
    Overflow,
}

impl fmt::Display for Uac1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Descriptor(error) => write!(f, "descriptor: {error}"),
            Self::Missing(what) => write!(f, "missing {what}"),
            Self::Unexpected(what) => write!(f, "unexpected {what}"),
            Self::BadValue(what) => write!(f, "invalid {what}"),
            Self::BadEntity(what) => write!(f, "unresolved {what}"),
            Self::Truncated(what) => write!(f, "{what} is truncated"),
            Self::Overflow => f.write_str("output buffer is too small"),
        }
    }
}

impl core::error::Error for Uac1Error {}

impl From<DescriptorError> for Uac1Error {
    fn from(error: DescriptorError) -> Self {
        Self::Descriptor(error)
    }
}

/// The AC interface header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcHeader {
    /// `bcdADC`.
    pub adc_version: u16,
    /// `wTotalLength` of the class-specific AC descriptors.
    pub total_length: u16,
    /// Streaming interface numbers.
    pub interface_numbers: [u8; 4],
    /// Number of entries in `interface_numbers`.
    pub count: u8,
}

/// The input terminal (USB streaming).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputTerminal {
    /// `bTerminalID`.
    pub id: u8,
    /// `wTerminalType`.
    pub terminal_type: u16,
    /// `bNrChannels`.
    pub channels: u8,
    /// `wChannelConfig`.
    pub channel_config: u16,
}

/// The output terminal (speaker or headset).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputTerminal {
    /// `bTerminalID`.
    pub id: u8,
    /// `wTerminalType`.
    pub terminal_type: u16,
    /// `bSourceID`.
    pub source_id: u8,
}

/// The feature unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureUnit {
    /// `bUnitID`.
    pub id: u8,
    /// `bSourceID`.
    pub source_id: u8,
    /// Control bitmaps: master plus one per channel.
    pub controls: [u8; MAX_CONTROLS],
    /// Number of control bytes (`channels + 1`).
    pub count: u8,
}

impl FeatureUnit {
    /// True when the master or channel control byte has the mute bit.
    #[must_use]
    pub fn supports_mute(&self) -> bool {
        self.controls
            .iter()
            .take(usize::from(self.count))
            .any(|control| control & 0x01 != 0)
    }

    /// True when the master or channel control byte has the volume bit.
    #[must_use]
    pub fn supports_volume(&self) -> bool {
        self.controls
            .iter()
            .take(usize::from(self.count))
            .any(|control| control & 0x02 != 0)
    }
}

/// The AS general descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsInterface {
    /// `bTerminalLink` (the input terminal ID).
    pub terminal_link: u8,
    /// `bDelay`.
    pub delay: u8,
    /// `wFormatTag` (1 = PCM).
    pub format_tag: u16,
}

/// The format type I descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatTypeI {
    /// `bNrChannels`.
    pub channels: u8,
    /// `bSubframeSize` (container bytes).
    pub subframe_bytes: u8,
    /// `bBitResolution`.
    pub bit_resolution: u8,
    /// Discrete sample rates.
    pub rates: [u32; MAX_RATES],
    /// Number of rates.
    pub count: u8,
}

impl FormatTypeI {
    /// True when `rate` is one of the supported rates.
    #[must_use]
    pub fn supports_rate(&self, rate: u32) -> bool {
        self.rates
            .iter()
            .take(usize::from(self.count))
            .any(|value| *value == rate)
    }
}

/// The operational AudioStreaming interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamingInterface {
    /// Standard interface descriptor.
    pub interface: InterfaceDescriptor,
    /// AS general descriptor.
    pub general: AsInterface,
    /// Format type I descriptor.
    pub format: FormatTypeI,
    /// Isochronous data endpoint.
    pub data: EndpointDescriptor,
    /// Asynchronous feedback endpoint, when the device has one.
    pub feedback: Option<EndpointDescriptor>,
}

/// A complete UAC1 descriptor set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Uac1Config {
    /// Device descriptor.
    pub device: DeviceDescriptor,
    /// Configuration header (`total_length` is recomputed on encode).
    pub config: ConfigDescriptor,
    /// AudioControl interface.
    pub control: InterfaceDescriptor,
    /// AC header.
    pub header: AcHeader,
    /// Input terminal.
    pub input: InputTerminal,
    /// Feature unit.
    pub feature: FeatureUnit,
    /// Output terminal.
    pub output: OutputTerminal,
    /// AudioStreaming interface.
    pub streaming: StreamingInterface,
}

impl Uac1Config {
    /// Decode a complete descriptor set (device, configuration, UAC1 interfaces/endpoints).
    ///
    /// # Errors
    ///
    /// Returns [`Uac1Error`] for a truncated/malformed descriptor, a wrong interface class or
    /// subclass, a missing required descriptor, an unresolved entity reference or an invalid
    /// endpoint/format.
    pub fn parse(bytes: &[u8]) -> Result<Self, Uac1Error> {
        let device = DeviceDescriptor::parse(bytes)?;
        let config = ConfigDescriptor::parse(&bytes[18..])?;
        let total = usize::from(config.total_length);
        let end = 18usize
            .checked_add(total)
            .ok_or(Uac1Error::BadValue("configuration length"))?;
        if end > bytes.len() {
            return Err(Uac1Error::Truncated("configuration"));
        }
        let mut control = None;
        let mut header = None;
        let mut input = None;
        let mut feature = None;
        let mut output = None;
        let mut streaming = None;
        let mut general = None;
        let mut format = None;
        let mut data = None;
        let mut feedback = None;
        let mut in_streaming = false;

        let mut pos = 18 + 9;
        while pos + 2 <= end {
            let length = usize::from(bytes[pos]);
            if length < 2 || pos + length > end {
                return Err(Uac1Error::BadValue("descriptor length"));
            }
            let kind = bytes[pos + 1];
            match kind {
                TYPE_INTERFACE => {
                    let interface = InterfaceDescriptor::parse(&bytes[pos..pos + length])?;
                    if interface.class != AUDIO_CLASS {
                        return Err(Uac1Error::BadValue("non-audio interface"));
                    }
                    if interface.subclass == SUBCLASS_AUDIOCONTROL {
                        if control.is_some() {
                            return Err(Uac1Error::Unexpected("control interface"));
                        }
                        control = Some(interface);
                        in_streaming = false;
                    } else if interface.subclass == SUBCLASS_AUDIOSTREAMING {
                        if streaming.is_some() {
                            return Err(Uac1Error::Unexpected("streaming interface"));
                        }
                        streaming = Some(interface);
                        in_streaming = true;
                    } else {
                        return Err(Uac1Error::BadValue("audio subclass"));
                    }
                }
                CS_INTERFACE => {
                    if length < 3 {
                        return Err(Uac1Error::Truncated("class-specific descriptor"));
                    }
                    let body = bytes
                        .get(pos..pos + length)
                        .ok_or(Uac1Error::Truncated("CS"))?;
                    if in_streaming {
                        match body[2] {
                            AS_GENERAL => {
                                if general.is_some() {
                                    return Err(Uac1Error::Unexpected("AS general"));
                                }
                                general = Some(parse_as_general(body)?);
                            }
                            FORMAT_TYPE_I => {
                                if format.is_some() {
                                    return Err(Uac1Error::Unexpected("format type"));
                                }
                                format = Some(parse_format(body)?);
                            }
                            _ => {}
                        }
                    } else {
                        match body[2] {
                            AC_HEADER => {
                                if header.is_some() {
                                    return Err(Uac1Error::Unexpected("AC header"));
                                }
                                header = Some(parse_header(body)?);
                            }
                            INPUT_TERMINAL => {
                                if input.is_some() {
                                    return Err(Uac1Error::Unexpected("input terminal"));
                                }
                                input = Some(parse_input(body)?);
                            }
                            OUTPUT_TERMINAL => {
                                if output.is_some() {
                                    return Err(Uac1Error::Unexpected("output terminal"));
                                }
                                output = Some(parse_output(body)?);
                            }
                            FEATURE_UNIT => {
                                let channels = input
                                    .as_ref()
                                    .ok_or(Uac1Error::Unexpected("feature before input"))?
                                    .channels;
                                if feature.is_some() {
                                    return Err(Uac1Error::Unexpected("feature unit"));
                                }
                                feature = Some(parse_feature(body, channels)?);
                            }
                            _ => {}
                        }
                    }
                }
                TYPE_ENDPOINT => {
                    let endpoint = EndpointDescriptor::parse(&bytes[pos..pos + length])?;
                    if !in_streaming {
                        return Err(Uac1Error::Unexpected("endpoint outside streaming"));
                    }
                    match endpoint.usage_type() {
                        UsageType::Data => {
                            if data.is_some() {
                                return Err(Uac1Error::Unexpected("data endpoint"));
                            }
                            data = Some(endpoint);
                        }
                        UsageType::Feedback => {
                            if feedback.is_some() {
                                return Err(Uac1Error::Unexpected("feedback endpoint"));
                            }
                            feedback = Some(endpoint);
                        }
                        _ => return Err(Uac1Error::BadValue("endpoint usage")),
                    }
                }
                _ => {}
            }
            pos = pos.checked_add(length).ok_or(Uac1Error::Overflow)?;
        }

        let config = Self {
            device,
            config,
            control: control.ok_or(Uac1Error::Missing("control interface"))?,
            header: header.ok_or(Uac1Error::Missing("AC header"))?,
            input: input.ok_or(Uac1Error::Missing("input terminal"))?,
            feature: feature.ok_or(Uac1Error::Missing("feature unit"))?,
            output: output.ok_or(Uac1Error::Missing("output terminal"))?,
            streaming: StreamingInterface {
                interface: streaming.ok_or(Uac1Error::Missing("streaming interface"))?,
                general: general.ok_or(Uac1Error::Missing("AS general"))?,
                format: format.ok_or(Uac1Error::Missing("format type"))?,
                data: data.ok_or(Uac1Error::Missing("data endpoint"))?,
                feedback,
            },
        };
        config.validate()?;
        Ok(config)
    }

    /// Check the entity graph and the endpoint/format invariants.
    ///
    /// # Errors
    ///
    /// Returns [`Uac1Error`] when a reference or a value is inconsistent.
    pub fn validate(&self) -> Result<(), Uac1Error> {
        if self.control.class != AUDIO_CLASS || self.control.subclass != SUBCLASS_AUDIOCONTROL {
            return Err(Uac1Error::BadValue("control interface class"));
        }
        if self.streaming.interface.class != AUDIO_CLASS
            || self.streaming.interface.subclass != SUBCLASS_AUDIOSTREAMING
        {
            return Err(Uac1Error::BadValue("streaming interface class"));
        }
        if self.header.count != 1
            || !self
                .header
                .interface_numbers
                .iter()
                .take(usize::from(self.header.count))
                .any(|number| *number == self.streaming.interface.number)
        {
            return Err(Uac1Error::BadEntity("streaming interface number"));
        }
        if self.input.terminal_type != TERMINAL_USB_STREAMING {
            return Err(Uac1Error::BadValue("input terminal type"));
        }
        if self.output.terminal_type != TERMINAL_SPEAKER {
            return Err(Uac1Error::BadValue("output terminal type"));
        }
        if self.feature.source_id != self.input.id || self.output.source_id != self.feature.id {
            return Err(Uac1Error::BadEntity("terminal/unit chain"));
        }
        if self.streaming.general.terminal_link != self.input.id {
            return Err(Uac1Error::BadEntity("AS terminal link"));
        }
        if self.input.channels == 0 || self.input.channels != self.streaming.format.channels {
            return Err(Uac1Error::BadValue("channel count"));
        }
        if usize::from(self.input.channels).saturating_add(1) > MAX_CONTROLS {
            return Err(Uac1Error::BadValue("channel count"));
        }
        if usize::from(self.feature.count) != usize::from(self.input.channels).saturating_add(1) {
            return Err(Uac1Error::BadValue("feature control count"));
        }
        if self.streaming.format.count == 0 || self.streaming.format.count > MAX_RATES as u8 {
            return Err(Uac1Error::BadValue("sample-rate count"));
        }
        if self.streaming.format.subframe_bytes == 0
            || self.streaming.format.subframe_bytes > 4
            || self.streaming.format.bit_resolution == 0
            || self.streaming.format.bit_resolution
                > self.streaming.format.subframe_bytes.saturating_mul(8)
        {
            return Err(Uac1Error::BadValue("format"));
        }
        let data = self.streaming.data;
        if data.transfer_type() != TransferType::Isochronous
            || data.address.direction != Direction::In
            || data.max_packet_size() == 0
        {
            return Err(Uac1Error::BadValue("data endpoint"));
        }
        if let Some(feedback) = self.streaming.feedback
            && (feedback.transfer_type() != TransferType::Isochronous
                || feedback.usage_type() != UsageType::Feedback
                || feedback.address.direction != Direction::In
                || feedback.max_packet_size() == 0)
        {
            return Err(Uac1Error::BadValue("feedback endpoint"));
        }
        let endpoints = 1usize.saturating_add(usize::from(self.streaming.feedback.is_some()));
        if usize::from(self.streaming.interface.endpoints) != endpoints {
            return Err(Uac1Error::BadValue("streaming endpoint count"));
        }
        if self.config.interfaces < 2 || self.control.number == self.streaming.interface.number {
            return Err(Uac1Error::BadValue("interface numbers"));
        }
        Ok(())
    }

    /// Encode the complete descriptor set into `out`; returns the byte count.
    ///
    /// # Errors
    ///
    /// Returns [`Uac1Error`] when the configuration is inconsistent or `out` is too small.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, Uac1Error> {
        self.validate()?;
        let count = usize::from(self.input.channels).saturating_add(1);
        let feature_len = 6usize.saturating_add(count);
        let ac_total = 9usize
            .saturating_add(12)
            .saturating_add(feature_len)
            .saturating_add(9);
        let mut pos = 0usize;
        put(out, &mut pos, &self.device.encode())?;

        let config_offset = pos;
        put(out, &mut pos, &self.config.encode())?;
        put(out, &mut pos, &self.control.encode())?;
        put(
            out,
            &mut pos,
            &encode_header(
                &self.header,
                self.streaming.interface.number,
                u16::try_from(ac_total).map_err(|_| Uac1Error::Overflow)?,
            ),
        )?;
        put(out, &mut pos, &encode_input(&self.input))?;
        let (feature, feature_len) = encode_feature(&self.feature, self.input.channels);
        put(out, &mut pos, &feature[..feature_len])?;
        put(out, &mut pos, &encode_output(&self.output))?;
        let mut interface = self.streaming.interface;
        interface.endpoints = if self.streaming.feedback.is_some() {
            2
        } else {
            1
        };
        put(out, &mut pos, &interface.encode())?;
        put(out, &mut pos, &encode_as_general(&self.streaming.general))?;
        let (format, format_len) = encode_format(&self.streaming.format);
        put(out, &mut pos, &format[..format_len])?;
        put(out, &mut pos, &self.streaming.data.encode())?;
        if let Some(feedback) = self.streaming.feedback {
            put(out, &mut pos, &feedback.encode())?;
        }

        let total = pos.checked_sub(config_offset).ok_or(Uac1Error::Overflow)?;
        let total = u16::try_from(total).map_err(|_| Uac1Error::Overflow)?;
        let header = config_offset + 2;
        if let Some(bytes) = out.get_mut(header..header + 2) {
            bytes.copy_from_slice(&total.to_le_bytes());
        } else {
            return Err(Uac1Error::Overflow);
        }
        Ok(pos)
    }
}

fn put(out: &mut [u8], pos: &mut usize, bytes: &[u8]) -> Result<(), Uac1Error> {
    let end = pos.checked_add(bytes.len()).ok_or(Uac1Error::Overflow)?;
    if let Some(slot) = out.get_mut(*pos..end) {
        slot.copy_from_slice(bytes);
        *pos = end;
        Ok(())
    } else {
        Err(Uac1Error::Overflow)
    }
}

fn encode_header(header: &AcHeader, interface: u8, total: u16) -> [u8; 9] {
    let version = header.adc_version.to_le_bytes();
    let total = total.to_le_bytes();
    [
        9,
        CS_INTERFACE,
        AC_HEADER,
        version[0],
        version[1],
        total[0],
        total[1],
        1,
        interface,
    ]
}

fn encode_input(input: &InputTerminal) -> [u8; 12] {
    let terminal = input.terminal_type.to_le_bytes();
    let config = input.channel_config.to_le_bytes();
    [
        12,
        CS_INTERFACE,
        INPUT_TERMINAL,
        input.id,
        terminal[0],
        terminal[1],
        0,
        input.channels,
        config[0],
        config[1],
        0,
        0,
    ]
}

fn encode_feature(feature: &FeatureUnit, channels: u8) -> ([u8; 16], usize) {
    let mut bytes = [0u8; 16];
    let count = usize::from(channels).saturating_add(1).min(MAX_CONTROLS);
    bytes[0] = 6u8.saturating_add(count as u8);
    bytes[1] = CS_INTERFACE;
    bytes[2] = FEATURE_UNIT;
    bytes[3] = feature.id;
    bytes[4] = feature.source_id;
    bytes[5] = 1;
    let mut index = 0usize;
    while index < count {
        bytes[6 + index] = feature.controls.get(index).copied().unwrap_or(0);
        index = index.saturating_add(1);
    }
    (bytes, 6 + count)
}

fn encode_output(output: &OutputTerminal) -> [u8; 9] {
    let terminal = output.terminal_type.to_le_bytes();
    [
        9,
        CS_INTERFACE,
        OUTPUT_TERMINAL,
        output.id,
        terminal[0],
        terminal[1],
        0,
        output.source_id,
        0,
    ]
}

fn encode_as_general(general: &AsInterface) -> [u8; 7] {
    let tag = general.format_tag.to_le_bytes();
    [
        7,
        CS_INTERFACE,
        AS_GENERAL,
        general.terminal_link,
        general.delay,
        tag[0],
        tag[1],
    ]
}

fn encode_format(format: &FormatTypeI) -> ([u8; 32], usize) {
    let mut bytes = [0u8; 32];
    let count = usize::from(format.count).min(MAX_RATES);
    let length = 8usize.saturating_add(count.saturating_mul(3));
    bytes[0] = length as u8;
    bytes[1] = CS_INTERFACE;
    bytes[2] = FORMAT_TYPE_I;
    bytes[3] = 1;
    bytes[4] = format.channels;
    bytes[5] = format.subframe_bytes;
    bytes[6] = format.bit_resolution;
    bytes[7] = count as u8;
    for (index, rate) in format.rates.iter().take(count).enumerate() {
        let raw = rate.to_le_bytes();
        let base = 8 + index.saturating_mul(3);
        bytes[base] = raw[0];
        bytes[base + 1] = raw[1];
        bytes[base + 2] = raw[2];
    }
    (bytes, length)
}

fn parse_header(body: &[u8]) -> Result<AcHeader, Uac1Error> {
    if body.len() < 9 {
        return Err(Uac1Error::Truncated("AC header"));
    }
    let count = body[7];
    if body.len() != 8 + usize::from(count) {
        return Err(Uac1Error::BadValue("AC header length"));
    }
    let mut interface_numbers = [0u8; 4];
    for (slot, value) in interface_numbers
        .iter_mut()
        .zip(body.get(8..8 + usize::from(count)).unwrap_or(&[]))
    {
        *slot = *value;
    }
    Ok(AcHeader {
        adc_version: u16::from_le_bytes([body[3], body[4]]),
        total_length: u16::from_le_bytes([body[5], body[6]]),
        interface_numbers,
        count,
    })
}

fn parse_input(body: &[u8]) -> Result<InputTerminal, Uac1Error> {
    if body.len() != 12 {
        return Err(Uac1Error::Truncated("input terminal"));
    }
    Ok(InputTerminal {
        id: body[3],
        terminal_type: u16::from_le_bytes([body[4], body[5]]),
        channels: body[7],
        channel_config: u16::from_le_bytes([body[8], body[9]]),
    })
}

fn parse_output(body: &[u8]) -> Result<OutputTerminal, Uac1Error> {
    if body.len() != 9 {
        return Err(Uac1Error::Truncated("output terminal"));
    }
    Ok(OutputTerminal {
        id: body[3],
        terminal_type: u16::from_le_bytes([body[4], body[5]]),
        source_id: body[7],
    })
}

fn parse_feature(body: &[u8], channels: u8) -> Result<FeatureUnit, Uac1Error> {
    if body.len() < 6 {
        return Err(Uac1Error::Truncated("feature unit"));
    }
    let control_size = usize::from(body[5]);
    let count = usize::from(channels).saturating_add(1);
    if control_size != 1 {
        return Err(Uac1Error::BadValue("feature control size"));
    }
    if body.len() != 6 + count || count > MAX_CONTROLS {
        return Err(Uac1Error::BadValue("feature control count"));
    }
    let mut controls = [0u8; MAX_CONTROLS];
    for (slot, value) in controls
        .iter_mut()
        .zip(body.get(6..6 + count).unwrap_or(&[]))
    {
        *slot = *value;
    }
    Ok(FeatureUnit {
        id: body[3],
        source_id: body[4],
        controls,
        count: count as u8,
    })
}

fn parse_as_general(body: &[u8]) -> Result<AsInterface, Uac1Error> {
    if body.len() != 7 {
        return Err(Uac1Error::Truncated("AS general"));
    }
    Ok(AsInterface {
        terminal_link: body[3],
        delay: body[4],
        format_tag: u16::from_le_bytes([body[5], body[6]]),
    })
}

fn parse_format(body: &[u8]) -> Result<FormatTypeI, Uac1Error> {
    if body.len() < 8 {
        return Err(Uac1Error::Truncated("format type"));
    }
    if body[3] != 1 {
        return Err(Uac1Error::BadValue("format type is not I"));
    }
    let count = usize::from(body[7]);
    if count == 0 || count > MAX_RATES || body.len() != 8 + count * 3 {
        return Err(Uac1Error::BadValue("sample-rate count"));
    }
    let mut rates = [0u32; MAX_RATES];
    for (index, slot) in rates.iter_mut().take(count).enumerate() {
        let base = 8 + index * 3;
        let value = u32::from(body[base])
            | (u32::from(body[base + 1]) << 8)
            | (u32::from(body[base + 2]) << 16);
        *slot = value;
    }
    if rates.iter().take(count).any(|rate| *rate == 0) {
        return Err(Uac1Error::BadValue("zero sample rate"));
    }
    Ok(FormatTypeI {
        channels: body[4],
        subframe_bytes: body[5],
        bit_resolution: body[6],
        rates,
        count: count as u8,
    })
}

#[cfg(test)]
#[path = "uac1_ut.rs"]
mod tests;
