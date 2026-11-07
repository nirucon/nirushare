# NIRUSHARE

Send clipboard text, screenshots and images between your own trusted NIRUX devices over Tailscale, or post them to a Discord channel through a webhook you set up.

**Status: alpha** · Part of the [NIRUX](https://github.com/nirucon/nirux) suite · [MIT License](LICENSE)

## About

NIRUSHARE sends clipboard text, screenshots and images between your own trusted
NIRUX devices over [Tailscale](https://tailscale.com). Send to one device,
several devices or **All my devices**. Destinations that are offline receive the
item through a durable queue when they return.

Pairing is explicit: merely being in the same tailnet is not authorization.
There is no central cloud, relay, telemetry, automatic clipboard sync or open
file-drop service.

## Features

- Send the clipboard, a file or stdin to one, several or all trusted devices.
- Explicit pairing with a one-time token; only trusted identities can send into
  an inbox.
- Durable outbox with retries (5 s backoff doubling to 5 minutes) and a 7-day
  expiry.
- Items up to 25 MB (text up to 1 MB).
- Inbox for received text; received images go to `Downloads/nirushare/`.
- `status` in plain words ("laptop is offline", "not trusted on laptop"),
  with attempts, next retry and recent transfers.
- Idle service: no Tailscale commands while nothing is pending; `send` wakes it
  through a private socket.
- Optional Discord webhooks: post clipboard text, links, screenshots, images
  and files to a channel, always after a preview that names the target, with
  **Undo** for 5 minutes. None are configured by default.
- NIRUX integration: session service, NIRU Launcher **Share** view, an optional
  NIRUBAR entry, NIRUNOTIFY actions (Copy, Open, Open folder), sharing from the
  NIRUSHOT review, and mail drafts.

## Requirements

- Linux (Debian, Arch and Void glibc, x86_64); the release binary is a static
  musl build.
- Tailscale installed, running and logged in on every device, with the devices
  able to reach each other.
- `xclip` (clipboard), `xdg-user-dir`, `xdg-open`/`xdg-email`, and optionally
  `notify-send`.

## Installation

### With NIRUX

NIRUSHARE is a core component: `nirux install` and `nirux update` install and
update it from the pinned manifest, and `niruwm-session` starts
`nirushare serve` in the NIRUWM session. NIRUX owns the installed generation;
use NIRUX for updates and removal and do not overwrite NIRUX-owned links by hand.

### Standalone

Not supported as a separate install; each release carries a static `nirushare`
binary and `SHA256SUMS`, and `nirushare serve` must run in your session for a
device to receive.

## Usage

In NIRU Launcher, open **Share**, or use the CLI:

```sh
# One-time pairing: show a token on one device and trust it on the other.
nirushare token
NIRUSHARE_TOKEN="<token from the other device>" nirushare trust other-device
nirushare trust other-device TOKEN          # the same, token as an argument
nirushare revoke other-device

nirushare send --clipboard --to other-device
nirushare send --clipboard --all
nirushare send --file picture.png --to other-device
printf 'some text' | nirushare send --stdin --to other-device

nirushare devices
nirushare inbox
nirushare copy ID
nirushare open ID
nirushare email                 # open a mail draft with the clipboard content
nirushare status                # text in a terminal; JSON with --json or in a pipe (--text forces text)
nirushare outbox                # queued items as JSON
nirushare serve                 # the receiving service (started by the NIRUX session)

# Inbox housekeeping: preview first, then the same command with --yes.
nirushare inbox --clear-read
nirushare inbox --older-than 30d
nirushare inbox --clear-read --older-than 30d --yes
```

`nirushare status` lists every trusted device (online or offline, last
successful contact) and every queued item with each recipient's state,
attempts, next retry and the last error in plain words, plus recently finished
transfers.

Inbox cleanup removes inbox entries and, for images, only the received file
inside `Downloads/nirushare/`; files elsewhere and symlinks are kept. Nothing is
removed without `--yes`. Copy and Open mark an item read.

`nirushare email` opens a draft with `xdg-email`; an image becomes an
attachment. Nothing is ever sent silently.

## Discord webhooks

NIRUSHARE can post to Discord channels through webhooks. Nothing is set up by
default: a webhook only exists once you add it to
`~/.config/nirushare/webhooks.toml`. You can add as many named webhooks as you
like. [docs/webhooks.example.toml](docs/webhooks.example.toml) is a complete
example.

```toml
[webhook.friends]
url = "https://discord.com/api/webhooks/<id>/<token>"
username = "Niru"            # optional
avatar_url = "https://..."   # optional
embeds = false               # optional: no link previews under links
max_file_mb = 10             # optional: the server's upload limit
```

**Keep the URL secret.** Anyone who has a webhook URL can post to that
channel. The file must be private (`chmod 600`). If it is readable by other
users, NIRUSHARE sets it to 0600 and tells you; if it belongs to another user
or is a symbolic link, NIRUSHARE refuses to use it. The URL is never printed,
logged, written to the queue, put on a command line or shown in an error.
Everywhere else a webhook is called by its name, as `discord:NAME`. Only
`https://discord.com` (or `discordapp.com`, `canary.`, `ptb.`) webhook URLs
are accepted.

**What can be sent:**
- Clipboard text. Up to 4 messages of 2000 characters are posted as messages,
  split at line breaks. Longer text is attached as a `.txt` file.
- A link (a single URL). Discord shows its preview unless `embeds = false`.
- A screenshot, image or any other file, as an attachment. The limit is
  `max_file_mb`, 10 MB by default, which is what a server without boosts allows.

Mentions are never parsed, so pasted text with `@everyone` or a role mention
pings nobody.

**Nothing is sent without a preview.** The preview shows the target, the kind
of content, its size, how many messages it becomes and the first characters
of the text. The content is fixed when the preview is made: what the preview
showed is what is sent, even if the clipboard changes in the meantime.

- **NIRU Launcher, Share view:** each webhook is listed as
  *Share clipboard to Discord · NAME*. Choosing one opens a preview view with
  **Send** and **Cancel**.
- **NIRUSHOT review:** **u** (share) opens the same Share view with the
  screenshot, so the Discord rows send the screenshot.
- **CLI:**

```sh
nirushare discord list                       # names only, never URLs
nirushare discord check                      # validate webhooks.toml, never posts
nirushare discord check --online             # also ask Discord if each webhook exists (a GET)
nirushare send --to discord:friends          # the clipboard: preview, then "Send? [y/N]"
nirushare send --to discord:friends shot.png # a file
printf 'text' | nirushare send --to discord:friends -
nirushare send --to discord:friends notes.md --yes   # no question; for scripts
nirushare discord undo ID                    # delete it again (within 5 minutes)
```

Without a terminal to ask on, `send --to discord:NAME` shows the preview and
sends nothing unless `--yes` is given. For other programs,
`nirushare discord preview --to NAME [--file F|--stdin|--clipboard] --json`
stages the content and prints the preview with a `stage` id;
`nirushare discord confirm ID` sends exactly that, and
`nirushare discord cancel ID` discards it. A preview expires after an hour.

**Undo.** Every message is posted with `?wait=true`, so Discord returns its id.
The "Sent to Discord" notification has an **Undo** action for 5 minutes, which
deletes all messages of the item. `nirushare discord undo ID` does the same.
After 5 minutes Undo is refused with a clear message.

**Queue and retries.** A Discord item goes through the same private outbox as
device transfers. The first attempt is made at once. When Discord rate-limits
(HTTP 429), the next attempt waits exactly as long as Discord asks
(`Retry-After`). Server errors and network failures are retried with the
usual backoff (5 s doubling to 5 minutes, for up to 7 days). A long text that
was cut off half-way continues with the next message and never posts a part
twice. A deleted webhook (401/403/404), a file that is too large (413) or a
rejected message (400) fails at once with the reason in the notification and
in `nirushare status`.

## Configuration

Pairing is done with `token`, `trust` and `revoke`, which write the trust
records to `~/.config/nirushare/config.json`. The only file you edit by hand is
the optional `~/.config/nirushare/webhooks.toml` (see Discord webhooks).
The receiving port is fixed (TCP 47831, reached over Tailscale only).

## Files and paths

| Path | Purpose |
|---|---|
| `~/.config/nirushare/config.json` | Own token and trusted devices |
| `~/.config/nirushare/webhooks.toml` | Discord webhooks (optional, 0600, edited by you) |
| `~/.local/state/nirushare/discord-staged/` | Previews waiting for Send (1 hour) |
| `~/.local/state/nirushare/discord-sent/` | Message ids for Undo (no content, 1 day) |
| `~/.local/share/nirushare/inbox/` | Received items (metadata and text) |
| `~/.local/share/nirushare/dedup/` | Dedup receipts, kept for 30 days |
| `~/.local/state/nirushare/out/` | Queued outgoing items |
| `~/.local/state/nirushare/history.jsonl` | Finished transfers (no content, newest 500 kept) |
| `$XDG_RUNTIME_DIR/nirushare/` | Private wake-up socket |
| XDG Downloads `/nirushare/` | Received images |

All directories are private to the user, and XDG variables replace the defaults
when set. When every recipient of a queued item is final (delivered, cancelled,
expired, rejected or failed) the item becomes a compact history record and its
payload is removed. Images over 2 MB show one progress notification, updated in
place where `notify-send` supports it.

## Troubleshooting

- **"tailscale is not running or not logged in":** start Tailscale and log in;
  it is the only transport.
- **"not trusted on DEVICE":** pair again: `nirushare token` on that device,
  then `nirushare trust DEVICE TOKEN` here.
- **An item stays queued:** both devices must eventually be online at the same
  time; this is a queue, not cloud storage. Pending items expire after 7 days.
- **"no Discord webhook named NAME":** add it to
  `~/.config/nirushare/webhooks.toml`; `nirushare discord check` validates the
  file and `--online` asks Discord whether the webhook still exists.
- **Nothing is received:** check that `nirushare serve` runs in the session
  (its log is `~/.local/state/niruwm/nirushare.log`).

### Privacy and scope

- Tailscale is the only transport between devices. The one exception is a
  Discord webhook that you configure yourself, used only after you confirm a
  preview. No central cloud, telemetry, automatic clipboard synchronization or
  open file-drop service.
- Received content is never executed or opened automatically.
- Configuration, trust records and inbox permissions are user-scoped.
- There is no relay, chat, central inbox or cloud-style delivery guarantee.

The product and transport contract is in
[SPECIFICATION.md](SPECIFICATION.md), and the native acceptance matrix with the
remaining edge cases in [ACCEPTANCE.md](ACCEPTANCE.md).

## Uninstall

NIRUSHARE is removed with the suite by `nirux uninstall`. Uninstalling keeps the
inbox, trust records and queue; delete `~/.config/nirushare`,
`~/.local/share/nirushare`, `~/.local/state/nirushare` and
`Downloads/nirushare` yourself if you want them gone.

## Building from source

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
# Two local instances, and Discord against a local mock server, on 127.0.0.1
# through a test-only transport override. The feature is never enabled in
# release builds, which accept only https://discord.com webhooks.
cargo test --locked --features test-transport
cargo build --release --locked
```

The declared `rust-version` is 1.85 (Debian 13); `.cargo/config.toml` makes
`cargo update` respect it. Releases are built by the tag workflow as a static
musl binary with a generated `SHA256SUMS`.

## Status, disclaimer and how it's made

**Status: alpha.** Sending between my own machines works every day, but the full
acceptance matrix in [ACCEPTANCE.md](ACCEPTANCE.md) has not passed yet.

NIRUX and every niru* project are made by me, Nicklas Rudolfsson
([@nirucon](https://github.com/nirucon)), for me. They are tailored to my own
needs, preferences and workflows, on my own machines. If they fit yours too,
that's a nice bonus.

- **Free and open source.** Anyone is free to use, study, change and share
  it under the [MIT License](LICENSE).
- **No guarantees.** There is no warranty of any kind. Use it at your own
  risk, read what you install and keep backups of anything that matters.
- **How it's made.** Some of these projects I built from scratch by hand. In
  more recent releases I have used Claude, Anthropic's AI, to do much of the
  heavy lifting under my direction: I decide what to build and how it should
  behave, and I test it on my own machines. It saves me a lot of time, and
  the result is quite OK.
- **Alpha, beta, dev, and maybe forever.** Most things here are alpha, beta
  or dev, and may stay that way. Maybe one day there will be a real stable
  release, but given how fast development moves, and how I am as a person,
  maybe never :)

| Label | Meaning |
|---|---|
| dev | Under active construction; may not build, install or work yet. |
| alpha | Works on my machines, but incomplete or rough; expect breaking changes. |
| beta | In daily use on my machines and mostly complete; rough edges remain. |

## License

MIT License, see [LICENSE](LICENSE). Copyright (c) 2026 Nicklas Rudolfsson.
