//! Two NIRUSHARE services talking on 127.0.0.1 through the test-only
//! transport override. Build and run with:
//!
//!     cargo test --locked --features test-transport
//!
//! The override (NIRUSHARE_TEST_*) exists only in builds with that feature;
//! release builds do not contain it.
#![cfg(feature = "test-transport")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_nirushare");
const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-not-really-an-image-but-typed-by-bytes";

struct Instance {
    name: &'static str,
    root: PathBuf,
    port: u16,
    peer: (&'static str, u16),
    child: Option<Child>,
}

impl Instance {
    fn new(name: &'static str, port: u16, peer: (&'static str, u16)) -> Self {
        let root = std::env::temp_dir().join(format!("nirushare-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Instance {
            name,
            root,
            port,
            peer,
            child: None,
        }
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(BIN);
        let r = &self.root;
        c.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", r)
            .env("XDG_CONFIG_HOME", r.join("config"))
            .env("XDG_DATA_HOME", r.join("data"))
            .env("XDG_STATE_HOME", r.join("state"))
            .env("XDG_RUNTIME_DIR", r.join("run"))
            .env("NIRUSHARE_TEST_NO_NOTIFY", "1")
            .env("NIRUSHARE_TEST_BODY_DEADLINE_MS", "3000")
            .env("NIRUSHARE_TEST_LISTEN", format!("127.0.0.1:{}", self.port))
            .env(
                "NIRUSHARE_TEST_PEERS",
                format!("{}=127.0.0.1:{}", self.peer.0, self.peer.1),
            )
            // Every connection comes from 127.0.0.1; whois names the peer.
            .env("NIRUSHARE_TEST_WHOIS", self.peer.0);
        std::fs::create_dir_all(r.join("run")).unwrap();
        c
    }
    fn run(&self, args: &[&str], stdin: Option<&[u8]>) -> String {
        let mut c = self.cmd();
        c.args(args)
            .env("NIRUSHARE_TOKEN", TOKEN)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn().unwrap();
        if let Some(input) = stdin {
            child.stdin.take().unwrap().write_all(input).unwrap();
        }
        drop(child.stdin.take());
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{} {args:?}: {}",
            self.name,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }
    fn start(&mut self) {
        let child = self
            .cmd()
            .arg("serve")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        self.child = Some(child);
        wait_for(Duration::from_secs(5), || {
            TcpStream::connect(("127.0.0.1", self.port)).is_ok()
        });
    }
    fn inbox(&self) -> Vec<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(&self.run(&["inbox"], None))
            .unwrap()
            .as_array()
            .cloned()
            .unwrap()
    }
    fn queue_len(&self) -> usize {
        std::fs::read_dir(self.root.join("state/nirushare/out"))
            .map(|d| d.count())
            .unwrap_or(0)
    }
    fn history(&self) -> String {
        std::fs::read_to_string(self.root.join("state/nirushare/history.jsonl")).unwrap_or_default()
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn wait_for(limit: Duration, mut ok: impl FnMut() -> bool) -> Duration {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < limit, "timed out after {limit:?}");
        std::thread::sleep(Duration::from_millis(100));
    }
    start.elapsed()
}
fn texts(items: &[serde_json::Value]) -> Vec<String> {
    items
        .iter()
        .filter_map(|i| i["text"].as_str().map(str::to_string))
        .collect()
}
fn post(port: u16, token: &str, id: &str, kind: &str, body: &[u8]) -> Result<u16, u16> {
    // Exactly what a 0.2.x sender sends: ureq send_bytes with these headers.
    let digest = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(body))
    };
    match ureq::post(&format!("http://127.0.0.1:{port}/share"))
        .set("Authorization", &format!("Bearer {token}"))
        .set("X-Nirushare-Transfer", id)
        .set("X-Nirushare-Kind", kind)
        .set("X-Nirushare-Name", "clipboard.txt")
        .set("X-Nirushare-Digest", &digest)
        .timeout(Duration::from_secs(30))
        .send_bytes(body)
    {
        Ok(r) => Ok(r.status()),
        Err(ureq::Error::Status(code, _)) => Err(code),
        Err(e) => panic!("transport error: {e}"),
    }
}

