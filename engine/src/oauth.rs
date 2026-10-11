//! OAuth 2.1 for apps on this computer that protect their MCP server with it
//! (T3 Code): the engine is the client, so the app's approval page names
//! OpenAgora. Authorization code with PKCE (S256), a client registered on the
//! fly (RFC 7591), and a loopback redirect to the engine (RFC 8252).

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// A PKCE verifier and its S256 challenge (RFC 7636).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn from_verifier(verifier: &str) -> Pkce {
        Pkce {
            verifier: verifier.to_string(),
            challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        }
    }

    pub fn new() -> Result<Pkce> {
        Ok(Pkce::from_verifier(&random_token(32)?))
    }
}

/// `bytes` random bytes from the OS, as URL-safe text.
pub fn random_token(bytes: usize) -> Result<String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|e| anyhow::anyhow!("no randomness from the OS: {e}"))?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

/// Percent-encodes a value for a URL query or a form body.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decodes a percent-encoded query value (`+` is a space).
pub fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // "%" and two hex digits; anything else is kept as it is.
        let escaped = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| Some(hex(bytes[i + 1])? * 16 + hex(bytes[i + 2])?))
            .flatten();
        match (bytes[i], escaped) {
            (_, Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b'+', None) => {
                out.push(b' ');
                i += 1;
            }
            (b, None) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `a=1&b=2` from pairs.
pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The query of a URL or request target, as pairs.
pub fn query(target: &str) -> Vec<(String, String)> {
    let Some((_, query)) = target.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((k, v)) => (decode(k), decode(v)),
            None => (decode(pair), String::new()),
        })
        .collect()
}

/// What the authorization page needs.
pub struct Authorize<'a> {
    pub endpoint: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub state: &'a str,
    pub pkce: &'a Pkce,
    /// The protected resource (RFC 8707), e.g. `http://127.0.0.1:3773/mcp`.
    pub resource: &'a str,
}

impl Authorize<'_> {
    pub fn url(&self) -> String {
        let sep = if self.endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        format!(
            "{}{sep}{}",
            self.endpoint,
            form(&[
                ("response_type", "code"),
                ("client_id", self.client_id),
                ("redirect_uri", self.redirect_uri),
                ("state", self.state),
                ("code_challenge", &self.pkce.challenge),
                ("code_challenge_method", "S256"),
                ("resource", self.resource),
            ])
        )
    }
}

/// A loopback listener for the authorization redirect.
pub struct Callback {
    listener: TcpListener,
    pub redirect_uri: String,
}

impl Callback {
    pub fn listen() -> Result<Callback> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .context("listening for the sign-in to come back")?;
        let port = listener.local_addr()?.port();
        Ok(Callback {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}/callback"),
        })
    }

    /// Waits for the browser to come back with a code for `state`. Answers the
    /// browser with a short page either way.
    pub fn wait(&self, state: &str, timeout: Duration) -> Result<String> {
        let deadline = Instant::now() + timeout;
        self.listener.set_nonblocking(true)?;
        loop {
            let stream = match self.listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        bail!("the sign-in didn't come back in time");
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            stream.set_nonblocking(false)?;
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
            let mut reader = BufReader::new(stream.try_clone()?);
            let mut line = String::new();
            reader.read_line(&mut line)?;
            let target = line
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            if !target.starts_with("/callback") {
                respond(stream, 404, "Not found.")?;
                continue;
            }
            let params = query(&target);
            let get = |name: &str| {
                params
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
            };
            if get("state").as_deref() != Some(state) {
                respond(stream, 400, "This sign-in isn't the one OpenAgora started.")?;
                continue;
            }
            if let Some(error) = get("error") {
                respond(
                    stream,
                    200,
                    "OpenAgora wasn't allowed. You can close this window.",
                )?;
                let detail = get("error_description").unwrap_or_default();
                bail!("the sign-in was refused: {error} {detail}");
            }
            let Some(code) = get("code") else {
                respond(stream, 400, "The sign-in came back without a code.")?;
                continue;
            };
            respond(
                stream,
                200,
                "OpenAgora is connected. You can close this window.",
            )?;
            return Ok(code);
        }
    }
}

fn respond(mut stream: std::net::TcpStream, status: u16, text: &str) -> Result<()> {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>OpenAgora</title><p style=\"font: 15px system-ui; margin: 3em\">{text}</p>"
    );
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc_7636() {
        // RFC 7636, appendix B.
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
        assert_eq!(
            pkce.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let fresh = Pkce::new().unwrap();
        assert!(fresh.verifier.len() >= 43);
        assert_ne!(fresh, Pkce::new().unwrap());
    }

    #[test]
    fn builds_and_reads_queries() {
        let pkce = Pkce::from_verifier("v");
        let url = Authorize {
            endpoint: "http://127.0.0.1:3773/oauth/mcp/authorize",
            client_id: "c 1",
            redirect_uri: "http://127.0.0.1:5000/callback",
            state: "s",
            pkce: &pkce,
            resource: "http://127.0.0.1:3773/mcp",
        }
        .url();
        assert!(url.starts_with("http://127.0.0.1:3773/oauth/mcp/authorize?response_type=code&client_id=c%201&redirect_uri=http%3A%2F%2F127.0.0.1%3A5000%2Fcallback"));
        let back = query(&url);
        assert_eq!(
            back.iter().find(|(k, _)| k == "resource").unwrap().1,
            "http://127.0.0.1:3773/mcp"
        );
        assert_eq!(
            back.iter().find(|(k, _)| k == "client_id").unwrap().1,
            "c 1"
        );
        assert_eq!(decode("a+b%2Fc%zz"), "a b/c%zz");
    }

    #[test]
    fn the_callback_takes_only_its_own_state() {
        let callback = Callback::listen().unwrap();
        let uri = callback.redirect_uri.clone();
        let browser = std::thread::spawn(move || {
            let port: u16 = uri
                .rsplit(':')
                .next()
                .unwrap()
                .split('/')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let get = |target: &str| {
                crate::http::request("GET", port, target, &[], None, Duration::from_secs(5))
                    .unwrap()
            };
            assert_eq!(get("/callback?code=x&state=other").status, 400);
            assert_eq!(get("/favicon.ico").status, 404);
            assert_eq!(get("/callback?code=the-code&state=mine").status, 200);
        });
        assert_eq!(
            callback.wait("mine", Duration::from_secs(10)).unwrap(),
            "the-code"
        );
        browser.join().unwrap();
    }
}
