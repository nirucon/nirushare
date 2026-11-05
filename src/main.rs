use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const PORT: u16 = 47831;
const MAX_BYTES: usize = 25 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

#[derive(Serialize, Deserialize, Default)]
struct Config {
    version: u32,
    trusted: Vec<Trusted>,
}
#[derive(Serialize, Deserialize, Clone)]
struct Trusted {
    host: String,
    token: String,
}
#[derive(Serialize, Deserialize)]
struct Outgoing {
    id: String,
    kind: String,
    name: String,
    digest: String,
    #[serde(default)]
    created: u64,
    recipients: Vec<Recipient>,
}
#[derive(Serialize, Deserialize)]
struct Recipient {
    host: String,
    #[serde(default, skip_serializing)]
    token: String,
    status: String,
    attempts: u32,
    last_error: String,
    /// Unix time of the next attempt; 0 means as soon as possible.
    #[serde(default)]
    next_attempt: u64,
    /// Consecutive unsuccessful passes (offline or failed), for backoff.
    #[serde(default)]
    backoff: u32,
}
#[derive(Serialize, Deserialize, Clone)]
struct InboxItem {
    id: String,
    kind: String,
    name: String,
    sender: String,
    received: u64,
    path: String,
    text: String,
    read: bool,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}
fn xdg(kind: &str) -> PathBuf {
    let env = match kind {
        "config" => "XDG_CONFIG_HOME",
        "data" => "XDG_DATA_HOME",
        "state" => "XDG_STATE_HOME",
        _ => "XDG_RUNTIME_DIR",
    };
    if let Some(v) = std::env::var_os(env) {
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    match kind {
        "config" => home().join(".config"),
        "data" => home().join(".local/share"),
        "state" => home().join(".local/state"),
        _ => PathBuf::from("/tmp"),
    }
}
fn config_dir() -> PathBuf {
    xdg("config").join("nirushare")
}
fn data_dir() -> PathBuf {
    xdg("data").join("nirushare")
}
fn state_dir() -> PathBuf {
    xdg("state").join("nirushare")
}
fn config_path() -> PathBuf {
    config_dir().join("config.json")
}
fn downloads_dir() -> PathBuf {
    let output = Command::new("xdg-user-dir").arg("DOWNLOAD").output();
    if let Ok(out) = output {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !p.is_empty() && Path::new(&p).is_dir() {
            return PathBuf::from(p);
        }
    }
    home().join("Downloads")
}
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
fn ensure_dirs() -> Result<(), String> {
    for p in [
        config_dir(),
        data_dir(),
        state_dir(),
        state_dir().join("out"),
        data_dir().join("inbox"),
        data_dir().join("dedup"),
    ] {
        private_dir(&p)?;
    }
    private_file(&config_path()).ok();
    Ok(())
}
fn load_config() -> Result<Config, String> {
    ensure_dirs()?;
    if !config_path().exists() {
        let cfg = Config {
            version: 1,
            trusted: Vec::new(),
        };
        save_config(&cfg)?;
        return Ok(cfg);
    }
    let bytes = fs::read(config_path()).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| format!("config: {e}"))
}
fn save_config(cfg: &Config) -> Result<(), String> {
    ensure_dirs()?;
    let tmp = config_path().with_extension(format!("tmp{}", std::process::id()));
    let data = serde_json::to_vec_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    private_file(&tmp)?;
    fs::rename(&tmp, config_path()).map_err(|e| e.to_string())?;
    private_file(&config_path())?;
    Ok(())
}
fn token() -> String {
    let mut b = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn now_id() -> String {
    let mut b = [0u8; 4];
    rand::thread_rng().fill_bytes(&mut b);
    format!("{}-{}", now_secs(), hex::encode(b))
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host
            .split('.')
            .all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'))
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}
fn private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn private_file(path: &Path) -> Result<(), String> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("{}: {e}", path.display()))
}
fn image_extension(body: &[u8]) -> Option<&'static str> {
    if body.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if body.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if body.len() >= 12 && &body[0..4] == b"RIFF" && &body[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

#[derive(Deserialize)]
struct TailscaleStatus {
    #[serde(rename = "Self")]
    own: Option<TailscalePeer>,
    #[serde(rename = "Peer")]
    peers: Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct TailscalePeer {
    #[serde(rename = "DNSName")]
    dns: Option<String>,
    #[serde(rename = "HostName")]
    host: Option<String>,
    #[serde(rename = "Online")]
    online: Option<bool>,
    #[serde(rename = "TailscaleIPs")]
    ips: Option<Vec<String>>,
}
struct Peer {
    host: String,
    ip: String,
    port: u16,
    online: bool,
}
fn host_of(peer: &TailscalePeer) -> String {
    peer.dns
        .as_deref()
        .map(|s| s.trim_end_matches('.'))
        .filter(|s| !s.is_empty())
        .or(peer.host.as_deref())
        .unwrap_or("unknown")
        .to_string()
}
fn ip_of(peer: &TailscalePeer) -> String {
    peer.ips
        .as_ref()
        .and_then(|v| v.iter().find(|ip| ip.contains('.')).cloned())
        .or_else(|| peer.ips.as_ref().and_then(|v| v.first().cloned()))
        .unwrap_or_default()
}
/// Test-only transport override, compiled only with the `test-transport`
/// feature (never in release builds): peers from NIRUSHARE_TEST_PEERS as
/// `host=ip:port,host=offline`, so two local instances can talk on 127.0.0.1.
#[cfg(feature = "test-transport")]
fn test_peers() -> Option<Vec<Peer>> {
    let spec = std::env::var("NIRUSHARE_TEST_PEERS").ok()?;
    let mut peers = Vec::new();
    for item in spec.split(',').filter(|s| !s.is_empty()) {
        let (host, addr) = item.split_once('=')?;
        let (ip, port, online) = match addr.rsplit_once(':') {
            Some((ip, port)) => (ip.to_string(), port.parse().ok()?, true),
            None => (String::new(), PORT, false),
        };
        peers.push(Peer {
            host: host.to_string(),
            ip,
            port,
            online,
        });
    }
    Some(peers)
}
fn tailscale() -> Result<(Option<Peer>, Vec<Peer>), String> {
    #[cfg(feature = "test-transport")]
    if let Some(peers) = test_peers() {
        return Ok((None, peers));
    }
    let out = Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        .map_err(|e| format!("tailscale unavailable: {e}"))?;
    if !out.status.success() {
        return Err("tailscale is not running or not logged in".into());
    }
    let status: TailscaleStatus =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("tailscale json: {e}"))?;
    let own = status.own.as_ref().map(|p| Peer {
        host: host_of(p),
        ip: ip_of(p),
        port: PORT,
        online: true,
    });
    let mut peers = Vec::new();
    if let Some(obj) = status.peers.as_ref().and_then(|v| v.as_object()) {
        for value in obj.values() {
            if let Ok(p) = serde_json::from_value::<TailscalePeer>(value.clone()) {
                peers.push(Peer {
                    host: host_of(&p),
                    ip: ip_of(&p),
                    port: PORT,
                    online: p.online.unwrap_or(false),
                });
            }
        }
    }
    peers.sort_by(|a, b| a.host.cmp(&b.host));
    Ok((own, peers))
}
fn whois_hosts(ip: &str) -> Result<Vec<String>, String> {
    // Test-only: the node name every local sender is reported as.
    #[cfg(feature = "test-transport")]
    if let Ok(host) = std::env::var("NIRUSHARE_TEST_WHOIS") {
        return Ok(vec![host]);
    }
    let out = Command::new("tailscale")
        .args(["whois", "--json", ip])
        .output()
        .map_err(|e| format!("tailscale whois: {e}"))?;
    if !out.status.success() {
        return Err("Tailscale cannot verify the sending node".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("whois json: {e}"))?;
    let node = value.get("Node").ok_or("whois has no node identity")?;
    let mut hosts = Vec::new();
    for key in ["Name", "ComputedName"] {
        if let Some(v) = node.get(key).and_then(|v| v.as_str()) {
            hosts.push(v.trim_end_matches('.').to_string());
        }
    }
    if let Some(v) = node
        .get("Hostinfo")
        .and_then(|v| v.get("Hostname"))
        .and_then(|v| v.as_str())
    {
        hosts.push(v.to_string());
    }
    hosts.retain(|h| !h.is_empty());
    if hosts.is_empty() {
        return Err("whois has no usable node name".into());
    }
    Ok(hosts)
}
fn host_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        || expected.split('.').next() == Some(actual)
        || actual.split('.').next() == Some(expected)
}
fn verify_remote(expected: &str, ip: &str) -> Result<(), String> {
    let hosts = whois_hosts(ip)?;
    if hosts.iter().any(|h| host_matches(expected, h)) {
        Ok(())
    } else {
        Err("Tailscale identity does not match the trusted host".into())
    }
}
fn find_peer<'a>(peers: &'a [Peer], host: &str) -> Option<&'a Peer> {
    peers
        .iter()
        .find(|p| p.host == host || p.host.split('.').next() == Some(host))
}
fn online_status(cfg: &Config) -> Vec<serde_json::Value> {
    let peers = tailscale().map(|(_, p)| p).unwrap_or_default();
    cfg.trusted.iter().map(|t| {
        let p = find_peer(&peers, &t.host);
        serde_json::json!({"host": t.host, "online": p.map(|p| p.online).unwrap_or(false), "address": p.map(|p| p.ip.clone()).unwrap_or_default()})
    }).collect()
}

