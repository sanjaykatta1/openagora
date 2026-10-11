//! JSON-RPC 2.0 over a stream of JSON lines, in both directions: the engine
//! answers a client's requests and can ask the client things too (MCP
//! elicitation, the desktop's confirmation dialog).

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use anyhow::{Result, bail};
use serde_json::{Value, json};

pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

type Reply = std::result::Result<Value, Value>;

/// Why a request to the peer got no result.
#[derive(Debug, PartialEq, Eq)]
pub enum RequestError {
    /// The peer didn't answer in time.
    Timeout,
    /// The connection closed first.
    Closed,
    /// The peer answered with an error.
    Remote(String),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::Timeout => f.write_str("no answer in time"),
            RequestError::Closed => f.write_str("the connection closed"),
            RequestError::Remote(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RequestError {}

/// One connected client.
pub struct Peer {
    out: Mutex<Box<dyn Write + Send>>,
    pending: Mutex<HashMap<String, mpsc::Sender<Reply>>>,
    next_id: AtomicU64,
    closed: AtomicBool,
}

/// What a message read from the peer turned out to be.
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// A reply to one of our requests; already delivered to its waiter.
    Reply,
    Invalid,
}

impl Peer {
    pub fn new(out: Box<dyn Write + Send>) -> Peer {
        Peer {
            out: Mutex::new(out),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            closed: AtomicBool::new(false),
        }
    }

    fn send(&self, message: &Value) -> Result<()> {
        if self.closed.load(Ordering::SeqCst) {
            bail!("the connection is closed");
        }
        let mut out = self.out.lock().unwrap();
        writeln!(out, "{message}")?;
        out.flush()?;
        Ok(())
    }

    pub fn respond(&self, id: &Value, result: Value) -> Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
    }

    pub fn respond_error(&self, id: &Value, code: i64, message: &str) -> Result<()> {
        self.send(&json!({
            "jsonrpc": "2.0", "id": id,
            "error": {"code": code, "message": message},
        }))
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    /// Asks the peer something and waits for its answer.
    pub fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RequestError> {
        let id = format!("oa-{}", self.next_id.fetch_add(1, Ordering::SeqCst));
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id.clone(), tx);
        let sent = self.send(&json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        }));
        if sent.is_err() {
            self.pending.lock().unwrap().remove(&id);
            return Err(RequestError::Closed);
        }
        let reply = rx.recv_timeout(timeout);
        self.pending.lock().unwrap().remove(&id);
        match reply {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => Err(RequestError::Remote(
                error["message"]
                    .as_str()
                    .unwrap_or("the request failed")
                    .to_string(),
            )),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(RequestError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RequestError::Closed),
        }
    }

    /// Sorts a message from the peer, delivering replies to their waiters.
    pub fn classify(&self, message: Value) -> Incoming {
        let method = message["method"].as_str().map(str::to_string);
        let id = message.get("id").cloned().filter(|id| !id.is_null());
        match (method, id) {
            (Some(method), Some(id)) => Incoming::Request {
                id,
                method,
                params: message.get("params").cloned().unwrap_or(Value::Null),
            },
            (Some(method), None) => Incoming::Notification {
                method,
                params: message.get("params").cloned().unwrap_or(Value::Null),
            },
            (None, Some(id)) => {
                let key = match &id {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let reply = match message.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                if let Some(waiter) = self.pending.lock().unwrap().remove(&key) {
                    let _ = waiter.send(reply);
                }
                Incoming::Reply
            }
            (None, None) => Incoming::Invalid,
        }
    }

    /// The connection ended: wake everyone still waiting for an answer.
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.pending.lock().unwrap().clear();
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn requests_wait_for_their_reply() {
        let sink = Sink::default();
        let peer = Arc::new(Peer::new(Box::new(sink.clone())));
        let asker = Arc::clone(&peer);
        let waiter = std::thread::spawn(move || {
            asker.request("confirm", json!({"q": 1}), Duration::from_secs(5))
        });
        // Wait until the request is on the wire, then answer it.
        let id = loop {
            let text = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
            if let Some(line) = text.lines().next() {
                break serde_json::from_str::<Value>(line).unwrap()["id"].clone();
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(matches!(
            peer.classify(json!({"jsonrpc": "2.0", "id": id, "result": {"ok": true}})),
            Incoming::Reply
        ));
        assert_eq!(waiter.join().unwrap().unwrap(), json!({"ok": true}));

        // A closed connection fails the waiter instead of hanging it.
        let asker = Arc::clone(&peer);
        let waiter =
            std::thread::spawn(move || asker.request("confirm", json!({}), Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(50));
        peer.close();
        assert!(waiter.join().unwrap().is_err());
    }
}
