# Changelog

## 0.4.0 (2026-11-07)

Discord webhooks.

- New: send clipboard text, links, screenshots, images and files to Discord
  channels through webhooks configured in `~/.config/nirushare/webhooks.toml`.
  No webhook exists by default. Several named webhooks are allowed, each with
  an optional `username`, `avatar_url`, `embeds = false` (no link previews) and
  `max_file_mb`. See README, "Discord webhooks", and
  `docs/webhooks.example.toml`.
- The webhook URL is treated as a credential. It is never printed, logged,
  queued, passed on a command line or shown in an error. webhooks.toml must be
  a private regular file: a readable one is set to 0600 with a warning, a
  symlink or another user's file is refused. Only https://discord.com webhook
  URLs are accepted.
- Nothing is sent without a preview naming the target:
  `nirushare send --to discord:NAME [FILE|-|--clipboard]` shows it and asks, or
  needs `--yes` without a terminal. `nirushare discord preview ... --json`
  stages content for other programs, `discord confirm ID` sends exactly the
  staged content, and `discord cancel ID` discards it.
- Undo: messages are posted with `?wait=true`; the notification's Undo action,
  or `nirushare discord undo ID`, deletes them within 5 minutes.
- Text longer than 4 messages of 2000 characters is attached as a `.txt` file;
  shorter text is split at line breaks. Mentions are never parsed.
- Delivery uses the outbox: HTTP 429 waits for `Retry-After`, 5xx and network
  errors back off, other 4xx fail at once with a clear reason. A text cut off
  half-way continues with the next part and never posts a part twice. The
  service and a CLI never work on the same entry at once.
- `nirushare discord list [--json]` (names only) and
  `nirushare discord check [--online]` (validates the file; `--online` asks
  Discord whether each webhook exists, with a GET; never posts).
- `nirushare status` lists the webhook names.
- `send` also takes a file as a plain argument and `-` for stdin.
- The HTTP client now has TLS (rustls, bundled roots) for Discord; device
  transfers still use plain HTTP inside Tailscale, as before.
- Requires NIRU Launcher 0.4.0 for the Discord rows in the Share view (older
  launchers keep working without them).