/// Compare two secrets without leaking where they differ or how long they
/// are: both are hashed to fixed-length SHA-256 digests, which are compared
/// with no early exit.
fn secrets_equal(a: &str, b: &str) -> bool {
    let (da, db) = (Sha256::digest(a.as_bytes()), Sha256::digest(b.as_bytes()));
    let diff = da
        .iter()
        .zip(db.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}
/// The trusted host whose token matches. Every record is compared, so the
/// time taken does not reveal which one (if any) matched.
fn credentials(cfg: &Config, token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    let mut found = None;
    for t in &cfg.trusted {
        if secrets_equal(&t.token, token) && found.is_none() {
            found = Some(t.host.clone());
        }
    }
    found
}
fn notifications_enabled() -> bool {
    #[cfg(feature = "test-transport")]
    if std::env::var_os("NIRUSHARE_TEST_NO_NOTIFY").is_some() {
        return false;
    }
    true
}
fn incoming_notice(item: &InboxItem) {
    if !notifications_enabled() {
        return;
    }
    let item = item.clone();
    thread::spawn(move || {
        let (summary, body) = if item.kind == "text" {
            (
                "Text received",
                format!("Text received from {}", item.sender),
            )
        } else {
            (
                "Image received",
                format!("{} from {} · image", item.name, item.sender),
            )
        };
        let mut command = Command::new("notify-send");
        command.args(["--app-name", "NIRU Share", "--wait", summary, &body]);
        if item.kind == "text" {
            command.args(["--action=copy=Copy", "--action=inbox=Open inbox"]);
        } else {
            command.args(["--action=open=Open", "--action=folder=Open folder"]);
        }
        let Ok(output) = command.output() else {
            return;
        };
        let action = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match action.as_str() {
            "copy" => {
                let _ = copy_item(&item.id);
            }
            "open" => {
                let _ = open_item(&item.id);
            }
            "folder" => {
                if let Some(parent) = Path::new(&item.path).parent() {
                    let _ = Command::new("xdg-open").arg(parent).status();
                }
            }
            "inbox" => {
                let _ = Command::new("nirulauncher").arg("--share").status();
            }
            _ => {}
        }
    });
}

// ── Receiving service ──────────────────────────────────────────────────────
// A small HTTP/1.1 server on std sockets, so every connection has real read
// deadlines: a slow or stalled sender holds one of a few workers for a bounded
// time and never blocks the others. Protocol 1 is unchanged: POST /share with
// Bearer token, X-Nirushare-* headers and a Content-Length body; GET /health.

const WORKERS: usize = 3;
const HEAD_LIMIT: usize = 16 * 1024;
const HEAD_DEADLINE: Duration = Duration::from_secs(10);
const BODY_DEADLINE: Duration = Duration::from_secs(60);

fn body_deadline() -> Duration {
    #[cfg(feature = "test-transport")]
    if let Some(ms) = std::env::var("NIRUSHARE_TEST_BODY_DEADLINE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        return Duration::from_millis(ms);
    }
    BODY_DEADLINE
}

struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    /// Body bytes that arrived together with the head.
    early_body: Vec<u8>,
}
impl HttpRequest {
    fn header(&self, name: &str) -> String {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
    fn headers_named(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

/// An HTTP error answer: status code and a short reason for the log.
struct HttpError(u16, String);
fn http_err(code: u16, msg: impl Into<String>) -> HttpError {
    HttpError(code, msg.into())
}

fn reason(code: u16) -> &'static str {
    match code {
        100 => "Continue",
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    }
}
fn respond(stream: &mut std::net::TcpStream, code: u16, body: &str) {
    let head = format!(
        "HTTP/1.1 {code} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reason(code),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

/// Read until the deadline or `want` more bytes, whichever comes first.
fn read_some(
    stream: &mut std::net::TcpStream,
    buf: &mut [u8],
    start: std::time::Instant,
    deadline: Duration,
) -> std::io::Result<usize> {
    use std::io::ErrorKind;
    loop {
        let left = deadline
            .checked_sub(start.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| std::io::Error::new(ErrorKind::TimedOut, "deadline"))?;
        stream.set_read_timeout(Some(left))?;
        match stream.read(buf) {
            Ok(n) => return Ok(n),
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(std::io::Error::new(ErrorKind::TimedOut, "deadline"))
            }
            Err(e) => return Err(e),
        }
    }
}

fn read_head(stream: &mut std::net::TcpStream) -> Result<HttpRequest, HttpError> {
    let start = std::time::Instant::now();
    let mut data = Vec::with_capacity(1024);
    let mut buf = [0u8; 4096];
    let end = loop {
        if let Some(pos) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos;
        }
        if data.len() > HEAD_LIMIT {
            return Err(http_err(400, "request head too large"));
        }
        match read_some(stream, &mut buf, start, HEAD_DEADLINE) {
            Ok(0) => return Err(http_err(400, "connection closed in the request head")),
            Ok(n) => data.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(http_err(408, "request head timed out"))
            }
            Err(e) => return Err(http_err(400, e.to_string())),
        }
    };
    let head = std::str::from_utf8(&data[..end]).map_err(|_| http_err(400, "head not UTF-8"))?;
    let mut lines = head.split("\r\n");
    let mut parts = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(path), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(http_err(400, "bad request line"));
    };
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") || !path.starts_with('/') {
        return Err(http_err(400, "bad request line"));
    }
    let mut headers = Vec::new();
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(http_err(400, "bad header"));
        };
        let token = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
        if name.is_empty() || !name.bytes().all(token) || headers.len() >= 64 {
            return Err(http_err(400, "bad header"));
        }
        headers.push((name.to_string(), value.trim().to_string()));
    }
    Ok(HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        headers,
        early_body: data[end + 4..].to_vec(),
    })
}

/// The announced body length. Bodies must be sized: chunked uploads are not
/// part of protocol 1, and every sender (0.2.x included) sets Content-Length.
fn content_length(req: &HttpRequest) -> Result<usize, HttpError> {
    if !req.header("Transfer-Encoding").is_empty() {
        return Err(http_err(501, "chunked bodies are not supported"));
    }
    let values = req.headers_named("Content-Length");
    let Some(first) = values.first() else {
        return Err(http_err(411, "missing Content-Length"));
    };
    if values.iter().any(|v| v != first)
        || first.is_empty()
        || first.len() > 12
        || !first.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(http_err(400, "bad Content-Length"));
    }
    first
        .parse()
        .map_err(|_| http_err(400, "bad Content-Length"))
}

fn read_body(
    stream: &mut std::net::TcpStream,
    mut body: Vec<u8>,
    len: usize,
    deadline: Duration,
) -> Result<Vec<u8>, HttpError> {
    let start = std::time::Instant::now();
    body.truncate(len);
    body.reserve_exact(len - body.len());
    let mut buf = vec![0u8; 64 * 1024];
    while body.len() < len {
        let want = buf.len().min(len - body.len());
        match read_some(stream, &mut buf[..want], start, deadline) {
            Ok(0) => return Err(http_err(400, "body ended early")),
            Ok(n) => body.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(http_err(408, "body read deadline passed"))
            }
            Err(e) => return Err(http_err(400, e.to_string())),
        }
    }
    Ok(body)
}

/// Received transfers are committed one at a time, so a retry racing the
/// original cannot create a second inbox item.
static COMMIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn handle_share(
    stream: &mut std::net::TcpStream,
    req: HttpRequest,
    remote: &str,
) -> Result<&'static str, HttpError> {
    let cfg = load_config().map_err(|e| http_err(500, e))?;
    let auth = req.header("Authorization");
    let token = auth.strip_prefix("Bearer ").unwrap_or("");
    let sender = credentials(&cfg, token).ok_or_else(|| http_err(401, "unauthorized"))?;
    verify_remote(&sender, remote).map_err(|e| http_err(403, e))?;
    let id = req.header("X-Nirushare-Transfer");
    let kind = req.header("X-Nirushare-Kind");
    let name = req.header("X-Nirushare-Name");
    let announced = req.header("X-Nirushare-Digest");
    if !valid_id(&id) || !matches!(kind.as_str(), "text" | "image") {
        return Err(http_err(400, "bad transfer metadata"));
    }
    let limit = if kind == "text" {
        MAX_TEXT_BYTES
    } else {
        MAX_BYTES
    };
    let len = content_length(&req)?;
    if len > limit {
        return Err(http_err(413, "payload too large"));
    }
    if req.header("Expect").eq_ignore_ascii_case("100-continue") {
        let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
    }
    let body = read_body(stream, req.early_body, len, body_deadline())?;
    let digest = digest(&body);
    if !announced.is_empty() && !announced.eq_ignore_ascii_case(&digest) {
        return Err(http_err(400, "payload digest mismatch"));
    }
    let _guard = COMMIT.lock().unwrap_or_else(|e| e.into_inner());
    commit_share(&sender, &id, &kind, &name, &body, &digest)
}

