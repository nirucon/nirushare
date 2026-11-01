# NIRUSHARE

**Part of [NIRUX — A minimal X11 desktop by Nicklas Rudolfsson](https://github.com/nirucon/nirux).**

NIRUSHARE sends clipboard text, screenshots and images between your own trusted
NIRUX devices over Tailscale. Send to one device, several devices or **All my
devices**. Destinations that are offline receive the item through a durable
queue when they return.

**Current release: 0.2.1.** The transfer core, trusted-device records, durable
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
nirushare devices
nirushare inbox
```

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
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release --locked
```

MIT. See [LICENSE](LICENSE).
