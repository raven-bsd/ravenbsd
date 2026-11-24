// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fixed-vector tests for the SNTP core.

use super::*;

const SERVER: Server = Server::new([192, 0, 2, 1], DEFAULT_PORT, 0);

fn request_time() -> NtpTime {
    NtpTime::new(Era::Era0, 1_000, 0)
}

fn response_packet(
    request: NtpTime,
    stratum: u8,
    receive_seconds: u32,
    transmit_seconds: u32,
) -> [u8; PACKET_LEN] {
    let mut packet = [0u8; PACKET_LEN];
    packet[0] = 0x24; // LI=0, VN=4, mode=4.
    packet[1] = stratum;
    packet[2] = 6;
    packet[3] = 0xEC; // -20.
    packet[4..8].copy_from_slice(&0x0000_1000_u32.to_be_bytes());
    packet[8..12].copy_from_slice(&0x0000_0200_u32.to_be_bytes());
    packet[12..16].copy_from_slice(&0x4750_5320_u32.to_be_bytes());
    packet[16..24].copy_from_slice(&NtpTimestamp::new(900, 0).to_be_bytes());
    packet[24..32].copy_from_slice(&request.timestamp().to_be_bytes());
    packet[32..40].copy_from_slice(&NtpTimestamp::new(receive_seconds, 0).to_be_bytes());
    packet[40..48].copy_from_slice(&NtpTimestamp::new(transmit_seconds, 0).to_be_bytes());
    packet
}

#[test]
fn request_bytes_are_a_sntp_v4_client_packet() {
    let transmit = NtpTime::new(Era::Era0, 0x1122_3344, 0x5566_7788);
    let packet = encode_request(transmit);
    assert_eq!(packet[0], 0x23);
    assert_eq!(&packet[1..40], &[0u8; 39]);
    assert_eq!(
        &packet[40..48],
        &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]
    );
}

#[test]
fn response_decodes_every_fixed_field() {
    let packet = response_packet(request_time(), 2, 1_005, 1_006);
    let response = decode_response(&packet).expect("decode");
    assert_eq!(response.leap, Leap::NoWarning);
    assert_eq!(response.version, 4);
    assert_eq!(response.mode, 4);
    assert_eq!(response.stratum, 2);
    assert_eq!(response.poll, 6);
    assert_eq!(response.precision, -20);
    assert_eq!(response.root_delay, NtpShort(0x0000_1000));
    assert_eq!(response.root_dispersion, NtpShort(0x0000_0200));
    assert_eq!(response.reference_id, 0x4750_5320);
    assert_eq!(response.reference, NtpTimestamp::new(900, 0));
    assert_eq!(response.originate, request_time().timestamp());
    assert_eq!(response.receive, NtpTimestamp::new(1_005, 0));
    assert_eq!(response.transmit, NtpTimestamp::new(1_006, 0));
}

#[test]
fn short_packet_is_rejected() {
    let short = [0u8; PACKET_LEN - 1];
    assert_eq!(decode_response(&short), Err(Error::PacketTooShort));
}

#[test]
fn wrong_version_and_mode_are_rejected() {
    let mut packet = response_packet(request_time(), 2, 1_005, 1_006);
    packet[0] = 0x1C; // VN=3, mode=4.
    assert_eq!(decode_response(&packet), Err(Error::BadVersion));
    let mut packet = response_packet(request_time(), 2, 1_005, 1_006);
    packet[0] = 0x21; // VN=4, mode=1.
    assert_eq!(decode_response(&packet), Err(Error::BadMode));
}

#[test]
fn unsynchronized_and_kiss_responses_are_rejected() {
    let mut packet = response_packet(request_time(), 2, 1_005, 1_006);
    packet[0] = 0xE4; // LI=3, VN=4, mode=4.
    let response = decode_response(&packet).expect("structural decode");
    assert_eq!(
        response.sample(SERVER, request_time(), request_time()),
        Err(Error::Unsynchronized)
    );

    let mut packet = response_packet(request_time(), 0, 1_005, 1_006);
    packet[12..16].copy_from_slice(b"RATE");
    let response = decode_response(&packet).expect("kiss decode");
    assert_eq!(
        response.sample(SERVER, request_time(), request_time()),
        Err(Error::KissOfDeath)
    );

    let packet = response_packet(request_time(), 16, 1_005, 1_006);
    let response = decode_response(&packet).expect("stratum decode");
    assert_eq!(
        response.sample(SERVER, request_time(), request_time()),
        Err(Error::Unsynchronized)
    );
}

