# NIRUSHARE

**Part of [NIRUX — A minimal X11 desktop by Nicklas Rudolfsson](https://github.com/nirucon/nirux).**

NIRUSHARE is the lightweight way to send clipboard text, screenshots and images
between your own trusted NIRUX devices over Tailscale. Choose one device, several
devices or **All my devices**. Offline destinations get a durable delivery queue.
Share by email opens a draft in the default mail client.

**Repository state: early implementation, 0.1.1.** The Tailscale-only transfer
core, trust records, durable queue, inbox, CLI, NIRUX session autostart, bar and
launcher entry points, notification actions, NIRUSHOT Review sharing and
image-attachment mail drafts are implemented. The complete native acceptance
matrix, the pairing/trust UI and full PAM edge-case support remain open; see
STATUS.md. Do not treat this as final production acceptance.

## Intended experience

- Open **Share clipboard…** in NIRU Launcher and preview what will be sent.
- Choose trusted recipients, including offline devices, and send.
- Receive a NIRUNOTIFY notification with **Copy**, **Open** or **Open folder**.
- Use the launcher inbox for received items and per-device delivery status.
- Optionally show an unread/outgoing indicator in NIRUBAR.
- Find received images under the actual XDG Downloads directory's `nirushare/`
  subfolder, normally `~/Downloads/nirushare/`.

There is no automatic clipboard synchronization, chat system, central cloud
service, telemetry or automatic opening of received files.

**Offline delivery has a precise limit:** without a relay, the sender and the
receiver must eventually be online together with their share services running.
If the sender is powered off, delivery waits. The application must say this
clearly rather than promise cloud-style delivery.

## Implementation assignment

Read in order:

1. [AGENTS.md](AGENTS.md) — authority, continuous execution and bare-metal rules.
2. [SPECIFICATION.md](SPECIFICATION.md) — product, transport, queue and UI contracts.
3. [ROADMAP.md](ROADMAP.md) — dependency-ordered implementation through full rollout.
4. [ACCEPTANCE.md](ACCEPTANCE.md) — regression and native verification requirements.
5. [RELEASES.md](RELEASES.md) — publishing, suite integration and generation cleanup.
6. [STATUS.md](STATUS.md) — evidence, progress, blockers and final delivery ledger.

### Execution prompt

> Implement and deliver this entire NIRUSHARE assignment, including its NIRUX
> installer, launcher, notifications, bar, screenshot and email integrations.
> Use the separately supplied bare-metal host assignment for development,
> builds and tests on Debian, Arch and Void. Continue in one sustained sweep
> until the complete required scope is implemented, tested, released, installed,
> activated and verified in normal operation. Give occasional concise progress
> updates; do not stop at a prototype, one component, one host or a release upload,
> and do not ask whether to continue between routine phases. Complete README
> reconciliation and dependency-safe cleanup of releases older than the current
> plus two functional generations. A final success report is permitted only after
> every required acceptance gate passes. If a real external blocker leaves no
> executable work, report the precise blocker as PARTIAL/BLOCKED instead of
> inventing completion. Never use Docker, containers, VMs, Xvfb, Xephyr or nested
> test desktops as a substitute for the assigned real machines.

The owner supplies online machines, capabilities, temporary credentials and
permissions in the execution conversation. No machine is authorized by name
in this repository and no credentials belong here.

## Current use

Configured by NIRUX 0.12.145+; the receiving service is started by the NIRUWM
session autostart. Pair trusted devices explicitly from a terminal:

```sh
token=$(nirushare token)
# Run the same command on the peer with the other side's token:
NIRUSHARE_TOKEN="$token" nirushare trust OTHER-DEVICE
```

Share the current clipboard to a trusted device or every trusted device:

```sh
nirushare send --clipboard --to studio
nirushare send --clipboard --all
nirushare devices
nirushare inbox
```

Received images are stored below the real XDG Downloads directory in
`nirushare/`. The Share icon is optional and intentionally absent from the
new-install bar default; it is present in the configured test machines.

## Delivery principles

Rust for the application; no GTK/Qt/WebView dependency for NIRUSHARE itself.
Reuse the NIRUX launcher and notification UI. Use supported Tailscale APIs and
standard transport libraries; do not invent cryptography or duplicate the suite's
providers. Keep optional features inert when absent and keep existing clipboard,
mail, screenshot and desktop workflows working without NIRUSHARE.

NIRUSHARE must support **Debian, Arch and Void**, including systemd/runit and
logind/elogind differences. All shipped UI, CLI help and documentation are English;
Swedish/Unicode text, filenames and real XDG directory choices must work.

## Documentation at delivery

Replace this specification-only introduction with accurate user installation,
pairing, sharing, offline delivery, email, troubleshooting and removal guidance
when the application is implemented. Preserve the durable architecture and
acceptance contracts. Never advertise planned commands as already implemented.

## License

MIT. See [LICENSE](LICENSE).
