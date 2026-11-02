#!/usr/bin/env python3
"""全球网测（CAICT）节点表拉取与解密工具

数据来源与原 App 完全一致：
  1. GET https://dlcv2.cnspeedtest.cn:8443/TaierAndroid/Config/ConfigMD5.php
     拿到配置清单，其中 name == "serverlist_encrypt_url" 指向加密节点表
  2. 下载该文件（一整段大写 hex 文本）
  3. DES/ECB/PKCS5 解密，密钥 "dw!@#$%^"（逆向自 libGSCore.so 的 Jni.e()）
  4. 输出 JSON / CSV

用法：
    python decrypt_serverlist.py                 # 拉取 + 解密 + 导出
    python decrypt_serverlist.py --local file.json  # 只解密本地已下载的密文
    python decrypt_serverlist.py --check         # 顺便做 TCP 可达性探测
"""
from __future__ import annotations

import argparse
import binascii
import concurrent.futures as futures
import csv
import json
import socket
import sys
import urllib.request

from Crypto.Cipher import DES  # pip install pycryptodome

BASE = "https://dlcv2.cnspeedtest.cn:8443"
DES_KEY = b"dw!@#$%^"
UA = "GlobalSpeedTest-PC/1.0"


def get(url: str, timeout: int = 20) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def find_serverlist_url() -> str:
    url = (
        f"{BASE}/TaierAndroid/Config/ConfigMD5.php"
        "?province=%E5%8C%97%E4%BA%AC&city=%E5%8C%97%E4%BA%AC"
        "&imei=TS0000000000000000&appname=%E5%85%A8%E7%90%83%E7%BD%91%E6%B5%8B"
        "&vercode=40408&type=globalspeed&ipv6=0"
    )
    items = json.loads(get(url).decode("utf-8"))
    for it in items:
        if it.get("name") == "serverlist_encrypt_url":
            return it["filename"]
    raise SystemExit("配置清单里没有 serverlist_encrypt_url")


def decrypt(hex_text: str) -> list[dict]:
    data = binascii.unhexlify(hex_text.strip())
    out = DES.new(DES_KEY, DES.MODE_ECB).decrypt(data)
    pad = out[-1]
    if 0 < pad <= 8:
        out = out[:-pad]
    return json.loads(out.decode("utf-8"))


def tcp_alive(node: dict, timeout: float = 2.0) -> bool:
    try:
        with socket.create_connection((node["hostip"], int(node["port"])), timeout=timeout):
            return True
    except OSError:
        return False


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--local", help="本地已下载的密文文件")
    ap.add_argument("--out", default="nodes")
    ap.add_argument("--check", action="store_true", help="做 TCP 可达性探测")
    args = ap.parse_args()

    if args.local:
        hex_text = open(args.local, encoding="utf-8").read()
    else:
        url = find_serverlist_url()
        print(f"[*] 节点表地址: {url}")
        hex_text = get(url).decode("utf-8")

    nodes = decrypt(hex_text)
    print(f"[+] 解密成功: {len(nodes)} 个节点")

    json.dump(nodes, open(f"{args.out}.json", "w", encoding="utf-8"),
              ensure_ascii=False, indent=1)
    with open(f"{args.out}.csv", "w", newline="", encoding="utf-8-sig") as f:
        w = csv.writer(f)
        w.writerow(["hostid", "hostname", "pname", "location", "oper",
                    "hostip", "port", "geoid", "cityid"])
        for n in nodes:
            w.writerow([n.get(k, "") for k in
                        ("hostid", "hostname", "pname", "location", "oper",
                         "hostip", "port", "geoid", "cityid")])
    print(f"[+] 已写出 {args.out}.json / {args.out}.csv")

    if args.check:
        with futures.ThreadPoolExecutor(max_workers=64) as ex:
            alive = list(ex.map(tcp_alive, nodes))
        ok = sum(alive)
        print(f"[+] 可达: {ok}/{len(nodes)}")
        json.dump([n for n, a in zip(nodes, alive) if a],
                  open(f"{args.out}_alive.json", "w", encoding="utf-8"),
                  ensure_ascii=False, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
