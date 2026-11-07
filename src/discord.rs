//! Discord webhooks: send text, links, screenshots, images and files to a
//! Discord channel through a webhook the user configured.
//!
//! Rules (see README, "Discord"):
//! - No webhook exists until the user adds one to
//!   `~/.config/nirushare/webhooks.toml`. The file must be private (0600); a
//!   file of ours that is readable by others is repaired and reported, one
//!   that belongs to someone else is refused.
//! - A webhook URL is a credential. It is never printed, logged, written to
//!   the queue, passed on a command line or included in an error message.
//!   Everything outside this file refers to a webhook by its name.
//! - Nothing is sent without a preview that names the target: the CLI asks,
//!   the launcher and the NIRUSHOT review show a preview view first.
//! - Every message is sent with `?wait=true`, so Discord returns the message
//!   id; for `UNDO_SECS` after sending, the notification's Undo action (or
//!   `nirushare discord undo ID`) deletes the messages again.
//! - Mentions are never parsed: pasted text containing @everyone or a role
//!   mention cannot ping anyone.
use super::*;
use std::collections::BTreeMap;

/// Discord's limit for the `content` of one message.
pub(crate) const MESSAGE_LIMIT: usize = 2000;
/// Longer text than this many messages is attached as a .txt file instead.
pub(crate) const MAX_TEXT_MESSAGES: usize = 4;
/// Upload limit of a Discord server without boosts. A webhook may raise it
/// with `max_file_mb` when its server allows more.
pub(crate) const DEFAULT_MAX_FILE_MB: u32 = 10;
/// How long a sent item can be taken back.
pub(crate) const UNDO_SECS: u64 = 5 * 60;
/// A preview that was not confirmed within this time is discarded.
const STAGE_KEEP_SECS: u64 = 60 * 60;
/// Undo records are kept a day for `nirushare status`, then removed.
const SENT_KEEP_SECS: u64 = 24 * 60 * 60;
/// SUPPRESS_EMBEDS: no link previews under the message.
const FLAG_SUPPRESS_EMBEDS: u32 = 1 << 2;

// ── Configuration ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebhookFile {
    #[serde(default)]
    webhook: BTreeMap<String, WebhookEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WebhookEntry {
    url: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default = "yes")]
    embeds: bool,
    #[serde(default)]
    max_file_mb: Option<u32>,
}
fn yes() -> bool {
    true
}

/// One configured webhook. `Debug` is deliberately not derived: the URL must
/// never reach a log line through `{:?}`.
#[derive(Clone)]
pub(crate) struct Webhook {
    pub(crate) name: String,
    url: HookUrl,
    pub(crate) username: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) embeds: bool,
    pub(crate) max_file_mb: u32,
}

/// A parsed webhook URL: everything before the query, and an optional
/// thread id (`?thread_id=`) that must accompany every request.
#[derive(Clone, PartialEq)]
pub(crate) struct HookUrl {
    base: String,
    host: String,
    thread_id: Option<String>,
}

impl Webhook {
    pub(crate) fn url_host(&self) -> &str {
        self.url.host()
    }
}

impl HookUrl {
    /// The host only, for status text: "discord.com".
    pub(crate) fn host(&self) -> &str {
        &self.host
    }
    fn query(&self, wait: bool) -> String {
        let mut parts = Vec::new();
        if wait {
            parts.push("wait=true".to_string());
        }
        if let Some(t) = &self.thread_id {
            parts.push(format!("thread_id={t}"));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("?{}", parts.join("&"))
        }
    }
    fn execute(&self) -> String {
        format!("{}{}", self.base, self.query(true))
    }
    fn message(&self, id: &str) -> String {
        format!("{}/messages/{id}{}", self.base, self.query(false))
    }
    fn info(&self) -> String {
        self.base.clone()
    }
}

const DISCORD_HOSTS: &[&str] = &[
    "discord.com",
    "discordapp.com",
    "canary.discord.com",
    "ptb.discord.com",
];

/// Accept only real Discord webhook URLs:
/// `https://discord.com/api[/vN]/webhooks/<id>/<token>[?thread_id=<id>]`.
/// The reason for a refusal never repeats the URL.
pub(crate) fn parse_url(raw: &str) -> Result<HookUrl, &'static str> {
    let raw = raw.trim();
    let (scheme_ok, rest, local) = if let Some(rest) = raw.strip_prefix("https://") {
        (true, rest, false)
    } else if let Some(rest) = test_local(raw) {
        (true, rest, true)
    } else {
        (false, raw, false)
    };
    if !scheme_ok {
        return Err("the URL must start with https://");
    }
    let (before_query, query) = match rest.split_once('?') {
        Some((a, q)) => (a, Some(q)),
        None => (rest, None),
    };
    let (host, path) = before_query
        .split_once('/')
        .ok_or("the URL is not a Discord webhook URL")?;
    if !local && !DISCORD_HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
        return Err("the URL is not on discord.com");
    }
    let mut segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    if segments.first() != Some(&"api") {
        return Err("the URL is not a Discord webhook URL");
    }
    segments.remove(0);
    if segments.first().is_some_and(|s| {
        s.len() >= 2 && s.starts_with('v') && s[1..].bytes().all(|b| b.is_ascii_digit())
    }) {
        segments.remove(0);
    }
    let [kind, id, token] = segments.as_slice() else {
        return Err("the URL is not a Discord webhook URL");
    };
    if *kind != "webhooks" {
        return Err("the URL is not a Discord webhook URL");
    }
    if id.is_empty() || id.len() > 24 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err("the webhook id in the URL is not valid");
    }
    if token.len() < 20
        || token.len() > 128
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("the webhook token in the URL is not valid");
    }
    let mut thread_id = None;
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        for pair in q.split('&') {
            match pair.split_once('=') {
                Some(("thread_id", v))
                    if !v.is_empty() && v.len() <= 24 && v.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    thread_id = Some(v.to_string())
                }
                Some(("wait", _)) => {}
                _ => return Err("the URL has an unsupported query (only thread_id is allowed)"),
            }
        }
    }
    let scheme = if local { "http" } else { "https" };
    Ok(HookUrl {
        base: format!("{scheme}://{host}/{}", path.trim_end_matches('/')),
        host: host.to_ascii_lowercase(),
        thread_id,
    })
}

