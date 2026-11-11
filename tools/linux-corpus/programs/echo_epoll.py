# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
import selectors
import socket

sel = selectors.DefaultSelector()
srv = socket.socket()
srv.bind(("127.0.0.1", 0))
srv.listen(8)
srv.setblocking(False)
sel.register(srv, selectors.EVENT_READ)
cli = socket.create_connection(srv.getsockname())
cli.sendall(b"epoll")
done = False
conn = None
while not done:
    for key, _ in sel.select(timeout=2):
        if key.fileobj is srv:
            conn, _ = srv.accept()
            conn.setblocking(False)
            sel.register(conn, selectors.EVENT_READ)
        else:
            data = key.fileobj.recv(16)
            assert data == b"epoll"
            done = True
print("epoll ok")