#[test]
fn originate_mismatch_and_zero_transmit_are_rejected() {
    let mut packet = response_packet(request_time(), 2, 1_005, 1_006);
    packet[24..32].copy_from_slice(&NtpTimestamp::new(999, 0).to_be_bytes());
    let response = decode_response(&packet).expect("decode");
    assert_eq!(
        response.sample(SERVER, request_time(), request_time()),
        Err(Error::OriginateMismatch)
    );

    let mut packet = response_packet(request_time(), 2, 1_005, 1_006);
    packet[40..48].copy_from_slice(&NtpTimestamp::ZERO.to_be_bytes());
    let response = decode_response(&packet).expect("decode");
    assert_eq!(
        response.sample(SERVER, request_time(), request_time()),
        Err(Error::ZeroTransmit)
    );
}

#[test]
fn offset_and_delay_match_the_fixed_exchange() {
    let t1 = request_time();
    let t4 = NtpTime::new(Era::Era0, 1_008, 0);
    let packet = response_packet(t1, 2, 1_005, 1_006);
    let response = decode_response(&packet).expect("decode");
    let sample = response.sample(SERVER, t1, t4).expect("sample");
    assert_eq!(sample.server, SERVER);
    assert_eq!(sample.stratum, 2);
    assert_eq!(sample.offset_ns, 1_500_000_000);
    assert_eq!(sample.delay_ns, 7_000_000_000);
    assert_eq!(sample.root_delay_ns, 62_500_000);
    assert_eq!(sample.root_dispersion_ns, 7_812_500);
    assert_eq!(sample.reference_id, 0x4750_5320);
}

#[test]
fn unix_roundtrip_and_fraction() {
    let unix_seconds = 1_767_225_600_i128; // 2026-01-01T00:00:00Z.
    let time = NtpTime::from_unix_ns(unix_seconds * NS_PER_SEC).expect("era0");
    assert_eq!(time.era, Era::Era0);
    assert_eq!(time.seconds, 3_976_214_400);
    assert_eq!(time.fraction, 0);
    assert_eq!(time.to_unix_ns(), unix_seconds * NS_PER_SEC);

    let half = NtpTime::from_unix_ns(unix_seconds * NS_PER_SEC + 500_000_000).expect("fraction");
    assert_eq!(half.fraction, 0x8000_0000);
    assert_eq!(half.to_unix_ns(), unix_seconds * NS_PER_SEC + 500_000_000);
}

#[test]
fn era1_roundtrip() {
    let unix_seconds = 2_085_978_496_i128 + 10; // First era-1 seconds.
    let time = NtpTime::from_unix_ns(unix_seconds * NS_PER_SEC).expect("era1");
    assert_eq!(time.era, Era::Era1);
    assert_eq!(time.seconds, 10);
    assert_eq!(time.to_unix_ns(), unix_seconds * NS_PER_SEC);
}

#[test]
fn era_is_chosen_by_the_closest_timestamp() {
    let reference = NtpTime::new(Era::Era0, 0xFFFF_FF00, 0);
    let wrapped = NtpTimestamp::new(0x0000_0100, 0);
    let chosen = NtpTime::from_timestamp(wrapped, reference).expect("era");
    assert_eq!(chosen.era, Era::Era1);
    assert_eq!(chosen.seconds, 0x100);
}

#[test]
fn out_of_range_unix_values_are_rejected() {
    assert_eq!(
        NtpTime::from_unix_ns(-(1_i128 << 64)),
        Err(Error::EraOutOfRange)
    );
    assert_eq!(
        NtpTime::from_unix_ns(1_i128 << 64),
        Err(Error::EraOutOfRange)
    );
}

#[test]
fn server_selection_prefers_lowest_priority() {
    let mut servers = ServerList::new();
    servers
        .push(Server::new([192, 0, 2, 1], DEFAULT_PORT, 5))
        .expect("push");
    servers
        .push(Server::new([192, 0, 2, 2], DEFAULT_PORT, 1))
        .expect("push");
    servers
        .push(Server::new([192, 0, 2, 3], DEFAULT_PORT, 3))
        .expect("push");
    assert_eq!(servers.len(), 3);
    assert_eq!(
        servers.select(),
        Some(Server::new([192, 0, 2, 2], DEFAULT_PORT, 1))
    );
    assert_eq!(servers.iter().count(), 3);
}

