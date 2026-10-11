//! A stand-in for T3 Code's server, as the engine meets it (checked against
//! T3 Code's source): OAuth for outside agents (metadata, registration,
//! authorization with PKCE and a loopback redirect, token), its MCP server at
//! `/mcp` over Streamable HTTP, and signed attachment uploads. It records what
//! it was asked, for tests to check.
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Default)]
pub struct State {
    pub registrations: Vec<Value>,
    /// What the user picks on the approval page.
    pub scope: String,
    /// The user says no on the approval page.
    pub deny: bool,
    /// Grants were taken back: `/mcp` answers 401.
    pub revoked: bool,
    codes: HashMap<String, (String, String, String, String)>,
    tokens: Vec<String>,
    sessions: Vec<String>,
    /// Every tool call: (tool, arguments).
    pub calls: Vec<(String, Value)>,
    pending: HashMap<String, (String, usize, String)>,
    /// Uploaded attachments: (id, bytes).
    pub uploads: Vec<(String, Vec<u8>)>,
    pub projects: Vec<Value>,
    /// What waiting on a run reports.
    pub run_status: String,
    pub requests: Vec<String>,
}

pub struct FakeT3 {
    pub port: u16,
    pub state: Arc<Mutex<State>>,
}

impl FakeT3 {
    pub fn start() -> FakeT3 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(State {
            scope: "orchestration:read orchestration:operate".into(),
            run_status: "running".into(),
            projects: vec![
                json!({"id": "proj-1", "title": "openagora", "workspaceRoot": "/src/openagora", "deletedAt": null}),
                json!({"id": "proj-2", "title": "website", "workspaceRoot": "/src/website", "deletedAt": null}),
            ],
            ..State::default()
        }));
        let shared = Arc::clone(&state);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = Arc::clone(&shared);
                std::thread::spawn(move || serve(stream, port, &state));
            }
        });
        FakeT3 { port, state }
    }

    pub fn calls_to(&self, tool: &str) -> Vec<Value> {
        self.state
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(t, _)| t == tool)
            .map(|(_, args)| args.clone())
            .collect()
    }
}

struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).ok()?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((k, v)) = header.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        target,
        headers,
        body,
    })
}