fn commit_share(
    sender: &str,
    id: &str,
    kind: &str,
    name: &str,
    body: &[u8],
    digest: &str,
) -> Result<&'static str, HttpError> {
    let io = |e: std::io::Error| http_err(500, e.to_string());
    let dedup_dir = data_dir().join("dedup").join(sender.replace('.', "_"));
    private_dir(&dedup_dir).map_err(|e| http_err(500, e))?;
    let dedup = dedup_dir.join(format!("{id}.json"));
    if let Ok(existing) = fs::read(&dedup) {
        let same = serde_json::from_slice::<serde_json::Value>(&existing)
            .ok()
            .and_then(|v| {
                v.get("digest")
                    .and_then(|d| d.as_str())
                    .map(|d| d == digest)
            })
            .unwrap_or(false);
        if !same {
            return Err(http_err(409, "transfer id conflict"));
        }
        return Ok("{\"ok\":true,\"duplicate\":true}");
    }
    let stamp = now_secs();
    let display = if name.is_empty() {
        format!("{kind}-{stamp}")
    } else {
        name.to_string()
    };
    let safe: String = display
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    let mut path = String::new();
    let mut text = String::new();
    if kind == "image" {
        let ext =
            image_extension(body).ok_or_else(|| http_err(415, "unsupported image content"))?;
        let dir = downloads_dir().join("nirushare");
        fs::create_dir_all(&dir).map_err(io)?;
        let filename = format!("{stamp}-{}.{}", id.replace('-', ""), ext);
        let dst = dir.join(filename);
        let tmp = dir.join(format!(".{}.part", id.replace('-', "")));
        fs::write(&tmp, body).map_err(io)?;
        fs::rename(&tmp, &dst).map_err(io)?;
        path = dst.display().to_string();
    } else {
        text = String::from_utf8_lossy(body).to_string();
    }
    let item = InboxItem {
        id: id.to_string(),
        kind: kind.to_string(),
        name: safe,
        sender: sender.to_string(),
        received: stamp,
        path,
        text,
        read: false,
    };
    let meta = data_dir().join("inbox").join(format!("{id}.json"));
    let tmp = meta.with_extension("part");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(&item).map_err(|e| http_err(500, e.to_string()))?,
    )
    .map_err(io)?;
    private_file(&tmp).map_err(|e| http_err(500, e))?;
    fs::rename(&tmp, &meta).map_err(io)?;
    private_file(&meta).map_err(|e| http_err(500, e))?;
    fs::write(
        &dedup,
        serde_json::to_vec(&serde_json::json!({"id": id, "digest": digest}))
            .map_err(|e| http_err(500, e.to_string()))?,
    )
    .map_err(io)?;
    private_file(&dedup).map_err(|e| http_err(500, e))?;
    record_contact(&contacts_path(), sender, "received", stamp);
    incoming_notice(&item);
    Ok("{\"ok\":true}")
}

fn handle_connection(mut stream: std::net::TcpStream) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let remote = match stream.peer_addr() {
        Ok(a) => a.ip().to_string(),
        Err(_) => return,
    };
    let req = match read_head(&mut stream) {
        Ok(r) => r,
        Err(HttpError(code, msg)) => {
            eprintln!("request rejected: {msg}");
            respond(&mut stream, code, "{\"ok\":false}");
            return;
        }
    };
    let result = if req.method == "GET" && req.path == "/health" {
        Ok("{\"ok\":true,\"protocol\":1}")
    } else if req.method == "POST" && req.path.starts_with("/share") {
        handle_share(&mut stream, req, &remote)
    } else {
        Err(http_err(404, "not found"))
    };
    match result {
        Ok(body) => respond(&mut stream, 200, body),
        Err(HttpError(code, msg)) => {
            if code != 404 {
                eprintln!("share rejected: {msg}");
            }
            respond(&mut stream, code, "{\"ok\":false}");
            // Let the sender read the answer instead of a reset: stop
            // writing and discard a bounded amount of unread body.
            let _ = stream.shutdown(std::net::Shutdown::Write);
            let start = std::time::Instant::now();
            let mut buf = [0u8; 16 * 1024];
            let mut drained = 0;
            while drained < MAX_TEXT_BYTES {
                match read_some(&mut stream, &mut buf, start, Duration::from_secs(2)) {
                    Ok(n) if n > 0 => drained += n,
                    _ => break,
                }
            }
        }
    }
}

fn listen_addr() -> Result<String, String> {
    #[cfg(feature = "test-transport")]
    if let Ok(addr) = std::env::var("NIRUSHARE_TEST_LISTEN") {
        return Ok(addr);
    }
    let (own, _) = tailscale()?;
    let own = own.ok_or("no Tailscale identity")?;
    if own.ip.is_empty() {
        return Err("no Tailscale IPv4 address".into());
    }
    Ok(format!("{}:{}", own.ip, PORT))
}

fn serve() -> Result<(), String> {
    use std::sync::mpsc;
    let addr = listen_addr()?;
    let listener = std::net::TcpListener::bind(&addr).map_err(|e| format!("bind {addr}: {e}"))?;
    let wake = match bind_wake() {
        Ok(sock) => Some(sock),
        Err(e) => {
            eprintln!("nirushare: no wake socket ({e}); checking the queue every minute");
            None
        }
    };
    thread::spawn(move || queue_worker(wake));
    // A small bounded pool: each connection is handled by one worker, with a
    // short backlog; beyond that a sender gets 503 and retries later.
    let (tx, rx) = mpsc::sync_channel::<std::net::TcpStream>(WORKERS * 4);
    let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
    for _ in 0..WORKERS {
        let rx = rx.clone();
        thread::spawn(move || loop {
            let next = match rx.lock() {
                Ok(guard) => guard.recv(),
                Err(_) => return,
            };
            match next {
                Ok(stream) => handle_connection(stream),
                Err(_) => return,
            }
        });
    }
    eprintln!("nirushare listening on {addr}");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(mpsc::TrySendError::Full(mut stream)) = tx.try_send(stream) {
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
                    respond(&mut stream, 503, "{\"ok\":false,\"busy\":true}");
                }
            }
            // Out of descriptors or similar: do not spin.
            Err(_) => thread::sleep(Duration::from_millis(200)),
        }
    }
    Ok(())
}

/// The queue worker sleeps until the next retry or expiry is due, or until
/// `nirushare send` wakes it through the wake socket. With nothing pending
/// it blocks until the daily dedup pruning: no Tailscale query, no spawn.
fn queue_worker(wake: Option<UnixDatagram>) {
    let mut next_prune = 0;
    loop {
        let now = now_secs();
        if now >= next_prune {
            prune_dedup(&data_dir().join("dedup"), now, DEDUP_KEEP_SECS);
            next_prune = now + 24 * 60 * 60;
        }
        let mut deadline = None;
        if let Ok(cfg) = load_config() {
            let mut peers = || tailscale().map(|(_, p)| p);
            let mut deliver = deliver_http;
            let contacts = contacts_path();
            let mut env = QueueEnv {
                cfg: &cfg,
                now: now_secs(),
                peers: &mut peers,
                deliver: &mut deliver,
                contacts: Some(&contacts),
            };
            deadline = process_queue(&state_dir().join("out"), &history_path(), &mut env);
        }
        let deadline = deadline.map_or(next_prune, |d| d.min(next_prune));
        let wait = Duration::from_secs(deadline.saturating_sub(now_secs()).max(1));
        match &wake {
            Some(sock) => wait_for_wake(sock, Some(wait)),
            // Without a wake socket new items are only noticed by a slow timer.
            None => thread::sleep(wait.min(WAKELESS_IDLE)),
        }
    }
}
const WAKELESS_IDLE: Duration = Duration::from_secs(60);

/// Dedup receipts must outlive every retry of a transfer (7-day expiry);
/// they are kept for 30 days and pruned at start and once a day.
const DEDUP_KEEP_SECS: u64 = 30 * 24 * 60 * 60;
fn prune_dedup(dir: &Path, now: u64, keep: u64) -> usize {
    let mut removed = 0;
    let Ok(senders) = fs::read_dir(dir) else {
        return 0;
    };
    for sender in senders.flatten() {
        let Ok(kind) = sender.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let Ok(receipts) = fs::read_dir(sender.path()) else {
            continue;
        };
        for receipt in receipts.flatten() {
            let path = receipt.path();
            let is_receipt = receipt.file_type().map(|t| t.is_file()).unwrap_or(false)
                && path.extension().is_some_and(|e| e == "json");
            if !is_receipt {
                continue;
            }
            let Some(mtime) = entry_mtime(&path) else {
                continue;
            };
            if now.saturating_sub(mtime) > keep && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

fn runtime_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v).join("nirushare"),
        _ => state_dir(),
    }
}
fn wake_path() -> PathBuf {
    runtime_dir().join("wake")
}
fn bind_wake() -> Result<UnixDatagram, String> {
    use std::os::unix::fs::FileTypeExt;
    let path = wake_path();
    private_dir(&runtime_dir())?;
    if let Ok(meta) = fs::symlink_metadata(&path) {
        if meta.file_type().is_socket() {
            let _ = fs::remove_file(&path);
        }
    }
    let sock = UnixDatagram::bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    private_file(&path)?;
    Ok(sock)
}
/// Block until a wake datagram arrives or `wait` passes (None: no timeout).
/// The datagram content is ignored; it only triggers a queue pass.
fn wait_for_wake(sock: &UnixDatagram, wait: Option<Duration>) {
    let mut buf = [0u8; 16];
    let _ = sock.set_read_timeout(wait);
    let _ = sock.recv(&mut buf);
    // Several sends may have woken us; one pass handles them all.
    if sock.set_nonblocking(true).is_ok() {
        while sock.recv(&mut buf).is_ok() {}
        let _ = sock.set_nonblocking(false);
    }
}
/// Tell a running service that the queue changed. Missing service: no-op.
fn wake_service() {
    if let Ok(sock) = UnixDatagram::unbound() {
        let _ = sock.send_to(b"q", wake_path());
    }
}

/// Retry delay after `n` consecutive unsuccessful passes: 5 s doubling to 5 min.
fn backoff_secs(n: u32) -> u64 {
    if n == 0 {
        return 0;
    }
    (5u64 << (n - 1).min(16)).min(300)
}
fn schedule_retry(r: &mut Recipient, now: u64) {
    r.backoff = r.backoff.saturating_add(1);
    r.next_attempt = now + backoff_secs(r.backoff);
}

/// A recipient in one of these states is never sent to again.
fn is_final(status: &str) -> bool {
    matches!(
        status,
        "delivered" | "cancelled" | "expired" | "rejected" | "failed"
    )
}
fn all_final(outgoing: &Outgoing) -> bool {
    outgoing.recipients.iter().all(|r| is_final(&r.status))
}
fn entry_mtime(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
        .filter(|&s| s > 0)
}
fn history_path() -> PathBuf {
    state_dir().join("history.jsonl")
}
fn write_meta(dir: &Path, outgoing: &Outgoing) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(outgoing).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!(".meta.json.tmp{}", std::process::id()));
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    private_file(&tmp)?;
    fs::rename(&tmp, dir.join("meta.json")).map_err(|e| e.to_string())
}

