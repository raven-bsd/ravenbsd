// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Layer B: FreeBSD `sig2str`/`str2sig` with BSD numbers, compared to the Linux-numbered Rust
//! implementation BY NAME. BSD numbers below 32 differ (SIGUSR1 is 30 there, 10 here), the BSD
//! real-time base is 65 while ours is 35, and EMT/INFO have no Linux counterpart (excluded and
//! counted as unsupported).
//!
//! Set `RAVEN_ORACLE_DIR` (the xtask diff does) to load `liboracle_signal_freebsd.so`.

#![allow(
    clippy::manual_range_contains,
    clippy::undocumented_unsafe_blocks,
    clippy::collapsible_if,
    clippy::needless_question_mark
)]
#![cfg(feature = "oracle-signal-freebsd")]

use core::ffi::{c_char, c_int};

use raven_libc_oracle::decls::signal_freebsd::*;
use raven_libc_oracle::{diff_case, rng::Rng};
use raven_libc_signal::sig2str::{SIG2STR_MAX, sig2str, str2sig};

const BSD_NSIG: i32 = 32;
const BSD_RTMIN: i32 = 65;
const BSD_RTMAX: i32 = 126;

/// The BSD name table of the shim, written from the ticket/oracle shim (not read from the C).
fn bsd_name(number: i32) -> Option<&'static str> {
    const NAMES: [&str; 31] = [
        "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "EMT", "FPE", "KILL", "BUS", "SEGV", "SYS",
        "PIPE", "ALRM", "TERM", "URG", "STOP", "TSTP", "CONT", "CHLD", "TTIN", "TTOU", "IO",
        "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "INFO", "USR1", "USR2",
    ];
    if number >= 1 && number <= 31 {
        NAMES.get((number - 1) as usize).copied()
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    Number(i32),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Sig2StrCase {
    kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Obs {
    Name(String),
    Number(i32),
    Unsupported(String),
    Error,
}

impl Sig2StrCase {
    fn random(rng: &mut Rng) -> Self {
        if rng.below(2) == 0 {
            Self {
                kind: Kind::Number((rng.below(140) as i32) - 8),
            }
        } else {
            let texts: Vec<String> = (1..=31)
                .filter_map(bsd_name)
                .flat_map(|name| {
                    let prefixed = format!("SIG{name}");
                    [name.to_string(), prefixed.clone(), prefixed.to_lowercase()]
                })
                .chain((0..=29).flat_map(|offset| {
                    [
                        format!("RTMIN+{offset}"),
                        format!("RTMAX-{offset}"),
                        format!("SIGRTMIN+{offset}"),
                    ]
                }))
                .collect();
            let index = rng.below(texts.len() as u64) as usize;
            Self {
                kind: Kind::Text(texts[index].clone()),
            }
        }
    }

    fn exhaustive() -> Box<dyn Iterator<Item = Self>> {
        let mut cases = Vec::new();
        for number in -2..=130 {
            cases.push(Self {
                kind: Kind::Number(number),
            });
        }
        for text in [
            "HUP", "SIGINT", "sigterm", "RTMIN", "RTMIN+1", "RTMIN+29", "RTMAX", "RTMAX-1",
            "RTMAX-29", "RTMIN+0", "RTMAX+1", "0", "1", "32", "64", "65", "127", "BOGUS",
        ] {
            cases.push(Self {
                kind: Kind::Text(text.to_string()),
            });
        }
        Box::new(cases.into_iter())
    }

    fn oracle_sig2str(number: i32) -> Option<String> {
        let mut buffer = [0i8; SIG2STR_MAX];
        let ret = unsafe { oracle_sig2str(number as c_int, buffer.as_mut_ptr()) };
        if ret != 0 {
            return None;
        }
        let bytes: Vec<u8> = buffer
            .iter()
            .map(|byte| *byte as u8)
            .take_while(|byte| *byte != 0)
            .collect();
        Some(String::from_utf8(bytes).ok()?)
    }

    fn oracle_str2sig(text: &str) -> Option<i32> {
        let mut bytes: Vec<u8> = text.as_bytes().to_vec();
        bytes.push(0);
        let mut number: c_int = 0;
        let ret = unsafe { oracle_str2sig(bytes.as_ptr().cast::<c_char>(), &mut number) };
        if ret == 0 { Some(number) } else { None }
    }

    fn oracle_number(number: i32) -> Obs {
        match Self::oracle_sig2str(number) {
            None => {
                // The donor rejects <=0 and >SIGRTMAX; our range is 1..=64 and 32..34 are
                // internal.
                if number >= 1 && number <= 64 && !(32..=34).contains(&number) {
                    // A name that exists in BSD but not in Linux is unsupported.
                    Obs::Unsupported(format!("oracle rejected {number}"))
                } else {
                    Obs::Error
                }
            }
            Some(name) => {
                if let Ok(decimal) = name.parse::<i32>() {
                    // The donor emits the decimal for BSD 32..64; ours must parse it.
                    return match str2sig(name.as_bytes()) {
                        Ok(got) if got == decimal => Obs::Number(got),
                        _ => Obs::Error,
                    };
                }
                if name.starts_with("RTMIN") || name.starts_with("RTMAX") {
                    let linux = if name == "RTMIN" {
                        35
                    } else if name == "RTMAX" {
                        64
                    } else if let Some(offset) = name.strip_prefix("RTMIN+") {
                        35 + offset.parse::<i32>().unwrap_or(999)
                    } else if let Some(offset) = name.strip_prefix("RTMAX-") {
                        64 - offset.parse::<i32>().unwrap_or(999)
                    } else {
                        return Obs::Error;
                    };
                    if !(35..=64).contains(&linux) {
                        return Obs::Error;
                    }
                    // The two `sig2str` rules split the real-time range at different midpoint
                    // numbers, so `RTMIN+15` (BSD, midpoint 95) and `RTMAX-14` (Linux,
                    // midpoint 49) are the same offset. Compare the mapped offset number.
                    return Obs::Number(linux);
                }
                match raven_libc_signal::number_of(&name) {
                    Some(linux) => {
                        let mut buffer = [0u8; SIG2STR_MAX];
                        match sig2str(linux, &mut buffer) {
                            Ok(length) if &buffer[..length] == name.as_bytes() => Obs::Name(name),
                            _ => Obs::Error,
                        }
                    }
                    None => Obs::Unsupported(name),
                }
            }
        }
    }

    fn oracle_text(text: &str) -> Obs {
        if let Some(bsd) = Self::oracle_str2sig(text) {
            if bsd >= BSD_RTMIN && bsd <= BSD_RTMAX {
                let Some(linux) = bsd_rt_to_linux(bsd) else {
                    return Obs::Error;
                };
                return match str2sig(text.as_bytes()) {
                    Ok(got) if got == linux => Obs::Number(linux),
                    _ => Obs::Error,
                };
            }
            if bsd >= BSD_NSIG && bsd <= 64 {
                return match str2sig(text.as_bytes()) {
                    Ok(got) if got == bsd => Obs::Number(bsd),
                    _ => Obs::Error,
                };
            }
            if bsd >= 1 && bsd < BSD_NSIG {
                if let Some(name) = Self::oracle_sig2str(bsd) {
                    if let Some(linux) = raven_libc_signal::number_of(&name) {
                        return match str2sig(text.as_bytes()) {
                            Ok(got) if got == linux => Obs::Number(linux),
                            _ => Obs::Error,
                        };
                    }
                    return Obs::Error;
                }
            }
        }
        match str2sig(text.as_bytes()) {
            Err(_) => Obs::Error,
            Ok(_) => Obs::Error,
        }
    }

    fn run_oracle(&self) -> Obs {
        match &self.kind {
            Kind::Number(number) => Self::oracle_number(*number),
            Kind::Text(text) => Self::oracle_text(text),
        }
    }

    fn run_subject(&self) -> Obs {
        match &self.kind {
            Kind::Number(number) => {
                let number = *number;
                if number >= BSD_RTMIN && number <= BSD_RTMAX {
                    return bsd_rt_to_linux(number).map_or(Obs::Error, Obs::Number);
                }
                if (32..=64).contains(&number) {
                    return match str2sig(number.to_string().as_bytes()) {
                        Ok(got) if got == number => Obs::Number(got),
                        _ => Obs::Error,
                    };
                }
                if let Some(name) = bsd_name(number) {
                    return match raven_libc_signal::number_of(name) {
                        Some(linux) => {
                            let mut buffer = [0u8; SIG2STR_MAX];
                            match sig2str(linux, &mut buffer) {
                                Ok(length) if &buffer[..length] == name.as_bytes() => {
                                    Obs::Name(name.to_string())
                                }
                                _ => Obs::Unsupported(name.to_string()),
                            }
                        }
                        None => Obs::Unsupported(name.to_string()),
                    };
                }
                let mut buffer = [0u8; SIG2STR_MAX];
                match sig2str(number, &mut buffer) {
                    Ok(length) => {
                        Obs::Name(String::from_utf8(buffer[..length].to_vec()).unwrap_or_default())
                    }
                    Err(_) => Obs::Error,
                }
            }
            Kind::Text(text) => match str2sig(text.as_bytes()) {
                Ok(number) => Obs::Number(number),
                Err(_) => Obs::Error,
            },
        }
    }
}

fn bsd_rt_to_linux(number: i32) -> Option<i32> {
    let value = if number == BSD_RTMIN {
        35
    } else if number == BSD_RTMAX {
        64
    } else if number <= (BSD_RTMIN + BSD_RTMAX) / 2 {
        35 + (number - BSD_RTMIN)
    } else {
        64 - (BSD_RTMAX - number)
    };
    if (35..=64).contains(&value) {
        Some(value)
    } else {
        None
    }
}

impl<'de> serde::Deserialize<'de> for Sig2StrCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            #[serde(default)]
            number: Option<i32>,
            #[serde(default)]
            text: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        match (raw.number, raw.text) {
            (Some(number), None) => Ok(Self {
                kind: Kind::Number(number),
            }),
            (None, Some(text)) => Ok(Self {
                kind: Kind::Text(text),
            }),
            _ => Err(serde::de::Error::custom("give exactly one of number/text")),
        }
    }
}

