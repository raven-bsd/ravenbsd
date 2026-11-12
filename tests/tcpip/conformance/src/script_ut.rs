// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn parses_a_handshake_script() {
    let steps = parse(
        "0.000 socket() = 3\n\
         0.000 bind(3, ..., 8080) = 0\n\
         0.000 listen(3, 1) = 0\n\
         +0.100 < S 0:0(0) win 65535 <mss 1460,sackOK,nop,wscale 7>\n\
         +0.050 > S. 0:0(0) ack 1 win 65535 <mss 1460,nop,wscale 7>\n",
    )
    .expect("parse");
    assert_eq!(steps.len(), 10);
    assert_eq!(steps[0], Step::Time(0));
    assert_eq!(steps[1], Step::Syscall(Syscall::Socket));
}

#[test]
fn rejects_inconsistent_ranges() {
    assert!(parse("< S 0:2(1)\n").is_err());
}

#[test]
fn relative_and_absolute_times() {
    let steps = parse("0.010\n+0.250\n").expect("parse");
    assert_eq!(steps, vec![Step::Time(10), Step::Wait(250)]);
}

#[test]
fn parses_options() {
    let steps = parse("> S. 0:0(0) ack 1 <mss 1400,TS val 7 ecr 3,sackOK>\n").expect("parse");
    let Step::Expect(packet) = &steps[0] else {
        panic!("not a packet");
    };
    assert_eq!(packet.options.len(), 3);
    assert_eq!(packet.options[0], OptionSpec::Mss(1400));
}

#[test]
fn parses_write_and_read_lengths() {
    let steps =
        parse("0.000 write(3, ..., 17) = 17\n0.000 read(3, ..., 4096) = 17\n").expect("parse");
    assert_eq!(steps[1], Step::Syscall(Syscall::Write { len: 17 }));
    assert_eq!(
        steps[3],
        Step::Syscall(Syscall::Read {
            len: 4096,
            expected: Some(17)
        })
    );
}
