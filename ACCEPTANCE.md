# Acceptance matrix and final gate

## Evidence rules

Use assigned bare-metal hosts for development, builds and verification on
Debian, Arch and Void. Do not create containers, VMs, Xvfb/Xephyr/nested desktops
or extra seats. Existing CI and native unit/mock tests are supplementary.

For every required case record UTC, case/roadmap ID, native host role/distro/ABI/
init/session, source SHA, build/tag/asset hash, exact commands or physical steps,
real exit status, observed result, cleanup and PASS/FAIL/NOT RUN/BLOCKED.

Use synthetic text/images and controlled paired targets. Never paste private
clipboard/mail contents, accounts, tokens or full personal logs into evidence.
Screenshots prove appearance, not persistence/authentication/delivery correctness.

## Required matrix

| Area | Required cases and evidence |
|---|---|
| Native baseline | Inventory Debian/Arch/Void builders/receivers, Tailscale API availability and permissions, user sessions, fonts/clipboard/notifier/default mail and real XDG paths. No hidden root/admin dependency. |
| Text clipboard | Empty/normal/multiline, Swedish letters, emoji, large text at/beyond limit, unsupported/NUL policy, clipboard owner gone/changed, both image/text formats. Preview matches exact queued snapshot; Escape sends nothing. |
| Images | Clipboard PNG; selected PNG/JPEG/WebP; screenshot Review with edits/crop; originals preserved and digests identical; corrupt content, mislabeled MIME, oversize/dimension limits, source changed during snapshot, symlink/non-regular paths rejected. |
| Paths/storage | Spaces/Unicode/leading dashes; localized/custom/symlinked XDG Downloads; duplicate names; traversal/symlink/hardlink attack; non-default home/prefix; permissions; full/read-only owned storage fixtures; quota/staging recovery; no overwrite/partial final file. |
| Trust | Two-way pairing/approval, denied/expired/reused pairing, unpaired peer, spoofed sender/header/name/IP, revoked peer, node re-enrollment/receiver-key change, wrong local user/inbox. Tailscale Online does not equal app availability or ownership. |
| Network boundaries | Only tailnet addresses/interface accepted, LAN/public listener absent, IPv4/IPv6 policy documented/verified, ACL block and re-enable, tailscaled stop/restart/logout/IP change, protocol mismatch and receiver down. No silent firewall/ACL/admin change. |
| Normal transfer | Debian <-> Arch, Debian <-> Void, Arch <-> Void, text and representative images. Correct receiver user, bytes/digest/metadata and durable receipt; sender UI/CLI/notifier state agrees. |
| Offline | Queue to offline receiver; it returns while sender online; sender restarts/reboots before delivery; sender offline when receiver returns then sender returns; receiver service disabled vs node online. UI explains waiting correctly. |
| Crash/retry | Interrupt before/during upload, after durable receiver commit but before receipt, sender dies before receipt persistence, receiver crash/restart, stale staging and resumed retry. Exactly one committed inbox/file effect and no repeated historic notifications. |
| Broadcast | One/several/All my devices, self excluded, explicit confirmation, payload stored once, each recipient independent, online/offline/permanent failure mix, aggregate summary, new device not added retroactively, removed/revoked device never gets pending data. |
| Queue policy | Retry backoff/jitter/cap/concurrency, transient vs permanent error, expiry, per-recipient/whole cancel, cancellation racing receipt, quota overflow/recovery, source snapshot lifetime. No silent drop, wrong Delivered or claimed remote recall. |
| Local IPC | Version/error handling, malformed/large requests, wrong UID/socket permissions, concurrent clients, atomic public status, daemon absent/restarting. Bar/status reads cannot expose payload content or trigger network sends. |
| Notifications | Real NIRUNOTIFY Copy/Open/Open-folder/Open-inbox actions, callback lifetime, expired popup, DND, missing notifier, daemon/notifier restart and reconnect. Explicit Copy remains pasteable after the helper exits. Locked/unknown lock state reveals no sensitive content, actions cannot bypass unlock. |
| Launcher | Setup/pair/revoke, explicit Share clipboard/image, previews, recipients, inbox/outbox/cancel/copy/save/open. Mouse and keyboard, Tab/arrows/Enter/Escape, slow provider errors, small screen/HiDPI/light/dark/contrast/focus; no blocking UI. |
| Bar | Icon glyph/fallback, empty/unread/pending/rejected states, no color-only signaling, click/toggle/reload/restart, custom module order and overflow. No per-frame tailscale/network/process/file scan; unrelated modules still work. |
| Screenshot | Current edited image shared, no stale capture; queue survives Review close/temp deletion; cancel leaves existing clipboard/save behavior intact; absent share service does not break NIRUSHOT. |
| Email | Actual default Thunderbird on a native session, body/newlines/Unicode/quotes/URI characters, long-text attachment, image paths, delayed attachment read after helper exit, user cancels, no default client, Tailscale absent. Draft only; no automatic email or account changes. Additional advertised clients must be tested. |
| Service lifecycle | First setup/enable/disable, repeated start, systemd-user and Void session/runit startup, session bus readiness, lock/logout/login/reboot, Tailscale unavailable, listener rebind, graceful shutdown and queue persistence. No root/duplicate/orphan listener, stale socket or unrelated session change. |
| Installer/recovery | Native official install/update/reinstall, no-packages with zero package calls, missing runtime dependency, candidate/final asset verification, compatible migration/downgrade/rollback or explicit rejection, owned uninstall and data-preserving default; explicit purge scope preview. Do not uninstall a personal desktop as a test. |
| Existing suite | Clipboard/history, Tailscale SSH view, notifier, screenshot review/copy, volume/media/network bar modules, launcher startup and themes continue working with share absent/disabled/offline. No shortcut/module/config regression. |
| Performance | Five-minute idle CPU/RSS/wakeups with empty queue, waiting receiver with backoff, maximum supported payload/broadcast, startup/clipboard-preview/share/receipt latency, repeated transfer cycles and cleanup. No growing memory/process count, clipboard polling or whole-payload buffering. |
| Documentation/release | Correct default branches, normal clone/build/install instructions, English UI/help, public links/suite identity, exact final source/tag/assets/versions/ABI/deps, immutable checksums, retained release dependency closure and final running identities. |

