import json,urllib.request,websocket,sys,base64
out=sys.argv[1] if len(sys.argv)>1 else "work/cdp_shot.png"
t=[x for x in json.load(urllib.request.urlopen("http://127.0.0.1:9222/json")) if x.get("type")=="page"][0]
ws=websocket.create_connection(t["webSocketDebuggerUrl"],timeout=30)
ws.send(json.dumps({"id":1,"method":"Page.captureScreenshot","params":{"format":"png","captureBeyondViewport":False}}))
while True:
    m=json.loads(ws.recv())
    if m.get("id")==1:
        data=m["result"]["data"]
        open(out,"wb").write(base64.b64decode(data))
        print("saved",out); break
ws.close()
