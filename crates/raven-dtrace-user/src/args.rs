// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line parsing for the `dtrace` utility.

/// Parsed command line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `-l`: list probes.
    pub list: bool,
    /// `-e <id>`: enable a probe.
    pub enable: Option<u32>,
    /// `-d <id>`: disable a probe.
    pub disable: Option<u32>,
    /// `-r <count>`: read up to `count` records.
    pub read: Option<usize>,
    /// `-s`: print per-CPU statistics.
    pub stats: bool,
    /// `-c`: clear the rings.
    pub clear: bool,
    /// `-u`: list the USDT marker sites.
    pub usdt: bool,
    /// `-h`: print usage.
    pub help: bool,
}

impl Options {
    /// True when no action was requested; the utility then defaults to `-l`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.list
            && self.enable.is_none()
            && self.disable.is_none()
            && self.read.is_none()
            && !self.stats
            && !self.clear
            && !self.usdt
            && !self.help
    }
}

/// Why a command line was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgError {
    /// An unknown option.
    Unknown,
    /// An option needs a value.
    MissingValue,
    /// A value is not a number.
    InvalidValue,
}

/// Parse the arguments after the program name.
///
/// # Errors
///
/// Returns [`ArgError`] for an unknown option, a missing value or a non-numeric value.
pub fn parse_args<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, ArgError> {
    let mut options = Options::default();
    let mut iterator = args.into_iter();
    while let Some(argument) = iterator.next() {
        match argument {
            "-l" | "--list" => options.list = true,
            "-s" | "--stats" => options.stats = true,
            "-c" | "--clear" => options.clear = true,
            "-u" | "--usdt" => options.usdt = true,
            "-h" | "--help" => options.help = true,
            "-e" | "--enable" => {
                options.enable = Some(parse_u32(iterator.next().ok_or(ArgError::MissingValue)?)?);
            }
            "-d" | "--disable" => {
                options.disable = Some(parse_u32(iterator.next().ok_or(ArgError::MissingValue)?)?);
            }
            "-r" | "--read" => {
                let value = iterator.next().ok_or(ArgError::MissingValue)?;
                options.read = Some(value.parse::<usize>().map_err(|_| ArgError::InvalidValue)?);
            }
            _ => return Err(ArgError::Unknown),
        }
    }
    Ok(options)
}

fn parse_u32(value: &str) -> Result<u32, ArgError> {
    value.parse::<u32>().map_err(|_| ArgError::InvalidValue)
}

#[cfg(test)]
#[path = "args_ut.rs"]
mod tests;
