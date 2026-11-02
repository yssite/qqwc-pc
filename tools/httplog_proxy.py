#!/usr/bin/env python3
"""Minimal dependency-free HTTP capture proxy.

Plain-HTTP requests are forwarded and fully logged (request line + headers +
first bytes of body).  HTTPS is tunnelled blindly through CONNECT and only the
host:port is recorded.  Responses are streamed so large downloads still work.
"""
import socket
import threading
import sys
import os
import time
import select

LOG_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "work", "capture", "http.log")
LOG_LOCK = threading.Lock()


def log(msg):
    with LOG_LOCK:
        with open(LOG_PATH, "a", encoding="utf-8", errors="replace") as f:
            f.write(msg + "\n")
    print(msg, flush=True)


def pipe(a, b, on_data=None, tag=""):
    try:
        while True:
            r, _, _ = select.select([a, b], [], [], 60)
            if not r:
                break
            for s in r:
                data = s.recv(65536)
                if not data:
                    return
                if s is a:
                    b.sendall(data)
                    if on_data:
                        on_data(data, tag)
                else:
                    a.sendall(data)
    except OSError:
        pass
    finally:
        for s in (a, b):
            try:
                s.close()
            except OSError:
                pass


def read_head(sock):
    buf = b""
    while b"\r\n\r\n" not in buf:
        chunk = sock.recv(8192)
        if not chunk:
            return None, buf
        buf += chunk
        if len(buf) > 262144:
            break
    head, _, rest = buf.partition(b"\r\n\r\n")
    return head, rest


def handle(client, addr):
    try:
        client.settimeout(30)
        head, rest = read_head(client)
        if head is None:
            client.close()
            return
        text = head.decode("latin-1", "replace")
        lines = text.split("\r\n")
        first = lines[0]
        method, _, rest_line = first.partition(" ")
        target = rest_line.split(" ")[0] if rest_line else ""

        if method.upper() == "CONNECT":
            host, _, port = target.partition(":")
            port = int(port or 443)
            upstream = socket.create_connection((host, port), timeout=20)
            client.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
            if rest:
                upstream.sendall(rest)
            log(f"[CONNECT] {host}:{port} t={time.strftime('%H:%M:%S')}")
            pipe(client, upstream)
            return

        # absolute-form target: http://host[:port]/path
        url = target
        if url.startswith("http://"):
            rest_path = url[7:]
            hostport, _, path = rest_path.partition("/")
            path = "/" + path
        else:
            host_header = next((l.split(":", 1)[1].strip() for l in lines[1:] if l.lower().startswith("host:")), "")
            hostport = host_header
            path = target
        host, _, port = hostport.partition(":")
        port = int(port or 80)

        headers = [l for l in lines[1:] if l and not l.lower().startswith("proxy-connection")]
        log("=" * 78)
        log(f"[HTTP] {method} http://{hostport}{path}")
        for h in headers:
            log(f"    {h}")

        body = rest
        clen = None
        for h in headers:
            if h.lower().startswith("content-length:"):
                try:
                    clen = int(h.split(":", 1)[1].strip())
                except ValueError:
                    clen = None
        if clen is not None:
            while len(body) < min(clen, 65536):
                chunk = client.recv(65536)
                if not chunk:
                    break
                body += chunk
        if body:
            preview = body[:512]
            log(f"    [BODY {len(body)}B] {preview!r}")

        upstream = socket.create_connection((host, port), timeout=20)
        req = "\r\n".join([first] + headers) + "\r\n\r\n"
        upstream.sendall(req.encode("latin-1"))
        if body:
            upstream.sendall(body)

        # stream response back, capturing only the header block
        got_head = False
        while True:
            chunk = upstream.recv(65536)
            if not chunk:
                break
            if not got_head:
                got_head = True
                head_resp = chunk.split(b"\r\n\r\n", 1)[0].decode("latin-1", "replace")
                log("    [RESP] " + head_resp.replace("\r\n", " | ")[:600])
            client.sendall(chunk)
    except Exception as exc:  # noqa: BLE001
        log(f"[ERR] {addr}: {exc}")
    finally:
        try:
            client.close()
        except OSError:
            pass


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8899
    os.makedirs(os.path.dirname(LOG_PATH), exist_ok=True)
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("0.0.0.0", port))
    srv.listen(128)
    log(f"listening on 0.0.0.0:{port}")
    while True:
        c, a = srv.accept()
        threading.Thread(target=handle, args=(c, a), daemon=True).start()


if __name__ == "__main__":
    main()