/// Finished transfers leave the queue: a compact record without payload is
/// appended to the history, and the entry directory (payload included) is
/// removed. History keeps the newest HISTORY_KEEP records.
const HISTORY_KEEP: usize = 500;
fn archive(dir: &Path, history: &Path, outgoing: &Outgoing, now: u64) -> Result<(), String> {
    let record = serde_json::json!({
        "id": outgoing.id,
        "kind": outgoing.kind,
        "name": outgoing.name,
        "created": outgoing.created,
        "finished": now,
        "recipients": outgoing.recipients.iter().map(|r| serde_json::json!({
            "host": r.host,
            "status": r.status,
            "attempts": r.attempts,
            "last_error": r.last_error,
        })).collect::<Vec<_>>(),
    });
    let mut lines: Vec<String> = fs::read_to_string(history)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    lines.push(record.to_string());
    let skip = lines.len().saturating_sub(HISTORY_KEEP);
    let mut data = lines[skip..].join("\n");
    data.push('\n');
    let tmp = history.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    private_file(&tmp)?;
    fs::rename(&tmp, history).map_err(|e| e.to_string())?;
    // Only the entry directory this record describes is removed; it lives in
    // the private queue and its name was validated as a transfer id.
    fs::remove_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}
fn read_history(history: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(history)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Why a delivery attempt failed, in words for `nirushare status`.
struct DeliverError {
    message: String,
    permanent: bool,
}
struct Delivery<'a> {
    outgoing: &'a Outgoing,
    peer: &'a Peer,
    token: &'a str,
    payload: &'a [u8],
}
struct QueueEnv<'a> {
    cfg: &'a Config,
    now: u64,
    /// Tailscale peers; Err when Tailscale cannot be asked.
    peers: &'a mut dyn FnMut() -> Result<Vec<Peer>, String>,
    deliver: &'a mut dyn FnMut(&Delivery) -> Result<(), DeliverError>,
    /// Where successful contacts are recorded (None in unit tests).
    contacts: Option<&'a Path>,
}

/// One pass over the outgoing queue. Final entries are archived without
/// their payload ever being read; the payload is read only right before a
/// send attempt.
/// Returns the Unix time of the next due retry or expiry, or None when no
/// recipient is pending.
fn process_queue(out: &Path, history: &Path, env: &mut QueueEnv) -> Option<u64> {
    let Ok(entries) = fs::read_dir(out) else {
        return None;
    };
    let mut deadline: Option<u64> = None;
    let mut peers: Option<Result<Vec<Peer>, String>> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if !path.is_dir() || !name.to_str().map(valid_id).unwrap_or(false) {
            continue;
        }
        let Ok(meta_bytes) = fs::read(path.join("meta.json")) else {
            continue;
        };
        let Ok(mut outgoing) = serde_json::from_slice::<Outgoing>(&meta_bytes) else {
            continue;
        };
        let mut changed = outgoing.recipients.iter().any(|r| !r.token.is_empty());
        if outgoing.created == 0 {
            // Entries written before `created` existed: date them by their
            // directory once, so expiry applies to them too.
            outgoing.created = entry_mtime(&path).unwrap_or(env.now);
            changed = true;
        }
        if all_final(&outgoing) {
            let _ = archive(&path, history, &outgoing, env.now);
            continue;
        }
        if outgoing.created > 0 && env.now.saturating_sub(outgoing.created) > EXPIRY_SECS {
            for r in &mut outgoing.recipients {
                if !is_final(&r.status) {
                    r.status = "expired".into();
                    r.last_error.clear();
                }
            }
            let _ = archive(&path, history, &outgoing, env.now);
            continue;
        }
        let mut payload: Option<Vec<u8>> = None;
        for i in 0..outgoing.recipients.len() {
            if is_final(&outgoing.recipients[i].status) {
                continue;
            }
            if outgoing.recipients[i].next_attempt > env.now {
                continue;
            }
            let host = outgoing.recipients[i].host.clone();
            let Some(trusted) = env.cfg.trusted.iter().find(|t| t.host == host) else {
                let r = &mut outgoing.recipients[i];
                r.status = "rejected".into();
                r.last_error = "no longer trusted on this device".into();
                changed = true;
                continue;
            };
            let peers = peers.get_or_insert_with(|| (env.peers)());
            let known = peers.as_ref().ok().and_then(|p| find_peer(p, &host));
            let Some(peer) = known.filter(|p| p.online && !p.ip.is_empty()) else {
                let short = short_host(&host);
                let r = &mut outgoing.recipients[i];
                r.status = "waiting".into();
                r.last_error = match (&peers, known) {
                    (Err(_), _) => "Tailscale is not running on this device".into(),
                    (_, None) => format!("{short} is not in this tailnet"),
                    _ => format!("{short} is offline"),
                };
                schedule_retry(r, env.now);
                changed = true;
                continue;
            };
            if payload.is_none() {
                match fs::read(path.join("payload")) {
                    Ok(bytes) => payload = Some(bytes),
                    Err(_) => {
                        let r = &mut outgoing.recipients[i];
                        r.status = "failed".into();
                        r.last_error = "the queued content is missing".into();
                        changed = true;
                        continue;
                    }
                }
            }
            let result = (env.deliver)(&Delivery {
                outgoing: &outgoing,
                peer,
                token: &trusted.token,
                payload: payload.as_deref().unwrap_or_default(),
            });
            let r = &mut outgoing.recipients[i];
            match result {
                Ok(()) => {
                    r.status = "delivered".into();
                    r.last_error.clear();
                    r.backoff = 0;
                    r.next_attempt = 0;
                    if let Some(contacts) = env.contacts {
                        record_contact(contacts, &host, "sent", env.now);
                    }
                }
                Err(e) => {
                    r.attempts += 1;
                    r.last_error = e.message;
                    r.status = if e.permanent { "failed" } else { "waiting" }.into();
                    schedule_retry(r, env.now);
                }
            }
            changed = true;
        }
        if all_final(&outgoing) {
            let _ = archive(&path, history, &outgoing, env.now);
            continue;
        }
        if changed {
            let _ = write_meta(&path, &outgoing);
        }
        let expiry = outgoing.created + EXPIRY_SECS + 1;
        for r in outgoing.recipients.iter().filter(|r| !is_final(&r.status)) {
            let due = r.next_attempt.max(env.now).min(expiry);
            deadline = Some(deadline.map_or(due, |d: u64| d.min(due)));
        }
    }
    deadline
}

fn short_host(host: &str) -> &str {
    host.split('.').next().unwrap_or(host)
}

/// Turn a failed attempt into words for `nirushare status`, and decide
/// whether retrying can help.
fn describe_failure(host: &str, err: &ureq::Error) -> DeliverError {
    let h = short_host(host);
    let (message, permanent) = match err {
        ureq::Error::Status(code, _) => match code {
            401 => (format!("not trusted on {h}"), false),
            403 => (
                format!("{h} could not verify this device through Tailscale"),
                false,
            ),
            408 => (format!("{h} timed out while receiving"), false),
            409 => (
                format!("{h} already has a different transfer with this id"),
                true,
            ),
            413 => (format!("too large for {h}"), true),
            415 => (format!("{h} does not accept this content"), true),
            503 => (format!("{h} is busy"), false),
            // 0.2.x receivers answer 500 to every rejection.
            500 => (
                format!("{h} refused it (not paired there, or an error)"),
                false,
            ),
            _ => (format!("{h} answered HTTP {code}"), false),
        },
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::ConnectionFailed => {
                (format!("NIRU Share is not running on {h}"), false)
            }
            ureq::ErrorKind::Io => (format!("{h} did not answer in time"), false),
            _ => (format!("{h}: {t}"), false),
        },
    };
    DeliverError { message, permanent }
}

fn deliver_http(d: &Delivery) -> Result<(), DeliverError> {
    let url = format!("http://{}:{}/share", d.peer.ip, d.peer.port);
    ureq::post(&url)
        .set("Authorization", &format!("Bearer {}", d.token))
        .set("X-Nirushare-Transfer", &d.outgoing.id)
        .set("X-Nirushare-Kind", &d.outgoing.kind)
        .set("X-Nirushare-Name", &d.outgoing.name)
        .set("X-Nirushare-Digest", &d.outgoing.digest)
        .timeout(Duration::from_secs(30))
        .send_bytes(d.payload)
        .map(|_| ())
        .map_err(|e| describe_failure(&d.peer.host, &e))
}

// ── Contacts and status ────────────────────────────────────────────────────

fn contacts_path() -> PathBuf {
    state_dir().join("contacts.json")
}
static CONTACTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// Remember the last successful contact with a trusted host, per direction.
fn record_contact(path: &Path, host: &str, direction: &str, now: u64) {
    let _guard = CONTACTS.lock().unwrap_or_else(|e| e.into_inner());
    let mut all: serde_json::Map<String, serde_json::Value> = fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let entry = all
        .entry(host.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = entry.as_object_mut() {
        obj.insert(direction.to_string(), now.into());
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    if fs::write(&tmp, serde_json::Value::Object(all).to_string()).is_ok()
        && private_file(&tmp).is_ok()
    {
        let _ = fs::rename(&tmp, path);
    }
}
fn read_contacts(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// "3 min", "2 h", "5 days": a coarse, readable span.
fn span(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs} s"),
        60..=3599 => format!("{} min", secs / 60),
        3600..=86_399 => format!("{} h", secs / 3600),
        _ => {
            let d = secs / 86_400;
            format!("{d} day{}", if d == 1 { "" } else { "s" })
        }
    }
}
fn ago(now: u64, then: u64) -> String {
    if then == 0 {
        "never".into()
    } else {
        format!("{} ago", span(now.saturating_sub(then)))
    }
}

fn read_outgoing(out: &Path) -> Vec<Outgoing> {
    let mut items: Vec<Outgoing> = fs::read_dir(out)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| fs::read(e.path().join("meta.json")).ok())
                .filter_map(|b| serde_json::from_slice(&b).ok())
                .collect()
        })
        .unwrap_or_default();
    items.sort_by_key(|o| o.created);
    items
}

