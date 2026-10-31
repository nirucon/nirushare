# Roadmap — complete NIRUSHARE delivery

Execute the entire sequence. Revalidate existing features and APIs first; reuse
working suite infrastructure. Keep STATUS.md current at milestones. A completed
phase does not authorize a final success report while later phases remain.

## N0 — Baseline, contracts and native targets

- [ ] Read all assignment documents and target-repository instructions. Inventory
  current branches/SHAs, NIRUX manifest, provider APIs, release contracts and versions.
- [ ] Record separately assigned Debian, Arch and Void hosts, native builders,
  Tailscale identities/capabilities, X11 sessions, init/session bus and mail clients.
- [ ] Verify a recovery connection and preserve configs/data before any deployment.
- [ ] Map existing launcher peer provider, NIRUBAR modules/helpers, NIRUNOTIFY
  action handling, NIRUSHOT Review/clipboard ownership and NIRUX install machinery.
- [ ] Resolve and document transport/listener, pairing, receiver-user identity,
  IPC, schemas, filesystem ordering, limits and dependency choices. Check supported
  Tailscale API permissions rather than assume operator/root access is available.
- [ ] Establish measured resource/latency budgets on actual hosts before optimizing.
  Define an idle baseline, transfer limit/concurrency and availability/retry policy.

**Gate:** compatible design/contracts and authorized native test matrix exist;
all dependencies are actually available on Debian/Arch/Void. No invented host list.

## N1 — Core crate, durable storage and local API

- [ ] Create the Rust application with locked dependencies, clear modules, MIT
  license/attribution, version/help/config validation and documented error codes.
- [ ] Implement XDG config/data/state/runtime and actual user-dir Downloads resolution.
  Private permissions, owned staging, collision handling, schema migration and
  atomic data/metadata writes are covered by regression/fault fixtures.
- [ ] Implement immutable payload snapshots, transfer/recipient/inbox/dedup state,
  configurable validated limits/expiry and crash recovery. No data-bearing logs.
- [ ] Implement one versioned, user-authenticated local IPC/status contract used by
  CLI and all suite clients. Expose cheap aggregate state separately from content.
- [ ] Implement explicit clipboard read, text/image validation, stdin text and
  selected-image-file commands with bounded I/O and safe argv/path handling.

**Gate:** native unit/fault tests prove queue persistence, payload identity,
filesystem safety, concurrent access and CLI/state consistency before networking.

## N2 — Authenticated tailnet transport and pairing

- [ ] Implement bounded local Tailscale discovery, known offline peers, provider
  errors, protocol capability checks and stable node/receiver endpoint records.
- [ ] Implement explicit pairing/approval, trust list and revocation; integrate a
  usable setup/manage-devices route, not a hidden undocumented trust file.
- [ ] Implement tailnet-only receiving, verified incoming identity, ACL behavior,
  dynamic rebinding, authenticated metadata/streaming upload and safe durable receipt.
- [ ] Reject unpaired/spoofed/revoked/wrong-user peers and malformed/oversized
  protocol requests. Do not open a LAN/public fallback or grant broad tailscaled
  admin access. Bound connections, request rates, staging and image decoding.
- [ ] Verify normal two-host text and PNG/image-file transfer with unchanged bytes.
  Ensure logging, discovery and status do not disclose clipboard/private content.

**Gate:** real paired hosts transfer correctly; negative identity/interface/ACL/
user-isolation tests fail closed and receiver durable commit precedes Delivered.

## N3 — Offline queue, broadcast and delivery recovery

- [ ] Persist sender queue across service restart/logout/reboot and receiver outages.
- [ ] Implement availability-triggered retry/backoff with bounded fallback,
  network interruption, sender/receiver crash and lost-receipt recovery.
- [ ] Implement receiver deduplication and receipt lifetime so retries cause one
  final file, one inbox item and one receipt notification.
- [ ] Implement multiple recipients and **All my devices** as one payload plus
  per-recipient snapshots/states; exclude self and never auto-select broadcast.
- [ ] Cover partial success, permanent reject, quota/disk failure, expiry, recipient
  removal/revocation, one-recipient/whole-transfer cancellation and cancel/ACK races.
- [ ] Clearly show why delivery waits and that the sender must return online too.

**Gate:** all offline/reboot/crash/broadcast cases in ACCEPTANCE.md pass on real
machines; no duplicate delivery, hidden data loss or fabricated Delivered state.

## N4 — Cohesive NIRUX UX

