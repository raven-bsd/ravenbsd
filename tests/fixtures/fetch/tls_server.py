#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Local verified-TLS HTTP server for the K199 gate.

The T3 test CA/server key is used with Python's OpenSSL-backed `ssl` module. This is a host test
fixture only; the target client's TLS stack is `raven-tls`/the OpenSSL add-on. `openssl s_server
-WWW` was not used because its close-delimited replies intermittently truncated under the known
netd/TCP retransmission race.
"""

import argparse
import socketserver
import ssl
import threading

BODY = b"known-http-body"


class Handler(socketserver.BaseRequestHandler):
    def handle(self):
        try:
            connection = self.server.context.wrap_socket(self.request, server_side=True)
        except (ssl.SSLError, OSError) as error:
            print(f"tls-server: handshake failed: {error}", flush=True)
            return
        try:
            request = b""
            while b"\r\n\r\n" not in request and len(request) < 8192:
                chunk = connection.recv(1024)
                if not chunk:
                    break
                request += chunk
            line = request.split(b"\r\n", 1)[0]
            parts = line.split(b" ")
            path = parts[1] if len(parts) >= 2 else b"/"
            print(
                f"tls-server: request {line.decode('ascii', 'replace')}",
                flush=True,
            )
            if path == b"/known.txt":
                status, body = b"200 OK", BODY
            else:
                status, body = b"404 Not Found", b"missing"
            response = (
                b"HTTP/1.1 "
                + status
                + b"\r\nContent-Type: text/plain\r\nContent-Length: "
                + str(len(body)).encode()
                + b"\r\nConnection: close\r\n\r\n"
                + body
            )
            connection.sendall(response)
        except OSError as error:
            print(f"tls-server: request failed: {error}", flush=True)
        finally:
            try:
                plain = connection.unwrap()
                plain.close()
            except (ssl.SSLError, OSError):
                connection.close()


class Server(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=24443)
    parser.add_argument("--seconds", type=int, default=180)
    parser.add_argument("--cert", required=True)
    parser.add_argument("--key", required=True)
    args = parser.parse_args()

    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    # The target netd/TCP path has a known TLS 1.3 post-handshake-ticket/CCS race. Pin the test
    # server to TLS 1.2, as the T2 s_server used -no_ticket; the client still verifies the chain
    # and IP name and no unverified path exists.
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.maximum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(args.cert, args.key)
    server = Server(("0.0.0.0", args.port), Handler)
    server.context = context
    print(f"tls-server: listening 0.0.0.0:{args.port}", flush=True)
    timer = threading.Timer(args.seconds, server.shutdown)
    timer.daemon = True
    timer.start()
    try:
        server.serve_forever()
    finally:
        server.server_close()
        print("tls-server: stopped", flush=True)


if __name__ == "__main__":
    main()
