// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Layer A: the eight musl sigset operations against the Rust model.
//!
//! Every operation x every signal 0..70 x sets drawn from empty, full, singleton and random
//! words. The default harness budget (20 seeds x 10,000) gives 200,000 random cases plus the
//! exhaustive small domain and the fixed corpus.

#![cfg(feature = "oracle-signal-musl")]
#![allow(clippy::undocumented_unsafe_blocks)]

use core::ffi::c_int;

use raven_libc_oracle::decls::signal_musl::*;
use raven_libc_oracle::{diff_case, rng::Rng};
use raven_libc_signal::SigSet;

const EMPTY: [u64; 16] = [0; 16];
const FULL: [u64; 16] = {
    let mut words = [0u64; 16];
    words[0] = raven_libc_signal::APP_MASK;
    words
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct SigSetCase {
    op: u8,
    sig: i32,
    set: [u64; 16],
    rhs: [u64; 16],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SigSetObs {
    ret: i32,
    errno: i32,
    set: [u64; 16],
    flag: bool,
}

impl SigSetCase {
    fn random(rng: &mut Rng) -> Self {
        let op = (rng.below(8)) as u8;
        let sig = (rng.below(72) as i32) - 1;
        let set = random_set(rng);
        let rhs = random_set(rng);
        Self { op, sig, set, rhs }
    }

    fn exhaustive() -> Box<dyn Iterator<Item = Self>> {
        let mut cases = Vec::new();
        for op in 0..8u8 {
            for sig in -1..=70 {
                for set in [EMPTY, FULL] {
                    cases.push(Self {
                        op,
                        sig,
                        set,
                        rhs: EMPTY,
                    });
                }
            }
        }
        Box::new(cases.into_iter())
    }

    fn run_oracle(&self) -> SigSetObs {
        let mut left = self.set;
        let mut right = self.rhs;
        let mut destination = self.set;
        raven_libc_oracle::call::errno_set(0);
        let pointer = left.as_mut_ptr().cast::<u8>();
        let ret = match self.op {
            0 => unsafe { oracle_sigaddset(pointer, self.sig as c_int) },
            1 => unsafe { oracle_sigdelset(pointer, self.sig as c_int) },
            2 => {
                let member = unsafe { oracle_sigismember(pointer.cast_const(), self.sig as c_int) };
                return SigSetObs {
                    ret: member,
                    errno: raven_libc_oracle::call::errno_get(),
                    set: left,
                    flag: member != 0,
                };
            }
            3 => unsafe { oracle_sigfillset(pointer) },
            4 => unsafe { oracle_sigemptyset(pointer) },
            5 => {
                let empty = unsafe { oracle_sigisemptyset(pointer.cast_const()) };
                return SigSetObs {
                    ret: empty,
                    errno: raven_libc_oracle::call::errno_get(),
                    set: left,
                    flag: empty != 0,
                };
            }
            6 => unsafe {
                oracle_sigandset(
                    destination.as_mut_ptr().cast::<u8>(),
                    pointer.cast_const(),
                    right.as_mut_ptr().cast::<u8>().cast_const(),
                )
            },
            _ => unsafe {
                oracle_sigorset(
                    destination.as_mut_ptr().cast::<u8>(),
                    pointer.cast_const(),
                    right.as_mut_ptr().cast::<u8>().cast_const(),
                )
            },
        };
        let set = if self.op >= 6 { destination } else { left };
        SigSetObs {
            ret,
            errno: raven_libc_oracle::call::errno_get(),
            set,
            flag: false,
        }
    }

    fn run_subject(&self) -> SigSetObs {
        let mut set = SigSet(self.set);
        let rhs = SigSet(self.rhs);
        match self.op {
            0 => match set.add(self.sig) {
                Ok(()) => SigSetObs {
                    ret: 0,
                    errno: 0,
                    set: set.0,
                    flag: false,
                },
                Err(error) => SigSetObs {
                    ret: -1,
                    errno: i32::from(error.0),
                    set: set.0,
                    flag: false,
                },
            },
            1 => match set.del(self.sig) {
                Ok(()) => SigSetObs {
                    ret: 0,
                    errno: 0,
                    set: set.0,
                    flag: false,
                },
                Err(error) => SigSetObs {
                    ret: -1,
                    errno: i32::from(error.0),
                    set: set.0,
                    flag: false,
                },
            },
            2 => match set.is_member(self.sig) {
                Ok(member) => SigSetObs {
                    ret: i32::from(member),
                    errno: 0,
                    set: set.0,
                    flag: member,
                },
                Err(error) => SigSetObs {
                    ret: 0,
                    errno: i32::from(error.0),
                    set: set.0,
                    flag: false,
                },
            },
            3 => {
                set.fill();
                SigSetObs {
                    ret: 0,
                    errno: 0,
                    set: set.0,
                    flag: false,
                }
            }
            4 => {
                set = SigSet::empty();
                SigSetObs {
                    ret: 0,
                    errno: 0,
                    set: set.0,
                    flag: false,
                }
            }
            5 => {
                let empty = set.is_empty();
                SigSetObs {
                    ret: i32::from(empty),
                    errno: 0,
                    set: set.0,
                    flag: empty,
                }
            }
            6 => SigSetObs {
                ret: 0,
                errno: 0,
                set: set.and(&rhs).0,
                flag: false,
            },
            _ => SigSetObs {
                ret: 0,
                errno: 0,
                set: set.or(&rhs).0,
                flag: false,
            },
        }
    }
}

fn random_set(rng: &mut Rng) -> [u64; 16] {
    let mut words = [0u64; 16];
    match rng.below(4) {
        0 => {}
        1 => words = FULL,
        2 => {
            let sig = (rng.below(64) + 1) as u32;
            words[0] = 1u64 << (sig - 1);
        }
        _ => words[0] = rng.next_u64() & raven_libc_signal::APP_MASK,
    }
    words
}

impl<'de> serde::Deserialize<'de> for SigSetCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            op: u8,
            sig: i32,
            #[serde(default)]
            set: Option<Vec<u64>>,
            #[serde(default)]
            rhs: Option<Vec<u64>>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let mut set = EMPTY;
        let mut rhs = EMPTY;
        for (index, value) in raw.set.unwrap_or_default().iter().enumerate().take(16) {
            set[index] = *value;
        }
        for (index, value) in raw.rhs.unwrap_or_default().iter().enumerate().take(16) {
            rhs[index] = *value;
        }
        Ok(Self {
            op: raw.op,
            sig: raw.sig,
            set,
            rhs,
        })
    }
}

impl raven_libc_oracle::diff::Case for SigSetCase {
    type Obs = SigSetObs;

    fn shrink(&self) -> Vec<Self> {
        let mut out = Vec::new();
        if self.sig > -1 {
            out.push(Self {
                sig: self.sig - 1,
                ..self.clone()
            });
        }
        if self.set[0] != 0 {
            out.push(Self {
                set: [
                    self.set[0] & (self.set[0] - 1),
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ],
                ..self.clone()
            });
        }
        out
    }

    fn to_toml(&self) -> String {
        format!(
            "op = {}\nsig = {}\nset = {:?}\nrhs = {:?}",
            self.op, self.sig, self.set, self.rhs
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "ret = {}\nerrno = {}\nflag = {}\nset = {:?}",
            obs.ret, obs.errno, obs.flag, obs.set
        )
    }
}

diff_case! {
    name: sigset_operations,
    case: SigSetCase,
    fixed: "signal/sigset/basic.toml",
    random: SigSetCase::random,
    exhaustive: (small, SigSetCase::exhaustive),
    oracle: |c| c.run_oracle(),
    subject: |c| c.run_subject(),
}
