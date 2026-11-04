# Agent assignment: NIRUSHARE 0.3.0, a one-shot build, queue, security and UX update

## 0. Quick start

1. Read this file, `README.md`, `SPECIFICATION.md`, `ACCEPTANCE.md` and `src/main.rs`.
   The audit baseline is `f2f2c6d` (0.2.1, pinned by NIRUX 0.12.199). Revalidate each item
   against current HEAD and skip items that are already fixed, noting that you did.
2. Implement **S1–S9, then U1–U3 in order**. Commit per item. Keep fmt, clippy
   `-D warnings` and tests green from S2 on.
3. Pass the gates in §4, release in §5 and report in §6.

This is an **execution** assignment. Do not stop after a plan or a branch. Make sensible
decisions, document them, and continue without routine questions.

## 1. Invariants (from SPECIFICATION.md, keep them)

- Tailscale is the only transport. Pairing is explicit: being in the same tailnet is
  **not** authorization. The remote node identity is verified with `tailscale whois`.
- Received content is never executed or opened automatically. Trust records, the queue
  and the inbox stay user-private (0700/0600).
- Protocol version 1 stays compatible: an old sender talking to a new receiver, and the
  other way round, must both work.
- Uninstalling preserves user data unless an explicit purge is previewed.
- Keep `rust-version = 1.85`. Locked builds must work on rustc 1.85.

## 2. Findings (verified 2026-11-04 on niruxwork, live service read-only)

### S1: The locked build fails on the declared Rust version (high)
`Cargo.lock` pulls `icu_collections`/`icu_locale_core` 2.4.0 (rustc 1.88) through the
`url` stack. Debian 13's rustc 1.85.1 cannot build it. **Fix:**
`CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update`. This was verified to
build and test on 1.85. Also add `resolver.incompatible-rust-versions = "fallback"` in
`.cargo/config.toml` so future updates respect the declared version.

### S2: There is no CI, and clippy fails
There is no `.github/workflows/ci.yml`, and
`cargo clippy --all-targets -- -D warnings` fails with `items after a test module`
(`src/main.rs:900`). **Fix:** move the test module last. Add CI with fmt, clippy, test,
a release build and an MSRV job on 1.85, the same as the other suite repos.

### S3: Delivered transfers are never removed, and the worker re-reads every payload (high)
`process_queue()` reads `meta.json` **and the full `payload`** (up to 25 MB) for every
directory in `~/.local/state/nirushare/out` every 5 s, including recipients that are
already `delivered`. Nothing ever deletes a finished entry. On niruxwork, 3 delivered
entries were still in the queue.
**Fix:**
- Skip entries whose recipients are all `delivered`, `cancelled`, `expired` or `rejected`,
  without reading the payload.
- Move them to a compact history record: id, kind, name, recipients, timestamps, and no
  payload.
- Remove the payload once all recipients are final.
- Read the payload only right before a send attempt.

### S4: Legacy entries can never expire
Expiry requires `created > 0`, but all three entries on niruxwork have `created: 0`
(written before the field existed), so `EXPIRY_SECS` never applies to them. **Fix:** when
`created == 0`, use the entry directory's mtime and write it back into `created` once.

### S5: The idle worker polls Tailscale forever
`queue_worker` runs `process_queue()` twice every 5 s, and each run spawns
`tailscale status --json` even when nothing is pending. A live trace showed 4 spawns in
20 s.
**Fix:**
- With no pending recipients, sleep until a new outgoing item is created. Use a wake pipe
  or inotify on `out/`, which `send` already writes to.
- With pending recipients, use exponential backoff per recipient (5 s → 5 min max) instead
  of a fixed 5 s, and call `tailscale status` once per cycle.
- Remove the duplicate `process_queue()` call.

### S6: Token comparison is not constant-time
`credentials()` uses `t.token == token`. **Fix:** a constant-time comparison (hash both
with SHA-256 first, then compare fixed-length digests in constant time; no new crate is
needed). Keep the whois verification after it.

### S7: One slow sender blocks every receive
`serve()` handles requests one at a time on the listener thread, and the body read has no
deadline. A slow or stalled trusted peer blocks every other sender. **Fix:**
- Handle each request on a small bounded worker pool (2–4).
- Set a 60 s read deadline for the body.
- Keep authorization before the body is read, as it is today.

### S8: The dedup directory grows forever
`data/nirushare/dedup/<sender>/<id>.json` gains one file per received transfer and is
never pruned. **Fix:** prune entries older than 30 days on service start and once a day.

### S9: A generated file is committed
`SHA256SUMS` in the repo root refers to a local build. **Fix:** remove it, ignore it, and
confirm that the release workflow generates its own checksums.

## 3. UX

### U1: Status that tells the truth
`nirushare status` (and JSON for the launcher's Share view) shows:
- per outgoing item: recipients, state, attempts, the next retry time, and the last error
  in plain words ("othala is offline", "not trusted on othala");
- per trusted device: online or offline, and the last successful contact.

### U2: Inbox housekeeping
`nirushare inbox --clear-read` and `--older-than 30d`, with a preview list first and
`--yes` to apply. Never delete files in Downloads without that explicit confirmation.

### U3: Progress for large images
While sending an item over 2 MB, update one notification with the progress per recipient,
using notify-send's replace-id where supported. Otherwise notify only at the start and the
end.

Out of scope: a relay or cloud, chat, automatic clipboard sync, and other transports.

## 4. Gates

Automated:
- fmt, clippy `-D warnings`, `cargo test --locked`, a release build, and the MSRV job.
- New tests:
  - queue state transitions, including skipping final entries without reading the
    payload (assert it with a payload file that cannot be read),
  - the legacy `created == 0` expiry,
  - backoff timing,
  - constant-time compare behavior,
  - dedup pruning,
  - and a local two-instance test on 127.0.0.1 with a test-only `NIRUSHARE_TEST_*`
    transport override that bypasses Tailscale. That override must be impossible to
    enable in release builds: put it behind `cfg(test)` or a feature that the release
    workflow does not build.

Native, on authorized hosts (niruxwork plus othala or void, which are already trusted
peers on the owner's tailnet):
- Send text and an image both ways.
- Send to an offline device, then bring it online.
- Check the stalled-sender behavior.
- Upgrade over the existing queue on niruxwork (the 3 delivered legacy entries must be
  archived and their payloads removed).
- Trace the idle service with `strace -f -e execve` for 60 s: 0 spawns with an empty
  queue.
- Do not remove the owner's trust records or inbox items.

## 5. Release

Bump to **0.3.0**. Tag `v0.3.0` through the normal workflow and verify the assets and
checksums. Then pin it in `nirucon/nirux` `manifest`:
- If `nirux/tools/suite-pin` exists, use it.
- Otherwise bump the manifest the usual way. Change only the nirushare line, and rebase if
  other pins land at the same time.

Install through `nirux update` on every host that tests it, and verify that the running
service (`/proc/<pid>/exe`) was restarted on the new generation.

## 6. Final report

For each item S1–S9 and U1–U3: the change, the tests and the evidence (idle spawns and
queue directory contents before and after, cross-host transfer results). Include release
URLs, installed versions and remaining limits. Update ACCEPTANCE.md with the native
results. Mark anything unfinished as **PARTIAL** and give the exact remaining steps.
