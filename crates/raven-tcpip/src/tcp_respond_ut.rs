// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the sans-IO `tcp_respond` port.

use super::*;

fn flags(first: TcpFlags, second: TcpFlags) -> TcpFlags {
    TcpFlags::from_bits(first.bits() | second.bits())
}

const V4_LOCAL: [u8; 4] = [192, 0, 2, 1];
const V4_REMOTE: [u8; 4] = [198, 51, 100, 7];

fn v4_request<'a>(
    incoming: &'a TcpHeader,
    connection: Option<ConnectionInfo>,
) -> RespondRequest<'a> {
    RespondRequest {
        local: Endpoint {
            address: IpAddr::V4(Ipv4Addr::new(V4_LOCAL)),
            port: 443,
        },
        remote: Endpoint {
            address: IpAddr::V4(Ipv4Addr::new(V4_REMOTE)),
            port: 4000,
        },
        incoming,
        mode: RespondMode::Reply,
        connection,
    }
}

fn incoming(flags: TcpFlags, sequence: u32, acknowledgement: u32) -> TcpHeader {
    TcpHeader {
        source_port: 4000,
        destination_port: 443,
        sequence,
        acknowledgement,
        flags,
        window: 1024,
        payload_len: 0,
    }
}

#[test]
fn reply_swaps_endpoints_and_uses_given_sequence_ack() {
    let header = incoming(flags(TcpFlags::ACK, TcpFlags::default()), 100, 200);
    let connection = ConnectionInfo {
        receive_space: 4096,
        receive_scale: 0,
        ip_ttl: 48,
        no_options: false,
        option_len: 0,
    };
    let result = tcp_respond(
        &v4_request(&header, Some(connection)),
        555,
        777,
        flags(TcpFlags::RST, TcpFlags::ACK),
    )
    .expect("response");

    assert_eq!(result.source.port, 443);
    assert_eq!(result.destination.port, 4000);
    assert_eq!(result.sequence, 777);
    assert_eq!(result.acknowledgement, 555);
    assert_eq!(result.flags, flags(TcpFlags::RST, TcpFlags::ACK));
    assert_eq!(result.ttl, 48);
    assert_eq!(result.window, 0, "RST carries a zero window");
    assert_eq!(
        result.checksum,
        ChecksumInput::V4 {
            source: Ipv4Addr::new(V4_LOCAL),
            destination: Ipv4Addr::new(V4_REMOTE),
            length: 20,
        }
    );
    assert_eq!(
        result.counters,
        RespondCounters {
            send_acks: 1,
            send_control: 0,
            send_total: 1,
        }
    );
    assert!(result.reset_logged);
}

#[test]
fn template_response_forces_ack_and_copies_window() {
    let header = incoming(flags(TcpFlags::RST, TcpFlags::default()), 10, 20);
    let connection = ConnectionInfo {
        receive_space: 32_768,
        receive_scale: 7,
        ip_ttl: 64,
        no_options: false,
        option_len: 0,
    };
    let mut request = v4_request(&header, Some(connection));
    request.mode = RespondMode::Template;
    let result = tcp_respond(&request, 20, 10, TcpFlags::RST).expect("template");

    assert_eq!(result.flags, TcpFlags::ACK);
    assert_eq!(result.sequence, 10);
    assert_eq!(result.acknowledgement, 20);
    assert_eq!(result.window, 256);
    assert_eq!(result.counters.send_acks, 1);
    assert_eq!(result.counters.send_control, 0);
    assert!(result.options_included);
}

#[test]
fn rst_selection_uses_ack_echo_or_sequence_space() {
    let acked = incoming(flags(TcpFlags::ACK, TcpFlags::default()), 100, 900);
    let selection = reset_selection(&acked);
    assert_eq!(selection.flags, TcpFlags::RST);
    assert_eq!(selection.sequence, 900);
    assert_eq!(selection.acknowledgement, 0);

    let syn = incoming(TcpFlags::SYN, 1000, 0);
    let selection = reset_selection(&syn);
    assert_eq!(selection.flags, flags(TcpFlags::RST, TcpFlags::ACK));
    assert_eq!(selection.sequence, 0);
    assert_eq!(selection.acknowledgement, 1001);

    let mut fin = incoming(TcpFlags::FIN, 2000, 0);
    fin.payload_len = 5;
    let selection = reset_selection(&fin);
    assert_eq!(selection.acknowledgement, 2006);
}

