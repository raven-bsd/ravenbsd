use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant as Wall};

use raven_netcore::testutil::{PairConfig, PairDevice};
use raven_netcore::{Device, DeviceCaps, Instant, Ipv4Addr, NetCore, TxError};

struct Timed {
    inner: PairDevice,
    spent: Rc<Cell<Duration>>,
}

impl Device for Timed {
    fn capabilities(&self) -> DeviceCaps {
        self.inner.capabilities()
    }
    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let start = Wall::now();
        let mut inner_time = Duration::ZERO;
        let got = self.inner.receive(now, &mut |bytes| {
            let t = Wall::now();
            consume(bytes);
            inner_time += t.elapsed();
        });
        self.spent
            .set(self.spent.get() + start.elapsed().saturating_sub(inner_time));
        got
    }
    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        let start = Wall::now();
        let mut inner_time = Duration::ZERO;
        let result = self.inner.transmit(now, len, &mut |buf| {
            let t = Wall::now();
            fill(buf);
            inner_time += t.elapsed();
        });
        self.spent
            .set(self.spent.get() + start.elapsed().saturating_sub(inner_time));
        result
    }
}

#[test]
fn split() {
    let mut total = Duration::ZERO;
    let (mut link_t, mut poll_t, mut send_t, mut recv_t) = (
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
    );
    let runs = 20;
    for _ in 0..runs {
        let spent = Rc::new(Cell::new(Duration::ZERO));
        let config = PairConfig {
            delay_ms: 10,
            ..PairConfig::reliable()
        };
        let (a, b) = PairDevice::pair(config);
        let mut client = NetCore::new(
            Timed {
                inner: a,
                spent: spent.clone(),
            },
            [2, 0, 0, 0, 0, 1],
            0,
        );
        let mut server = NetCore::new(
            Timed {
                inner: b,
                spent: spent.clone(),
            },
            [2, 0, 0, 0, 0, 2],
            0,
        );
        client.set_ipv4(Ipv4Addr::new(192, 0, 2, 1), 24, None);
        server.set_ipv4(Ipv4Addr::new(192, 0, 2, 2), 24, None);
        let l = server.tcp_listen(80, 8).unwrap();
        let c = client
            .tcp_connect((Ipv4Addr::new(192, 0, 2, 2), 80), None)
            .unwrap();
        let mut now = 0i64;
        let srv = loop {
            if let Ok(Some(s)) = server.tcp_accept(l) {
                break s;
            }
            now += 1;
            client.poll(now);
            server.poll(now);
        };
        spent.set(Duration::ZERO);
        let payload = vec![7u8; 1 << 20];
        let (mut sent, mut got) = (0usize, 0usize);
        let mut buf = vec![0u8; 65536];
        let wall = Wall::now();
        while got < payload.len() {
            let t = Wall::now();
            if sent < payload.len() {
                if let Ok(n) = client.send(c, &payload[sent..]) {
                    sent += n;
                }
            }
            send_t += t.elapsed();
            let t = Wall::now();
            while let Ok(n) = server.recv(srv, &mut buf) {
                if n == 0 {
                    break;
                }
                got += n;
            }
            recv_t += t.elapsed();
            now += 1;
            let t = Wall::now();
            client.poll(now);
            server.poll(now);
            poll_t += t.elapsed();
        }
        total += wall.elapsed();
        link_t += spent.get();
    }
    let per = |d: Duration| d / runs;
    println!(
        "total {:?} link {:?} poll(incl link) {:?} send {:?} recv {:?}",
        per(total),
        per(link_t),
        per(poll_t),
        per(send_t),
        per(recv_t)
    );
}

#[test]
fn harness_overhead() {
    let t = Wall::now();
    let payload: Vec<u8> = (0..(1usize << 20)).map(|i| (i % 251) as u8).collect();
    let pattern = t.elapsed();
    let t = Wall::now();
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for byte in &payload {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let hashed = t.elapsed();
    let t = Wall::now();
    let mut r = Vec::new();
    for chunk in payload.chunks(65536) {
        r.extend_from_slice(chunk);
    }
    println!(
        "pattern {pattern:?} hash {hashed:?} extend {:?} ({value})",
        t.elapsed()
    );
}