fn respond(mut stream: &TcpStream, status: u16, headers: &[(&str, String)], body: &[u8]) {
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        204 => "No Content",
        302 => "Found",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        _ => "Not Found",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn json_response(stream: &TcpStream, status: u16, value: Value, extra: &[(&str, String)]) {
    let mut headers = vec![("Content-Type", "application/json".to_string())];
    headers.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
    respond(stream, status, &headers, value.to_string().as_bytes());
}

fn query(target: &str) -> HashMap<String, String> {
    let decode = |s: &str| {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'%' if i + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
                    out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                    i += 3;
                }
                b'+' => {
                    out.push(b' ');
                    i += 1;
                }
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    };
    target
        .split_once('?')
        .map(|(_, q)| q)
        .unwrap_or(target)
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

fn serve(stream: TcpStream, port: u16, state: &Arc<Mutex<State>>) {
    let Some(request) = read_request(&stream) else {
        return;
    };
    let origin = format!("http://127.0.0.1:{port}");
    let path = request.target.split('?').next().unwrap_or("").to_string();
    state
        .lock()
        .unwrap()
        .requests
        .push(format!("{} {path}", request.method));
    match (request.method.as_str(), path.as_str()) {
        ("GET", "/.well-known/oauth-authorization-server") => json_response(
            &stream,
            200,
            json!({
                "issuer": origin,
                "authorization_endpoint": format!("{origin}/oauth/mcp/authorize"),
                "token_endpoint": format!("{origin}/oauth/mcp/token"),
                "registration_endpoint": format!("{origin}/oauth/mcp/register"),
                "code_challenge_methods_supported": ["S256"],
            }),
            &[],
        ),
        ("POST", "/oauth/mcp/register") => {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
            let mut s = state.lock().unwrap();
            let ok = body["token_endpoint_auth_method"] == "none"
                && body["redirect_uris"]
                    .as_array()
                    .is_some_and(|uris| uris.iter().any(|u| u == "http://127.0.0.1/callback"));
            if !ok {
                return json_response(
                    &stream,
                    400,
                    json!({"error": "invalid_client_metadata"}),
                    &[],
                );
            }
            let id = format!("client-{}", s.registrations.len() + 1);
            s.registrations.push(body);
            json_response(
                &stream,
                201,
                json!({"client_id": id, "client_name": "OpenAgora"}),
                &[],
            )
        }
        ("GET", "/oauth/mcp/authorize") => {
            let q = query(&request.target);
            let mut s = state.lock().unwrap();
            let known = q.get("client_id").is_some_and(|c| {
                c.starts_with("client-")
                    && c[7..]
                        .parse::<usize>()
                        .is_ok_and(|n| n <= s.registrations.len())
            });
            let redirect = q.get("redirect_uri").cloned().unwrap_or_default();
            let loopback =
                redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/callback");
            let resource_ok =
                q.get("resource").map(String::as_str) == Some(&format!("{origin}/mcp"));
            let pkce_ok = q.get("code_challenge_method").map(String::as_str) == Some("S256")
                && q.get("code_challenge").is_some_and(|c| c.len() >= 43);
            if !(known
                && loopback
                && resource_ok
                && pkce_ok
                && q.get("response_type").map(String::as_str) == Some("code"))
            {
                return respond(&stream, 400, &[], b"bad authorization request");
            }
            let state_param = q.get("state").cloned().unwrap_or_default();
            if s.deny {
                let location = format!("{redirect}?error=access_denied&state={state_param}");
                return respond(&stream, 302, &[("Location", location)], b"");
            }
            let code = format!("code-{}", s.codes.len() + 1);
            s.codes.insert(
                code.clone(),
                (
                    q["client_id"].clone(),
                    q["code_challenge"].clone(),
                    redirect.clone(),
                    q["resource"].clone(),
                ),
            );
            let location = format!("{redirect}?code={code}&state={state_param}");
            respond(&stream, 302, &[("Location", location)], b"")
        }
        ("POST", "/oauth/mcp/token") => {
            let form = query(&String::from_utf8_lossy(&request.body));
            let mut s = state.lock().unwrap();
            let Some((client, challenge, redirect, resource)) =
                form.get("code").and_then(|c| s.codes.remove(c))
            else {
                return json_response(&stream, 400, json!({"error": "invalid_grant"}), &[]);
            };
            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
            let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
            let ok = form.get("grant_type").map(String::as_str) == Some("authorization_code")
                && form.get("client_id") == Some(&client)
                && form.get("redirect_uri") == Some(&redirect)
                && form.get("resource").is_none_or(|r| *r == resource)
                && computed == challenge;
            if !ok {
                return json_response(
                    &stream,
                    400,
                    json!({"error": "invalid_grant", "error_description": "the PKCE or the client didn't match"}),
                    &[],
                );
            }
            let token = format!("token-{}", s.tokens.len() + 1);
            s.tokens.push(token.clone());
            let scope = s.scope.clone();
            json_response(
                &stream,
                200,
                json!({"access_token": token, "token_type": "Bearer", "expires_in": 2_592_000, "scope": scope}),
                &[],
            )
        }
        ("POST", "/mcp") => mcp(&stream, &request, state),
        ("POST", upload) if upload.starts_with("/api/attachments/upload/") => {
            let token = upload
                .trim_start_matches("/api/attachments/upload/")
                .to_string();
            let mut s = state.lock().unwrap();
            let Some((id, size, _mime)) = s.pending.get(&token).cloned() else {
                return respond(&stream, 404, &[], b"Not Found");
            };
            if request.body.len() != size {
                return respond(
                    &stream,
                    400,
                    &[],
                    b"Content-Length must match the upload size.",
                );
            }
            s.pending.remove(&token);
            s.uploads.push((id, request.body.clone()));
            respond(&stream, 204, &[], b"")
        }
        _ => respond(&stream, 404, &[], b"Not Found"),
    }
}

fn tool_result(value: Value) -> Value {
    json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value, "isError": false})
}

fn tool_error(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": true})
}