/// Tests may point a webhook at a mock server on 127.0.0.1 over plain HTTP.
/// Normal builds accept only https://discord.com.
#[cfg(feature = "test-transport")]
fn test_local(raw: &str) -> Option<&str> {
    raw.strip_prefix("http://")
        .filter(|rest| rest.starts_with("127.0.0.1:"))
}
#[cfg(not(feature = "test-transport"))]
fn test_local(_raw: &str) -> Option<&str> {
    None
}

pub(crate) fn webhooks_path() -> PathBuf {
    config_dir().join("webhooks.toml")
}

/// A valid webhook name: what follows `discord:`.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The user id of this process, without libc: the owner of /proc/self.
fn my_uid() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata("/proc/self").ok().map(|m| m.uid())
}

/// Check the file's owner and mode before reading it. Returns a warning when
/// the mode had to be repaired.
pub(crate) fn check_private(path: &Path) -> Result<Option<String>, String> {
    use std::os::unix::fs::MetadataExt;
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "{} is a symbolic link; webhooks.toml must be a regular private file",
            path.display()
        ));
    }
    if let Some(uid) = my_uid() {
        if meta.uid() != uid {
            return Err(format!(
                "{} belongs to another user; refusing to use it",
                path.display()
            ));
        }
    }
    if meta.mode() & 0o077 != 0 {
        private_file(path)?;
        return Ok(Some(format!(
            "{} was readable by other users; its permissions were set to 0600. \
             Anyone who read it can post to your channels: consider creating new webhooks in Discord.",
            path.display()
        )));
    }
    Ok(None)
}

/// Parse webhooks.toml text. Errors give the line, never the line's text.
pub(crate) fn parse_webhooks(text: &str) -> Result<Vec<Webhook>, String> {
    let file: WebhookFile = toml::from_str(text).map_err(|e| {
        let line = e
            .span()
            .map(|s| text[..s.start.min(text.len())].matches('\n').count() + 1);
        match line {
            Some(l) => format!("webhooks.toml: invalid TOML on line {l}"),
            None => "webhooks.toml: invalid TOML".to_string(),
        }
    })?;
    let mut hooks = Vec::new();
    for (name, entry) in file.webhook {
        if !valid_name(&name) {
            return Err(format!(
                "webhooks.toml: the name \"{}\" may only use letters, digits, - and _ (at most 32)",
                name.chars().take(40).collect::<String>()
            ));
        }
        let url = parse_url(&entry.url).map_err(|why| format!("webhooks.toml: {name}: {why}"))?;
        if let Some(a) = &entry.avatar_url {
            if !a.starts_with("https://") {
                return Err(format!(
                    "webhooks.toml: {name}: avatar_url must start with https://"
                ));
            }
        }
        if let Some(u) = &entry.username {
            let n = u.chars().count();
            if n == 0 || n > 80 {
                return Err(format!(
                    "webhooks.toml: {name}: username must be 1-80 characters"
                ));
            }
        }
        hooks.push(Webhook {
            name,
            url,
            username: entry.username.filter(|u| !u.trim().is_empty()),
            avatar_url: entry.avatar_url,
            embeds: entry.embeds,
            max_file_mb: entry
                .max_file_mb
                .unwrap_or(DEFAULT_MAX_FILE_MB)
                .clamp(1, 500),
        });
    }
    Ok(hooks)
}

/// The configured webhooks; none when the file does not exist.
pub(crate) fn load_webhooks() -> Result<Vec<Webhook>, String> {
    let path = webhooks_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    if let Some(warning) = check_private(&path)? {
        eprintln!("nirushare: {warning}");
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_webhooks(&text)
}

pub(crate) fn find_webhook(name: &str) -> Result<Webhook, String> {
    load_webhooks()?
        .into_iter()
        .find(|w| w.name == name)
        .ok_or_else(|| {
            format!(
                "no Discord webhook named \"{name}\" in {}",
                webhooks_path().display()
            )
        })
}

// ── Content ────────────────────────────────────────────────────────────────

/// Split text into Discord messages of at most `limit` UTF-16 units (how
/// Discord counts), at a line break when possible, else at a space.
pub(crate) fn split_text(text: &str, limit: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text.trim_end();
    while !rest.is_empty() {
        let units: usize = rest.chars().map(char::len_utf16).sum();
        if units <= limit {
            parts.push(rest.to_string());
            break;
        }
        // Byte index of the longest prefix that fits.
        let mut used = 0;
        let mut cut = 0;
        for (i, ch) in rest.char_indices() {
            if used + ch.len_utf16() > limit {
                break;
            }
            used += ch.len_utf16();
            cut = i + ch.len_utf8();
        }
        let window = &rest[..cut];
        let at = window
            .rfind('\n')
            .filter(|&i| i > cut / 2)
            .or_else(|| window.rfind(' ').filter(|&i| i > cut / 2))
            .map(|i| i + 1)
            .unwrap_or(cut);
        let (head, tail) = rest.split_at(at);
        let head = head.trim_end();
        if !head.is_empty() {
            parts.push(head.to_string());
        }
        rest = tail.trim_start_matches('\n');
    }
    parts
}

/// A single URL and nothing else is sent as a link.
pub(crate) fn is_link(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with("https://") || t.starts_with("http://"))
        && t.len() > 8
        && !t.chars().any(char::is_whitespace)
}

/// How an item will be posted.
#[derive(Debug, PartialEq)]
pub(crate) enum Plan {
    /// Text in this many messages.
    Messages(Vec<String>),
    /// One message with the payload attached under this file name.
    Attachment { filename: String, note: String },
}

