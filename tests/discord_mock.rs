//! Discord webhooks against a local mock server on 127.0.0.1. Nothing here
//! talks to Discord. Run with:
//!
//!     cargo test --locked --features test-transport
//!
//! Only test-transport builds accept a plain-HTTP 127.0.0.1 webhook URL.
#![cfg(feature = "test-transport")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_nirushare");
const SECRET: &str = "MockTokenThatMustNeverBeLogged_0123456789";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-a-png-by-its-magic-bytes";

#[derive(Clone, Debug)]
struct Req {
    method: String,
    path: String,
    headers: String,
    body: Vec<u8>,
}

/// A tiny Discord: POST number `limit` (counted from 1) answers 429.
struct Mock {
    port: u16,
    log: Arc<Mutex<Vec<Req>>>,
    limit: Arc<Mutex<u32>>,
}

impl Mock {
    fn start() -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(Vec::new()));
        let limit = Arc::new(Mutex::new(0u32));
        let (l2, lim2) = (log.clone(), limit.clone());
        std::thread::spawn(move || {
            let mut next_id = 1000u64;
            let mut posts = 0u32;
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let path = parts.next().unwrap_or_default().to_string();
                let mut headers = String::new();
                let mut length = 0usize;
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                    headers.push_str(&h);
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                l2.lock().unwrap().push(Req {
                    method: method.clone(),
                    path,
                    headers,
                    body,
                });
                let mut stream = stream;
                let reply = |stream: &mut std::net::TcpStream,
                             status: &str,
                             extra: &str,
                             body: &str| {
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                };
                match method.as_str() {
                    "POST" => {
                        posts += 1;
                        // `limit` is the number of the POST that gets a 429.
                        let mut lim = lim2.lock().unwrap();
                        if *lim == posts {
                            *lim = 0;
                            reply(
                                &mut stream,
                                "429 Too Many Requests",
                                "Retry-After: 1\r\n",
                                r#"{"message":"You are being rate limited.","retry_after":1,"global":false}"#,
                            );
                        } else {
                            next_id += 1;
                            reply(
                                &mut stream,
                                "200 OK",
                                "",
                                &format!(r#"{{"id":"{next_id}"}}"#),
                            );
                        }
                    }
                    "DELETE" => reply(&mut stream, "204 No Content", "", ""),
                    "GET" => reply(&mut stream, "200 OK", "", r#"{"name":"mock hook"}"#),
                    _ => reply(&mut stream, "405 Method Not Allowed", "", "{}"),
                }
            }
        });
        Mock { port, log, limit }
    }
    fn requests(&self) -> Vec<Req> {
        self.log.lock().unwrap().clone()
    }
    fn posts(&self) -> Vec<Req> {
        self.requests()
            .into_iter()
            .filter(|r| r.method == "POST")
            .collect()
    }
}

