"""对未验证过的界面/命令做一轮冒烟测试，收集异常。"""
import json
import sys
import time
import urllib.request

import websocket


def conn():
    ts = json.load(urllib.request.urlopen("http://127.0.0.1:9222/json", timeout=5))
    page = [t for t in ts if t.get("type") == "page"][0]
    return websocket.create_connection(page["webSocketDebuggerUrl"], timeout=60)


def run(ws, expr, timeout=60):
    mid = run.mid = getattr(run, "mid", 0) + 1
    ws.send(json.dumps({"id": mid, "method": "Runtime.evaluate",
                        "params": {"expression": expr, "returnByValue": True,
                                   "awaitPromise": True, "timeout": timeout * 1000}}))
    t0 = time.time()
    while time.time() - t0 < timeout + 5:
        try:
            msg = json.loads(ws.recv())
        except Exception:
            return {"error": "ws timeout"}
        if msg.get("id") == mid:
            res = msg.get("result", {})
            if "exceptionDetails" in res:
                return {"error": res["exceptionDetails"].get("text"),
                        "detail": json.dumps(res["exceptionDetails"])[:600]}
            return {"value": res.get("result", {}).get("value")}
    return {"error": "evaluate timeout"}


def main():
    ws = conn()
    ws.send(json.dumps({"id": 900, "method": "Runtime.enable"}))
    ws.send(json.dumps({"id": 901, "method": "Log.enable"}))

    steps = []

    def step(name, expr, timeout=60):
        r = run(ws, expr, timeout)
        steps.append((name, r))
        print(f"[{name}] {json.dumps(r, ensure_ascii=False)[:400]}")
        return r

    step("tabs", """(()=>{const out=[];document.querySelectorAll('.tab').forEach(t=>{t.click();out.push(t.dataset.view)});switchView('test');return out.join(',')})()""")
    step("nodes-render", "(()=>{switchView('nodes');return document.querySelectorAll('#nodeList .node-row').length})()")
    step("nodes-filter", """(()=>{document.getElementById('provinceFilter').value='四川';renderNodes();const n=document.querySelectorAll('#nodeList .node-row').length;document.getElementById('provinceFilter').value='';renderNodes();return n})()""")
    step("probe-one", """(async()=>{const n=state.nodes.find(x=>x.pname==='四川')||state.nodes[0];const st=await invoke('probe_node',{host:n.hostip,port:parseInt(n.port)});return JSON.stringify({node:n.hostname,rtt:st.avg})})()""")
    step("history-list", """(async()=>{const h=await invoke('history_list');state.history=h;renderHistory();return h.length})()""")
    step("history-export", """(async()=>{const p=await invoke('history_export');return p})()""")
    step("diag-ping", """(async()=>{const st=await invoke('diag_ping',{host:'223.5.5.5',port:443,count:4});return JSON.stringify(st)})()""")
    step("diag-dns", """(async()=>{const r=await invoke('diag_dns',{host:'www.baidu.com'});return JSON.stringify(r)})()""")
    step("diag-trace", """(async()=>{const r=await invoke('diag_traceroute',{host:'223.5.5.5'});return (r||'').split('\\n').slice(0,4).join(' | ')})()""", 90)
    step("settings-save", """(async()=>{const s={...state.settings,duration_sec:8};state.settings=await invoke('save_settings',{settings:s});return state.settings.duration_sec})()""")
    step("match-api", """(async()=>{const v=await invoke('match_nodes',{ip:state.publicIp||'1.1.1.1',province:state.settings.province,city:state.settings.city,oper:state.settings.oper});return JSON.stringify(v).slice(0,200)})()""")

    # 收集异常
    ws.settimeout(1.5)
    errs = []
    try:
        while True:
            m = json.loads(ws.recv())
            if m.get("method") == "Runtime.exceptionThrown":
                errs.append(json.dumps(m["params"]["exceptionDetails"].get("text"))[:200])
            if m.get("method") == "Log.entryAdded" and m["params"]["entry"].get("level") == "error":
                errs.append(m["params"]["entry"].get("text", "")[:200])
    except Exception:
        pass
    print("EXCEPTIONS:", errs[:10])
    ws.close()


if __name__ == "__main__":
    sys.exit(main())
