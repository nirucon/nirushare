# NIRUSHARE delivery status

**State: PARTIAL — core service and NIRUX integration released as 0.1.1; full delivery gates remain open.**

The owner will provide the bare-metal targets and temporary access separately
when assigning execution. No credentials or authorized host list are stored here.
Do not interpret this document as proof of a working service or shipped feature.

## Work ledger

| Phase | State | Next action / evidence |
|---|---|---|
| N0 — baseline/contracts | PARTIAL | Bare-metal targets, service and release baseline established. |
| N1 — durable core/local API | PARTIAL | Core queue/storage/CLI exists; full regression matrix remains. |
| N2 — transport/trust | PARTIAL | Tailnet service and pairwise trust verified; pairing UI remains. |
| N3 — offline/broadcast | PARTIAL | Queue/receipt/dedup core exists; fault/offline matrix remains. |
| N4 — NIRUX UX | PARTIAL | Launcher, bar, notify and NIRUSHOT routes implemented; native acceptance remains. |
| N5 — email/packaging/services | PARTIAL | NIRUX component/autostart and mail draft implemented; lifecycle matrix remains. |
| N6 — native hardening | NOT STARTED | Complete ACCEPTANCE matrix and measured resource verification. |
| N7 — release/rollout/docs/cleanup | NOT STARTED | Official final assets, activation, README reconciliation and retention. |

## Assigned host matrix

Fill from the execution conversation, not from Tailscale peer discovery.

| Distro | Authorized role | OS/ABI/init/session | Node/receiver identity recorded privately | Candidate/final hash | Native state |
|---|---|---|---|---|---|
| Debian | UNASSIGNED | NOT INVENTORIED | NOT RECORDED | NOT BUILT | NOT RUN |
| Arch | UNASSIGNED | NOT INVENTORIED | NOT RECORDED | NOT BUILT | NOT RUN |
| Void | UNASSIGNED | NOT INVENTORIED | NOT RECORDED | NOT BUILT | NOT RUN |

Do not put node secrets, pairing tokens, private payloads or SSH credentials in
this public file. Keep actual private host details in protected local evidence.

## Decision ledger

Record transport/library/identity proof, ports/multi-user endpoints, pairing,
protocol/IPC schemas, durable commit/receipt ordering, dedup/retention windows,
service mechanism, limits, email attachment lifetime and resource budgets after
N0. Explain deviations from examples without dropping required behavior.

## Milestone evidence template

```text
UTC / phase / case IDs / PASS-FAIL-NOT RUN-BLOCKED:
Repo + source SHA / candidate or tag / artifact SHA-256:
Native distro + ABI + init + authorized role:
Reproduction / implementation / regression coverage:
Exact commands + true exit statuses / physical user-flow steps:
Downloaded / installed / running identities:
Observed behavior and measurements / redacted evidence location:
Config/data preserved / cleanup and restore performed:
Remaining blocker / independent work continuing / precise next action:
```

## Final delivery

Not yet available. Only fill as COMPLETE after every final gate in ACCEPTANCE.md:
all required functionality, native Debian/Arch/Void evidence, compatible official
release set, full deployment/activation, truthful docs and dependency-safe cleanup.
Otherwise use PARTIAL/BLOCKED with the exact unresolved external requirement.
