# Suite packaging, immutable release and generation cleanup

## Packaging responsibilities

- NIRUSHARE owns its Rust source, CLI/local IPC/protocol schemas, service/config
  templates, tests and component release assets.
- NIRUX owns normal distro package provisioning, suite manifest pins, integration,
  activation and coherent update/status/doctor/removal behavior. The application
  never invokes apt/pacman/xbps while running.
- Standalone install checks/documents dependencies and owns only its installed
  files. Do not install a heavy runtime or build toolchain on target users merely
  to run a release binary. Do not promise static portability without checking it.
- Inspect actual Debian/Arch/Void package names, default paths, Tailscale local
  API permissions and service users. No hardcoded Debian-only integration path.
- The NIRUX feature can be installed without automatic trust enrollment or network
  exposure. Respect explicit receiving opt-out. An offline Tailscale service must
  not break suite installation or unrelated apps.
- Changes to launcher/bar/notifier/screenshot are coordinated component releases,
  not untracked host edits or private APIs advertised as stable.

## One accepted release set

1. Inventory existing branches, release workflows, publisher ownership and pins.
2. Develop and test candidates on assigned native ABI builders. Lock dependencies;
   retain source SHA/build identity and exact test commands/results.
3. Complete affected native candidate acceptance before tagging.
4. Create immutable version tags that match source/metadata/installer/version output.
5. Exactly one publisher creates each component release from the exact tag. If an
   automatic tag workflow owns publishing, wait for it; do not run a parallel
   manual `gh release create/upload` which overwrites assets/checksums.
6. Generate archives/binaries/service templates/checksums together from verified
   source. No stale copied archives, missing desktop assets, mismatched checksum
   filenames, leaked development paths or incomplete release marked latest.
7. Download official component assets and verify hashes, archive layout, runtime
   dependencies, versions and native functionality. Then update NIRUX's pins.
8. Publish the accepted suite and install/activate exact official assets on all
   assigned hosts. Verify downloaded/installed/running identity and real workflows.
9. Perform cleanup below, revalidate retained dependencies and finalize docs/status.

Never move published tags or replace published assets/checksums to fix a failed
release. Publish a new accepted patch and retire the bad release after recovery is
available. Do not bypass failed release gates to save CI/storage time.

## Current plus two functional generations

The owner requests cleanup of versions older than two generations. Apply this to
NIRUSHARE and affected related NIRUX repositories as **current accepted release
plus the previous two functional releases**, with dependency closure.

- Before deletion, record the proposed set and trace all retained NIRUX manifests,
  standalone installers, NIRUWM/bar pins, extras/theme pins and recovery paths.
- Keep an older component asset if a retained supported suite needs it. Either
  retain that dependency as a documented exception or deliberately retire its
  referring suite generation first. Never break an installer by blind date sorting.
- Remove obsolete Releases/assets, failed drafts, duplicate scratch assets,
  unreferenced task-owned test branches/tags and temporary Actions artifacts/caches
  after checking references and ownership. Set sane ongoing retention.
- Keep genuine published release tags/source history as provenance. Do not rewrite
  history, delete entire repositories, remove another contributor's unmerged work
  or change repository visibility under a generic cleanup instruction.
- Preserve durable LICENSE/attribution, architecture/API/user/test docs and useful
  regression fixtures. Remove stale one-off prompts, generated logs/screenshots/
  recordings/build trees and broken documentation references when genuinely obsolete.
- Preserve actual current/running/previous working local generations. Remove older
  owned candidates/generations only after verifying no active process/reference
  needs them and rollback remains complete. Do not delete an in-use generation.
- Never delete user inbox files, real pending share payloads, mail profiles,
  downloads, recordings, Nextcloud/projects or unrelated packages as cache cleanup.

## Public documentation

NIRUSHARE's final README must document real install/dependencies, explicit setup/
pair/revoke, CLI, Share clipboard/image, **All my devices**, offline limits,
service lifecycle, XDG storage, inbox/outbox, notifications/DND/lock behavior,
mail-draft/attachment semantics, limits, troubleshooting, disable/uninstall/purge
and release/compatibility boundaries.

Update NIRUX's component table, install/update/doctor/activation documentation and
the affected NIRU Launcher, NIRUBAR, NIRUNOTIFY and NIRUSHOT READMEs/help. Explain
new options without claiming NIRUSHARE is a required clipboard/network provider.
Use **NIRUX — A minimal X11 desktop**, English UI/docs and accurate X11 scope.
Keep Swedish/Unicode examples synthetic and avoid private hosts/user identities.

Remove the specification-only status from README only after implementation and
acceptance. Keep the useful protocol/architecture/contributor contracts accurate;
do not ship stale promises or private execution details as user documentation.

## Cleanup ledger

Record repo/ref/release/asset or owned path, reason, reference check, result and
retained dependency exceptions in STATUS.md. Verify retained official URLs/hashes
and final native operation after cleanup. Final delivery includes documentation,
activation and cleanup, not just uploading a binary.
