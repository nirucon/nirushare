# NIRUSHARE specification

Everything here is required first-release scope unless explicitly marked deferred.
UI labels are English. Exact CLI syntax and dependency choices may be refined
after inspecting current suite APIs, but the behavior contracts must be preserved.

## 1. Product scope

Deliver:

1. Explicit clipboard text and PNG-image sharing.
2. Selected PNG/JPEG/WebP image-file sharing, preserving originals.
3. One, several or **All my devices** recipients over Tailscale.
4. Durable offline queue, retries, per-recipient receipts, cancellation and expiry.
5. Explicit pairing/trust and a user-scoped receiving service.
6. NIRU Launcher Share view, inbox and outbox.
7. NIRUNOTIFY reception/actions and an optional NIRUBAR indicator.
8. NIRUSHOT Review -> Share integration using the same transport/queue.
9. Share by email -> a draft in the default mail client, never automatic send.
10. Stable CLI/local IPC/status schemas, installers, docs and accepted native rollout.

Deferred: chat, read receipts, central/cloud relay, automatic clipboard-history
sync, folders, arbitrary executable/file types, public links, Internet-facing
services, large-file transfer, remote command execution and smartphone clients.
Do not implement these opportunistically before the required delivery is complete.

## 2. Recipient and trust model

Each trusted destination is a stable Tailscale node identity plus an explicit
NIRUSHARE receiver identity/inbox belonging to a local OS user. A display name,
hostname or IP is not sufficient identity. Do not merge different users on one
machine or treat a machine reinstall as the same paired receiver automatically.

- Discovery uses the local Tailscale daemon's supported APIs/status, with bounded
  fallback where required. Do not enumerate public networks or external accounts.
- Keep known offline peers visible. Existing suite peer lists may deliberately
  filter offline peers; extend/reuse the provider without breaking existing SSH views.
- Confirm application capability and compatible protocol before calling a peer
  available. Tailscale installed/Online=true alone does not prove NIRUSHARE readiness.
- The trusted **My devices** group is explicit. Tailnet members, tagged nodes and
  shared devices are not automatically owned or authorized NIRUSHARE recipients.
- Pairing must prove the expected remote node and target receiver; use a deliberate
  approval/code flow or an equivalent documented authenticated procedure. Bound
  attempts/expiry; do not pass or print pairing secrets in normal logs/argv.
- Unknown/revoked peers cannot send into the inbox. Incoming identity must be
  verified through the actual trusted transport and supported Tailscale identity
  mechanism; never trust a claimed JSON sender or an HTTP header.
- Verify tailscaled access permission requirements; never grant broad daemon
  admin privileges merely for discovery/identity queries. Fail closed when the
  required identity validation cannot be performed.
- Re-enrollment, key/receiver identity changes and revocation need explicit
  reconciliation, not silent rebinding by hostname or reused address.
- Pairing records the actual receiver endpoint. Different users on the same node
  must not fight over one fixed port, share inbox permissions or silently receive
  each other's messages. Handle conflicts explicitly and test user separation.

### All my devices

Show **All my devices · N devices** above individual recipients. It is an
intentional choice, never a preselected broadcast default.

- Snapshot trusted eligible receiver identities when Send is confirmed.
- Exclude the current receiver/device as appropriate; avoid loopback duplicates.
- Store payload once and maintain separate recipient states.
- A failed/offline receiver cannot block delivery to the others.
- A newly added device does not receive old broadcasts.
- A removed/revoked device cannot receive an old queued item after revocation.
- Allow cancelling one pending recipient or the entire outgoing transfer.
- Present a single aggregate notification/status, with individual detail available.

Named custom groups are deferred; do not expand scope into a contact-management app.

## 3. Transfer and offline contract

There is no central storage service. Delivery requires the sender AND destination
services eventually online together. Sender shutdown, logout/service stop or a
tailnet ACL denial means waiting. Explain this in UI/help rather than imply the
recipient can fetch data from a powered-off sender.

### User-visible states

```text
Queued -> Waiting for device -> Sending -> Delivered
                                |
                                +-> Retry scheduled

Cancelled | Expired | Rejected | Failed (action required)
```

Differentiate Offline, Service unavailable, Not paired, Incompatible protocol,
Authorization denied, Disk/quota full and Unknown. Never display failure as success.

### Durability and protocol

- Create a transfer UUID and immutable payload snapshot before reporting Queued.
  Later clipboard/file changes must not alter queued content.