pub(crate) fn plan(kind: &str, name: &str, payload: &[u8]) -> Plan {
    if kind == "text" || kind == "link" {
        let text = String::from_utf8_lossy(payload);
        let parts = split_text(&text, MESSAGE_LIMIT);
        if parts.len() <= MAX_TEXT_MESSAGES && !parts.is_empty() {
            return Plan::Messages(parts);
        }
        let chars = text.chars().count();
        return Plan::Attachment {
            filename: if name.ends_with(".txt") {
                safe_filename(name)
            } else {
                "text.txt".into()
            },
            note: format!("Text ({chars} characters) attached."),
        };
    }
    Plan::Attachment {
        filename: safe_filename(name),
        note: String::new(),
    }
}

/// A file name Discord accepts and that says nothing about the local path.
pub(crate) fn safe_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    let clean: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .take(100)
        .collect();
    let clean = clean.trim_start_matches('.').to_string();
    if clean.is_empty() {
        "file".into()
    } else {
        clean
    }
}

// ── Requests ───────────────────────────────────────────────────────────────

/// The result of one request, in words that never contain the URL.
#[derive(Debug, PartialEq)]
pub(crate) enum Failure {
    /// Rate limited: try again after this many seconds.
    RateLimited(u64),
    /// Worth retrying with backoff (5xx, network).
    Retry(String),
    /// Retrying cannot help.
    Permanent(String),
}

/// The wait Discord asks for after a 429: the Retry-After header or the
/// JSON `retry_after`, in seconds (fractions round up), at least 1 s.
pub(crate) fn retry_after(header: Option<&str>, body: &str) -> u64 {
    let from_header = header.and_then(|h| h.trim().parse::<f64>().ok());
    let from_body = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("retry_after").and_then(|r| r.as_f64()));
    let secs = from_header.or(from_body).unwrap_or(5.0);
    if !secs.is_finite() || secs < 0.0 {
        return 5;
    }
    (secs.ceil() as u64).clamp(1, 3600)
}

pub(crate) fn classify(code: u16, retry_header: Option<&str>, body: &str) -> Failure {
    match code {
        429 => Failure::RateLimited(retry_after(retry_header, body)),
        500..=599 => Failure::Retry(format!("Discord is having trouble (HTTP {code})")),
        401 | 403 => Failure::Permanent(
            "Discord refused the webhook: it was deleted or its token is wrong".into(),
        ),
        404 => {
            Failure::Permanent("the webhook does not exist (deleted, or the URL is wrong)".into())
        }
        413 => Failure::Permanent("too large for this Discord server".into()),
        400 => Failure::Permanent("Discord rejected the message (HTTP 400)".into()),
        _ => Failure::Permanent(format!("Discord answered HTTP {code}")),
    }
}

fn transport_failure(t: &ureq::Transport) -> Failure {
    // `Transport`'s Display includes the URL; only its kind is used here.
    let what = match t.kind() {
        ureq::ErrorKind::Dns => "Discord could not be found (DNS)",
        ureq::ErrorKind::ConnectionFailed => "could not connect to Discord",
        ureq::ErrorKind::TooManyRedirects => "Discord redirected too often",
        ureq::ErrorKind::Io => "the connection to Discord failed or timed out",
        ureq::ErrorKind::InvalidUrl | ureq::ErrorKind::UnknownScheme => {
            return Failure::Permanent("the webhook URL cannot be used".into())
        }
        _ => "the connection to Discord failed",
    };
    Failure::Retry(what.into())
}

fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(60))
            .timeout_write(Duration::from_secs(60))
            .redirects(0)
            .user_agent(concat!(
                "DiscordBot (https://github.com/nirucon/nirushare, ",
                env!("CARGO_PKG_VERSION"),
                ")"
            ))
            .build()
    })
}

fn failure_of(err: ureq::Error) -> Failure {
    match err {
        ureq::Error::Status(code, response) => {
            let header = response.header("retry-after").map(str::to_string);
            let body = response.into_string().unwrap_or_default();
            classify(code, header.as_deref(), &body)
        }
        ureq::Error::Transport(t) => transport_failure(&t),
    }
}

fn message_id(response: ureq::Response) -> Result<String, Failure> {
    let body = response
        .into_string()
        .map_err(|_| Failure::Retry("Discord's answer could not be read".into()))?;
    serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("id").and_then(|i| i.as_str()).map(str::to_string))
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| Failure::Permanent("Discord did not return a message id".into()))
}

/// The JSON part of a message.
pub(crate) fn payload_json(
    hook: &Webhook,
    content: &str,
    attachment: Option<&str>,
) -> serde_json::Value {
    let mut v = serde_json::json!({
        "content": content,
        "allowed_mentions": { "parse": [] },
    });
    if let Some(u) = &hook.username {
        v["username"] = u.clone().into();
    }
    if let Some(a) = &hook.avatar_url {
        v["avatar_url"] = a.clone().into();
    }
    if !hook.embeds {
        v["flags"] = FLAG_SUPPRESS_EMBEDS.into();
    }
    if let Some(name) = attachment {
        v["attachments"] = serde_json::json!([{ "id": 0, "filename": name }]);
    }
    v
}

fn post_text(hook: &Webhook, content: &str) -> Result<String, Failure> {
    let body = payload_json(hook, content, None).to_string();
    agent()
        .post(&hook.url.execute())
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(failure_of)
        .and_then(message_id)
}

pub(crate) fn multipart(boundary: &str, json: &str, filename: &str, data: &[u8]) -> Vec<u8> {
    let mut body = Vec::with_capacity(data.len() + json.len() + 512);
    let _ = write!(
        body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\
         Content-Type: application/json\r\n\r\n{json}\r\n\
         --{boundary}\r\nContent-Disposition: form-data; name=\"files[0]\"; filename=\"{filename}\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    );
    body.extend_from_slice(data);
    let _ = write!(body, "\r\n--{boundary}--\r\n");
    body
}

fn post_file(hook: &Webhook, note: &str, filename: &str, data: &[u8]) -> Result<String, Failure> {
    let json = payload_json(hook, note, Some(filename)).to_string();
    let boundary = format!("nirushare-{}", token());
    let body = multipart(&boundary, &json, filename, data);
    agent()
        .post(&hook.url.execute())
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(&body)
        .map_err(failure_of)
        .and_then(message_id)
}

fn delete_message(hook: &Webhook, id: &str) -> Result<(), Failure> {
    match agent().delete(&hook.url.message(id)).call() {
        Ok(_) => Ok(()),
        // Already gone (deleted by hand in Discord): nothing left to undo.
        Err(ureq::Error::Status(404, _)) => Ok(()),
        Err(e) => Err(failure_of(e)),
    }
}

// ── Queue delivery ─────────────────────────────────────────────────────────

/// What `process_queue` passes for a Discord recipient.
pub(crate) struct Job<'a> {
    pub(crate) id: &'a str,
    pub(crate) kind: &'a str,
    pub(crate) name: &'a str,
    pub(crate) payload: &'a [u8],
    pub(crate) now: u64,
}