fn tools() -> Value {
    json!([
        {"name": "t3_project_list", "description": "List registered projects.", "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true}},
        {"name": "t3_thread_list", "description": "List T3 threads.", "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true}},
        {"name": "t3_thread_read", "description": "Read a T3 thread.", "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": false}},
        {"name": "t3_thread_send", "description": "Send a message to a T3 thread.", "inputSchema": {"type": "object"}, "annotations": {"destructiveHint": true}},
        {"name": "t3_thread_launch", "description": "Create a top-level thread.", "inputSchema": {"type": "object"}, "annotations": {"destructiveHint": true}},
        {"name": "t3_thread_wait", "description": "Wait for a run.", "inputSchema": {"type": "object"}, "annotations": {"readOnlyHint": true}},
        {"name": "t3_attachment_prepare_upload", "description": "Create a signed upload URL.", "inputSchema": {"type": "object"}},
        {"name": "t3_thread_send_attachments", "description": "Send uploaded attachments.", "inputSchema": {"type": "object"}},
        {"name": "create_threads", "description": "Needs an agent running inside a T3 thread. Create threads.", "inputSchema": {"type": "object"}},
    ])
}

fn mcp(stream: &TcpStream, request: &Request, state: &Arc<Mutex<State>>) {
    let mut s = state.lock().unwrap();
    // T3 refuses browsers' requests to its MCP server.
    if request.header("Origin").is_some() {
        return respond(stream, 403, &[], b"Forbidden");
    }
    let bearer = request
        .header("Authorization")
        .and_then(|a| a.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    if s.revoked || !s.tokens.contains(&bearer) {
        return respond(stream, 401, &[("WWW-Authenticate", "Bearer".into())], b"");
    }
    let accept = request.header("Accept").unwrap_or_default();
    if !(accept.contains("application/json") && accept.contains("text/event-stream")) {
        return respond(stream, 400, &[], b"Not Acceptable");
    }
    let message: Value = serde_json::from_slice(&request.body).unwrap_or_default();
    let method = message["method"].as_str().unwrap_or_default().to_string();
    if method == "initialize" {
        let session = format!("session-{}", s.sessions.len() + 1);
        s.sessions.push(session.clone());
        return json_response(
            stream,
            200,
            json!({"jsonrpc": "2.0", "id": message["id"], "result": {
                "protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "t3", "version": "0"},
            }}),
            &[("Mcp-Session-Id", session)],
        );
    }
    let session_ok = request
        .header("Mcp-Session-Id")
        .is_some_and(|id| s.sessions.iter().any(|known| known == id));
    if !session_ok {
        return respond(stream, 404, &[], b"unknown session");
    }
    if message.get("id").is_none() {
        return respond(stream, 202, &[], b"");
    }
    let result = match method.as_str() {
        "tools/list" => json!({"tools": tools()}),
        "tools/call" => {
            let tool = message["params"]["name"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let args = message["params"]["arguments"].clone();
            s.calls.push((tool.clone(), args.clone()));
            call(&mut s, &tool, &args)
        }
        _ => {
            return json_response(
                stream,
                200,
                json!({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32601, "message": "unknown method"}}),
                &[],
            );
        }
    };
    // T3 answers some requests as an event stream.
    let reply = json!({"jsonrpc": "2.0", "id": message["id"], "result": result});
    if method == "tools/call" {
        let body = format!("event: message\ndata: {reply}\n\n");
        respond(
            stream,
            200,
            &[("Content-Type", "text/event-stream".into())],
            body.as_bytes(),
        );
    } else {
        json_response(stream, 200, reply, &[]);
    }
}

fn call(s: &mut State, tool: &str, args: &Value) -> Value {
    match tool {
        "t3_project_list" => tool_result(json!({"projects": s.projects, "nextCursor": null})),
        "t3_thread_list" => tool_result(
            json!({"threads": [{"threadId": "thread-1", "title": "Fix the crash"}], "nextCursor": null, "total": 1}),
        ),
        "t3_attachment_prepare_upload" => {
            let upload = &args["upload"];
            let size = upload["sizeBytes"].as_u64().unwrap_or(0) as usize;
            let mime = upload["mimeType"].as_str().unwrap_or_default().to_string();
            if !["image/png", "image/jpeg", "image/gif", "image/webp"].contains(&mime.as_str())
                || size == 0
            {
                return tool_error("unsupported upload");
            }
            let id = format!("attachment-{}", s.pending.len() + s.uploads.len() + 1);
            let token = format!("signed-{id}");
            s.pending.insert(token.clone(), (id.clone(), size, mime));
            tool_result(
                json!({"attachmentId": id, "relativeUrl": format!("/api/attachments/upload/{token}"), "expiresAt": 0}),
            )
        }
        "t3_thread_launch" => {
            let has_project = args.get("projectId").is_some();
            let scratch = args["scratch"] == true;
            if args["title"].as_str().is_none_or(str::is_empty) || has_project == scratch {
                return tool_error("pass a title, and projectId or scratch");
            }
            for attachment in args["attachments"].as_array().into_iter().flatten() {
                let id = attachment["id"].as_str().unwrap_or_default();
                if !s.uploads.iter().any(|(uploaded, _)| uploaded == id) {
                    return tool_error("attachments must be uploaded first");
                }
            }
            let project = args["projectId"].as_str().unwrap_or("scratch");
            tool_result(json!({
                "threadId": "thread-1", "projectId": project,
                "modelSelection": {"instanceId": "codex", "model": "gpt"}, "runId": "run-1", "status": "starting",
            }))
        }
        "t3_thread_wait" => {
            let status = s.run_status.clone();
            tool_result(
                json!({"threadId": args["threadId"], "runId": "run-1", "status": status, "timedOut": status == "running"}),
            )
        }
        "t3_thread_read" => {
            let items = if args["view"] == "activity" {
                json!([
                    {"type": "command", "title": "npm test", "text": "12 passed", "status": "completed"},
                    {"type": "file_change", "title": "src/parser.ts", "text": "+2 -1", "status": "completed"},
                ])
            } else {
                json!([
                    {"type": "user_message", "title": null, "text": "Fix the crash in this screenshot", "status": "completed"},
                    {"type": "assistant_message", "title": null, "text": "Fixed the null check in parser.ts; the tests pass.", "status": "completed"},
                ])
            };
            tool_result(
                json!({"thread": {"threadId": args["threadId"], "status": s.run_status}, "recentRuns": [], "items": items, "nextPosition": null, "hasMore": false}),
            )
        }
        "t3_thread_send" | "t3_thread_send_attachments" => tool_result(
            json!({"threadId": args["threadId"], "messageId": "message-2", "runId": "run-2", "status": "queued"}),
        ),
        _ => tool_error("unknown tool"),
    }
}