- [ ] Add the launcher's Share view, preview, device setup, recipient selection,
  inbox/outbox, per-device detail and cancel/copy/open/save routes with keyboard
  and mouse. Preserve ordinary launcher/Tailscale SSH behavior and module order.
- [ ] Add notification receipt/delivery summaries and working persistent actions.
  DND/lock/privacy/provider-absent/restart cases preserve inbox state and privacy.
- [ ] Add optional NIRUBAR icon/count/pending state, font-safe fallback, direct
  launcher route and Quick Settings toggle. Read only cheap local status.
- [ ] Add NIRUSHOT Review Share using the current edited image with a durable
  queue snapshot. Preserve capture, clipboard, save and cancel contracts.
- [ ] Add selected-image sharing via a documented picker/desktop action. No new
  conflicting default shortcut or silent association changes.
- [ ] Verify theme/font/padding/focus/contrast/light/dark/small-screen/HiDPI behavior;
  English UI with Swedish/Unicode data, localized XDG paths and useful errors.

**Gate:** real keyboard/mouse flows work end to end in the existing NIRUX session,
including notifier actions, screenshot edits, missing service and slow providers.

## N5 — Email, packaging and service lifecycle

- [ ] Implement Share by email through the actual default client; test Thunderbird
  and absence/unsupported-client cases. Never send or change mail accounts.
- [ ] Validate body/URI/attachment encoding, long text fallback and attachment
  lifetime after the launch helper exits. No shell/header/option injection.
- [ ] Implement standalone installation/update/removal with dependency checks and
  owned configs/service files; normal packages are provisioned by NIRUX.
- [ ] Integrate NIRUSHARE into NIRUX's real manifest/asset/version/dependency and
  status/doctor flows. Optional receiving can be enabled/disabled truthfully;
  absence or opt-out never breaks unrelated desktop functions.
- [ ] Implement systemd-user and native Void/session/runit lifecycle without root,
  duplicate listeners, stale IPC/sockets or logout leaks. Test retry after actual
  login/Tailscale/service restarts and orderly logout with queued data.
- [ ] Verify transactionality, non-default home/prefix, original-backup preservation,
  no-packages behavior, rollback compatibility and owned uninstall/data preservation.

**Gate:** native install/update/enable/disable/restart/logout/remove and email flows
pass on Debian, Arch and Void with the exact dependencies the installer provisions.

## N6 — Hardening, performance and full native acceptance

- [ ] Execute the entire ACCEPTANCE matrix, including cross-distro transfers in
  both directions, offline cases, broadcast, lock/privacy and fault regressions.
- [ ] Measure idle CPU/RSS/wakeups, startup/share latency and transfer behavior at
  limits. Remove unnecessary polling, process spawning and payload memory copies.
- [ ] Reproduce and fix every failure; rerun affected cases and relevant consumers.
  Do not hide failures by weakening tests, dropping required scope or changing defaults.
- [ ] Recheck file/inbox/receipt ownership, identity/ACL failures, decompression
  limits, status/log redaction and outgoing/received data retention.
- [ ] Confirm all existing affected suite workflows still function without sharing,
  with receiving disabled, with Tailscale absent/offline and after NIRUSHARE restart.

**Gate:** candidate is ready for release with complete evidence and no unresolved
mandatory correctness/security/UX/dependency/native operation failures.

## N7 — Final releases, rollout, docs and generation cleanup

- [ ] Publish accepted component releases once from exact immutable source tags;
  native ABI builds, complete assets, checksums and one publisher per release.
- [ ] Download and accept final assets, then update dependent NIRUX pins. Never
  race manual uploads against automatic publishers or overwrite published assets.
- [ ] Install through normal official paths and activate exact final versions on
  every assigned target; compare source/release/downloaded/installed/running identities.
- [ ] Perform real final transfers, queued offline recovery, broadcast, notifications,
  bar/launcher/screenshot/mail flows on the final installed suite.
- [ ] Rewrite NIRUSHARE README as a usable shipped-app guide; update NIRUX and
  affected component READMEs/help/links under the shared suite identity.
- [ ] Execute RELEASES.md cleanup, including obsolete related release assets older
  than current plus two functional generations, dependency closure and safe local
  generation cleanup. Remove task-owned artifacts/access and restore test state.
- [ ] Complete STATUS.md and deliver the final report only after the final gate.

**Final gate:** NIRUSHARE and all required integrations are fully operational on
the assigned Debian/Arch/Void hosts; acceptance, docs, activation and cleanup are
finished. If an unavoidable external blocker prevents that, only a precise
PARTIAL/BLOCKED report is allowed, never a success report.