#[test]
fn server_list_is_bounded() {
    let mut servers = ServerList::new();
    for index in 0..MAX_SERVERS {
        servers
            .push(Server::new([192, 0, 2, index as u8], DEFAULT_PORT, 0))
            .expect("push");
    }
    assert_eq!(
        servers.push(Server::new([192, 0, 2, 9], DEFAULT_PORT, 0)),
        Err(Error::ServerListFull)
    );
}

#[test]
fn best_sample_uses_stratum_then_delay_then_priority() {
    let base = Sample {
        server: SERVER,
        leap: Leap::NoWarning,
        stratum: 2,
        offset_ns: 0,
        delay_ns: 10,
        root_delay_ns: 0,
        root_dispersion_ns: 0,
        reference_id: 0,
        receive: request_time(),
        transmit: request_time(),
    };
    let mut lower_stratum = base;
    lower_stratum.stratum = 1;
    lower_stratum.delay_ns = 20;
    assert!(lower_stratum.better_than(&base));

    let mut lower_delay = base;
    lower_delay.delay_ns = 5;
    assert!(lower_delay.better_than(&base));

    let mut higher_priority = base;
    higher_priority.server = Server::new([192, 0, 2, 2], DEFAULT_PORT, 9);
    assert!(base.better_than(&higher_priority));
}

struct MockClock {
    now: NtpTime,
    set: Option<NtpTime>,
}

impl Clock for MockClock {
    fn now(&self) -> NtpTime {
        self.now
    }

    fn set(&mut self, time: NtpTime) -> Result<(), Error> {
        self.set = Some(time);
        Ok(())
    }
}

struct MockTransport {
    packets: [Option<[u8; PACKET_LEN]>; 3],
    calls: usize,
}

impl Transport for MockTransport {
    fn exchange(
        &mut self,
        _server: Server,
        _request: &[u8; PACKET_LEN],
        response: &mut [u8; PACKET_LEN],
        _timeout_ms: u32,
    ) -> Result<usize, Error> {
        let packet = self
            .packets
            .get(self.calls)
            .copied()
            .flatten()
            .ok_or(Error::Timeout);
        self.calls = self.calls.saturating_add(1);
        let packet = packet?;
        response.copy_from_slice(&packet);
        Ok(PACKET_LEN)
    }
}

#[test]
fn retry_policy_survives_two_timeouts_and_sets_the_clock() {
    let t1 = request_time();
    let packet = response_packet(t1, 2, 1_005, 1_006);
    let mut servers = ServerList::new();
    servers.push(SERVER).expect("push");
    let client = Client::new(servers, RetryPolicy::new(3, 250));
    let mut clock = MockClock { now: t1, set: None };
    let mut transport = MockTransport {
        packets: [None, None, Some(packet)],
        calls: 0,
    };
    let sample = client.sync(&mut clock, &mut transport).expect("sync");
    assert_eq!(transport.calls, 3);
    assert_eq!(sample.offset_ns, 5_500_000_000);
    assert_eq!(clock.set, Some(NtpTime::new(Era::Era0, 1_005, 0x8000_0000)));
}

#[test]
fn all_timeouts_return_timeout() {
    let mut servers = ServerList::new();
    servers.push(SERVER).expect("push");
    let client = Client::new(servers, RetryPolicy::new(1, 250));
    let mut clock = MockClock {
        now: request_time(),
        set: None,
    };
    let mut transport = MockTransport {
        packets: [None, None, None],
        calls: 0,
    };
    assert_eq!(client.sync(&mut clock, &mut transport), Err(Error::Timeout));
    assert_eq!(clock.set, None);
}

#[test]
fn empty_server_list_is_no_server() {
    let client = Client::new(ServerList::new(), RetryPolicy::new(1, 100));
    let mut clock = MockClock {
        now: request_time(),
        set: None,
    };
    let mut transport = MockTransport {
        packets: [None, None, None],
        calls: 0,
    };
    assert_eq!(
        client.sync(&mut clock, &mut transport),
        Err(Error::NoServer)
    );
}

#[test]
fn ntp_short_roundtrip() {
    let short = NtpShort::from_ns(500_000_000);
    assert_eq!(short.to_ns(), 500_000_000);
    assert_eq!(NtpShort(0).to_ns(), 0);
    assert_eq!(NtpShort::from_ns(-1), NtpShort(0));
}