/// The status snapshot: trusted devices with reachability and last contact,
/// every queued item with per-recipient state, and recent finished items.
fn status_json(
    cfg: &Config,
    peers: &Result<Vec<Peer>, String>,
    outgoing: &[Outgoing],
    history: &[serde_json::Value],
    contacts: &serde_json::Map<String, serde_json::Value>,
    now: u64,
) -> serde_json::Value {
    let devices: Vec<_> = cfg
        .trusted
        .iter()
        .map(|t| {
            let p = peers.as_ref().ok().and_then(|p| find_peer(p, &t.host));
            let c = contacts.get(&t.host);
            let at = |k: &str| {
                c.and_then(|c| c.get(k))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0)
            };
            serde_json::json!({
                "host": t.host,
                "online": p.map(|p| p.online).unwrap_or(false),
                "address": p.map(|p| p.ip.clone()).unwrap_or_default(),
                "last_sent": at("sent"),
                "last_received": at("received"),
                "last_contact": at("sent").max(at("received")),
            })
        })
        .collect();
    let outgoing: Vec<_> = outgoing
        .iter()
        .map(|o| {
            let created = o.created;
            serde_json::json!({
                "id": o.id,
                "kind": o.kind,
                "name": o.name,
                "created": created,
                "expires": if created > 0 { created + EXPIRY_SECS } else { 0 },
                "recipients": o.recipients.iter().map(|r| serde_json::json!({
                    "host": r.host,
                    "state": r.status,
                    "attempts": r.attempts,
                    "next_retry": if is_final(&r.status) { 0 } else { r.next_attempt.max(now) },
                    "last_error": r.last_error,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let recent: Vec<_> = history.iter().rev().take(10).cloned().collect();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "tailscale": peers.is_ok(),
        "trusted": cfg.trusted.len(),
        "devices": devices,
        "outgoing": outgoing,
        "recent": recent,
    })
}

fn status_text(v: &serde_json::Value, now: u64) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let str_of = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_string();
    let num = |v: &serde_json::Value| v.as_u64().unwrap_or(0);
    let _ = writeln!(s, "NIRU Share {}", str_of(&v["version"]));
    if v["tailscale"] == false {
        let _ = writeln!(s, "Tailscale is not running on this device.");
    }
    let _ = writeln!(s, "\nDevices");
    let devices = v["devices"].as_array().cloned().unwrap_or_default();
    if devices.is_empty() {
        let _ = writeln!(s, "  none paired");
    }
    for d in &devices {
        let _ = writeln!(
            s,
            "  {:<16} {:<8} last contact {}",
            short_host(&str_of(&d["host"])),
            if d["online"] == true {
                "online"
            } else {
                "offline"
            },
            ago(now, num(&d["last_contact"]))
        );
    }
    let _ = writeln!(s, "\nOutgoing");
    let outgoing = v["outgoing"].as_array().cloned().unwrap_or_default();
    if outgoing.is_empty() {
        let _ = writeln!(s, "  nothing queued");
    }
    for o in &outgoing {
        let expires = num(&o["expires"]);
        let _ = writeln!(
            s,
            "  {} ({}) · queued {} · expires in {}",
            str_of(&o["name"]),
            str_of(&o["kind"]),
            ago(now, num(&o["created"])),
            span(expires.saturating_sub(now))
        );
        for r in o["recipients"].as_array().into_iter().flatten() {
            let mut line = format!(
                "    {:<16} {:<9} {} attempt{}",
                short_host(&str_of(&r["host"])),
                str_of(&r["state"]),
                num(&r["attempts"]),
                if num(&r["attempts"]) == 1 { "" } else { "s" }
            );
            let next = num(&r["next_retry"]);
            if next > now {
                let _ = write!(line, " · next try in {}", span(next - now));
            } else if next > 0 {
                line.push_str(" · next try now");
            }
            let err = str_of(&r["last_error"]);
            if !err.is_empty() {
                let _ = write!(line, " · {err}");
            }
            let _ = writeln!(s, "{line}");
        }
    }
    let recent = v["recent"].as_array().cloned().unwrap_or_default();
    if !recent.is_empty() {
        let _ = writeln!(s, "\nRecent");
        for h in &recent {
            let states: Vec<String> = h["recipients"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|r| {
                    format!(
                        "{} {}",
                        short_host(&str_of(&r["host"])),
                        str_of(&r["status"])
                    )
                })
                .collect();
            let _ = writeln!(
                s,
                "  {} ({}) · {} · {}",
                str_of(&h["name"]),
                str_of(&h["kind"]),
                ago(now, num(&h["finished"])),
                states.join(", ")
            );
        }
    }
    s
}

fn read_clipboard() -> Result<(String, String, Vec<u8>), String> {
    let image = Command::new("xclip")
        .args(["-selection", "clipboard", "-t", "image/png", "-o"])
        .output();
    if let Ok(out) = image {
        if out.status.success() && out.stdout.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Ok(("image".into(), "clipboard.png".into(), out.stdout));
        }
    }
    let out = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .output()
        .map_err(|e| format!("xclip: {e}"))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err("clipboard is empty or unreadable".into());
    }
    Ok(("text".into(), "clipboard.txt".into(), out.stdout))
}
fn send(args: &[String]) -> Result<(), String> {
    let cfg = load_config()?;
    let all = args.iter().any(|a| a == "--all");
    let mut recipients = Vec::new();
    let mut file: Option<PathBuf> = None;
    let mut stdin_text = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--to" if i + 1 < args.len() => {
                let host = args[i + 1].clone();
                if !valid_host(&host) {
                    return Err("invalid host".into());
                }
                let t = cfg
                    .trusted
                    .iter()
                    .find(|t| t.host == host || t.host.split('.').next() == Some(host.as_str()))
                    .ok_or_else(|| format!("not trusted: {host}"))?;
                recipients.push(Recipient {
                    host: t.host.clone(),
                    token: String::new(),
                    status: "pending".into(),
                    attempts: 0,
                    last_error: String::new(),
                    next_attempt: 0,
                    backoff: 0,
                });
                i += 2;
            }
            "--all" => {
                i += 1;
            }
            "--file" if i + 1 < args.len() => {
                file = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--clipboard" => {
                i += 1;
            }
            "--stdin" => {
                stdin_text = true;
                i += 1;
            }
            _ => return Err(format!("unknown send option: {}", args[i])),
        }
    }
    if all {
        for t in &cfg.trusted {
            recipients.push(Recipient {
                host: t.host.clone(),
                token: String::new(),
                status: "pending".into(),
                attempts: 0,
                last_error: String::new(),
                next_attempt: 0,
                backoff: 0,
            });
        }
    }
    if recipients.is_empty() {
        return Err("choose --to HOST or --all".into());
    }
    let (kind, name, payload) = if let Some(path) = file {
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.len() > MAX_BYTES {
            return Err("file too large".into());
        }
        image_extension(&bytes)
            .ok_or_else(|| "only PNG, JPEG or WebP images are supported".to_string())?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        ("image".to_string(), name, bytes)
    } else if stdin_text {
        let mut text = Vec::new();
        std::io::stdin()
            .take(MAX_TEXT_BYTES as u64 + 1)
            .read_to_end(&mut text)
            .map_err(|e| format!("stdin: {e}"))?;
        if text.len() > MAX_TEXT_BYTES {
            return Err("text too large".into());
        }
        if text.is_empty() {
            return Err("no text on stdin".into());
        }
        ("text".to_string(), "text.txt".to_string(), text)
    } else {
        read_clipboard()?
    };
    let id = now_id();
    let dir = state_dir().join("out").join(&id);
    private_dir(&dir)?;
    fs::write(dir.join("payload"), &payload).map_err(|e| e.to_string())?;
    private_file(&dir.join("payload"))?;
    let outgoing = Outgoing {
        id: id.clone(),
        kind,
        name,
        digest: digest(&payload),
        created: now_secs(),
        recipients,
    };
    write_meta(&dir, &outgoing)?;
    wake_service();
    println!("{id}");
    Ok(())
}
fn trust(host: &str, token: &str) -> Result<(), String> {
    if !valid_host(host) || token.len() < 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid host or token".into());
    }
    let mut cfg = load_config()?;
    cfg.trusted.retain(|t| t.host != host);
    cfg.trusted.push(Trusted {
        host: host.to_string(),
        token: token.to_string(),
    });
    save_config(&cfg)?;
    println!("trusted {host}");
    Ok(())
}

fn revoke(host: &str) -> Result<(), String> {
    let mut cfg = load_config()?;
    let before = cfg.trusted.len();
    cfg.trusted.retain(|t| t.host != host);
    if cfg.trusted.len() == before {
        return Err(format!("not trusted: {host}"));
    }
    save_config(&cfg)?;
    println!("revoked {host}");
    Ok(())
}