- Use a versioned bounded protocol: sender/receiver identity, transfer ID,
  content type, byte length, payload digest and optional untrusted original name.
- Authenticate before accepting metadata/payload. Validate lengths/limits first;
  stream to private staging, compute digest and enforce the announced size.
- Commit payload and metadata durably before issuing a Delivered receipt. Document
  atomic rename/fsync ordering and recovery on every supported filesystem path.
- Delivery means persisted at the correct receiver, not read, opened or copied.
- Use at-least-once retry with idempotent receiving effects: a lost receipt or
  sender/receiver crash must not create duplicate files, inbox items or notifications.
- Persist deduplication receipts/tombstones for at least the supported retry/expiry
  window. A retry after receiver restart must return the existing durable receipt.
- Retry only transient conditions with capped exponential backoff and jitter;
  retry sooner on verified peer/service availability changes. Bound concurrency.
- Persist queue, recipients, attempts, expiry and last error over logout/reboot.
  Permanent rejection/revocation requires a clear action, not endless blind retries.
- Resume interrupted small-image transfers safely. Full chunk-range resume is
  optional in v1; discarding only the owned incomplete staging object and retrying
  from zero is acceptable when bounded and idempotent.
- Cancellation races must be truthful: an acknowledged Delivered item cannot be
  advertised as remotely recalled. Local deletion is not receiver-side deletion.
- Reject unsupported protocol/content and unknown fields/versions according to a
  documented compatibility policy. Never guess an incompatible schema.

Initial conservative defaults: seven-day pending expiry, text up to 1 MiB, images
up to 25 MiB and an outgoing spool cap of 512 MiB. Make limits configurable within
validated bounds; enforce receiver-side independent limits, per-peer request/rate
limits, staging reservation and an image dimension/decoded-pixel ceiling. Test
expiry/disk full and quota recovery. Do not silently discard accepted queued data.

## 4. Storage and filesystem behavior

Honor real XDG variables and user-dir configuration, including localized/custom
Downloads paths and symlinks. Never invent `~/Downloads` when another folder is
configured. Explain a fallback if Downloads cannot be resolved.

| Purpose | Default |
|---|---|
| Received images | XDG Downloads + `/nirushare/`, normally `~/Downloads/nirushare/` |
| Text inbox and persistent received metadata | `$XDG_DATA_HOME/nirushare/` |
| Outgoing queue, payload snapshots and delivery state | `$XDG_STATE_HOME/nirushare/` |
| Config/trust/preferences | `$XDG_CONFIG_HOME/nirushare/` |
| Local control socket/ephemeral state | private `$XDG_RUNTIME_DIR/nirushare/` |

- Internal directories are private (0700), sensitive files 0600; honor ownership
  and never make payload/trust state globally readable. No plaintext payload logs.
- Do not claim the disk spool is encrypted at rest unless implemented and tested
  with a sound key lifecycle. Tailscale transport encryption does not encrypt disk.
- Never concatenate remote path components into local destination paths. Use
  generated collision-resistant names, bounded sanitized display names and
  validated MIME/extension handling. Preserve original image bytes.
- Reject traversal, symlink/hardlink tricks, non-regular selected source files,
  partial writes, overwritten existing files and cross-user socket access.
- Use private staging and atomic finalization; crashes, concurrent deliveries,
  read-only/full disks and restarts leave either a valid committed item or an
  identifiable task-owned incomplete staging object, never a misleading final file.
- Validate image type from content and bound decoding/thumbnail memory. Do not
  render SVG/HTML or execute content as an image. No automatic file opening.
- Text preserves UTF-8/newlines; any byte/NUL restrictions are explicit and tested.
  Do not normalize clipboard contents silently or damage Unicode/emoji.
- Received text is an inbox item, not automatically a file in Downloads. Offer
  Copy and Save as text. Received images are real files linked by inbox metadata.
- Inbox dismissal/read state does not delete downloaded files. Never delete real
  received files or owner-queued items as generic cache cleanup. Cleanup policies
  must distinguish metadata, receipts, staging, internal delivered copies and files.
- Uninstall preserves inbox/trust/queue by default. Explicit purge previews exact
  owned scope, never recursively deletes the whole Downloads directory.

## 5. Transport and local service

Use Rust with a small, justified dependency set and standard maintained protocol/
transport libraries. No custom crypto or GTK/Qt/WebView for the core service/CLI.
Reuse existing NIRUX UI components rather than create a second UI stack.

