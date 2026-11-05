# NIRUSHARE

**Part of [NIRUX — A minimal X11 desktop by Nicklas Rudolfsson](https://github.com/nirucon/nirux).**

NIRUSHARE sends clipboard text, screenshots and images between your own trusted
NIRUX devices over Tailscale. Send to one device, several devices or **All my
devices**. Destinations that are offline receive the item through a durable
queue when they return.

**Current release: 0.3.0.** The transfer core, trusted-device records, durable
queue, inbox, CLI, NIRUX session service, launcher and bar entry points,
NIRUNOTIFY actions, NIRUSHOT Review sharing and email drafts are implemented.
Pairing is explicit; merely being in the same tailnet is not authorization.

## Use

NIRUX starts the receiving service in the NIRUWM session. In NIRU Launcher,
open **Share**, or use the CLI:

```sh
# One-time pairing. Show a token on one device and trust it on the other.
nirushare token
NIRUSHARE_TOKEN="<token from the other device>" nirushare trust other-device

nirushare send --clipboard --to other-device
nirushare send --clipboard --all
nirushare send --file picture.png --to other-device
printf 'some text' | nirushare send --stdin --to other-device
nirushare devices
nirushare inbox
nirushare status          # text in a terminal; JSON with --json or in a pipe
nirushare outbox          # queued items as JSON

# Inbox housekeeping: preview first, then the same command with --yes.
nirushare inbox --clear-read
nirushare inbox --older-than 30d
nirushare inbox --clear-read --older-than 30d --yes
```

`nirushare status` lists every trusted device (online or offline, last
successful contact) and every queued item with each recipient's state,
attempts, next retry and the last error in plain words, such as "othala is
offline" or "not trusted on othala", plus recently finished transfers.

Inbox cleanup removes inbox entries and, for images, only the received file
inside `Downloads/nirushare/`; files elsewhere and symlinks are kept. Nothing
is removed without `--yes`. Copy and Open mark an item read.

## Queue

Queued items live in `$XDG_STATE_HOME/nirushare/out/`. When every recipient
is final (delivered, cancelled, expired, rejected or failed) the item moves
to a compact record in `history.jsonl` (no content, newest 500 kept) and its
payload is removed. Pending items expire after 7 days. An offline or failing
recipient is retried with backoff from 5 s doubling to 5 minutes. With
nothing pending the service sleeps: `nirushare send` wakes it through a
private socket in `$XDG_RUNTIME_DIR/nirushare/`, and it does not run
Tailscale commands while idle. Images over 2 MB show one progress
notification, updated in place where notify-send supports it. Dedup receipts
for received transfers are kept for 30 days.

Received images are stored under the real XDG Downloads directory in
`nirushare/`. Received text is an inbox item; copy it or save it explicitly.
NIRUNOTIFY offers Copy, Open and Open folder actions where applicable.

## Privacy and scope

- Tailscale is the only transport. NIRUSHARE does not use a central cloud,
  telemetry, automatic clipboard synchronization or an open file-drop service.
- Only explicitly trusted receiver identities can send into an inbox.
- The queue is durable, but offline delivery requires sender and receiver to be
  online together eventually; it is not cloud storage.
- Received content is not executed or opened automatically.
- Configuration, trust records and inbox permissions are user-scoped.
- Uninstalling preserves inbox/trust/queue by default; an explicit purge must
  preview what it removes.

## NIRUX integration

- Receiving service: NIRUWM session.
- Launcher: Share view, pairing and inbox.
- Bar: optional Share entry (one icon; no permanent text).
- Notifications: NIRUNOTIFY actions.
- NIRUSHOT: Review image sharing.
- NIRUCLIP/email: an image attachment opens a mail draft; nothing is sent
  silently.

NIRUX owns the installed generation. Use NIRUX for updates and removal; do not
overwrite NIRUX-owned links by hand.

## Limits

There is no relay, chat, central inbox or cloud-style delivery guarantee.
Tailscale must work between the devices. The full native acceptance matrix and
remaining PAM/trust UI edge cases are tracked in [ACCEPTANCE.md](ACCEPTANCE.md);
the durable product/transport contract is in [SPECIFICATION.md](SPECIFICATION.md).
Repository state must not be described as finished production acceptance until
that checklist passes.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
# Two local instances on 127.0.0.1 through a test-only transport override.
# The feature is never enabled in release builds.
cargo test --locked --features test-transport
cargo build --release --locked
```

The declared `rust-version` is 1.85 (Debian 13); `.cargo/config.toml` makes
`cargo update` respect it. Releases are built by the tag workflow as a static
musl binary with a generated `SHA256SUMS`.

MIT. See [LICENSE](LICENSE).
