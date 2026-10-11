#!/usr/bin/env python3
"""A stand-in for OpenCode's server in OpenAgora's tests: OpenCode 1's
`opencode serve` API (--api 1, password from OPENCODE_SERVER_PASSWORD) or
OpenCode 2's background service (--api 2, registered in --registration with
a password of its own). Every request is appended to $FAKE_OPENCODE_LOG as
one JSON line. A prompt makes the session ask to edit src/app.ts; once the
request is answered, the session finishes and says what it did."""
import argparse
import base64
import json
import os
import secrets
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

parser = argparse.ArgumentParser()
parser.add_argument("--api", choices=["1", "2"], required=True)
parser.add_argument("--port", type=int, default=0)
parser.add_argument("--registration")
args = parser.parse_args()

if args.api == "1":
    PASSWORD = os.environ.get("OPENCODE_SERVER_PASSWORD") or None
else:
    PASSWORD = secrets.token_hex(12)

LOCK = threading.Lock()
SESSIONS = {}
PENDING = {}
MESSAGES = {}
BUSY = set()
COUNT = [0]


def next_id(prefix):
    COUNT[0] += 1
    return f"{prefix}_{COUNT[0]}"


def log(entry):
    path = os.environ.get("FAKE_OPENCODE_LOG")
    if path:
        with LOCK, open(path, "a") as out:
            out.write(json.dumps(entry) + "\n")


def finish(session, answer):
    text = (
        "Fixed the bug in src/app.ts."
        if answer in ("once", "always")
        else "I wasn't allowed to edit src/app.ts, so I stopped."
    )
    MESSAGES.setdefault(session, []).append(text)
    BUSY.discard(session)


def ask_to_edit(session):
    request = next_id("per")
    PENDING[request] = session
    BUSY.add(session)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def send(self, code, body=None):
        data = b"" if body is None else json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def authorized(self):
        if not PASSWORD:
            return True
        expected = "Basic " + base64.b64encode(f"opencode:{PASSWORD}".encode()).decode()
        return self.headers.get("Authorization") == expected

    def body(self):
        length = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(length)) if length else None

    def do_GET(self):
        self.route("GET")

    def do_POST(self):
        self.route("POST")

    def route(self, method):
        url = urlparse(self.path)
        query = {k: v[0] for k, v in parse_qs(url.query).items()}
        body = self.body() if method == "POST" else None
        log({"method": method, "path": url.path, "query": query, "body": body, "auth": "Authorization" in self.headers})
        if not self.authorized():
            return self.send(401, {"error": "unauthorized"})
        parts = url.path.strip("/").split("/")
        with LOCK:
            handled = (v1 if args.api == "1" else v2)(self, method, parts, query, body)
        if handled is None:
            self.send(404, {"error": "not found"})


def v1(h, method, parts, query, body):
    if method == "GET" and parts == ["global", "health"]:
        return h.send(200, {"healthy": True, "version": "1.19.3"})
    if method == "POST" and parts == ["session"]:
        session = next_id("ses")
        SESSIONS[session] = {"id": session, "directory": query.get("directory"), "title": body.get("title")}
        return h.send(200, SESSIONS[session])
    if method == "POST" and len(parts) == 3 and parts[0] == "session" and parts[2] == "prompt_async":
        if parts[1] not in SESSIONS:
            return h.send(404, {"error": "no such session"})
        ask_to_edit(parts[1])
        return h.send(204)
    if method == "GET" and parts == ["session", "status"]:
        return h.send(200, {s: {"type": "busy"} for s in BUSY})
    if method == "GET" and parts == ["permission"]:
        return h.send(200, [
            {"id": r, "sessionID": s, "permission": "edit", "patterns": ["src/app.ts"], "metadata": {}, "always": ["src/app.ts"]}
            for r, s in PENDING.items()
        ])
    if method == "POST" and len(parts) == 3 and parts[0] == "permission" and parts[2] == "reply":
        session = PENDING.pop(parts[1], None)
        if session is None:
            return h.send(404, {"error": "no such request"})
        finish(session, body["reply"])
        return h.send(200, True)
    if method == "GET" and len(parts) == 3 and parts[0] == "session" and parts[2] == "message":
        messages = [{"info": {"role": "user"}, "parts": [{"type": "text", "text": "the prompt"}]}]
        for text in MESSAGES.get(parts[1], []):
            messages.append({
                "info": {"role": "assistant", "time": {"created": 1, "completed": 2}},
                "parts": [{"type": "text", "text": "Thinking…", "synthetic": True}, {"type": "text", "text": text}],
            })
        return h.send(200, messages)
    return None


def v2(h, method, parts, query, body):
    if method == "GET" and parts == ["api", "info"]:
        return h.send(200, {"version": "2.0.26"})
    if parts[:2] != ["api", "session"]:
        return None
    rest = parts[2:]
    if method == "POST" and rest == []:
        session = next_id("ses")
        SESSIONS[session] = {"id": session, "location": body.get("location"), "title": body.get("title")}
        return h.send(200, {"data": SESSIONS[session]})
    if method == "GET" and rest == ["active"]:
        return h.send(200, {"data": {s: {"type": "running"} for s in BUSY}})
    if len(rest) >= 2 and rest[0] not in SESSIONS:
        return h.send(404, {"error": "no such session"})
    if method == "POST" and len(rest) == 2 and rest[1] == "prompt":
        ask_to_edit(rest[0])
        return h.send(200, {"data": {"id": next_id("msg")}})
    if method == "GET" and len(rest) == 2 and rest[1] == "permission":
        return h.send(200, {"data": [
            {"id": r, "sessionID": s, "action": "edit", "resources": ["src/app.ts"]}
            for r, s in PENDING.items() if s == rest[0]
        ]})
    if method == "POST" and len(rest) == 4 and rest[1] == "permission" and rest[3] == "reply":
        session = PENDING.pop(rest[2], None)
        if session != rest[0]:
            return h.send(404, {"error": "no such request"})
        finish(session, body["decision"])
        return h.send(204)
    if method == "GET" and len(rest) == 2 and rest[1] == "message":
        data = [{"type": "user", "text": "the prompt"}]
        for text in MESSAGES.get(rest[0], []):
            data.append({"type": "assistant", "content": [{"type": "reasoning", "text": "hmm"}, {"type": "text", "text": text}], "time": {"created": 1, "completed": 2}})
            data.append({"type": "idle", "outcome": "succeeded"})
        return h.send(200, {"data": data, "cursor": {}})
    return None


server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
if args.registration:
    record = {"id": "fake", "version": "2.0.26", "url": f"http://127.0.0.1:{server.server_address[1]}", "pid": os.getpid(), "password": PASSWORD}
    partial = args.registration + ".partial"
    with open(partial, "w") as out:
        json.dump(record, out)
    os.replace(partial, args.registration)
server.serve_forever()
