#!/usr/bin/env python3
"""kukuapi-rs gateway integration test script."""
import subprocess, time, json, sys, os
from urllib.request import Request, urlopen
from urllib.error import URLError

BASE = "http://127.0.0.1:18081"
KEY = "cpk-gGzppK18mQHHXowct6kOeDhCQjasnIh3M9m4OYGYMLqtGB7w"

def req(method, path, headers=None, data=None):
    r = Request(f"{BASE}{path}", data=data, headers=headers or {}, method=method)
    try:
        resp = urlopen(r, timeout=5)
        return resp.status, resp.read().decode()
    except URLError as e:
        return getattr(e, 'code', 0), str(e.reason) if hasattr(e, 'reason') else str(e)

print("=== kukuapi-rs Gateway Test ===")
proc = subprocess.Popen(
    ["./target/debug/kukuapi-rs"],
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    cwd=os.path.dirname(os.path.abspath(__file__))
)
print(f"Server PID: {proc.pid}")
time.sleep(4)

try:
    # Health
    s, b = req("GET", "/health")
    print(f"  GET  /health               : {s} {'✅' if s==200 else '❌'}")
    
    # Ping (POST, no auth, no state)
    s, b = req("POST", "/ping")
    print(f"  POST /ping (simple)        : {s} {'✅' if s==200 else '❌'} body={b[:30]}")
    
    # Models
    s, b = req("GET", "/v1/models", {"x-api-key": KEY})
    if s == 200:
        d = json.loads(b)
        print(f"  GET  /v1/models            : {s} ✅ ({len(d['data'])} models)")
    else:
        print(f"  GET  /v1/models            : {s} ❌")
    
    # Chat (POST, with state+headers+json)
    h = {"x-api-key": KEY, "Content-Type": "application/json"}
    d = json.dumps({"model":"gpt-5.4","messages":[{"role":"user","content":"hi"}]}).encode()
    s, b = req("POST", "/v1/chat/completions", h, d)
    print(f"  POST /v1/chat/completions  : {s} {'✅' if s in (200,503) else '❌'} body={b[:80]}")
    
    # Usage
    s, b = req("GET", "/v1/usage", {"x-api-key": KEY})
    print(f"  GET  /v1/usage             : {s} {'✅' if s==200 else '❌'}")
    
except Exception as e:
    print(f"ERROR: {e}")
finally:
    proc.terminate()
    proc.wait(timeout=5)
    print(f"\nServer stopped (exit: {proc.returncode})")