struct Home {
    root: PathBuf,
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
impl Home {
    fn new(tag: &str) -> Home {
        let root = std::env::temp_dir().join(format!("nirushare-dc-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("config/nirushare")).unwrap();
        std::fs::create_dir_all(root.join("run")).unwrap();
        Home { root }
    }
    fn cmd(&self) -> Command {
        // `setsid` gives the child no controlling terminal, so "is there a
        // terminal to confirm on?" has the same answer everywhere.
        let mut c = Command::new("setsid");
        c.arg(BIN);
        let r = &self.root;
        c.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", r)
            .env("XDG_CONFIG_HOME", r.join("config"))
            .env("XDG_DATA_HOME", r.join("data"))
            .env("XDG_STATE_HOME", r.join("state"))
            .env("XDG_RUNTIME_DIR", r.join("run"))
            .env("NIRUSHARE_TEST_NO_NOTIFY", "1")
            .env("NIRUSHARE_TEST_LISTEN", "127.0.0.1:0")
            .env("NIRUSHARE_TEST_PEERS", "")
            .env("NIRUSHARE_TEST_WHOIS", "nobody");
        c
    }
    fn run(&self, args: &[&str], stdin: &[u8]) -> Output {
        let mut child = self
            .cmd()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        child.wait_with_output().unwrap()
    }
    fn hooks(&self, port: u16, extra: &str) {
        let f = self.root.join("config/nirushare/webhooks.toml");
        std::fs::write(
            &f,
            format!(
                "[webhook.mock]\nurl = \"http://127.0.0.1:{port}/api/webhooks/123456789/{SECRET}\"\nusername = \"Niru test\"\n{extra}"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    fn state(&self) -> PathBuf {
        self.root.join("state/nirushare")
    }
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Nothing anywhere in the state or data directories, or in any output,
/// contains the webhook token.
fn assert_no_secret(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            assert_no_secret(&p);
        } else if let Ok(bytes) = std::fs::read(&p) {
            assert!(
                !String::from_utf8_lossy(&bytes).contains(SECRET),
                "{} contains the webhook token",
                p.display()
            );
        }
    }
}

fn wait_for(mut f: impl FnMut() -> bool, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

struct Service(Child);
impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn discord_preview_send_retry_and_undo_against_a_mock() {
    if Command::new("setsid").arg("true").status().is_err() {
        eprintln!("setsid missing; skipped");
        return;
    }
    let mock = Mock::start();
    let home = Home::new("main");
    home.hooks(mock.port, "");

    // The list names the webhook, repairs the file mode and never shows the URL.
    let o = home.run(&["discord", "list", "--json"], b"");
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("\"name\":\"mock\""), "{}", text(&o));
    assert!(
        text(&o).contains("0600"),
        "the repair is reported: {}",
        text(&o)
    );
    assert!(!text(&o).contains(SECRET));
    let mode = std::fs::metadata(home.root.join("config/nirushare/webhooks.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let o = home.run(&["discord", "check", "--online"], b"");
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("mock hook"), "{}", text(&o));
    assert!(mock.posts().is_empty(), "check never posts");

    // No terminal and no --yes: a preview, then a refusal. Nothing is sent.
    let o = home.run(&["send", "--to", "discord:mock", "-"], b"hello @everyone");
    assert!(!o.status.success());
    assert!(text(&o).contains("To:      Discord · mock"), "{}", text(&o));
    assert!(text(&o).contains("--yes"), "{}", text(&o));
    assert!(
        mock.posts().is_empty(),
        "nothing is sent without confirmation"
    );

    // Confirmed: a short text is one message, sent with wait=true, the
    // configured name and no mentions.
    let o = home.run(
        &["send", "--to", "discord:mock", "-", "--yes"],
        b"hello @everyone",
    );
    assert!(o.status.success(), "{}", text(&o));
    let posts = mock.posts();
    assert_eq!(posts.len(), 1);
    assert!(posts[0]
        .path
        .ends_with(&format!("/api/webhooks/123456789/{SECRET}?wait=true")));
    let body: serde_json::Value = serde_json::from_slice(&posts[0].body).unwrap();
    assert_eq!(body["content"], "hello @everyone");
    assert_eq!(body["username"], "Niru test");
    assert_eq!(body["allowed_mentions"]["parse"], serde_json::json!([]));
    let first_id = String::from_utf8_lossy(&o.stdout).trim().to_string();

    // Undo deletes the message.
    let o = home.run(&["discord", "undo", &first_id], b"");
    assert!(o.status.success(), "{}", text(&o));
    let deletes: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|r| r.method == "DELETE")
        .collect();
    assert_eq!(deletes.len(), 1);
    assert!(
        deletes[0].path.ends_with("/messages/1001"),
        "{:?}",
        deletes[0].path
    );

    // A long text in three messages; Discord rate-limits the second one.
    // The CLI leaves it queued, the service honours Retry-After and finishes
    // without posting the first part again.
    let long: String = (0..250)
        .map(|i| format!("line {i:03} of a long note\n"))
        .collect();
    let before = mock.posts().len();
    // The second POST of this item gets a 429 with Retry-After: 1.
    *mock.limit.lock().unwrap() = before as u32 + 2;
    let o = home.run(
        &["send", "--to", "discord:mock", "--stdin", "--yes"],
        long.as_bytes(),
    );
    assert!(o.status.success(), "{}", text(&o));
    let long_id = String::from_utf8_lossy(&o.stdout).trim().to_string();
    let service = Service(
        home.cmd()
            .arg("serve")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let sent = home.state().join(format!("discord-sent/{long_id}.json"));
    assert!(
        wait_for(|| sent.exists(), 15),
        "the service delivered the rest"
    );
    drop(service);
    let posts: Vec<_> = mock.posts().into_iter().skip(before).collect();
    let contents: Vec<String> = posts
        .iter()
        .map(|p| {
            serde_json::from_slice::<serde_json::Value>(&p.body).unwrap()["content"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let first_parts = contents
        .iter()
        .filter(|c| c.starts_with("line 000"))
        .count();
    assert_eq!(first_parts, 1, "part 1 is posted once: {}", contents.len());
    let record: serde_json::Value = serde_json::from_slice(&std::fs::read(&sent).unwrap()).unwrap();
    let messages = record["messages"].as_array().unwrap().len();
    assert!(messages >= 3, "{record}");
    assert_eq!(posts.len(), messages + 1, "one 429, then every part once");
    for c in &contents {
        assert!(c.chars().count() <= 2000);
    }

    // A screenshot goes as a multipart upload with its file name.
    let shot = home.root.join("Shot 2026.png");
    std::fs::write(&shot, PNG).unwrap();
    let o = home.run(
        &[
            "send",
            "--to",
            "discord:mock",
            shot.to_str().unwrap(),
            "--yes",
        ],
        b"",
    );
    assert!(o.status.success(), "{}", text(&o));
    let last = mock.posts().pop().unwrap();
    assert!(last
        .headers
        .to_ascii_lowercase()
        .contains("multipart/form-data"));
    let raw = String::from_utf8_lossy(&last.body);
    assert!(raw.contains("filename=\"Shot_2026.png\""), "{raw}");
    assert!(raw.contains("\"attachments\""));

    // The launcher's path: preview (staged), then confirm.
    let o = home.run(
        &["discord", "preview", "--to", "mock", "--stdin", "--json"],
        b"https://example.com/article",
    );
    assert!(o.status.success(), "{}", text(&o));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["kind"], "link");
    assert_eq!(v["to"], "discord:mock");
    assert!(!text(&o).contains(SECRET));
    let n = mock.posts().len();
    let o = home.run(&["discord", "confirm", v["stage"].as_str().unwrap()], b"");
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(mock.posts().len(), n + 1);
    let o = home.run(&["discord", "confirm", v["stage"].as_str().unwrap()], b"");
    assert!(!o.status.success(), "a preview is sent once");

    // Undo after the window: refused, nothing deleted.
    let path = home.state().join(format!("discord-sent/{long_id}.json"));
    let mut rec: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    rec["sent"] = (rec["sent"].as_u64().unwrap() - 301).into();
    std::fs::write(&path, rec.to_string()).unwrap();
    let deletes_before = mock
        .requests()
        .iter()
        .filter(|r| r.method == "DELETE")
        .count();
    let o = home.run(&["discord", "undo", &long_id], b"");
    assert!(!o.status.success());
    assert!(text(&o).contains("no longer possible"), "{}", text(&o));
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count(),
        deletes_before
    );

    // Status names the webhook and never shows the token.
    let o = home.run(&["status", "--text"], b"");
    assert!(text(&o).contains("discord:mock"), "{}", text(&o));
    assert!(!text(&o).contains(SECRET));
    assert_no_secret(&home.root.join("state"));
    assert_no_secret(&home.root.join("data"));
}

#[test]
fn a_deleted_webhook_fails_once_with_a_clear_reason() {
    if Command::new("setsid").arg("true").status().is_err() {
        return;
    }
    // A port nothing listens on is a transport failure (retried); a 404 from
    // Discord is permanent. Use a mock that answers 404 to everything.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}");
        }
    });
    let home = Home::new("gone");
    home.hooks(port, "embeds = false\n");
    let o = home.run(&["send", "--to", "discord:mock", "-", "--yes"], b"x");
    assert!(!o.status.success());
    assert!(text(&o).contains("does not exist"), "{}", text(&o));
    assert!(!text(&o).contains(SECRET));
    let o = home.run(&["send", "--to", "discord:nope", "-", "--yes"], b"x");
    assert!(
        text(&o).contains("no Discord webhook named \"nope\""),
        "{}",
        text(&o)
    );
    assert_no_secret(&home.root.join("state"));
}
