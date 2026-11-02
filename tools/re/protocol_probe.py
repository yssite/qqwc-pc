#!/usr/bin/env python3
"""全球网测（CAICT）测速协议最小实现 / 验证工具

从 Android 版 v4.4.8 逆向还原的完整会话流程：

    GET  /speed/dovalid?key=&flag=true&bandwidth=200&model=Android&imei=<TS..>
                     &time=<unix>&app=globalspeed&token=<md5>&pkg=<pkg>
         → "1-<uuid>"
    GET  /speed/File(1G).dl?r=<unix>&key=<uuid>        # 下行，原始 socket 读
    POST /speed/doAnalsLoad.do  (Key: <uuid>, multipart 随机流)   # 上行
    POST /speed/dovalid?key=<uuid>                     # 结束会话

token 算法（逆向自 libGSCore.so 的 Jni.b，实网验证通过）：
    md5( md5("model={model}&imei={imei}")
       + md5("stime={time}&band={bandwidth}&rand=12345555") )

用法：
    python protocol_probe.py --host 182.151.203.2 --port 65499 --secs 5
"""
from __future__ import annotations

import argparse
import hashlib
import os
import random
import socket
import sys
import time
import urllib.request

SALT = "12345555"
UA_DL = ("Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 "
         "(KHTML, like Gecko) Chrome/100.0.4896.60 Safari/537.36")
UA_UL = "Dalvik/1.6.0 (Linux; U; Android 4.2.2; GT-I9505 Build/JDQ39)"
UL_PATH = "/speed/doAnalsLoad.do"


def md5(s: str) -> str:
    return hashlib.md5(s.encode()).hexdigest()


def make_token(model: str, imei: str, ts: int, bandwidth: int) -> str:
    b1 = f"model={model}&imei={imei}"
    b2 = f"stime={ts}&band={bandwidth}&rand={SALT}"
    return md5(md5(b1) + md5(b2))


def dev_imei() -> str:
    return "TS" + "".join(random.choice("0123456789ABCDEF") for _ in range(16))


def open_session(host: str, port: int, imei: str, bandwidth: int) -> str:
    ts = int(time.time())
    token = make_token("Android", imei, ts, bandwidth)
    url = (
        f"http://{host}:{port}/speed/dovalid?key=&flag=true"
        f"&bandwidth={bandwidth}&model=Android&imei={imei}&time={ts}"
        f"&app=globalspeed&token={token}&pkg=com.cnspeedtest.globalspeed"
    )
    req = urllib.request.Request(url, headers={"User-Agent": UA_DL})
    body = urllib.request.urlopen(req, timeout=10).read().decode().strip()
    if not body.startswith("1"):
        raise SystemExit(f"会话被拒: {body!r}")
    return body[2:]


def download(host: str, port: int, uuid: str, secs: float) -> int:
    s = socket.create_connection((host, port), timeout=10)
    path = f"/speed/File(1G).dl?r={int(time.time())}&key={uuid}"
    s.sendall((
        f"GET {path} HTTP/1.1\r\nAccept: */*\r\nConnection: close\r\n"
        f"User-Agent: {UA_DL}\r\nHost: {host}:{port}\r\n\r\n"
    ).encode())
    s.settimeout(2)
    total, t0 = 0, time.time()
    while time.time() - t0 < secs:
        try:
            chunk = s.recv(65536)
        except socket.timeout:
            continue
        if not chunk:
            break
        total += len(chunk)
    s.close()
    return total


def upload(host: str, port: int, uuid: str, secs: float) -> int:
    s = socket.create_connection((host, port), timeout=10)
    s.sendall((
        f"POST {UL_PATH} HTTP/1.1\r\nConnection: close\r\nCache-Control: no-cache\r\n"
        f"Charset: UTF-8\r\nKey: {uuid}\r\n"
        "Content-Type: multipart/form-data;boundary=00content0boundary00\r\n"
        f"User-Agent: {UA_UL}\r\nHost: {host}:{port}\r\n"
        "Accept-Encoding: gzip\r\nContent-Length: 900000000\r\n\r\n"
        "--00content0boundary00\r\n"
        'Content-Disposition: form-data; name="upload";filename="SPEED_probe"\r\n\r\n'
    ).encode())
    buf = os.urandom(16384)
    total, t0 = 0, time.time()
    while time.time() - t0 < secs:
        s.sendall(buf)
        total += len(buf)
    s.shutdown(socket.SHUT_WR)
    s.close()
    return total


def close_session(host: str, port: int, uuid: str) -> None:
    req = urllib.request.Request(f"http://{host}:{port}/speed/dovalid?key={uuid}",
                                 data=b"", method="POST")
    try:
        urllib.request.urlopen(req, timeout=10).read()
    except Exception as e:  # 结束失败不影响结果
        print(f"[!] 结束会话失败: {e}")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", required=True)
    ap.add_argument("--port", type=int, default=65499)
    ap.add_argument("--secs", type=float, default=5)
    ap.add_argument("--bandwidth", type=int, default=200,
                    help="会话上报的签约带宽（原版 WiFi 分支为 200）")
    args = ap.parse_args()

    imei = dev_imei()
    print(f"[*] 设备标识 {imei}")
    uuid = open_session(args.host, args.port, imei, args.bandwidth)
    print(f"[+] 会话 uuid = {uuid}")

    n = download(args.host, args.port, uuid, args.secs)
    print(f"[+] 下行 {n/1e6*8/args.secs:.2f} Mbps  ({n/1048576:.1f} MB)")

    n = upload(args.host, args.port, uuid, args.secs)
    print(f"[+] 上行 {n/1e6*8/args.secs:.2f} Mbps  ({n/1048576:.1f} MB)")

    close_session(args.host, args.port, uuid)
    print("[+] 会话已结束")
    return 0


if __name__ == "__main__":
    sys.exit(main())