- One per-user service owns receiving, retries, discovery cache and state.
  A listener is necessary for background reception; do not claim daemon-free
  reception. No clipboard owner/poller or file-indexing daemon.
- Default reception exists while the user's NIRUX session/service is running.
  Pre-login/system-wide reception is deferred. Lock-screen behavior is separate
  from logout: preserve the user's privacy without bypassing session security.
- Installation makes the feature available; network receiving/trust is explicitly
  enabled during setup, never by secretly enrolling or changing Tailscale login.
  The execution assignment may authorize enabling it on its named test targets.
- Start/stop/restart through a supported user/session mechanism on systemd and
  Void/runit. No orphan processes, duplicate listeners, root receiver or competing
  startup paths. Verify session bus readiness and normal logout cleanup.
- Bind only to actual Tailscale addresses/interfaces, not `0.0.0.0`, `::` or LAN.
  Validate route/interface/identity behavior and rebind safely after Tailscale
  restart/address changes; absence/logout must never expose a fallback public port.
- Choose/document a tailnet-only listener/port and real firewall/ACL requirements.
  Do not modify tailnet ACLs, enable Tailscale Funnel/Serve public exposure or
  relax host firewalls silently. No NAT traversal or internet service of our own.
- Tailscale provides encrypted node transport; application pairing and authorization
  still apply. If using TLS, use standard validation/provisioning, not a fake
  self-signed trust bypass. Any relay/proxy path must preserve verified peer identity.
- Local IPC is versioned, user-owned and permission-checked, with peer-UID checks
  where available. A local unauthenticated HTTP control endpoint is not acceptable.
- No network traffic when nothing is queued except the minimum justified local
  daemon subscriptions. Do not spawn tailscale CLI commands every frame/second.
  Observe supported local availability events or use slow adaptive bounded fallback.
- Stream transfer I/O, limit parallel peers/buffers, reuse snapshots for broadcast,
  and yield under backpressure. UI/network/disk/cancellation paths stay bounded.
- A versioned atomic local status snapshot exposes counts/provider/queue state
  only. The bar must not read clipboard or full inbox content to render an icon.

## 6. CLI and integration contract

Design a small documented CLI/local API supporting these operations (syntax may
be refined once, then tested as a contract):

```text
nirushare --version | --help | --check-config
nirushare service
nirushare devices [--json]
nirushare pair ... | revoke ...
nirushare send --clipboard --to DEVICE
nirushare send --clipboard --all-my-devices
nirushare send --image PATH --to DEVICE
nirushare inbox [--json] | outbox [--json] | status --json
nirushare copy ITEM | open ITEM | save-text ITEM PATH
nirushare cancel TRANSFER [--recipient DEVICE]
nirushare share-email ...
```

Use IDs and argv, not shell text. STDIN should support explicit text input without
putting secrets in argv/history. Machine-readable stdout stays clean; errors use
stderr and stable nonzero exit codes. Never autostart a remote service or send
material just by listing peers/status. Provider absence is a truthful cheap state.

Inspect current clipboard/MIME/notification APIs. Clipboard reading is explicit
and asynchronous/bounded; support X11 ownership/MIME behavior, selection changes,
empty/unsupported contents and a clipboard provider disappearing during a read.
Do not replace NIRUCLIP/clipboard history or take clipboard ownership until Copy.
When multiple supported representations exist, select/offer the content type
deterministically and preview the actual representation that will be queued.
An explicit Copy action must remain pasteable after its short CLI/notification
helper exits. Use a verified existing clipboard-persistence contract or let the
share service hold the X11 selection after that action, without monitoring later
clipboard contents. Do not introduce a competing clipboard manager.

## 7. Launcher, notification, bar and screenshot UX

### NIRU Launcher

Add discoverable entries **NIRU Share**, **Share clipboard…**, **Share image…**,
**Share inbox** and **Share by email…**, with suitable English/Swedish search aliases.

```text
Share clipboard
Text · 248 characters
────────────────────────────
All my devices      4 devices
────────────────────────────
Othala              Online
Studio              Offline · queued delivery
Algiz               Online
Void                Service unavailable
────────────────────────────
Share by email…
```

- Compact themed centered UI, real text/image preview, correct byte/item summary,
  visible focus, mouse/Tab/arrows/Enter/Escape, readable light/dark/error states.
- No automatic All-my-devices default. Remembering the last single destination is
  allowed. Clearly label queue expiry and the sender-online requirement.
- Recipient selection and multi-recipient/all-device confirmation must show the
  actual destination set before accepting the transfer. Cancel has no send effect.
