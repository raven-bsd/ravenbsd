// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `nc` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_nc::{MAX_READ, Options, parse, parse_ipv4};
    use raven_proto_net::socket::{Call, Endpoint, Response, SocketRequest};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const RETRY_MILLIS: u64 = 5;

    type TypedRead = (Vec<u8>, Option<Endpoint>);

    fn main(args: raven_rt::Args) -> i32 {
        let values: Vec<&str> = args.iter().skip(1).collect();
        let options = match parse(&values) {
            Ok(options) => options,
            Err(error) => {
                raven_rt::eprintln!("nc: {}", error.message());
                return 1;
            }
        };
        match run(&options) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("nc: {error}");
                1
            }
        }
    }

    fn run(options: &Options<'_>) -> Result<(), String> {
        if options.listen {
            listen(options)
        } else {
            connect(options)
        }
    }

    fn listen(options: &Options<'_>) -> Result<(), String> {
        let port = options.port.ok_or("listener port missing")?;
        let client = Client::new();
        if options.udp {
            let path = format!("net:udp/127.0.0.1:{port}");
            let handle = client
                .open(&path, READ_WRITE)
                .map_err(|error| error.description().to_string())?;
            expect_ok(call(
                &client,
                handle,
                Call::Bind(Endpoint {
                    ip: [127, 0, 0, 1],
                    port,
                }),
            )?)?;
            raven_rt::println!("nc: listening udp 127.0.0.1:{port}");
            let read_len = listener_read_len(options);
            let (data, source) = typed_read_retry(&client, handle, read_len, options.timeout_secs)?;
            print_received(&data);
            if options.echo {
                let source = source.ok_or("udp source missing")?;
                typed_write(&client, handle, &data, Some(source))?;
                raven_rt::println!("nc: echo {} bytes", data.len());
            }
            let _ = client.close(handle);
            raven_rt::println!("nc: done");
            return Ok(());
        }

        let path = format!("net:tcp/127.0.0.1:{port}");
        let listener = client
            .open(&path, READ_WRITE)
            .map_err(|error| error.description().to_string())?;
        expect_ok(call(&client, listener, Call::Listen { backlog: 4 })?)?;
        raven_rt::println!("nc: listening tcp 127.0.0.1:{port}");
        let id = accept_retry(&client, listener, options.timeout_secs)?;
        let accepted = client
            .open(&format!("net:accept/{id}"), READ_WRITE)
            .map_err(|error| error.description().to_string())?;
        let mut buffer = [0u8; MAX_READ];
        let length = read_retry(&client, accepted, &mut buffer, options.timeout_secs)?;
        let data = buffer.get(..length).ok_or("short read")?;
        print_received(data);
        if options.echo {
            client
                .write(accepted, 0, data)
                .map_err(|error| error.description().to_string())?;
            raven_rt::println!("nc: echo {} bytes", data.len());
        }
        let _ = client.close(accepted);
        let _ = client.close(listener);
        raven_rt::println!("nc: done");
        Ok(())
    }

    fn connect(options: &Options<'_>) -> Result<(), String> {
        let host = options.host.ok_or("host missing")?;
        let port = options.remote_port.ok_or("port missing")?;
        let ip = parse_ipv4(host).map_err(|error| error.message().to_string())?;
        let endpoint = Endpoint { ip, port };
        let client = Client::new();
        if options.udp {
            let bind_port = options.bind.ok_or("UDP bind port missing")?;
            let path = format!("net:udp/{host}:{port}");
            let handle = client
                .open(&path, READ_WRITE)
                .map_err(|error| error.description().to_string())?;
            expect_ok(call(
                &client,
                handle,
                Call::Bind(Endpoint {
                    ip: [127, 0, 0, 1],
                    port: bind_port,
                }),
            )?)?;
            raven_rt::println!("nc: connected udp {host}:{port}");
            if let Some(text) = options.send {
                typed_write(&client, handle, text.as_bytes(), Some(endpoint))?;
            }
            if options.read_len > 0 {
                let (data, _) =
                    typed_read_retry(&client, handle, options.read_len, options.timeout_secs)?;
                print_received(&data);
            }
            let _ = client.close(handle);
            raven_rt::println!("nc: done");
            return Ok(());
        }

        let path = format!("net:tcp/{host}:{port}");
        let handle = client
            .open(&path, READ_WRITE)
            .map_err(|error| error.description().to_string())?;
        connect_retry(&client, handle, endpoint, options.timeout_secs)?;
        raven_rt::println!("nc: connected tcp {host}:{port}");
        if let Some(text) = options.send {
            client
                .write(handle, 0, text.as_bytes())
                .map_err(|error| error.description().to_string())?;
        }
        if options.read_len > 0 {
            let mut buffer = [0u8; MAX_READ];
            let length = read_retry(&client, handle, &mut buffer, options.timeout_secs)?;
            let data = buffer.get(..length).ok_or("short read")?;
            print_received(data);
        }
        let _ = client.close(handle);
        raven_rt::println!("nc: done");
        Ok(())
    }

    fn listener_read_len(options: &Options<'_>) -> usize {
        if options.read_len == 0 {
            256
        } else {
            options.read_len
        }
    }

    fn print_received(data: &[u8]) {
        let text = core::str::from_utf8(data).unwrap_or("<binary>");
        raven_rt::println!("nc: received {text}");
    }

    fn call(client: &Client, handle: Handle, operation: Call) -> Result<Response, String> {
        let request = SocketRequest::Call(operation);
        let mut encoded = [0u8; 64];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("request encode failed"))?;
        let mut response = [0u8; 128];
        let read = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        Response::decode(&response[..read]).map_err(|_| String::from("response decode failed"))
    }

    fn expect_ok(response: Response) -> Result<(), String> {
        match response {
            Response::Ok => Ok(()),
            Response::Err(error) => Err(error.description().to_string()),
            _ => Err(String::from("unexpected response")),
        }
    }

    fn connect_retry(
        client: &Client,
        handle: Handle,
        endpoint: Endpoint,
        timeout_secs: u64,
    ) -> Result<(), String> {
        let mut retry = Retry::new(timeout_secs);
        loop {
            match call(client, handle, Call::Connect(endpoint))? {
                Response::Ok => return Ok(()),
                Response::Err(Errno::EAGAIN) => {}
                Response::Err(error) => return Err(error.description().to_string()),
                _ => return Err(String::from("unexpected connect response")),
            }
            if !retry.wait() {
                return Err(String::from("connect timed out"));
            }
        }
    }

    fn accept_retry(client: &Client, listener: Handle, timeout_secs: u64) -> Result<u32, String> {
        let mut retry = Retry::new(timeout_secs);
        loop {
            match call(client, listener, Call::Accept)? {
                Response::Socket { id } => return Ok(id),
                Response::Err(Errno::EAGAIN) => {}
                Response::Err(error) => return Err(error.description().to_string()),
                _ => return Err(String::from("unexpected accept response")),
            }
            if !retry.wait() {
                return Err(String::from("accept timed out"));
            }
        }
    }

    fn read_retry(
        client: &Client,
        handle: Handle,
        buffer: &mut [u8],
        timeout_secs: u64,
    ) -> Result<usize, String> {
        let mut retry = Retry::new(timeout_secs);
        loop {
            match client.read(handle, 0, buffer) {
                Ok(length) => return Ok(length),
                Err(Errno::EAGAIN) => {}
                Err(error) => return Err(error.description().to_string()),
            }
            if !retry.wait() {
                return Err(String::from("read timed out"));
            }
        }
    }

    fn typed_read_retry(
        client: &Client,
        handle: Handle,
        len: usize,
        timeout_secs: u64,
    ) -> Result<TypedRead, String> {
        let mut retry = Retry::new(timeout_secs);
        loop {
            if let Some(result) = typed_read(client, handle, len)? {
                return Ok(result);
            }
            if !retry.wait() {
                return Err(String::from("read timed out"));
            }
        }
    }

    fn typed_read(
        client: &Client,
        handle: Handle,
        len: usize,
    ) -> Result<Option<TypedRead>, String> {
        let len = u32::try_from(len).map_err(|_| String::from("read length"))?;
        let request = SocketRequest::Read { len };
        let mut encoded = [0u8; 32];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("read encode failed"))?;
        let mut response = [0u8; 2048];
        let read = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        let bytes = response.get(..read).ok_or("short read response")?;
        if bytes.get(4..6) == Some(&[6, 0]) {
            return match Response::decode(bytes.get(..6).ok_or("short error")?) {
                Ok(Response::Err(Errno::EAGAIN)) => Ok(None),
                Ok(Response::Err(error)) => Err(error.description().to_string()),
                _ => Err(String::from("bad read error response")),
            };
        }
        let flags = *bytes.get(6).ok_or("short read flags")?;
        let header = if flags & 1 != 0 { 17 } else { 11 };
        let header = Response::decode(bytes.get(..header).ok_or("short read header")?)
            .map_err(|_| String::from("read header decode failed"))?;
        let Response::Read { len, from } = header else {
            return Err(String::from("unexpected read response"));
        };
        let end = header_length(header) + len as usize;
        let payload = bytes
            .get(header_length(header)..end)
            .ok_or("short read payload")?;
        Ok(Some((payload.to_vec(), from)))
    }

    fn header_length(response: Response) -> usize {
        match response {
            Response::Read { from: Some(_), .. } => 17,
            _ => 11,
        }
    }

    fn typed_write(
        client: &Client,
        handle: Handle,
        data: &[u8],
        to: Option<Endpoint>,
    ) -> Result<(), String> {
        let request = SocketRequest::Write { data, to };
        let mut encoded = [0u8; 1536];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| String::from("write encode failed"))?;
        let mut response = [0u8; 128];
        let read = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        match Response::decode(&response[..read]) {
            Ok(Response::Write { len }) if len as usize == data.len() => Ok(()),
            Ok(Response::Err(error)) => Err(error.description().to_string()),
            _ => Err(String::from("unexpected write response")),
        }
    }

    struct Retry {
        left: u32,
    }

    impl Retry {
        fn new(timeout_secs: u64) -> Self {
            let steps = timeout_secs.saturating_mul(1000 / RETRY_MILLIS) as u32;
            Self { left: steps.max(1) }
        }

        fn wait(&mut self) -> bool {
            if self.left == 0 {
                return false;
            }
            self.left -= 1;
            let _ = raven_rt::nanosleep(RETRY_MILLIS * 1_000_000);
            true
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