Every mandatory behavior needs coverage. Run applicable broad flows on each
distro and all three cross-distro pairs in both directions. Assign physical
lock/reboot/input/mail tests to suitable real hosts. Missing physical hardware,
credentials or a down host is NOT RUN/BLOCKED, not a simulated PASS; continue
independent work and report the precise remaining external requirement.

## Failure-injection requirements

Native unit/protocol fixtures must cover framing/schema/parser/queue/fsync-order/
dedup/tombstone/expiry/cancel/quota/authorization/path-validation transitions,
including failpoints around atomic persistence and receipt transmission.
Use owned fixture files and fake peers; never fill a real personal disk or use
unapproved recipients to simulate failure. Tests may not send real mail, invoke
power, expose a public port or destroy user data.

## Native resource gate

Establish budgets in N0 and record before/after. The minimum behavior gate is:

- No automatic clipboard observation or continuous payload/file indexing.
- No recurring network peer scans/status subprocess each second/frame.
- One idle service per enabled receiver user; UI/bar/notifier do not create their
  own transport daemon. Empty queues produce no active remote transfer work.
- Pending offline delivery uses availability events or slow capped backoff.
- Payload I/O and image preview memory are bounded; broadcast reuses one snapshot.
- CLI/UI cancellation/timeout remains responsive and repeated cycles do not leak
  processes, sockets, descriptors, buffers or staging files.

Record measured RSS/CPU/latency rather than advertise a guessed performance claim.

## Final success gate — all required

1. N0–N7 are complete with all mandatory matrix cases passed.
2. Protocol/trust/user-boundaries and durable delivery/retry/broadcast behavior
   are verified, including offline reboot and lost-receipt regressions.
3. Launcher, bar, NIRUNOTIFY, NIRUSHOT and default-mail integrations are usable,
   themed, nonblocking and preserve their original functionality.
4. Final official assets are downloaded, checksum/identity verified, installed
   normally and activated on every assigned Debian/Arch/Void target.
5. Real final installed transfers and offline/broadcast/UI/mail flows pass after
   publication, not just against a build-tree candidate.
6. NIRUSHARE/NIRUX/affected READMEs/help are accurate and mutually linked.
7. RELEASES.md cleanup is complete with retained dependency/recovery evidence;
   task-created data/access/config/service artifacts are cleaned.
8. STATUS.md contains the final evidence ledger and no unresolved mandatory gate.

Do not stop with a final success report before this gate. If no executable work
remains solely because an external requirement is unavailable, use a precise
PARTIAL/BLOCKED report, not “fully operational” or “100% complete.”

## 0.3.0 results (2026-11-05)

Candidate: source at `v0.3.0`, built on othala (Debian 13). The candidate
service ran from a private temporary home on othala, listening on 127.0.0.1
through the test-only transport override (a `test-transport` build) while
using the real Tailscale CLI for peers; the installed 0.2.1 service was left
running. All temporary homes, test items and helper processes were removed.

| Case | Host | Result |
|---|---|---|
| Locked build and tests, rustc 1.85.1 and stable | othala | PASS: 18 unit tests, 1 two-instance test |
| Static musl release binary runs (`--version`, `status`, `inbox --clear-read`, `send` refusal) | void (runit), algiz (Arch) | PASS |
| Idle service, empty queue, `strace -f -e execve` for 60 s | othala | PASS: 0 execve, 0 CPU ticks (0.2.1: 12 `tailscale status` runs, 72 execve with PATH lookups, 11 ticks) |
| Upgrade over a legacy queue: 3 `delivered` entries with `created: 0`, 600 KiB payloads | othala (temp home) | PASS: archived to `history.jsonl` on start, queue directory empty, payloads removed, `created` taken from the entry mtime |
| New sender to 0.2.1 receiver: text via `send --stdin`, 8x8 PNG via `send --file` | othala -> void | PASS: both delivered on the first attempt, one `tailscale status` per send, digest identical on void; test items, file and dedup receipts removed from void afterwards |
| Offline receiver then online, stalled sender, 0.2.x-style sender, duplicates, 401/400/409/413, 3 MB image | two local instances (CI and othala) | PASS (`tests/two_instances.rs`) |

Deferred to the NIRUX deployment (needs the installed service on port 47831):
- 0.2.1 sender (void or algiz) to an installed 0.3.0 receiver, text and image.
- Both directions between installed 0.3.0 services; an offline device that comes
  back; `nirushare status` on the sender while it waits.
- A stalled sender against an installed receiver while another host sends.
- The upgrade over niruxwork's real queue (3 delivered legacy entries) and the
  60 s idle `strace` of the installed service there.
- `/proc/<pid>/exe` of the restarted service on the new generation, and the
  progress notification for an image over 2 MB on a real NIRUNOTIFY session.
