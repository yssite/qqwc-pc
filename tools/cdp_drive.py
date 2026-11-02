"""通过 WebView2 远程调试端口驱动前端：触发测速并采集实时状态。"""
import json
import time
import sys
import urllib.request

import websocket


def targets():
    with urllib.request.urlopen("http://127.0.0.1:9222/json", timeout=5) as r:
        return json.load(r)


def pick():
    for t in targets():
        if t.get("type") == "page" and "index.html" in (t.get("url") or ""):
            return t
    for t in targets():
        if t.get("type") == "page":
            return t
    return None


def main():
    action = sys.argv[1] if len(sys.argv) > 1 else "dump"
    t = pick()
    if not t:
        print("NO_TARGET")
        return 1
    ws = websocket.create_connection(t["webSocketDebuggerUrl"], timeout=20)

    def ev(expr):
        ws.send(json.dumps({"id": 1, "method": "Runtime.evaluate",
                            "params": {"expression": expr, "returnByValue": True, "awaitPromise": True}}))
        while True:
            msg = json.loads(ws.recv())
            if msg.get("id") == 1:
                res = msg.get("result", {})
                if "exceptionDetails" in res:
                    return {"error": res["exceptionDetails"].get("text"), "detail": str(res["exceptionDetails"])[:400]}
                return res.get("result", {}).get("value")

    if action == "start":
        print("start ->", ev("startTest(), 'started'"))
        ws.close()
        return 0

    if action == "stop":
        print("stop ->", ev("stopTest(), 'stopped'"))
        ws.close()
        return 0

    if action == "watch":
        dur = float(sys.argv[2]) if len(sys.argv) > 2 else 12
        end = time.time() + dur
        while time.time() < end:
            v = ev("JSON.stringify({phase:document.getElementById('phaseText').textContent,"
                   "ping:document.getElementById('mPing').textContent,"
                   "down:document.getElementById('mDown').textContent,"
                   "up:document.getElementById('mUp').textContent,"
                   "liveD:document.getElementById('liveDown').textContent,"
                   "running:state.running})")
            print(time.strftime("%H:%M:%S"), v)
            time.sleep(1.5)
        ws.close()
        return 0

    v = ev("JSON.stringify({nodes:state.nodes.length,settings:state.settings,"
           "ip:state.localIp,err:document.getElementById('phaseText').textContent})")
    print(v)
    ws.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