impl raven_libc_oracle::diff::Case for Sig2StrCase {
    type Obs = Obs;

    fn shrink(&self) -> Vec<Self> {
        match &self.kind {
            Kind::Number(number) if *number > -2 => vec![Self {
                kind: Kind::Number(number - 1),
            }],
            _ => Vec::new(),
        }
    }

    fn to_toml(&self) -> String {
        match &self.kind {
            Kind::Number(number) => format!("number = {number}"),
            Kind::Text(text) => format!("text = {text:?}"),
        }
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("{obs:?}")
    }
}

diff_case! {
    name: sig2str_by_name,
    case: Sig2StrCase,
    fixed: "signal/sig2str/basic.toml",
    random: Sig2StrCase::random,
    exhaustive: (small, Sig2StrCase::exhaustive),
    oracle: |c| c.run_oracle(),
    subject: |c| c.run_subject(),
}

/// Decimal strings are interpreted in the caller's numbering, so they are not compared to the
/// BSD donor by name; the POSIX.1-2024 rule is a Rust-side fixed vector.
#[test]
fn decimal_strings_are_linux_numbered() {
    for number in 1..=64 {
        assert_eq!(str2sig(number.to_string().as_bytes()), Ok(number));
    }
    assert!(str2sig(b"0").is_err());
    assert!(str2sig(b"65").is_err());
}