/// `status` prints JSON when asked (--json) or when stdout is not a
/// terminal, so scripts keep getting JSON; people get text.
fn status(args: &[String]) -> Result<(), String> {
    use std::io::IsTerminal;
    let cfg = load_config()?;
    let now = now_secs();
    let peers = tailscale().map(|(_, p)| p);
    let v = status_json(
        &cfg,
        &peers,
        &read_outgoing(&state_dir().join("out")),
        &read_history(&history_path()),
        &read_contacts(&contacts_path()),
        now,
    );
    let json = args.iter().any(|a| a == "--json")
        || (!args.iter().any(|a| a == "--text") && !std::io::stdout().is_terminal());
    if json {
        println!("{v}");
    } else {
        print!("{}", status_text(&v, now));
    }
    Ok(())
}
fn outbox() -> Result<(), String> {
    let out = read_outgoing(&state_dir().join("out"));
    let v = status_json(
        &Config::default(),
        &Ok(vec![]),
        &out,
        &[],
        &Default::default(),
        now_secs(),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&v["outgoing"]).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn list_devices() -> Result<(), String> {
    let cfg = load_config()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&online_status(&cfg)).map_err(|e| e.to_string())?
    );
    Ok(())
}
fn load_inbox(dir: &Path) -> Vec<InboxItem> {
    let mut items = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "json") {
                if let Ok(bytes) = fs::read(e.path()) {
                    if let Ok(item) = serde_json::from_slice::<InboxItem>(&bytes) {
                        items.push(item);
                    }
                }
            }
        }
    }
    items.sort_by_key(|i| i.received);
    items
}
fn list_inbox() -> Result<(), String> {
    ensure_dirs()?;
    let items = load_inbox(&data_dir().join("inbox"));
    println!(
        "{}",
        serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?
    );
    Ok(())
}