/// Something that posts one part and returns its message id. The real one
/// talks to Discord; unit tests record calls.
pub(crate) trait Poster {
    fn text(&mut self, hook: &Webhook, content: &str) -> Result<String, Failure>;
    fn file(
        &mut self,
        hook: &Webhook,
        note: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<String, Failure>;
}
pub(crate) struct Http;
impl Poster for Http {
    fn text(&mut self, hook: &Webhook, content: &str) -> Result<String, Failure> {
        post_text(hook, content)
    }
    fn file(
        &mut self,
        hook: &Webhook,
        note: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<String, Failure> {
        post_file(hook, note, filename, data)
    }
}

/// Deliver an item to one Discord recipient, continuing after the parts that
/// were already posted. Updates the recipient's state; returns true when the
/// item has just been delivered completely.
pub(crate) fn deliver(
    job: &Job,
    r: &mut Recipient,
    hooks: &Result<Vec<Webhook>, String>,
    poster: &mut dyn Poster,
) -> bool {
    let name = r
        .host
        .strip_prefix("discord:")
        .unwrap_or_default()
        .to_string();
    let hook = match hooks {
        Ok(hooks) => hooks.iter().find(|h| h.name == name),
        Err(e) => {
            // A broken file is something the user can fix: keep the item.
            r.status = "waiting".into();
            r.last_error = e.clone();
            schedule_retry(r, job.now);
            return false;
        }
    };
    let Some(hook) = hook else {
        r.status = "failed".into();
        r.last_error = format!("the Discord webhook \"{name}\" is not configured any more");
        return false;
    };
    let plan = plan(job.kind, job.name, job.payload);
    let limit = hook.max_file_mb as usize * 1024 * 1024;
    if let Plan::Attachment { .. } = plan {
        if job.payload.len() > limit {
            r.status = "failed".into();
            r.last_error = format!(
                "too large for Discord ({:.1} MB; this webhook allows {} MB)",
                job.payload.len() as f64 / (1024.0 * 1024.0),
                hook.max_file_mb
            );
            return false;
        }
    }
    let total = match &plan {
        Plan::Messages(parts) => parts.len(),
        Plan::Attachment { .. } => 1,
    };
    while r.message_ids.len() < total {
        let index = r.message_ids.len();
        let result = match &plan {
            Plan::Messages(parts) => poster.text(hook, &parts[index]),
            Plan::Attachment { filename, note } => poster.file(hook, note, filename, job.payload),
        };
        match result {
            Ok(id) => r.message_ids.push(id),
            Err(failure) => {
                r.attempts += 1;
                match failure {
                    Failure::RateLimited(secs) => {
                        r.status = "waiting".into();
                        r.last_error = format!("Discord asked to wait {}", span(secs));
                        r.next_attempt = job.now + secs;
                    }
                    Failure::Retry(why) => {
                        r.status = "waiting".into();
                        r.last_error = why;
                        schedule_retry(r, job.now);
                    }
                    Failure::Permanent(why) => {
                        r.status = "failed".into();
                        r.last_error = why;
                    }
                }
                return false;
            }
        }
    }
    r.attempts += 1;
    r.status = "delivered".into();
    r.last_error.clear();
    r.backoff = 0;
    r.next_attempt = 0;
    // Unit tests never write to the real state directory.
    if !cfg!(test) {
        let _ = record_sent(job, &name, &r.message_ids);
    }
    true
}

// ── Undo ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub(crate) struct Sent {
    pub(crate) id: String,
    pub(crate) webhook: String,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) messages: Vec<String>,
    pub(crate) sent: u64,
    #[serde(default)]
    pub(crate) undone: bool,
}

fn sent_dir() -> PathBuf {
    state_dir().join("discord-sent")
}

fn record_sent(job: &Job, webhook: &str, messages: &[String]) -> Result<(), String> {
    ensure_dirs()?;
    let dir = sent_dir();
    private_dir(&dir)?;
    prune(&dir, job.now, SENT_KEEP_SECS);
    let record = Sent {
        id: job.id.to_string(),
        webhook: webhook.to_string(),
        name: job.name.to_string(),
        kind: job.kind.to_string(),
        messages: messages.to_vec(),
        sent: job.now,
        undone: false,
    };
    write_private_json(&dir.join(format!("{}.json", job.id)), &record)
}

fn write_private_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    private_file(&tmp)?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Remove entries (files or directories) older than `keep`.
fn prune(dir: &Path, now: u64, keep: u64) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name();
        let Some(stem) = name.to_str() else { continue };
        let id = stem.trim_end_matches(".json");
        if !valid_id(id) {
            continue;
        }
        if entry_mtime(&e.path()).is_some_and(|m| now.saturating_sub(m) > keep) {
            let _ = if e.path().is_dir() {
                fs::remove_dir_all(e.path())
            } else {
                fs::remove_file(e.path())
            };
        }
    }
}

/// Whether an item sent at `sent` can still be taken back at `now`.
pub(crate) fn undo_open(sent: u64, now: u64) -> bool {
    now.saturating_sub(sent) <= UNDO_SECS
}