#[test]
fn ipv6_checksum_input_uses_local_to_remote() {
    let local = Ipv6Addr::new([0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let remote = Ipv6Addr::new([0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]);
    let header = incoming(flags(TcpFlags::ACK, TcpFlags::default()), 1, 2);
    let request = RespondRequest {
        local: Endpoint {
            address: IpAddr::V6(local),
            port: 443,
        },
        remote: Endpoint {
            address: IpAddr::V6(remote),
            port: 4000,
        },
        incoming: &header,
        mode: RespondMode::Reply,
        connection: None,
    };
    let result = tcp_respond(&request, 2, 1, flags(TcpFlags::RST, TcpFlags::ACK)).expect("v6");
    assert_eq!(
        result.checksum,
        ChecksumInput::V6 {
            source: local,
            destination: remote,
            length: 20,
        }
    );
    assert_eq!(result.ttl, IP_DEFTTL);
}

#[test]
fn mixed_address_families_are_rejected() {
    let header = incoming(flags(TcpFlags::ACK, TcpFlags::default()), 1, 2);
    let mut request = v4_request(&header, None);
    request.remote.address = IpAddr::V6(Ipv6Addr::LOOPBACK);
    assert_eq!(
        tcp_respond(&request, 2, 1, TcpFlags::RST),
        Err(RespondError::AddressFamilyMismatch)
    );
}

#[test]
fn drop_synchronized_sends_reset_and_counts_drop() {
    let outcome = tcp_drop(&DropRequest {
        synchronized: true,
        error: Errno::ECONNRESET,
        soft_error: None,
    });
    assert!(outcome.reset_required);
    assert_eq!(outcome.error, Errno::ECONNRESET);
    assert_eq!(
        outcome.counters,
        DropCounters {
            drops: 1,
            connection_drops: 0,
            closed: 1,
        }
    );
}

#[test]
fn drop_unsynchronized_counts_connection_and_prefers_soft_timeout() {
    let outcome = tcp_drop(&DropRequest {
        synchronized: false,
        error: Errno::ETIMEDOUT,
        soft_error: Some(Errno::ECONNRESET),
    });
    assert!(!outcome.reset_required);
    assert_eq!(outcome.error, Errno::ECONNRESET);
    assert_eq!(
        outcome.counters,
        DropCounters {
            drops: 0,
            connection_drops: 1,
            closed: 1,
        }
    );
}

#[test]
fn close_marks_state_closed_and_disconnected() {
    let outcome = tcp_close(&CloseRequest {
        state: TcpState::Established,
    });
    assert_eq!(outcome.state, TcpState::Closed);
    assert!(outcome.state_changed);
    assert!(outcome.disconnected);
    assert_eq!(outcome.counters.closed, 1);

    let already_closed = tcp_close(&CloseRequest {
        state: TcpState::Closed,
    });
    assert!(!already_closed.state_changed);
    assert_eq!(already_closed.counters.closed, 1);
}

#[test]
fn discard_flushes_queues_and_saves_rtt_ssthresh() {
    let outcome = tcp_discardcb(&DiscardRequest {
        rtt_samples: 4,
        inqueue_frames: 3,
        snd_ssthresh: 1000,
        maxseg: 1460,
        send_high_water: 65_536,
        ipv6: false,
    });
    assert!(outcome.reassembly_flushed);
    assert!(outcome.cc_detached);
    assert_eq!(outcome.inqueue_dropped, 3);
    assert!(outcome.rtt_saved);
    assert_eq!(outcome.ssthresh, Some(3000));

    let few_samples = tcp_discardcb(&DiscardRequest {
        rtt_samples: 3,
        inqueue_frames: 0,
        snd_ssthresh: 1000,
        maxseg: 1460,
        send_high_water: 65_536,
        ipv6: false,
    });
    assert!(!few_samples.rtt_saved);
    assert_eq!(few_samples.ssthresh, None);
}
