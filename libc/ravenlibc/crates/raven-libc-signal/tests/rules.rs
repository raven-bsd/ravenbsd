// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-signal rules: the Linux name table, the default-action table and `sig2str` for every
//! number. One test per signal keeps a wrong row visible by name.

#![allow(
    unused_assignments,
    clippy::undocumented_unsafe_blocks,
    clippy::explicit_auto_deref,
    clippy::never_loop,
    clippy::identity_op,
    clippy::unnecessary_cast
)]
use raven_libc_abi::generated::signal;
use raven_libc_signal::defaults::{DefaultAction, default_action};
use raven_libc_signal::names::{name_of, number_of};
use raven_libc_signal::sig2str::{SIG2STR_MAX, sig2str, str2sig};

#[test]
fn name_01() {
    assert_eq!(name_of(1), Some("HUP"));
    assert_eq!(number_of("HUP"), Some(1));
    assert_eq!(number_of("SIGHUP"), Some(1));
}
#[test]
fn name_02() {
    assert_eq!(name_of(2), Some("INT"));
    assert_eq!(number_of("INT"), Some(2));
    assert_eq!(number_of("SIGINT"), Some(2));
}
#[test]
fn name_03() {
    assert_eq!(name_of(3), Some("QUIT"));
    assert_eq!(number_of("QUIT"), Some(3));
    assert_eq!(number_of("SIGQUIT"), Some(3));
}
#[test]
fn name_04() {
    assert_eq!(name_of(4), Some("ILL"));
    assert_eq!(number_of("ILL"), Some(4));
    assert_eq!(number_of("SIGILL"), Some(4));
}
#[test]
fn name_05() {
    assert_eq!(name_of(5), Some("TRAP"));
    assert_eq!(number_of("TRAP"), Some(5));
    assert_eq!(number_of("SIGTRAP"), Some(5));
}
#[test]
fn name_06() {
    assert_eq!(name_of(6), Some("ABRT"));
    assert_eq!(number_of("ABRT"), Some(6));
    assert_eq!(number_of("SIGABRT"), Some(6));
}
#[test]
fn name_07() {
    assert_eq!(name_of(7), Some("BUS"));
    assert_eq!(number_of("BUS"), Some(7));
    assert_eq!(number_of("SIGBUS"), Some(7));
}
#[test]
fn name_08() {
    assert_eq!(name_of(8), Some("FPE"));
    assert_eq!(number_of("FPE"), Some(8));
    assert_eq!(number_of("SIGFPE"), Some(8));
}
#[test]
fn name_09() {
    assert_eq!(name_of(9), Some("KILL"));
    assert_eq!(number_of("KILL"), Some(9));
    assert_eq!(number_of("SIGKILL"), Some(9));
}
#[test]
fn name_10() {
    assert_eq!(name_of(10), Some("USR1"));
    assert_eq!(number_of("USR1"), Some(10));
    assert_eq!(number_of("SIGUSR1"), Some(10));
}
#[test]
fn name_11() {
    assert_eq!(name_of(11), Some("SEGV"));
    assert_eq!(number_of("SEGV"), Some(11));
    assert_eq!(number_of("SIGSEGV"), Some(11));
}
#[test]
fn name_12() {
    assert_eq!(name_of(12), Some("USR2"));
    assert_eq!(number_of("USR2"), Some(12));
    assert_eq!(number_of("SIGUSR2"), Some(12));
}
#[test]
fn name_13() {
    assert_eq!(name_of(13), Some("PIPE"));
    assert_eq!(number_of("PIPE"), Some(13));
    assert_eq!(number_of("SIGPIPE"), Some(13));
}
#[test]
fn name_14() {
    assert_eq!(name_of(14), Some("ALRM"));
    assert_eq!(number_of("ALRM"), Some(14));
    assert_eq!(number_of("SIGALRM"), Some(14));
}
#[test]
fn name_15() {
    assert_eq!(name_of(15), Some("TERM"));
    assert_eq!(number_of("TERM"), Some(15));
    assert_eq!(number_of("SIGTERM"), Some(15));
}
#[test]
fn name_16() {
    assert_eq!(name_of(16), Some("STKFLT"));
    assert_eq!(number_of("STKFLT"), Some(16));
    assert_eq!(number_of("SIGSTKFLT"), Some(16));
}
#[test]
fn name_17() {
    assert_eq!(name_of(17), Some("CHLD"));
    assert_eq!(number_of("CHLD"), Some(17));
    assert_eq!(number_of("SIGCHLD"), Some(17));
}
#[test]
fn name_18() {
    assert_eq!(name_of(18), Some("CONT"));
    assert_eq!(number_of("CONT"), Some(18));
    assert_eq!(number_of("SIGCONT"), Some(18));
}
#[test]
fn name_19() {
    assert_eq!(name_of(19), Some("STOP"));
    assert_eq!(number_of("STOP"), Some(19));
    assert_eq!(number_of("SIGSTOP"), Some(19));
}
#[test]
fn name_20() {
    assert_eq!(name_of(20), Some("TSTP"));
    assert_eq!(number_of("TSTP"), Some(20));
    assert_eq!(number_of("SIGTSTP"), Some(20));
}
#[test]
fn name_21() {
    assert_eq!(name_of(21), Some("TTIN"));
    assert_eq!(number_of("TTIN"), Some(21));
    assert_eq!(number_of("SIGTTIN"), Some(21));
}
#[test]
fn name_22() {
    assert_eq!(name_of(22), Some("TTOU"));
    assert_eq!(number_of("TTOU"), Some(22));
    assert_eq!(number_of("SIGTTOU"), Some(22));
}
#[test]
fn name_23() {
    assert_eq!(name_of(23), Some("URG"));
    assert_eq!(number_of("URG"), Some(23));
    assert_eq!(number_of("SIGURG"), Some(23));
}
#[test]
fn name_24() {
    assert_eq!(name_of(24), Some("XCPU"));
    assert_eq!(number_of("XCPU"), Some(24));
    assert_eq!(number_of("SIGXCPU"), Some(24));
}
#[test]
fn name_25() {
    assert_eq!(name_of(25), Some("XFSZ"));
    assert_eq!(number_of("XFSZ"), Some(25));
    assert_eq!(number_of("SIGXFSZ"), Some(25));
}
#[test]
fn name_26() {
    assert_eq!(name_of(26), Some("VTALRM"));
    assert_eq!(number_of("VTALRM"), Some(26));
    assert_eq!(number_of("SIGVTALRM"), Some(26));
}
#[test]
fn name_27() {
    assert_eq!(name_of(27), Some("PROF"));
    assert_eq!(number_of("PROF"), Some(27));
    assert_eq!(number_of("SIGPROF"), Some(27));
}
#[test]
fn name_28() {
    assert_eq!(name_of(28), Some("WINCH"));
    assert_eq!(number_of("WINCH"), Some(28));
    assert_eq!(number_of("SIGWINCH"), Some(28));
}
#[test]
fn name_29() {
    assert_eq!(name_of(29), Some("IO"));
    assert_eq!(number_of("IO"), Some(29));
    assert_eq!(number_of("SIGIO"), Some(29));
}
#[test]
fn name_30() {
    assert_eq!(name_of(30), Some("PWR"));
    assert_eq!(number_of("PWR"), Some(30));
    assert_eq!(number_of("SIGPWR"), Some(30));
}
#[test]
fn name_31() {
    assert_eq!(name_of(31), Some("SYS"));
    assert_eq!(number_of("SYS"), Some(31));
    assert_eq!(number_of("SIGSYS"), Some(31));
}
#[test]
fn default_01() {
    assert_eq!(default_action(1), Some(DefaultAction::Terminate));
}
#[test]
fn default_02() {
    assert_eq!(default_action(2), Some(DefaultAction::Terminate));
}
#[test]
fn default_03() {
    assert_eq!(default_action(3), Some(DefaultAction::Core));
}
#[test]
fn default_04() {
    assert_eq!(default_action(4), Some(DefaultAction::Core));
}
#[test]
fn default_05() {
    assert_eq!(default_action(5), Some(DefaultAction::Core));
}
#[test]
fn default_06() {
    assert_eq!(default_action(6), Some(DefaultAction::Core));
}
#[test]
fn default_07() {
    assert_eq!(default_action(7), Some(DefaultAction::Core));
}
#[test]
fn default_08() {
    assert_eq!(default_action(8), Some(DefaultAction::Core));
}
#[test]
fn default_09() {
    assert_eq!(default_action(9), Some(DefaultAction::Terminate));
}
#[test]
fn default_10() {
    assert_eq!(default_action(10), Some(DefaultAction::Terminate));
}
#[test]
fn default_11() {
    assert_eq!(default_action(11), Some(DefaultAction::Core));
}
#[test]
fn default_12() {
    assert_eq!(default_action(12), Some(DefaultAction::Terminate));
}
#[test]
fn default_13() {
    assert_eq!(default_action(13), Some(DefaultAction::Terminate));
}
#[test]
fn default_14() {
    assert_eq!(default_action(14), Some(DefaultAction::Terminate));
}
#[test]
fn default_15() {
    assert_eq!(default_action(15), Some(DefaultAction::Terminate));
}
#[test]
fn default_16() {
    assert_eq!(default_action(16), Some(DefaultAction::Terminate));
}
#[test]
fn default_17() {
    assert_eq!(default_action(17), Some(DefaultAction::Ignore));
}
#[test]
fn default_18() {
    assert_eq!(default_action(18), Some(DefaultAction::Continue));
}
#[test]
fn default_19() {
    assert_eq!(default_action(19), Some(DefaultAction::Stop));
}
#[test]
fn default_20() {
    assert_eq!(default_action(20), Some(DefaultAction::Stop));
}
#[test]
fn default_21() {
    assert_eq!(default_action(21), Some(DefaultAction::Stop));
}
#[test]
fn default_22() {
    assert_eq!(default_action(22), Some(DefaultAction::Stop));
}
#[test]
fn default_23() {
    assert_eq!(default_action(23), Some(DefaultAction::Ignore));
}
#[test]
fn default_24() {
    assert_eq!(default_action(24), Some(DefaultAction::Core));
}
#[test]
fn default_25() {
    assert_eq!(default_action(25), Some(DefaultAction::Core));
}
#[test]
fn default_26() {
    assert_eq!(default_action(26), Some(DefaultAction::Terminate));
}
#[test]
fn default_27() {
    assert_eq!(default_action(27), Some(DefaultAction::Terminate));
}
#[test]
fn default_28() {
    assert_eq!(default_action(28), Some(DefaultAction::Ignore));
}
#[test]
fn default_29() {
    assert_eq!(default_action(29), Some(DefaultAction::Terminate));
}
#[test]
fn default_30() {
    assert_eq!(default_action(30), Some(DefaultAction::Terminate));
}
#[test]
fn default_31() {
    assert_eq!(default_action(31), Some(DefaultAction::Core));
}
#[test]
fn sig2str_01() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(1, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"HUP");
    assert_eq!(str2sig(b"HUP"), Ok(1));
}
#[test]
fn sig2str_02() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(2, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"INT");
    assert_eq!(str2sig(b"INT"), Ok(2));
}
#[test]
fn sig2str_03() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(3, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"QUIT");
    assert_eq!(str2sig(b"QUIT"), Ok(3));
}
#[test]
fn sig2str_04() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(4, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"ILL");
    assert_eq!(str2sig(b"ILL"), Ok(4));
}
#[test]
fn sig2str_05() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(5, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"TRAP");
    assert_eq!(str2sig(b"TRAP"), Ok(5));
}
#[test]
fn sig2str_06() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(6, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"ABRT");
    assert_eq!(str2sig(b"ABRT"), Ok(6));
}
#[test]
fn sig2str_07() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(7, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"BUS");
    assert_eq!(str2sig(b"BUS"), Ok(7));
}
#[test]
fn sig2str_08() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(8, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"FPE");
    assert_eq!(str2sig(b"FPE"), Ok(8));
}
#[test]
fn sig2str_09() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(9, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"KILL");
    assert_eq!(str2sig(b"KILL"), Ok(9));
}
#[test]
fn sig2str_10() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(10, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"USR1");
    assert_eq!(str2sig(b"USR1"), Ok(10));
}
#[test]
fn sig2str_11() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(11, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"SEGV");
    assert_eq!(str2sig(b"SEGV"), Ok(11));
}
#[test]
fn sig2str_12() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(12, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"USR2");
    assert_eq!(str2sig(b"USR2"), Ok(12));
}
#[test]
fn sig2str_13() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(13, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"PIPE");
    assert_eq!(str2sig(b"PIPE"), Ok(13));
}
#[test]
fn sig2str_14() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(14, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"ALRM");
    assert_eq!(str2sig(b"ALRM"), Ok(14));
}
#[test]
fn sig2str_15() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(15, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"TERM");
    assert_eq!(str2sig(b"TERM"), Ok(15));
}
#[test]
fn sig2str_16() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(16, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"STKFLT");
    assert_eq!(str2sig(b"STKFLT"), Ok(16));
}
#[test]
fn sig2str_17() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(17, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"CHLD");
    assert_eq!(str2sig(b"CHLD"), Ok(17));
}
#[test]
fn sig2str_18() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(18, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"CONT");
    assert_eq!(str2sig(b"CONT"), Ok(18));
}
#[test]
fn sig2str_19() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(19, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"STOP");
    assert_eq!(str2sig(b"STOP"), Ok(19));
}
#[test]
fn sig2str_20() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(20, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"TSTP");
    assert_eq!(str2sig(b"TSTP"), Ok(20));
}
#[test]
fn sig2str_21() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(21, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"TTIN");
    assert_eq!(str2sig(b"TTIN"), Ok(21));
}
#[test]
fn sig2str_22() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(22, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"TTOU");
    assert_eq!(str2sig(b"TTOU"), Ok(22));
}
#[test]
fn sig2str_23() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(23, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"URG");
    assert_eq!(str2sig(b"URG"), Ok(23));
}
#[test]
fn sig2str_24() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(24, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"XCPU");
    assert_eq!(str2sig(b"XCPU"), Ok(24));
}
#[test]
fn sig2str_25() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(25, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"XFSZ");
    assert_eq!(str2sig(b"XFSZ"), Ok(25));
}
#[test]
fn sig2str_26() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(26, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"VTALRM");
    assert_eq!(str2sig(b"VTALRM"), Ok(26));
}
#[test]
fn sig2str_27() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(27, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"PROF");
    assert_eq!(str2sig(b"PROF"), Ok(27));
}
#[test]
fn sig2str_28() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(28, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"WINCH");
    assert_eq!(str2sig(b"WINCH"), Ok(28));
}
#[test]
fn sig2str_29() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(29, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"IO");
    assert_eq!(str2sig(b"IO"), Ok(29));
}
#[test]
fn sig2str_30() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(30, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"PWR");
    assert_eq!(str2sig(b"PWR"), Ok(30));
}
#[test]
fn sig2str_31() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(31, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"SYS");
    assert_eq!(str2sig(b"SYS"), Ok(31));
}
#[test]
fn sig2str_32() {
    let mut buffer = [0u8; SIG2STR_MAX];
    assert!(sig2str(32, &mut buffer).is_err());
}
#[test]
fn sig2str_33() {
    let mut buffer = [0u8; SIG2STR_MAX];
    assert!(sig2str(33, &mut buffer).is_err());
}
#[test]
fn sig2str_34() {
    let mut buffer = [0u8; SIG2STR_MAX];
    assert!(sig2str(34, &mut buffer).is_err());
}
#[test]
fn sig2str_35() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(35, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN");
    assert_eq!(str2sig(b"RTMIN"), Ok(35));
}
#[test]
fn sig2str_36() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(36, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+1");
    assert_eq!(str2sig(b"RTMIN+1"), Ok(36));
}
#[test]
fn sig2str_37() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(37, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+2");
    assert_eq!(str2sig(b"RTMIN+2"), Ok(37));
}
#[test]
fn sig2str_38() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(38, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+3");
    assert_eq!(str2sig(b"RTMIN+3"), Ok(38));
}
#[test]
fn sig2str_39() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(39, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+4");
    assert_eq!(str2sig(b"RTMIN+4"), Ok(39));
}
#[test]
fn sig2str_40() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(40, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+5");
    assert_eq!(str2sig(b"RTMIN+5"), Ok(40));
}
#[test]
fn sig2str_41() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(41, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+6");
    assert_eq!(str2sig(b"RTMIN+6"), Ok(41));
}
#[test]
fn sig2str_42() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(42, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+7");
    assert_eq!(str2sig(b"RTMIN+7"), Ok(42));
}
#[test]
fn sig2str_43() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(43, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+8");
    assert_eq!(str2sig(b"RTMIN+8"), Ok(43));
}
#[test]
fn sig2str_44() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(44, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+9");
    assert_eq!(str2sig(b"RTMIN+9"), Ok(44));
}
#[test]
fn sig2str_45() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(45, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+10");
    assert_eq!(str2sig(b"RTMIN+10"), Ok(45));
}
#[test]
fn sig2str_46() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(46, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+11");
    assert_eq!(str2sig(b"RTMIN+11"), Ok(46));
}
#[test]
fn sig2str_47() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(47, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+12");
    assert_eq!(str2sig(b"RTMIN+12"), Ok(47));
}
#[test]
fn sig2str_48() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(48, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+13");
    assert_eq!(str2sig(b"RTMIN+13"), Ok(48));
}
#[test]
fn sig2str_49() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(49, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMIN+14");
    assert_eq!(str2sig(b"RTMIN+14"), Ok(49));
}
#[test]
fn sig2str_50() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(50, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-14");
    assert_eq!(str2sig(b"RTMAX-14"), Ok(50));
}
#[test]
fn sig2str_51() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(51, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-13");
    assert_eq!(str2sig(b"RTMAX-13"), Ok(51));
}
#[test]
fn sig2str_52() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(52, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-12");
    assert_eq!(str2sig(b"RTMAX-12"), Ok(52));
}
#[test]
fn sig2str_53() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(53, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-11");
    assert_eq!(str2sig(b"RTMAX-11"), Ok(53));
}
#[test]
fn sig2str_54() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(54, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-10");
    assert_eq!(str2sig(b"RTMAX-10"), Ok(54));
}
#[test]
fn sig2str_55() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(55, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-9");
    assert_eq!(str2sig(b"RTMAX-9"), Ok(55));
}
#[test]
fn sig2str_56() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(56, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-8");
    assert_eq!(str2sig(b"RTMAX-8"), Ok(56));
}
#[test]
fn sig2str_57() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(57, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-7");
    assert_eq!(str2sig(b"RTMAX-7"), Ok(57));
}
#[test]
fn sig2str_58() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(58, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-6");
    assert_eq!(str2sig(b"RTMAX-6"), Ok(58));
}
#[test]
fn sig2str_59() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(59, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-5");
    assert_eq!(str2sig(b"RTMAX-5"), Ok(59));
}
#[test]
fn sig2str_60() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(60, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-4");
    assert_eq!(str2sig(b"RTMAX-4"), Ok(60));
}
#[test]
fn sig2str_61() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(61, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-3");
    assert_eq!(str2sig(b"RTMAX-3"), Ok(61));
}
#[test]
fn sig2str_62() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(62, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-2");
    assert_eq!(str2sig(b"RTMAX-2"), Ok(62));
}
#[test]
fn sig2str_63() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(63, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX-1");
    assert_eq!(str2sig(b"RTMAX-1"), Ok(63));
}
#[test]
fn sig2str_64() {
    let mut buffer = [0u8; SIG2STR_MAX];
    let length = sig2str(64, &mut buffer).unwrap();
    assert_eq!(&buffer[..length], b"RTMAX");
    assert_eq!(str2sig(b"RTMAX"), Ok(64));
}
#[test]
fn every_rtmin_plus_and_rtmax_minus_round_trips() {
    for offset in 1..=14 {
        let mut buffer = [0u8; SIG2STR_MAX];
        let a = rtmin_plus(offset);
        let length = sig2str(a, &mut buffer).unwrap();
        assert_eq!(str2sig(&buffer[..length]), Ok(a));
        let b = rtmax_minus(offset);
        let length = sig2str(b, &mut buffer).unwrap();
        assert_eq!(str2sig(&buffer[..length]), Ok(b));
    }
}

fn rtmin_plus(offset: i32) -> i32 {
    signal::SIGRTMIN + offset
}

fn rtmax_minus(offset: i32) -> i32 {
    signal::SIGRTMAX - offset
}

#[test]
fn internal_numbers_have_no_name_and_no_default() {
    for sig in 32..=34 {
        assert_eq!(name_of(sig), None);
        assert_eq!(default_action(sig), None);
    }
}

#[test]
fn realtime_signals_default_to_terminate() {
    for sig in 35..=64 {
        assert_eq!(default_action(sig), Some(DefaultAction::Terminate));
    }
}