/// Delete every message of a sent item, within the undo window.
pub(crate) fn undo(id: &str) -> Result<String, String> {
    if !valid_id(id) {
        return Err("invalid id".into());
    }
    let path = sent_dir().join(format!("{id}.json"));
    let mut record: Sent = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("nothing to undo: this item was not sent to Discord from this device")?;
    if record.undone {
        return Ok(format!("{} was already removed from Discord", record.name));
    }
    if !undo_open(record.sent, now_secs()) {
        return Err(format!(
            "Undo is no longer possible: {} was sent more than {} ago",
            record.name,
            span(UNDO_SECS)
        ));
    }
    let hook = find_webhook(&record.webhook)?;
    for m in record.messages.iter().rev() {
        delete_message(&hook, m).map_err(|f| match f {
            Failure::RateLimited(s) => format!("Discord asked to wait {}; try again", span(s)),
            Failure::Retry(w) | Failure::Permanent(w) => w,
        })?;
    }
    record.undone = true;
    let _ = write_private_json(&path, &record);
    Ok(format!(
        "Removed {} from Discord · {}",
        record.name, record.webhook
    ))
}

/// The notification shown after a successful send, with an Undo action
/// for UNDO_SECS. Runs in its own process (`nirushare discord undo-notice ID`)
/// so a CLI that sent the item can exit at once.
pub(crate) fn spawn_undo_notice(id: &str) {
    if !notifications_enabled() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    if let Ok(mut child) = Command::new(exe)
        .args(["discord", "undo-notice", id])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        // Reap it in the long-running service; a CLI exits before and the
        // child carries on alone.
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

pub(crate) fn undo_notice(id: &str) -> Result<(), String> {
    let path = sent_dir().join(format!("{id}.json"));
    let record: Sent = fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("unknown item")?;
    let left = UNDO_SECS.saturating_sub(now_secs().saturating_sub(record.sent));
    if left == 0 {
        return Ok(());
    }
    let parts = record.messages.len();
    let body = format!(
        "{} · {}{} · Undo for {}",
        record.name,
        record.webhook,
        if parts > 1 {
            format!(" · {parts} messages")
        } else {
            String::new()
        },
        span(UNDO_SECS)
    );
    let out = Command::new("notify-send")
        .args([
            "--app-name",
            "NIRU Share",
            "--expire-time",
            &(left * 1000).to_string(),
            "--wait",
            "--action=undo=Undo",
            "Sent to Discord",
            &body,
        ])
        .output()
        .map_err(|e| format!("notify-send: {e}"))?;
    if String::from_utf8_lossy(&out.stdout).trim() == "undo" {
        let (summary, text) = match undo(id) {
            Ok(msg) => ("Removed from Discord", msg),
            Err(e) => ("Could not undo", e),
        };
        let _ = Command::new("notify-send")
            .args(["--app-name", "NIRU Share", summary, &text])
            .status();
    }
    Ok(())
}

pub(crate) fn failure_notice(name: &str, webhook: &str, why: &str, retrying: bool) {
    if !notifications_enabled() {
        return;
    }
    let summary = if retrying {
        "Discord: will retry"
    } else {
        "Could not send to Discord"
    };
    let body = format!("{name} · {webhook}: {why}");
    let _ = Command::new("notify-send")
        .args(["--app-name", "NIRU Share", summary, &body])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map(|mut c| {
            thread::spawn(move || {
                let _ = c.wait();
            })
        });
}

// ── Preview (staging) ──────────────────────────────────────────────────────

/// A previewed item waiting for confirmation. Staging fixes the content:
/// what the preview showed is what is sent, even if the clipboard changes.
#[derive(Serialize, Deserialize)]
pub(crate) struct Staged {
    pub(crate) id: String,
    pub(crate) webhook: String,
    pub(crate) kind: String,
    pub(crate) name: String,
    pub(crate) created: u64,
}

fn staged_dir() -> PathBuf {
    state_dir().join("discord-staged")
}

pub(crate) fn stage(
    webhook: &str,
    kind: &str,
    name: &str,
    payload: &[u8],
) -> Result<Staged, String> {
    ensure_dirs()?;
    let root = staged_dir();
    private_dir(&root)?;
    prune(&root, now_secs(), STAGE_KEEP_SECS);
    let id = now_id();
    let dir = root.join(&id);
    private_dir(&dir)?;
    fs::write(dir.join("payload"), payload).map_err(|e| e.to_string())?;
    private_file(&dir.join("payload"))?;
    let s = Staged {
        id,
        webhook: webhook.into(),
        kind: kind.into(),
        name: name.into(),
        created: now_secs(),
    };
    write_private_json(&dir.join("meta.json"), &s)?;
    Ok(s)
}

pub(crate) fn read_stage(id: &str) -> Result<(Staged, Vec<u8>), String> {
    if !valid_id(id) {
        return Err("invalid preview id".into());
    }
    let dir = staged_dir().join(id);
    let s: Staged = fs::read(dir.join("meta.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("this preview expired or was already sent; open it again")?;
    if now_secs().saturating_sub(s.created) > STAGE_KEEP_SECS {
        let _ = fs::remove_dir_all(&dir);
        return Err("this preview expired; open it again".into());
    }
    let payload = fs::read(dir.join("payload")).map_err(|e| e.to_string())?;
    Ok((s, payload))
}

pub(crate) fn drop_stage(id: &str) {
    if valid_id(id) {
        let _ = fs::remove_dir_all(staged_dir().join(id));
    }
}

/// The preview as JSON (the launcher reads this) and as text (the CLI).
pub(crate) fn preview_json(hook: &Webhook, s: &Staged, payload: &[u8]) -> serde_json::Value {
    let plan = plan(&s.kind, &s.name, payload);
    let (messages, attachment) = match &plan {
        Plan::Messages(p) => (p.len(), serde_json::Value::Null),
        Plan::Attachment { filename, .. } => (1, filename.clone().into()),
    };
    let excerpt = if s.kind == "text" || s.kind == "link" {
        let text = String::from_utf8_lossy(payload);
        let one: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .take(200)
            .collect();
        one.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        String::new()
    };
    let too_large = matches!(plan, Plan::Attachment { .. })
        && payload.len() > hook.max_file_mb as usize * 1024 * 1024;
    serde_json::json!({
        "stage": s.id,
        "to": format!("discord:{}", hook.name),
        "webhook": hook.name,
        "host": hook.url.host(),
        "username": hook.username,
        "embeds": hook.embeds,
        "kind": s.kind,
        "name": s.name,
        "bytes": payload.len(),
        "characters": if s.kind == "text" || s.kind == "link" {
            String::from_utf8_lossy(payload).chars().count()
        } else { 0 },
        "messages": messages,
        "attachment": attachment,
        "excerpt": excerpt,
        "too_large": too_large,
        "max_file_mb": hook.max_file_mb,
        "undo_seconds": UNDO_SECS,
    })
}

pub(crate) fn preview_text(v: &serde_json::Value) -> String {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let n = |k: &str| v[k].as_u64().unwrap_or(0);
    let mut out = format!(
        "To:      Discord · {} ({}){}\n",
        s("webhook"),
        s("host"),
        v["username"]
            .as_str()
            .map(|u| format!(" as {u}"))
            .unwrap_or_default()
    );
    let what = match s("kind").as_str() {
        "text" => format!("text, {} characters", n("characters")),
        "link" => "a link".to_string(),
        "image" => format!("image {}, {}", s("name"), size(n("bytes"))),
        _ => format!("file {}, {}", s("name"), size(n("bytes"))),
    };
    out.push_str(&format!("Content: {what}\n"));
    match v["attachment"].as_str() {
        Some(file) => out.push_str(&format!("Sent as: one message with {file} attached\n")),
        None => out.push_str(&format!(
            "Sent as: {} message{}\n",
            n("messages"),
            if n("messages") == 1 { "" } else { "s" }
        )),
    }
    let excerpt = s("excerpt");
    if !excerpt.is_empty() {
        out.push_str(&format!("Preview: {excerpt}\n"));
    }
    if v["embeds"] == false {
        out.push_str("Links:   no previews (embeds off)\n");
    }
    if v["too_large"] == true {
        out.push_str(&format!(
            "Problem: larger than {} MB, Discord will refuse it\n",
            n("max_file_mb")
        ));
    }
    out
}

fn size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{bytes} bytes")
    }
}

// ── Commands ───────────────────────────────────────────────────────────────

/// `nirushare discord list [--json]`: names and options, never URLs.
pub(crate) fn list(args: &[String]) -> Result<(), String> {
    let hooks = load_webhooks()?;
    if args.iter().any(|a| a == "--json") {
        let v: Vec<_> = hooks
            .iter()
            .map(|h| {
                serde_json::json!({
                    "name": h.name,
                    "to": format!("discord:{}", h.name),
                    "host": h.url.host(),
                    "username": h.username,
                    "embeds": h.embeds,
                    "max_file_mb": h.max_file_mb,
                })
            })
            .collect();
        println!("{}", serde_json::Value::from(v));
        return Ok(());
    }
    if hooks.is_empty() {
        println!(
            "No Discord webhooks. Add one to {} (see `nirushare discord help`).",
            webhooks_path().display()
        );
    }
    for h in &hooks {
        println!(
            "discord:{:<20} {}{}{}",
            h.name,
            h.url.host(),
            h.username
                .as_ref()
                .map(|u| format!(" · as {u}"))
                .unwrap_or_default(),
            if h.embeds { "" } else { " · embeds off" }
        );
    }
    Ok(())
}

/// `nirushare discord check [--online]`: validate the file; with --online
/// also ask Discord whether each webhook exists (a GET, never a post).
pub(crate) fn check(args: &[String]) -> Result<(), String> {
    let path = webhooks_path();
    if !path.exists() {
        println!("{} does not exist: no Discord webhooks.", path.display());
        return Ok(());
    }
    if let Some(w) = check_private(&path)? {
        println!("repaired: {w}");
    } else {
        println!("{}: private (0600), ok", path.display());
    }
    let hooks = parse_webhooks(&fs::read_to_string(&path).map_err(|e| e.to_string())?)?;
    println!(
        "{} webhook{} valid",
        hooks.len(),
        if hooks.len() == 1 { "" } else { "s" }
    );
    let online = args.iter().any(|a| a == "--online");
    let mut bad = 0;
    for h in &hooks {
        if !online {
            println!("  discord:{} ({}) ok", h.name, h.url.host());
            continue;
        }
        match agent().get(&h.url.info()).call() {
            Ok(r) => {
                let channel = r
                    .into_string()
                    .ok()
                    .and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok())
                    .and_then(|v| v["name"].as_str().map(str::to_string))
                    .unwrap_or_default();
                println!(
                    "  discord:{} works (Discord calls it \"{channel}\")",
                    h.name
                );
            }
            Err(e) => {
                bad += 1;
                let why = match failure_of(e) {
                    Failure::RateLimited(s) => format!("rate limited, try in {}", span(s)),
                    Failure::Retry(w) | Failure::Permanent(w) => w,
                };
                println!("  discord:{} problem: {why}", h.name);
            }
        }
    }
    if bad > 0 {
        return Err(format!("{bad} webhook(s) did not work"));
    }
    Ok(())
}

