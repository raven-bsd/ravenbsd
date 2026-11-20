#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Multi-request local HTTP server for the K199 curl/wget gate."""

import argparse
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

KNOWN = b"known-http-body"
INSULT = b"insult: your code is so slow it missed the TLB shootdown"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format, *args):
        print("server: " + (format % args), flush=True)

    def do_GET(self):
        self.close_connection = True
        path = self.path.split("?", 1)[0]
        if path == "/known.txt":
            self.respond(200, KNOWN, "text/plain")
        elif path == "/insult.txt":
            self.respond(200, INSULT, "text/plain")
        elif path == "/header":
            value = self.headers.get("X-Test", "missing")
            self.respond(200, f"header={value}".encode(), "text/plain")
        elif path == "/redirect":
            self.send_response(302)
            self.send_header("Location", "/known.txt")
            self.send_header("Content-Length", "0")
            self.end_headers()
        elif path == "/slow":
            # K219: sleep well past the 1 s client cap; the test must prove a timeout, not a
            # fast reply that happens to finish under load.
            time.sleep(5)
            self.respond(200, b"slow-body", "text/plain")
        elif path == "/missing":
            self.respond(404, b"missing", "text/plain")
        else:
            self.respond(404, b"missing", "text/plain")

    def respond(self, status, body, content_type):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        try:
            self.wfile.write(body)
        except BrokenPipeError:
            pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=18081)
    parser.add_argument("--seconds", type=int, default=180)
    args = parser.parse_args()

    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.daemon_threads = True
    print(f"server: listening 127.0.0.1:{args.port}", flush=True)
    timer = threading.Timer(args.seconds, server.shutdown)
    timer.daemon = True
    timer.start()
    try:
        server.serve_forever()
    finally:
        server.server_close()
        print("server: stopped", flush=True)


if __name__ == "__main__":
    main()
