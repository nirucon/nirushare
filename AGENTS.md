# Agent instructions — deliver NIRUSHARE end to end

## Current scope and authority

The current repository contains the implementation assignment. Creating these
documents is not itself a claim that the application exists. When the owner
instructs you to execute this assignment, implement the complete SPECIFICATION
and ROADMAP, not a self-selected subset.

That execution assignment authorizes necessary changes, dependency provisioning,
tests, commits, pushes, new immutable releases, normal installation/activation
and bounded cleanup in the related NIRUX repositories. Inspect each repository's
own instructions, architecture, status and history before editing. Related targets
include NIRUX, NIRU Launcher, NIRUBAR, NIRUNOTIFY and NIRUSHOT; touch NIRUWM/session
integration only where genuinely necessary. Preserve unrelated work and features.

## Continuous execution — mandatory

- Work through discovery, implementation, native tests, releases, deployment,
  documentation and cleanup in one sustained sweep. Do not stop after a plan,
  prototype, green build, successful transfer, one distribution or installation.
- Do not ask “shall I continue?” or request routine approval between tasks.
  Decide implementation details using the documented contracts and record why.
- Emit brief factual status at meaningful milestones: completed work, measured
  evidence, remaining work and genuine blockers. Do not narrate every tool call.
- If one device/task is blocked, continue independent work and return to it.
  Never idle indefinitely, repeatedly ask the same question or silently shrink
  the scope to make a completion claim possible.
- Final success requires every mandatory gate in ACCEPTANCE.md. No blanket
  “100% complete” based on versions, mocks, screenshots, compilation or remote CI.
- A missing physical capability, offline host, unavailable credential or owner-only
  action cannot be fabricated. Ask only for a genuine external requirement after
  independent work is exhausted; report PARTIAL/BLOCKED with exact continuation.
- If the execution environment ends, leave a truthful resumable STATUS.md.
  Do not present an environment limit as successful delivery.
- Do not spawn subagents unless the owner separately authorizes delegation.

## Bare-metal development, builds and acceptance

- The owner separately supplies online SSH targets and roles for Debian, Arch
  and Void, temporary credentials and physical-test limits. Do not infer an
  authorized machine from old planning documents or tailnet discovery.
- Develop, build and test candidates on assigned native hosts. The controller
  may inspect/orchestrate Git/SSH and maintain documentation.
- Do not create or run Docker/Podman, containers, VMs, Xvfb, Xephyr, nested X,
  additional Xorg test servers or extra seats. Normal real greeter/session startup
  is part of the system being used, not permission for a parallel test desktop.
- Existing CI is supplementary. Pure unit/protocol/fault fixtures are useful on
  native hosts and must be labeled synthetic; they do not prove actual offline,
  clipboard, notification, mail or graphical behavior.
- Verify actual distribution, ABI, init, XDG paths, session bus, Tailscale state,
  graphical session and available input/image/mail applications before deployment.

## Preservation and access

- Inventory current/recovery generations, selected/running DM and suite identities.
  Do not change login, autologin, PAM, networking or unrelated services merely to
  test sharing. Never enable temporary autologin or passwordless authentication.
- Preserve personal files, existing downloads, clipboard contents/history, mail
  profiles/accounts/OAuth, recordings, themes, custom keys, Nextcloud and projects.
  Never use personal clipboard/mail contents as test fixtures.
- Do not send real private data to every tailnet peer to test discovery. Use
  synthetic content and explicitly paired test recipients on authorized hosts.
- Back up affected configuration once, preserve permissions/comments/unknown keys,
  use atomic writes and respect explicit opt-outs/custom paths.
- Keep SSH/Tailscale access available. Coordinate disruptive session/reboot tests
  on assigned machines; never interrupt unsaved work without an agreed window.
- Secrets must not appear in argv, shell scripts, process logs, Git, screenshots,
  notifications, public issues or release assets. Use protected stdin/runtime
  mechanisms and redact evidence. Never log payloads, clipboard text, passwords,
  pairing tokens, OAuth data or private node details by default.
- Remove only identified task-created temporary access, services, keys, files,
  test payloads and builds after acceptance. Preserve real inbox files and queued
  owner data. Do not repurpose cleanup as broad home/package deletion.

## Engineering rules

- Verify current APIs before choosing them. Tailscale membership is not application
  capability, device ownership, receiver-user identity or pairing authorization.
- Use one authoritative transfer/state model. CLI, UI, bar and notifications
  consume it; they must not each implement transport, retries or recipient discovery.
- Use argv, bounded asynchronous work, concurrent pipe draining, deadlines,
  cancellation and exact exit statuses. No clipboard text in shell interpolation.
- Keep network/crypto/protocol handling out of UI event loops. Choose maintained
  standard libraries; do not invent encryption, use insecure discovery broadcasts
  or silently expose a listener on LAN/public interfaces.
- Write regressions for real risks: durable receipt, retry/idempotence, revocation,
  path traversal/symlinks, disk failure, image limits, notification privacy and
  incorrect state. Do not create tests that merely restate field names.
- No backend rewrites or new repositories for tiny helpers. Do not introduce a
  heavy framework or mandatory daemon for unrelated suite functions.
- Measure idle/startup/latency/memory. Cheap local bar state, no per-frame Tailscale
  subprocesses, network scans or filesystem scans. Missing optional providers
  cannot freeze or break normal NIRUX use.

## Publication and completion

- Build and accept candidates before tagging. One publisher per release. Inspect
  existing workflows: never race a manual upload against a tag-triggered publisher.
- Published tags/assets/checksums are immutable. Repair a bad release with a new
  verified patch, not asset replacement, retagging or copied old binaries.
- Verify final downloads and native behavior before pinning dependent suite releases.
  Install through the normal supported path, then verify real running identities.
  An installed-but-inactive build is not COMPLETE.
- Keep README/help/examples accurate and linked under **NIRUX — A minimal X11
  desktop**. Keep public docs English and free of private execution/host details.
- Complete RELEASES.md cleanup: current plus two functional generations with
  dependency closure, retained recovery and exact deletion evidence.
- Maintain STATUS.md throughout. All required delivery items must be operational
  before the success report; list external blockers truthfully if that is impossible.