pub(crate) const HELP: &str = "\
Discord webhooks (none until you add one):

  ~/.config/nirushare/webhooks.toml, mode 0600:

    [webhook.friends]
    url = \"https://discord.com/api/webhooks/<id>/<token>\"
    username = \"Niru\"          # optional
    avatar_url = \"https://...\" # optional
    embeds = false             # optional: no link previews
    max_file_mb = 10           # optional upload limit of that server

Commands:
  nirushare discord list [--json]          configured names (never URLs)
  nirushare discord check [--online]       validate the file; --online asks
                                           Discord whether each webhook exists
  nirushare send --to discord:NAME [FILE|-|--clipboard] [--yes]
                                           preview, confirm, send
  nirushare discord preview --to NAME [--file F|--stdin|--clipboard] [--json]
  nirushare discord confirm PREVIEW-ID     send a previewed item
  nirushare discord cancel PREVIEW-ID
  nirushare discord undo ID                delete a sent item (5 minutes)
";

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://discord.com/api/webhooks/123456789012345678/AbCdEfGhIjKlMnOpQrStUvWxYz_-0123456789";

    #[test]
    fn only_discord_webhook_urls_are_accepted() {
        let u = parse_url(URL).unwrap();
        assert_eq!(u.host(), "discord.com");
        assert!(u.execute().ends_with("?wait=true"));
        assert!(u.message("42").ends_with("/messages/42"));
        let t = parse_url(&format!("{URL}?thread_id=99")).unwrap();
        assert!(t.execute().ends_with("?wait=true&thread_id=99"));
        assert!(t.message("1").ends_with("/messages/1?thread_id=99"));
        assert!(parse_url(&URL.replace("discord.com/api", "discord.com/api/v10")).is_ok());
        assert!(parse_url(&URL.replace("discord.com", "discordapp.com")).is_ok());
        for bad in [
            URL.replace("https://", "http://"),
            URL.replace("discord.com", "evil.example"),
            URL.replace("discord.com", "discord.com.evil.example"),
            URL.replace("webhooks", "channels"),
            URL.replace("123456789012345678", "12x"),
            format!("{URL}/extra"),
            format!("{URL}?token=1"),
            "https://discord.com/api/webhooks/1/short".to_string(),
        ] {
            assert!(parse_url(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn local_mock_urls_need_the_test_feature() {
        let local = "http://127.0.0.1:9/api/webhooks/1/abcdefghijklmnopqrstuvwxyz";
        assert_eq!(parse_url(local).is_ok(), cfg!(feature = "test-transport"));
    }

    #[test]
    fn errors_never_contain_the_url() {
        let text = format!(
            "[webhook.friends]\nurl = \"{URL}\"\nusername = \"Niru\"\n[webhook.b]\nurl = \"{}\"\n",
            URL.replace("discord.com", "evil.example")
        );
        let err = parse_webhooks(&text).err().unwrap();
        assert!(
            !err.contains("AbCdEf") && !err.contains("123456789012345678"),
            "{err}"
        );
        assert!(err.contains("b:"), "{err}");
        let broken = format!("[webhook.x]\nurl = \"{URL}\nembeds = maybe\n");
        let err = parse_webhooks(&broken).err().unwrap();
        assert!(!err.contains("AbCdEf"), "{err}");
        assert!(err.contains("line"), "{err}");
        let unknown = format!("[webhook.x]\nurl = \"{URL}\"\ncolour = 1\n");
        assert!(!parse_webhooks(&unknown).err().unwrap().contains("AbCdEf"));
    }

    #[test]
    fn several_named_webhooks_with_options() {
        let text = format!(
            "[webhook.friends]\nurl = \"{URL}\"\nusername = \"Niru\"\navatar_url = \"https://example.com/a.png\"\n\
             [webhook.work-log]\nurl = \"{URL}\"\nembeds = false\nmax_file_mb = 50\n"
        );
        let hooks = parse_webhooks(&text).unwrap();
        assert_eq!(hooks.len(), 2);
        let f = &hooks[0];
        assert_eq!(f.name, "friends");
        assert_eq!(f.username.as_deref(), Some("Niru"));
        assert!(f.embeds);
        assert_eq!(f.max_file_mb, DEFAULT_MAX_FILE_MB);
        let w = &hooks[1];
        assert!(!w.embeds);
        assert_eq!(w.max_file_mb, 50);
        let j = payload_json(w, "hi", None);
        assert_eq!(j["flags"], 4);
        assert_eq!(j["allowed_mentions"]["parse"], serde_json::json!([]));
        assert!(j.get("username").is_none());
        let j = payload_json(f, "", Some("a.png"));
        assert_eq!(j["username"], "Niru");
        assert!(j.get("flags").is_none());
        assert_eq!(j["attachments"][0]["filename"], "a.png");
        assert!(parse_webhooks("").unwrap().is_empty());
        assert!(parse_webhooks(&format!("[webhook.\"bad name\"]\nurl = \"{URL}\"\n")).is_err());
    }

    #[test]
    fn webhooks_file_must_be_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("nirushare-hooks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("webhooks.toml");
        fs::write(&f, "").unwrap();
        fs::set_permissions(&f, fs::Permissions::from_mode(0o644)).unwrap();
        let warning = check_private(&f).unwrap();
        assert!(warning.unwrap().contains("0600"));
        assert_eq!(
            fs::metadata(&f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(check_private(&f).unwrap().is_none());
        let link = dir.join("link.toml");
        std::os::unix::fs::symlink(&f, &link).unwrap();
        assert!(check_private(&link).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn text_is_split_at_lines_and_long_text_is_attached() {
        assert_eq!(split_text("hello", 2000), vec!["hello"]);
        let lines: String = (0..300)
            .map(|i| format!("line {i:03} of the text\n"))
            .collect();
        let parts = split_text(&lines, 2000);
        assert!(parts.len() > 1);
        for p in &parts {
            assert!(p.chars().map(char::len_utf16).sum::<usize>() <= 2000);
            assert!(p.ends_with("of the text"), "split at a line end");
        }
        assert_eq!(parts.join("\n"), lines.trim_end());
        // No break anywhere: a hard cut, nothing lost.
        let solid = "x".repeat(4500);
        let parts = split_text(&solid, 2000);
        assert_eq!(
            parts.iter().map(String::len).collect::<Vec<_>>(),
            vec![2000, 2000, 500]
        );
        // Emoji count as two UTF-16 units, as Discord counts them.
        let emoji = "😀".repeat(1500);
        assert!(split_text(&emoji, 2000)
            .iter()
            .all(|p| p.chars().count() <= 1000));
        assert!(
            matches!(plan("text", "clipboard.txt", lines.as_bytes()), Plan::Messages(p) if p.len() <= 4)
        );
        let long = "word ".repeat(2500);
        match plan("text", "clipboard.txt", long.as_bytes()) {
            Plan::Attachment { filename, note } => {
                assert_eq!(filename, "clipboard.txt");
                assert!(note.contains("12500"));
            }
            other => panic!("{other:?}"),
        }
        assert!(
            matches!(plan("image", "../../Shot 1.png", b"x"), Plan::Attachment { filename, .. } if filename == "Shot_1.png")
        );
    }

    #[test]
    fn links_and_file_names() {
        assert!(is_link(" https://example.com/a?b=1 \n"));
        assert!(!is_link("see https://example.com"));
        assert!(!is_link("https://"));
        assert_eq!(safe_filename(".hidden"), "hidden");
        assert_eq!(safe_filename(""), "file");
        assert_eq!(safe_filename("a\"b\r\n.txt"), "a_b__.txt");
    }

    #[test]
    fn rate_limits_honour_retry_after() {
        assert_eq!(retry_after(Some("2"), ""), 2);
        assert_eq!(retry_after(Some("0.3"), ""), 1);
        assert_eq!(
            retry_after(None, r#"{"retry_after": 12.5, "global": false}"#),
            13
        );
        assert_eq!(retry_after(Some("7"), r#"{"retry_after": 1}"#), 7);
        assert_eq!(retry_after(None, "garbage"), 5);
        assert_eq!(classify(429, Some("3"), ""), Failure::RateLimited(3));
        assert!(matches!(classify(502, None, ""), Failure::Retry(_)));
        assert!(matches!(classify(404, None, ""), Failure::Permanent(_)));
        assert!(matches!(classify(401, None, ""), Failure::Permanent(_)));
        assert!(
            matches!(classify(413, None, ""), Failure::Permanent(m) if m.contains("too large"))
        );
    }

    #[test]
    fn undo_window() {
        assert!(undo_open(1000, 1000 + UNDO_SECS));
        assert!(!undo_open(1000, 1001 + UNDO_SECS));
    }

    /// Answers from a script and records what was posted.
    struct Script {
        answers: Vec<Result<String, Failure>>,
        posted: Vec<String>,
    }
    impl Poster for Script {
        fn text(&mut self, _: &Webhook, content: &str) -> Result<String, Failure> {
            self.posted.push(content.to_string());
            self.answers.remove(0)
        }
        fn file(&mut self, _: &Webhook, _: &str, name: &str, _: &[u8]) -> Result<String, Failure> {
            self.posted.push(format!("file:{name}"));
            self.answers.remove(0)
        }
    }
    fn hooks() -> Result<Vec<Webhook>, String> {
        parse_webhooks(&format!(
            "[webhook.friends]\nurl = \"{URL}\"\nmax_file_mb = 1\n"
        ))
    }
    fn rec() -> Recipient {
        Recipient {
            host: "discord:friends".into(),
            token: String::new(),
            status: "pending".into(),
            attempts: 0,
            last_error: String::new(),
            next_attempt: 0,
            backoff: 0,
            message_ids: Vec::new(),
        }
    }

    #[test]
    fn a_rate_limited_part_is_retried_without_reposting_earlier_parts() {
        let text: String = (0..200)
            .map(|i| format!("line {i:03} of the text\n"))
            .collect();
        let job = Job {
            id: "1-a",
            kind: "text",
            name: "clipboard.txt",
            payload: text.as_bytes(),
            now: 100,
        };
        let hooks = hooks();
        let mut r = rec();
        let mut p = Script {
            answers: vec![Ok("11".into()), Err(Failure::RateLimited(7))],
            posted: vec![],
        };
        assert!(!deliver(&job, &mut r, &hooks, &mut p));
        assert_eq!(r.status, "waiting");
        assert_eq!(r.next_attempt, 107, "Retry-After decides the next try");
        assert_eq!(r.message_ids, vec!["11"]);
        let job = Job { now: 107, ..job };
        let mut p2 = Script {
            answers: vec![Ok("12".into()), Ok("13".into())],
            posted: vec![],
        };
        assert!(deliver(&job, &mut r, &hooks, &mut p2));
        assert_eq!(r.status, "delivered");
        assert_eq!(r.message_ids, vec!["11", "12", "13"]);
        assert_eq!(
            p.posted.len() + p2.posted.len(),
            4,
            "part 2 is tried twice, part 1 once"
        );
        assert!(
            !p2.posted[0].starts_with("line 000"),
            "part 1 is not posted again"
        );
    }

    #[test]
    fn permanent_failures_and_size_limits_stop_retrying() {
        let hooks = hooks();
        let mut r = rec();
        let job = Job {
            id: "1-b",
            kind: "link",
            name: "clipboard.txt",
            payload: b"https://example.com",
            now: 5,
        };
        let mut p = Script {
            answers: vec![Err(Failure::Permanent(
                "the webhook does not exist (deleted, or the URL is wrong)".into(),
            ))],
            posted: vec![],
        };
        assert!(!deliver(&job, &mut r, &hooks, &mut p));
        assert_eq!(r.status, "failed");
        let big = vec![0u8; 2 * 1024 * 1024];
        let mut r = rec();
        let job = Job {
            id: "1-c",
            kind: "file",
            name: "a.bin",
            payload: &big,
            now: 5,
        };
        let mut p = Script {
            answers: vec![],
            posted: vec![],
        };
        assert!(!deliver(&job, &mut r, &hooks, &mut p));
        assert!(r.last_error.contains("too large"));
        assert!(p.posted.is_empty());
        let mut r = Recipient {
            host: "discord:gone".into(),
            ..rec()
        };
        assert!(!deliver(
            &job,
            &mut r,
            &hooks,
            &mut Script {
                answers: vec![],
                posted: vec![]
            }
        ));
        assert_eq!(r.status, "failed");
        let mut r = rec();
        let broken: Result<Vec<Webhook>, String> =
            Err("webhooks.toml: invalid TOML on line 3".into());
        assert!(!deliver(
            &job,
            &mut r,
            &broken,
            &mut Script {
                answers: vec![],
                posted: vec![]
            }
        ));
        assert_eq!(
            r.status, "waiting",
            "a broken file is fixable: keep the item"
        );
    }

    #[test]
    fn multipart_body_has_json_and_file() {
        let b = multipart("B", "{\"content\":\"\"}", "a.png", b"PNGDATA");
        let t = String::from_utf8_lossy(&b);
        assert!(t.starts_with("--B\r\n"));
        assert!(t.contains("name=\"payload_json\""));
        assert!(t.contains("name=\"files[0]\"; filename=\"a.png\""));
        assert!(t.contains("PNGDATA\r\n--B--\r\n"));
    }
}
