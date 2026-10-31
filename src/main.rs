use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
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
fn tailscale() -> Result<(Option<Peer>, Vec<Peer>), String> {
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
        online: true,
    });
    let mut peers = Vec::new();
    if let Some(obj) = status.peers.as_ref().and_then(|v| v.as_object()) {
        for value in obj.values() {
            if let Ok(p) = serde_json::from_value::<TailscalePeer>(value.clone()) {
                peers.push(Peer {
                    host: host_of(&p),
                    ip: ip_of(&p),
                    online: p.online.unwrap_or(false),
                });
            }
        }
    }
    peers.sort_by(|a, b| a.host.cmp(&b.host));
    Ok((own, peers))
}
fn whois_hosts(ip: &str) -> Result<Vec<String>, String> {
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

fn credentials(cfg: &Config, token: &str) -> Option<String> {
    cfg.trusted
        .iter()
        .find(|t| t.token == token)
        .map(|t| t.host.clone())
}
fn incoming_notice(item: &InboxItem) {
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

fn header_value(request: &tiny_http::Request, name: &str) -> String {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default()
}

fn handle_share(mut request: tiny_http::Request) -> Result<(), String> {
    let cfg = load_config()?;
    let auth = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Authorization"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();
    let token = auth.strip_prefix("Bearer ").unwrap_or("");
    let sender = credentials(&cfg, token).ok_or_else(|| "unauthorized".to_string())?;
    let remote = request
        .remote_addr()
        .ok_or_else(|| "missing peer address".to_string())?
        .ip()
        .to_string();
    verify_remote(&sender, &remote)?;
    let id = header_value(&request, "X-Nirushare-Transfer");
    let kind = header_value(&request, "X-Nirushare-Kind");
    let name = header_value(&request, "X-Nirushare-Name");
    let announced = header_value(&request, "X-Nirushare-Digest");
    if !valid_id(&id) || !matches!(kind.as_str(), "text" | "image") {
        return Err("bad transfer metadata".into());
    }
    let mut body = Vec::new();
    request
        .as_reader()
        .take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    let limit = if kind == "text" {
        MAX_TEXT_BYTES
    } else {
        MAX_BYTES
    };
    if body.len() > limit {
        return Err("payload too large".into());
    }
    let digest = digest(&body);
    if !announced.is_empty() && !announced.eq_ignore_ascii_case(&digest) {
        return Err("payload digest mismatch".into());
    }
    let dedup_dir = data_dir().join("dedup").join(sender.replace('.', "_"));
    private_dir(&dedup_dir)?;
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
            return Err("transfer id conflict".into());
        }
        let response = tiny_http::Response::from_string("{\"ok\":true,\"duplicate\":true}")
            .with_header(
                tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
                    .unwrap(),
            );
        let _ = request.respond(response);
        return Ok(());
    }
    let stamp = now_secs();
    let display = if name.is_empty() {
        format!("{kind}-{stamp}")
    } else {
        name.clone()
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
        let ext = image_extension(&body).ok_or_else(|| "unsupported image content".to_string())?;
        let dir = downloads_dir().join("nirushare");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let filename = format!("{stamp}-{}.{}", id.replace('-', ""), ext);
        let dst = dir.join(filename);
        let tmp = dir.join(format!(".{}.part", id.replace('-', "")));
        fs::write(&tmp, &body).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &dst).map_err(|e| e.to_string())?;
        path = dst.display().to_string();
    } else {
        text = String::from_utf8_lossy(&body).to_string();
    }
    let item = InboxItem {
        id: id.clone(),
        kind: kind.clone(),
        name: safe,
        sender: sender.clone(),
        received: stamp,
        path,
        text,
        read: false,
    };
    let meta = data_dir().join("inbox").join(format!("{id}.json"));
    let tmp = meta.with_extension("part");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(&item).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    private_file(&tmp)?;
    fs::rename(&tmp, &meta).map_err(|e| e.to_string())?;
    private_file(&meta)?;
    fs::write(
        &dedup,
        serde_json::to_vec(&serde_json::json!({"id": id, "digest": digest}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    private_file(&dedup)?;
    incoming_notice(&item);
    let response = tiny_http::Response::from_string("{\"ok\":true}").with_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
    );
    let _ = request.respond(response);
    Ok(())
}
fn serve() -> Result<(), String> {
    let (own, _) = tailscale()?;
    let own = own.ok_or("no Tailscale identity")?;
    if own.ip.is_empty() {
        return Err("no Tailscale IPv4 address".into());
    }
    let addr = format!("{}:{}", own.ip, PORT);
    let server = tiny_http::Server::http(&addr).map_err(|e| format!("bind {addr}: {e}"))?;
    thread::spawn(queue_worker);
    eprintln!("nirushare listening on {addr}");
    for request in server.incoming_requests() {
        if request.method() == &tiny_http::Method::Get && request.url() == "/health" {
            let _ = request.respond(tiny_http::Response::from_string(
                "{\"ok\":true,\"protocol\":1}",
            ));
            continue;
        }
        if request.method() == &tiny_http::Method::Post && request.url().starts_with("/share") {
            if let Err(e) = handle_share(request) {
                eprintln!("share rejected: {e}");
            }
            continue;
        }
        let _ = request
            .respond(tiny_http::Response::from_string("{\"ok\":false}").with_status_code(404));
    }
    Ok(())
}

fn queue_worker() {
    loop {
        let _ = process_queue();
        thread::sleep(Duration::from_secs(5));
    }
}
fn process_queue() -> Result<(), String> {
    let cfg = load_config()?;
    let peers = tailscale().map(|(_, p)| p).unwrap_or_default();
    let dir = state_dir().join("out");
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let meta_path = path.join("meta.json");
        let payload_path = path.join("payload");
        let Ok(meta_bytes) = fs::read(&meta_path) else {
            continue;
        };
        let Ok(mut outgoing) = serde_json::from_slice::<Outgoing>(&meta_bytes) else {
            continue;
        };
        let Ok(payload) = fs::read(&payload_path) else {
            continue;
        };
        if outgoing.created > 0 && now_secs().saturating_sub(outgoing.created) > EXPIRY_SECS {
            for r in &mut outgoing.recipients {
                if r.status != "delivered" && r.status != "cancelled" {
                    r.status = "expired".into();
                    r.last_error.clear();
                }
            }
            let _ = fs::write(
                path.join("meta.json"),
                serde_json::to_vec_pretty(&outgoing).unwrap_or_default(),
            );
            continue;
        }
        let mut changed = outgoing.recipients.iter().any(|r| !r.token.is_empty());
        for r in &mut outgoing.recipients {
            if r.status == "delivered" || r.status == "cancelled" {
                continue;
            }
            let Some(trusted) = cfg.trusted.iter().find(|t| t.host == r.host) else {
                r.status = "rejected".into();
                r.last_error = "not trusted".into();
                changed = true;
                continue;
            };
            let Some(peer) = find_peer(&peers, &r.host) else {
                if r.status != "waiting" {
                    r.status = "waiting".into();
                    changed = true;
                }
                continue;
            };
            if !peer.online || peer.ip.is_empty() {
                if r.status != "waiting" {
                    r.status = "waiting".into();
                    changed = true;
                }
                continue;
            }
            let url = format!("http://{}:{}/share", peer.ip, PORT);
            let res = ureq::post(&url)
                .set("Authorization", &format!("Bearer {}", trusted.token))
                .set("X-Nirushare-Transfer", &outgoing.id)
                .set("X-Nirushare-Kind", &outgoing.kind)
                .set("X-Nirushare-Name", &outgoing.name)
                .set("X-Nirushare-Digest", &outgoing.digest)
                .timeout(Duration::from_secs(30))
                .send_bytes(&payload);
            match res {
                Ok(_) => {
                    r.status = "delivered".into();
                    r.last_error.clear();
                }
                Err(e) => {
                    r.attempts += 1;
                    r.last_error = e.to_string();
                    r.status = "waiting".into();
                }
            }
            changed = true;
        }
        if changed {
            let _ = fs::write(
                path.join("meta.json"),
                serde_json::to_vec_pretty(&outgoing).unwrap_or_default(),
            );
        }
    }
    Ok(())
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
    } else {
        read_clipboard()?
    };
    let id = now_id();
    let dir = state_dir().join("out").join(&id);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("payload"), &payload).map_err(|e| e.to_string())?;
    let outgoing = Outgoing {
        id: id.clone(),
        kind,
        name,
        digest: digest(&payload),
        created: now_secs(),
        recipients,
    };
    fs::write(
        dir.join("meta.json"),
        serde_json::to_vec_pretty(&outgoing).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
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

fn status() -> Result<(), String> {
    let cfg = load_config()?;
    println!(
        "{}",
        serde_json::json!({"trusted": cfg.trusted.len(), "devices": online_status(&cfg)})
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
fn list_inbox() -> Result<(), String> {
    ensure_dirs()?;
    let mut items = Vec::new();
    if let Ok(rd) = fs::read_dir(data_dir().join("inbox")) {
        for e in rd.flatten() {
            if let Ok(bytes) = fs::read(e.path()) {
                if let Ok(item) = serde_json::from_slice::<InboxItem>(&bytes) {
                    items.push(item);
                }
            }
        }
    }
    items.sort_by_key(|i| i.received);
    println!(
        "{}",
        serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?
    );
    Ok(())
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
#[cfg(test)]
mod tests {
    use super::*;

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--version") => {
            println!("NIRUSHARE {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("--help") | None => {
            println!("NIRUSHARE {}\nUsage: nirushare serve|devices|trust HOST TOKEN|revoke HOST|token|send --to HOST|--all|--file PATH|inbox|status|copy ID|open ID|email", env!("CARGO_PKG_VERSION"));
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
        Some("inbox") => list_inbox(),
        Some("status") => status(),
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