/// "30d", "12h", "2w" or a plain number of days, in seconds.
fn parse_age(text: &str) -> Option<u64> {
    let (num, unit) = match text.char_indices().find(|(_, c)| !c.is_ascii_digit()) {
        Some((i, _)) => (&text[..i], &text[i..]),
        None => (text, "d"),
    };
    let n: u64 = num.parse().ok()?;
    let unit = match unit {
        "h" => 3600,
        "d" => 86_400,
        "w" => 7 * 86_400,
        _ => return None,
    };
    n.checked_mul(unit)
}
/// The received file an inbox item owns, if it is a regular file inside
/// the NIRUSHARE folder of Downloads. Anything else is never deleted.
fn owned_file(item: &InboxItem, received_dir: &Path) -> Option<PathBuf> {
    if item.path.is_empty() {
        return None;
    }
    let path = PathBuf::from(&item.path);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    let parent = path.parent()?.canonicalize().ok()?;
    (parent == received_dir.canonicalize().ok()?).then_some(path)
}
fn select_inbox(
    items: &[InboxItem],
    read_only: bool,
    older: Option<u64>,
    now: u64,
) -> Vec<&InboxItem> {
    items
        .iter()
        .filter(|i| !read_only || i.read)
        .filter(|i| older.is_none_or(|age| now.saturating_sub(i.received) > age))
        .collect()
}
/// Remove one inbox item and the received file it owns. The dedup receipt
/// stays, so a late retry of the same transfer is not stored again.
fn remove_inbox_item(inbox: &Path, item: &InboxItem, received_dir: &Path) -> Result<(), String> {
    if !valid_id(&item.id) {
        return Err("invalid item id".into());
    }
    if let Some(file) = owned_file(item, received_dir) {
        fs::remove_file(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    let meta = inbox.join(format!("{}.json", item.id));
    fs::remove_file(&meta).map_err(|e| format!("{}: {e}", meta.display()))
}
/// `inbox --clear-read` / `--older-than AGE`: preview what would go, and
/// remove it only with `--yes`.
fn inbox_cleanup(args: &[String]) -> Result<(), String> {
    let (mut read_only, mut older, mut yes) = (false, None, false);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--clear-read" => read_only = true,
            "--yes" => yes = true,
            "--older-than" if i + 1 < args.len() => {
                older =
                    Some(parse_age(&args[i + 1]).ok_or("--older-than takes e.g. 30d, 12h or 2w")?);
                i += 1;
            }
            "--json" => {}
            other => return Err(format!("unknown inbox option: {other}")),
        }
        i += 1;
    }
    if !read_only && older.is_none() {
        return list_inbox();
    }
    ensure_dirs()?;
    let inbox = data_dir().join("inbox");
    let received_dir = downloads_dir().join("nirushare");
    let items = load_inbox(&inbox);
    let now = now_secs();
    let chosen = select_inbox(&items, read_only, older, now);
    if chosen.is_empty() {
        println!("Nothing to remove.");
        return Ok(());
    }
    println!(
        "{} {} inbox item{}:",
        if yes { "Removing" } else { "Would remove" },
        chosen.len(),
        if chosen.len() == 1 { "" } else { "s" }
    );
    for item in &chosen {
        let what = if item.kind == "text" {
            format!("text, {} characters", item.text.chars().count())
        } else {
            item.name.clone()
        };
        let mut line = format!(
            "  {} from {} · received {}{}",
            what,
            short_host(&item.sender),
            ago(now, item.received),
            if item.read { " · read" } else { " · unread" }
        );
        if item.kind == "image" {
            match owned_file(item, &received_dir) {
                Some(file) => line.push_str(&format!("\n      and the file {}", file.display())),
                None if !item.path.is_empty() => {
                    line.push_str(" (its file is kept: not in Downloads/nirushare)")
                }
                None => {}
            }
        }
        println!("{line}");
    }
    if !yes {
        println!("Nothing was removed. Run the same command with --yes to remove these.");
        return Ok(());
    }
    let mut failed = 0;
    for item in &chosen {
        if let Err(e) = remove_inbox_item(&inbox, item, &received_dir) {
            eprintln!("nirushare: {e}");
            failed += 1;
        }
    }
    if failed > 0 {
        return Err(format!("{failed} item(s) could not be removed"));
    }
    println!("Removed {}.", chosen.len());
    Ok(())
}
/// Copy and Open mark an item read, so `inbox --clear-read` can find it.
fn mark_read(id: &str) {
    let Ok(mut item) = read_item(id) else {
        return;
    };
    if item.read {
        return;
    }
    item.read = true;
    let meta = data_dir().join("inbox").join(format!("{id}.json"));
    let tmp = meta.with_extension("part");
    if let Ok(data) = serde_json::to_vec_pretty(&item) {
        if fs::write(&tmp, data).is_ok() && private_file(&tmp).is_ok() {
            let _ = fs::rename(&tmp, &meta);
        }
    }
}
fn copy_item(id: &str) -> Result<(), String> {
    let item = read_item(id)?;
    if item.kind == "text" {
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        child
            .stdin
            .as_mut()
            .ok_or("xclip stdin")?
            .write_all(item.text.as_bytes())
            .map_err(|e| e.to_string())?;
        child.wait().map_err(|e| e.to_string())?;
    } else if !item.path.is_empty() {
        let _ = Command::new("xclip")
            .args([
                "-selection",
                "clipboard",
                "-t",
                "image/png",
                "-i",
                &item.path,
            ])
            .status();
    }
    mark_read(id);
    Ok(())
}
fn read_item(id: &str) -> Result<InboxItem, String> {
    if !valid_id(id) {
        return Err("invalid item id".into());
    }
    let path = data_dir().join("inbox").join(format!("{id}.json"));
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
fn open_item(id: &str) -> Result<(), String> {
    let item = read_item(id)?;
    if item.path.is_empty() {
        return Err("text item has no file".into());
    }
    Command::new("xdg-open")
        .arg(item.path)
        .status()
        .map_err(|e| e.to_string())?;
    mark_read(id);
    Ok(())
}
fn email() -> Result<(), String> {
    let (kind, _, payload) = read_clipboard()?;
    if kind == "text" {
        let body = String::from_utf8_lossy(&payload).to_string();
        let uri = format!(
            "mailto:?subject={}&body={}",
            url_encode("NIRU Share"),
            url_encode(&body)
        );
        Command::new("xdg-email")
            .arg(uri)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let dir = state_dir().join("email");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("share-{}.png", now_id()));
    fs::write(&path, &payload).map_err(|e| e.to_string())?;
    Command::new("xdg-email")
        .args(["--subject", "NIRU Share", "--attach"])
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--version") => {
            println!("NIRUSHARE {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help") | None => {
            println!("NIRUSHARE {}\nUsage: nirushare serve|devices|trust HOST TOKEN|revoke HOST|token|send --to HOST|--all [--file PATH|--stdin]|inbox [--clear-read] [--older-than 30d] [--yes]|outbox|status [--json|--text]|copy ID|open ID|email", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("serve") => serve(),
        Some("devices") => list_devices(),
        Some("token") => {
            println!("{}", token());
            Ok(())
        }
        Some("trust") if args.len() == 2 => {
            let value = std::env::var("NIRUSHARE_TOKEN").unwrap_or_default();
            trust(&args[1], &value)
        }
        Some("trust") if args.len() == 3 => trust(&args[1], &args[2]),
        Some("revoke") if args.len() == 2 => revoke(&args[1]),
        Some("send") => send(&args[1..]),
        Some("inbox") => inbox_cleanup(&args[1..]),
        Some("status") => status(&args[1..]),
        Some("outbox") => outbox(),
        Some("copy") if args.len() == 2 => copy_item(&args[1]),
        Some("open") if args.len() == 2 => open_item(&args[1]),
        Some("email") => email(),
        Some(other) => Err(format!("unknown command: {other}")),
    };
    if let Err(e) = result {
        eprintln!("nirushare: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private scratch directory, removed again when the test ends.
    struct TempDir(PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    impl std::ops::Deref for TempDir {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }
    fn temp_dir(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "nirushare-test-{}-{tag}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn recipient(host: &str, status: &str) -> Recipient {
        Recipient {
            host: host.into(),
            token: String::new(),
            status: status.into(),
            attempts: 0,
            last_error: String::new(),
            next_attempt: 0,
            backoff: 0,
        }
    }
    fn queue_entry(out: &Path, id: &str, created: u64, recipients: Vec<Recipient>) -> PathBuf {
        let dir = out.join(id);
        fs::create_dir_all(&dir).unwrap();
        let o = Outgoing {
            id: id.into(),
            kind: "text".into(),
            name: "clipboard.txt".into(),
            digest: digest(b"hello"),
            created,
            recipients,
        };
        write_meta(&dir, &o).unwrap();
        dir
    }
    /// A payload that cannot be read as a file: reading it fails even as root.
    fn unreadable_payload(dir: &Path) {
        fs::create_dir_all(dir.join("payload")).unwrap();
    }
    fn trusted_cfg(hosts: &[&str]) -> Config {
        Config {
            version: 1,
            trusted: hosts
                .iter()
                .map(|h| Trusted {
                    host: h.to_string(),
                    token: "ab".repeat(24),
                })
                .collect(),
        }
    }
    fn online(host: &str) -> Peer {
        Peer {
            host: host.into(),
            ip: "127.0.0.1".into(),
            port: PORT,
            online: true,
        }
    }
    fn read_meta(dir: &Path) -> Outgoing {
        serde_json::from_slice(&fs::read(dir.join("meta.json")).unwrap()).unwrap()
    }

    #[test]
    fn final_entries_are_archived_without_reading_the_payload() {
        let root = temp_dir("final");
        let out = root.join("out");
        let history = root.join("history.jsonl");
        let now = 1_800_000_000;
        let done = queue_entry(
            &out,
            "1-done",
            now - 10,
            vec![
                recipient("othala", "delivered"),
                recipient("void", "cancelled"),
            ],
        );
        unreadable_payload(&done);
        let cfg = trusted_cfg(&["othala", "void"]);
        let mut peer_calls = 0;
        let mut peers = || {
            peer_calls += 1;
            Ok(vec![online("othala")])
        };
        let mut sends = 0;
        let mut deliver = |_: &Delivery| -> Result<(), DeliverError> {
            sends += 1;
            Ok(())
        };
        let mut env = QueueEnv {
            cfg: &cfg,
            now,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: None,
        };
        process_queue(&out, &history, &mut env);
        assert!(!done.exists(), "final entry and its payload are removed");
        assert_eq!(sends, 0);
        assert_eq!(peer_calls, 0, "no Tailscale query without due recipients");
        let records = read_history(&history);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"], "1-done");
        assert_eq!(records[0]["recipients"][0]["status"], "delivered");
        assert!(records[0].get("payload").is_none());
    }

    #[test]
    fn payload_is_read_only_for_a_send_attempt() {
        let root = temp_dir("lazy");
        let out = root.join("out");
        let history = root.join("history.jsonl");
        let now = 1_800_000_000;
        // void is offline: its unreadable payload must not matter yet.
        let waiting = queue_entry(&out, "2-wait", now, vec![recipient("void", "pending")]);
        unreadable_payload(&waiting);
        let cfg = trusted_cfg(&["void"]);
        let mut peers = || Ok(vec![]);
        let mut deliver = |_: &Delivery| -> Result<(), DeliverError> { panic!("no send") };
        let mut env = QueueEnv {
            cfg: &cfg,
            now,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: None,
        };
        process_queue(&out, &history, &mut env);
        assert_eq!(read_meta(&waiting).recipients[0].status, "waiting");
    }

    #[test]
    fn queue_state_transitions() {
        let root = temp_dir("states");
        let out = root.join("out");
        let history = root.join("history.jsonl");
        let now = 1_800_000_000;
        let dir = queue_entry(
            &out,
            "3-mixed",
            now,
            vec![
                recipient("othala", "pending"),
                recipient("algiz", "pending"),
                recipient("gone", "pending"),
            ],
        );
        fs::write(dir.join("payload"), b"hello").unwrap();
        let cfg = trusted_cfg(&["othala", "algiz"]);
        let mut peers = || Ok(vec![online("othala"), online("algiz")]);
        let mut deliver = |d: &Delivery| -> Result<(), DeliverError> {
            assert_eq!(d.payload, b"hello");
            if d.peer.host == "othala" {
                Ok(())
            } else {
                Err(DeliverError {
                    message: "algiz is offline".into(),
                    permanent: false,
                })
            }
        };
        let mut env = QueueEnv {
            cfg: &cfg,
            now,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: None,
        };
        process_queue(&out, &history, &mut env);
        let meta = read_meta(&dir);
        let by = |h: &str| meta.recipients.iter().find(|r| r.host == h).unwrap();
        assert_eq!(by("othala").status, "delivered");
        assert_eq!(by("algiz").status, "waiting");
        assert_eq!(by("algiz").attempts, 1);
        assert_eq!(by("algiz").last_error, "algiz is offline");
        assert_eq!(by("gone").status, "rejected");
        assert!(dir.join("payload").exists());

        // A permanent failure makes every recipient final: archived, payload gone.
        let mut peers = || Ok(vec![online("algiz")]);
        let mut deliver = |_: &Delivery| -> Result<(), DeliverError> {
            Err(DeliverError {
                message: "too large for algiz".into(),
                permanent: true,
            })
        };
        let mut env = QueueEnv {
            cfg: &cfg,
            now: now + 1000,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: None,
        };
        process_queue(&out, &history, &mut env);
        assert!(!dir.exists());
        let records = read_history(&history);
        assert_eq!(records[0]["recipients"][1]["status"], "failed");
    }

    #[test]
    fn legacy_entries_without_created_expire_by_directory_age() {
        let root = temp_dir("legacy");
        let out = root.join("out");
        let history = root.join("history.jsonl");
        let fresh = queue_entry(&out, "4-fresh", 0, vec![recipient("void", "waiting")]);
        let old = queue_entry(&out, "5-old", 0, vec![recipient("void", "waiting")]);
        unreadable_payload(&old);
        let mtime = entry_mtime(&fresh).unwrap();
        let cfg = trusted_cfg(&["void"]);
        let mut peers = || Ok(vec![]);
        let mut deliver = |_: &Delivery| -> Result<(), DeliverError> { panic!("no send") };
        // Shortly after: the fresh entry gets its directory time written back once.
        let mut env = QueueEnv {
            cfg: &cfg,
            now: mtime + 60,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: None,
        };
        process_queue(&out, &history, &mut env);
        assert_eq!(read_meta(&fresh).created, mtime);
        // Past the expiry window, both legacy entries expire and are archived.
        let mut env = QueueEnv {
            cfg: &cfg,
            now: mtime + EXPIRY_SECS + 61,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: None,
        };
        process_queue(&out, &history, &mut env);
        assert!(!fresh.exists() && !old.exists());
        let records = read_history(&history);
        assert_eq!(records.len(), 2);
        for r in &records {
            assert_eq!(r["recipients"][0]["status"], "expired");
            assert!(r["created"].as_u64().unwrap() > 0);
        }
    }

    #[test]
    fn backoff_doubles_from_five_seconds_to_five_minutes() {
        let seq: Vec<u64> = (0..10).map(backoff_secs).collect();
        assert_eq!(seq, [0, 5, 10, 20, 40, 80, 160, 300, 300, 300]);
        assert_eq!(backoff_secs(u32::MAX), 300);
    }

    #[test]
    fn offline_recipients_back_off_and_query_tailscale_once_per_pass() {
        let root = temp_dir("backoff");
        let out = root.join("out");
        let history = root.join("history.jsonl");
        let now = 1_800_000_000;
        let a = queue_entry(&out, "6-a", now, vec![recipient("void", "pending")]);
        let b = queue_entry(&out, "7-b", now, vec![recipient("void", "pending")]);
        let cfg = trusted_cfg(&["void"]);
        let calls = std::cell::Cell::new(0);
        let mut peers = || {
            calls.set(calls.get() + 1);
            Ok(vec![])
        };
        let mut deliver = |_: &Delivery| -> Result<(), DeliverError> { panic!("offline") };
        let mut pass = |at: u64| {
            let mut env = QueueEnv {
                cfg: &cfg,
                now: at,
                peers: &mut peers,
                deliver: &mut deliver,
                contacts: None,
            };
            process_queue(&out, &history, &mut env)
        };
        assert_eq!(pass(now), Some(now + 5));
        assert_eq!(calls.get(), 1, "one Tailscale query for two entries");
        // Not due yet: no query at all.
        assert_eq!(pass(now + 4), Some(now + 5));
        assert_eq!(calls.get(), 1);
        assert_eq!(pass(now + 5), Some(now + 15));
        assert_eq!(pass(now + 15), Some(now + 35));
        assert_eq!(calls.get(), 3);
        let r = &read_meta(&a).recipients[0];
        assert_eq!(
            (r.status.as_str(), r.backoff, r.next_attempt),
            ("waiting", 3, now + 35)
        );
        assert_eq!(read_meta(&b).recipients[0].backoff, 3);
        // Far in the backoff the delay is capped at five minutes.
        let mut at = now + 35;
        let mut gap = 0;
        for _ in 0..10 {
            let next = pass(at).unwrap();
            gap = next - at;
            at = next;
        }
        assert_eq!(gap, 300);
    }

    #[test]
    fn an_empty_queue_has_no_deadline() {
        let root = temp_dir("empty");
        let out = root.join("out");
        fs::create_dir_all(&out).unwrap();
        let cfg = trusted_cfg(&[]);
        let mut peers = || -> Result<Vec<Peer>, String> { panic!("no Tailscale query when idle") };
        let mut deliver = |_: &Delivery| -> Result<(), DeliverError> { panic!("no send") };
        let mut env = QueueEnv {
            cfg: &cfg,
            now: 1,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: None,
        };
        assert_eq!(process_queue(&out, &root.join("h"), &mut env), None);
    }

    #[test]
    fn wake_socket_ends_the_idle_wait() {
        let root = temp_dir("wake");
        let path = root.join("wake");
        let sock = UnixDatagram::bind(&path).unwrap();
        let sender = UnixDatagram::unbound().unwrap();
        sender.send_to(b"q", &path).unwrap();
        sender.send_to(b"q", &path).unwrap();
        let start = std::time::Instant::now();
        wait_for_wake(&sock, Some(Duration::from_secs(30)));
        assert!(start.elapsed() < Duration::from_secs(2));
        // Both datagrams were drained: the next wait times out.
        let start = std::time::Instant::now();
        wait_for_wake(&sock, Some(Duration::from_millis(200)));
        assert!(start.elapsed() >= Duration::from_millis(150));
    }

    #[test]
    fn token_compare_is_exact_and_checks_every_record() {
        assert!(secrets_equal("abc", "abc"));
        assert!(!secrets_equal("abc", "abd"));
        assert!(!secrets_equal("abc", "abcd"));
        assert!(!secrets_equal("", "a"));
        let long = "f".repeat(48);
        assert!(!secrets_equal(&long, &long[..47]));
        let mut cfg = trusted_cfg(&["othala", "void"]);
        cfg.trusted[1].token = "cd".repeat(24);
        assert_eq!(
            credentials(&cfg, &"ab".repeat(24)).as_deref(),
            Some("othala")
        );
        assert_eq!(credentials(&cfg, &"cd".repeat(24)).as_deref(), Some("void"));
        assert_eq!(credentials(&cfg, &"cd".repeat(23)), None);
        assert_eq!(credentials(&cfg, ""), None);
        let empty = Config {
            version: 1,
            trusted: vec![Trusted {
                host: "x".into(),
                token: String::new(),
            }],
        };
        assert_eq!(
            credentials(&empty, ""),
            None,
            "an empty token never matches"
        );
    }

    #[test]
    fn dedup_receipts_older_than_30_days_are_pruned() {
        let root = temp_dir("dedup");
        let dir = root.join("othala_ts_net");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1-a.json"), "{}").unwrap();
        fs::write(dir.join("2-b.json"), "{}").unwrap();
        fs::write(dir.join("notes.txt"), "keep").unwrap();
        fs::write(root.join("stray.json"), "{}").unwrap();
        let mtime = entry_mtime(&dir.join("1-a.json")).unwrap();
        assert_eq!(
            prune_dedup(&root, mtime + DEDUP_KEEP_SECS - 10, DEDUP_KEEP_SECS),
            0
        );
        assert!(dir.join("1-a.json").exists());
        assert_eq!(
            prune_dedup(&root, mtime + DEDUP_KEEP_SECS + 10, DEDUP_KEEP_SECS),
            2
        );
        assert!(!dir.join("1-a.json").exists() && !dir.join("2-b.json").exists());
        assert!(dir.join("notes.txt").exists() && root.join("stray.json").exists());
        assert_eq!(prune_dedup(&root.join("missing"), 0, 0), 0);
    }

    #[test]
    fn status_tells_the_truth_per_recipient_and_device() {
        let root = temp_dir("status");
        let out = root.join("out");
        let history = root.join("history.jsonl");
        let contacts = root.join("contacts.json");
        let now = 1_800_000_000;
        let dir = queue_entry(
            &out,
            "8-status",
            now,
            vec![
                recipient("othala.tail.ts.net", "pending"),
                recipient("void.tail.ts.net", "pending"),
            ],
        );
        fs::write(dir.join("payload"), b"hello").unwrap();
        let cfg = trusted_cfg(&[
            "othala.tail.ts.net",
            "void.tail.ts.net",
            "algiz.tail.ts.net",
        ]);
        let mut peers = || {
            Ok(vec![
                Peer {
                    online: false,
                    ..online("othala.tail.ts.net")
                },
                online("void.tail.ts.net"),
            ])
        };
        let mut deliver = |d: &Delivery| -> Result<(), DeliverError> {
            assert_eq!(d.peer.host, "void.tail.ts.net");
            Ok(())
        };
        let mut env = QueueEnv {
            cfg: &cfg,
            now,
            peers: &mut peers,
            deliver: &mut deliver,
            contacts: Some(&contacts),
        };
        process_queue(&out, &history, &mut env);
        record_contact(&contacts, "algiz.tail.ts.net", "received", now - 7200);
        let peers = Ok(vec![online("void.tail.ts.net")]);
        let v = status_json(
            &cfg,
            &peers,
            &read_outgoing(&out),
            &read_history(&history),
            &read_contacts(&contacts),
            now + 2,
        );
        let r = &v["outgoing"][0]["recipients"];
        assert_eq!(r[0]["state"], "waiting");
        assert_eq!(r[0]["last_error"], "othala is offline");
        assert_eq!(r[0]["next_retry"], now + 5);
        assert_eq!(r[1]["state"], "delivered");
        assert_eq!(v["devices"][1]["online"], true);
        assert_eq!(v["devices"][1]["last_sent"], now);
        assert_eq!(v["devices"][2]["last_contact"], now - 7200);
        let text = status_text(&v, now + 2);
        assert!(text.contains("othala is offline"), "{text}");
        assert!(text.contains("next try in 3 s"), "{text}");
        assert!(text.contains("last contact 2 h ago"), "{text}");
        assert!(text.contains("last contact never"), "{text}");
    }

    #[test]
    fn failures_are_described_in_plain_words() {
        let status = |code| {
            let e = ureq::Error::Status(code, ureq::Response::new(code, "x", "").unwrap());
            let d = describe_failure("othala.tail.ts.net", &e);
            (d.message, d.permanent)
        };
        assert_eq!(status(401), ("not trusted on othala".to_string(), false));
        assert_eq!(status(413), ("too large for othala".to_string(), true));
        assert!(!status(500).1 && !status(503).1);
        let refused = ureq::get("http://127.0.0.1:9/").call().unwrap_err();
        assert_eq!(
            describe_failure("othala", &refused).message,
            "NIRU Share is not running on othala"
        );
    }

    #[test]
    fn inbox_cleanup_selects_previews_and_keeps_foreign_files() {
        assert_eq!(parse_age("30d"), Some(30 * 86_400));
        assert_eq!(parse_age("12h"), Some(12 * 3600));
        assert_eq!(parse_age("2w"), Some(14 * 86_400));
        assert_eq!(parse_age("7"), Some(7 * 86_400));
        for bad in ["", "d", "3m", "-1d", "1.5d"] {
            assert_eq!(parse_age(bad), None, "{bad}");
        }
        let root = temp_dir("inbox");
        let inbox = root.join("inbox");
        let received = root.join("Downloads/nirushare");
        fs::create_dir_all(&inbox).unwrap();
        fs::create_dir_all(&received).unwrap();
        let now = 1_800_000_000;
        let item = |id: &str, kind: &str, age: u64, read: bool, path: &Path| InboxItem {
            id: id.into(),
            kind: kind.into(),
            name: "x.png".into(),
            sender: "othala".into(),
            received: now - age,
            path: path.display().to_string(),
            text: String::new(),
            read,
        };
        let mine = received.join("1-a.png");
        fs::write(&mine, b"png").unwrap();
        let foreign = root.join("elsewhere.png");
        fs::write(&foreign, b"png").unwrap();
        let link = received.join("link.png");
        std::os::unix::fs::symlink(&foreign, &link).unwrap();
        let items = vec![
            item("1-a", "image", 40 * 86_400, true, &mine),
            item("2-b", "image", 40 * 86_400, true, &foreign),
            item("3-c", "image", 40 * 86_400, false, &link),
            item("4-d", "text", 86_400, true, Path::new("")),
        ];
        for i in &items {
            fs::write(
                inbox.join(format!("{}.json", i.id)),
                serde_json::to_vec(i).unwrap(),
            )
            .unwrap();
        }
        let ids = |v: Vec<&InboxItem>| v.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids(select_inbox(&items, true, None, now)),
            ["1-a", "2-b", "4-d"]
        );
        assert_eq!(
            ids(select_inbox(&items, false, Some(30 * 86_400), now)),
            ["1-a", "2-b", "3-c"]
        );
        assert_eq!(
            ids(select_inbox(&items, true, Some(30 * 86_400), now)),
            ["1-a", "2-b"]
        );
        assert_eq!(owned_file(&items[0], &received), Some(mine.clone()));
        assert_eq!(owned_file(&items[1], &received), None, "outside the folder");
        assert_eq!(owned_file(&items[2], &received), None, "a symlink");
        for i in &items[..3] {
            remove_inbox_item(&inbox, i, &received).unwrap();
        }
        assert!(!mine.exists(), "the owned received file is removed");
        assert!(foreign.exists() && fs::symlink_metadata(&link).is_ok());
        let left: Vec<_> = load_inbox(&inbox).into_iter().map(|i| i.id).collect();
        assert_eq!(left, ["4-d"]);
    }

    #[test]
    fn transfer_ids_reject_paths_and_junk() {
        assert!(valid_id("1793467977-0d11e618"));
        assert!(valid_id("abc_DEF-123"));
        for bad in ["", "../x", "a/b", "a\\b", "a.json", "..", "a\0b"] {
            assert!(!valid_id(bad), "accepted {bad:?}");
        }
        assert!(!valid_id(&"a".repeat(97)));
    }

    #[test]
    fn host_names_are_bounded() {
        assert!(valid_host("othala.tailnet.ts.net"));
        assert!(valid_host("othala"));
        for bad in ["", "a/b", "a b", "..", "a\0b"] {
            assert!(!valid_host(bad), "accepted {bad:?}");
        }
    }

    #[test]
    fn image_type_comes_from_bytes() {
        assert_eq!(image_extension(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(image_extension(b"\xff\xd8\xffrest"), Some("jpg"));
        assert_eq!(image_extension(b"RIFF0000WEBPrest"), Some("webp"));
        assert_eq!(image_extension(b"<svg>"), None);
        assert_eq!(image_extension(b"<html>"), None);
    }

    #[test]
    fn encoding_is_utf8_and_percent_safe() {
        assert_eq!(url_encode("NIRU Share"), "NIRU%20Share");
        assert_eq!(url_encode("åäö"), "%C3%A5%C3%A4%C3%B6");
    }
}