#[test]
fn two_local_instances_exchange_text_and_images() {
    let (pa, pb) = (free_port(), free_port());
    let mut alpha = Instance::new("alpha", pa, ("beta", pb));
    let mut beta = Instance::new("beta", pb, ("alpha", pa));
    alpha.run(&["trust", "beta"], None);
    beta.run(&["trust", "alpha"], None);

    // Queued while the receiver is offline, delivered once it comes up.
    alpha.run(
        &["send", "--to", "beta", "--stdin"],
        Some("hej från alpha ✓".as_bytes()),
    );
    alpha.start();
    wait_for(Duration::from_secs(5), || {
        alpha.history().is_empty() && alpha.queue_len() == 1
    });
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(alpha.queue_len(), 1, "still queued while beta is offline");
    let status: serde_json::Value =
        serde_json::from_str(&alpha.run(&["status", "--json"], None)).unwrap();
    let r = &status["outgoing"][0]["recipients"][0];
    assert_eq!(r["state"], "waiting");
    assert_eq!(r["last_error"], "NIRU Share is not running on beta");
    assert!(r["next_retry"].as_u64().unwrap() > 0);
    assert!(alpha
        .run(&["status", "--text"], None)
        .contains("NIRU Share is not running on beta"));
    beta.start();
    let took = wait_for(Duration::from_secs(15), || alpha.queue_len() == 0);
    assert!(
        took < Duration::from_secs(12),
        "backoff retry took {took:?}"
    );
    assert_eq!(texts(&beta.inbox()), ["hej från alpha ✓"]);
    assert!(alpha.history().contains("\"delivered\""));
    let status: serde_json::Value =
        serde_json::from_str(&alpha.run(&["status", "--json"], None)).unwrap();
    assert!(status["outgoing"].as_array().unwrap().is_empty());
    assert!(status["devices"][0]["last_sent"].as_u64().unwrap() > 0);
    assert_eq!(status["recent"][0]["recipients"][0]["status"], "delivered");

    // An image the other way, sent while both run: the wake socket starts
    // the pass at once instead of a timer.
    let img = beta.root.join("pic.png");
    std::fs::write(&img, PNG).unwrap();
    beta.run(
        &["send", "--to", "alpha", "--file", img.to_str().unwrap()],
        None,
    );
    let took = wait_for(Duration::from_secs(5), || {
        alpha.inbox().iter().any(|i| i["kind"] == "image")
    });
    assert!(took < Duration::from_secs(3), "wake took {took:?}");
    let item = alpha
        .inbox()
        .into_iter()
        .find(|i| i["kind"] == "image")
        .unwrap();
    let path = PathBuf::from(item["path"].as_str().unwrap());
    // The receiver's XDG Downloads (xdg-user-dir), under its own home here.
    assert!(path.starts_with(&alpha.root) && path.parent().unwrap().ends_with("nirushare"));
    assert_eq!(std::fs::read(&path).unwrap(), PNG);
    assert_eq!(beta.queue_len(), 0);
    assert_eq!(item["sender"], "beta");

    // A large image is streamed with a fixed Content-Length.
    let mut big = PNG.to_vec();
    big.resize(3 * 1024 * 1024 + 17, 7);
    let big_path = beta.root.join("big.png");
    std::fs::write(&big_path, &big).unwrap();
    beta.run(
        &[
            "send",
            "--to",
            "alpha",
            "--file",
            big_path.to_str().unwrap(),
        ],
        None,
    );
    wait_for(Duration::from_secs(10), || beta.queue_len() == 0);
    let got = alpha
        .inbox()
        .into_iter()
        .filter(|i| i["kind"] == "image")
        .count();
    assert_eq!(got, 2);
    let big_item = alpha
        .inbox()
        .into_iter()
        .find(|i| {
            i["kind"] == "image"
                && std::fs::metadata(i["path"].as_str().unwrap())
                    .ok()
                    .map(|m| m.len())
                    == Some(big.len() as u64)
        })
        .expect("the large image arrived");
    assert!(std::fs::read(big_item["path"].as_str().unwrap()).unwrap() == big);
    std::fs::remove_file(big_item["path"].as_str().unwrap()).unwrap();
    std::fs::remove_file(
        alpha
            .root
            .join("data/nirushare/inbox")
            .join(format!("{}.json", big_item["id"].as_str().unwrap())),
    )
    .unwrap();

    // A stalled sender holds one worker until its body deadline, while
    // other transfers keep flowing.
    let mut stalled = TcpStream::connect(("127.0.0.1", pb)).unwrap();
    write!(
        stalled,
        "POST /share HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer {TOKEN}\r\n\
         X-Nirushare-Transfer: 1-stalled\r\nX-Nirushare-Kind: text\r\n\
         Content-Length: 100\r\n\r\nonly a few bytes"
    )
    .unwrap();
    let stall_start = Instant::now();
    std::thread::sleep(Duration::from_millis(300));
    alpha.run(&["send", "--to", "beta", "--stdin"], Some(b"while stalled"));
    let took = wait_for(Duration::from_secs(5), || {
        texts(&beta.inbox()).contains(&"while stalled".to_string())
    });
    assert!(
        took < Duration::from_secs(3),
        "blocked by the stalled sender: {took:?}"
    );
    stalled
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut answer = String::new();
    let _ = stalled.read_to_string(&mut answer);
    assert!(answer.starts_with("HTTP/1.1 408"), "got {answer:?}");
    let held = stall_start.elapsed();
    assert!(
        held >= Duration::from_millis(2500) && held < Duration::from_secs(6),
        "{held:?}"
    );
    assert!(!texts(&beta.inbox()).iter().any(|t| t.starts_with("only")));

    // Protocol 1 compatibility with a 0.2.x sender, duplicates and auth.
    assert_eq!(post(pb, TOKEN, "1-old", "text", b"from 0.2"), Ok(200));
    assert_eq!(post(pb, TOKEN, "1-old", "text", b"from 0.2"), Ok(200));
    assert_eq!(
        texts(&beta.inbox())
            .iter()
            .filter(|t| *t == "from 0.2")
            .count(),
        1,
        "a retried transfer is stored once"
    );
    assert_eq!(post(pb, TOKEN, "1-old", "text", b"other"), Err(409));
    assert_eq!(post(pb, &"f".repeat(48), "1-x", "text", b"x"), Err(401));
    assert_eq!(post(pb, TOKEN, "../x", "text", b"x"), Err(400));
    assert_eq!(
        post(pb, TOKEN, "1-big", "text", &vec![b'a'; 1024 * 1024 + 1]),
        Err(413)
    );
    let health = ureq::get(&format!("http://127.0.0.1:{pb}/health"))
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    assert_eq!(health, "{\"ok\":true,\"protocol\":1}");
    assert!(!Path::new(&beta.root).join("nirushare").exists());

    // Inbox housekeeping: a preview first, removal only with --yes.
    std::thread::sleep(Duration::from_millis(1100));
    let before = alpha.inbox().len();
    let preview = alpha.run(&["inbox", "--older-than", "0d"], None);
    assert!(
        preview.contains("Would remove 1 inbox item") && preview.contains("and the file"),
        "{preview}"
    );
    assert!(path.exists() && alpha.inbox().len() == before);
    let done = alpha.run(&["inbox", "--older-than", "0d", "--yes"], None);
    assert!(done.contains("Removed 1."), "{done}");
    assert!(!path.exists() && alpha.inbox().is_empty());
    assert!(alpha
        .run(&["inbox", "--clear-read"], None)
        .contains("Nothing to remove"));
    assert!(!Path::new(&beta.root).join("Downloads/nirushare").exists());
}