- Inbox/outbox shows sender/recipient display names, content kind/size, timestamp,
  unread state, per-device delivery and useful retry/cancel/open/copy details.
- Clipboard previews stay local and ephemeral; no usage-history logging of prompts
  or clipboard text. Escape clears temporary buffers without destroying queue data.
- All state/provider loading is bounded background work; no blocking UI startup.

### NIRUNOTIFY

- Text receipt: **Text received from DEVICE** with Copy / Open inbox.
- Image receipt: **Image received from DEVICE · SIZE** with Open / Open folder.
- Delivery summaries aggregate multi-recipient state, not a burst per retry.
- Persist receipt/unread status in NIRUSHARE; a notification expiring is not read
  acknowledgment. DND suppresses popups but leaves inbox/unread intact.
- When locked, unknown lock state or privacy mode, show no text/image preview or
  personal filenames. No notification action may bypass unlocking. Conservative
  generic receipt text by default is acceptable and should be documented.
- Verify actual ActionInvoked support and client lifetime. Do not assume a
  short-lived `notify-send` invocation can service actions after it exits.
- Restart/reconnect cannot re-notify every historic item. Missing notifier leaves
  correct inbox state and does not fail durable delivery.

### NIRUBAR

- Optional static share/inbox glyph, font-availability detection and readable
  fallback. Muted empty state, unread count/indicator and distinguishable pending
  outgoing state; no blinking, constant animation or color-only meaning.
- Clicking opens the launcher Share view directly; expose a Quick Settings toggle.
  Respect custom module order; default placement is next to communications/settings
  actions with documented padding/separators, without disturbing workspace/title UI.
- Consume cheap local status only. No transfer/discovery/polling implementation in
  the bar and no per-frame process spawn. Missing service cannot freeze the bar.

### NIRUSHOT and file routes

- Add explicit Share to screenshot Review using the current edited/cropped image,
  not an old capture. Cancel preserves existing save/copy behavior and clipboard.
- A completed transfer owns a durable snapshot; a Review temp file must not vanish
  before it has been copied into the queue.
- Install a small desktop/image action or documented file-picker route for selected
  image files. Handle spaces, Unicode and leading dashes with correct argv/desktop
  field codes. Do not change default image/mail associations silently.

## 8. Email

- Open a draft in the user's actual default client using supported desktop APIs,
  `xdg-email` or capability-tested adapters. Thunderbird is the first native
  acceptance case, not the only hardcoded client.
- Never send mail automatically, choose recipients silently, configure accounts,
  upload mail contents or modify the owner's existing mail profile.
- Handle text as body where safe; offer a UTF-8 text attachment for content beyond
  practical URI/command limits. Image attachments keep their bytes and safe names.
- Correctly encode content, newlines, `&`, `?`, quotes, Unicode and filenames;
  never let payload syntax become headers, options or shell commands.
- Preserve draft attachment lifetime. Do not delete a staging attachment on helper
  exit if the mail client still reads it later; define owned retention/cleanup.
- Email mode works even when Tailscale/receiving service is unavailable. Missing
  mail client or unsupported attachment behavior has a clear error/fallback.

## 9. Completion boundary

The first-release scope above must be complete, integrated and running on the
assigned Debian/Arch/Void hosts. Update NIRUX and affected component READMEs/help,
publish/verify immutable compatible assets, activate the exact versions and clean
older-than-two-functional-generations release material with dependency closure.
Do not turn absent hardware or an untested mode into a successful final report.

## Addendum (0.4.0): Discord webhooks

Discord webhooks are an explicit, opt-in outbound destination, not a device
transport. The device protocol, pairing and trust rules above are unchanged.

- None exist by default. They are configured by the user in
  `~/.config/nirushare/webhooks.toml`, which must be a private (0600) regular
  file owned by the user. Only `https://discord.com/api/webhooks/...` URLs (and
  the discordapp.com, canary and ptb hosts) are accepted.
- The URL is a credential: never printed, logged, queued, passed in argv or
  included in errors. Queue entries, previews and undo records store the
  webhook's name.
- Nothing is sent without a preview that names the target. The content is
  staged at preview time so the confirmed content is exactly the previewed one.
- Messages are posted with `?wait=true`; the returned ids allow Undo (DELETE)
  for 5 minutes. Mentions are never parsed (`allowed_mentions.parse = []`).
- Delivery uses the durable outbox. HTTP 429 honours `Retry-After`; 5xx and
  network errors back off; other 4xx are permanent failures.

