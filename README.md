# Midnight UI Patcher for LG webOS TVs

A source-only Rust GUI and CLI for applying selected Quick Settings theme edits to an owner's own LG webOS TV. It fetches the TV's QML over root SSH, validates strict structural matches, patches locally, and transfers a per-install bundle directly to that TV.

QML (Qt Modeling Language) is a declarative language for defining user interfaces. This tool edits selected QML files that make up parts of webOS Quick Settings.

This is an independent, unofficial project; it is not affiliated with or endorsed by LG Electronics.

This repo does not distribute the QML files or generated bundles.

## Owner workflow

`cargo run` opens the GUI. Use `cargo run -- --help` for the progressively disclosed CLI; top-level help lists the main command groups, and each group has focused subcommand help.

Both interfaces use the same local connection settings and deployment logic. Settings accept a root SSH target or OpenSSH alias, optional port, private-key path, and custom OpenSSH config file path, plus optional model and firmware labels. These settings are stored in the user-local config; passwords and key contents are never saved.

### CLI

```sh
cargo run -- config set --ssh-target my-tv --ssh-config-file ~/.ssh/config
cargo run -- patches list
cargo run -- --format json device inspect
```
Replace `my-tv` with an alias configured in the selected OpenSSH config.
Use `--ssh-config-file PATH` when the alias is in a non-default OpenSSH config; blank uses OpenSSH's default config.

Inspection is read-only: it checks the live hook, lock, mounts, hashes, and backups, fetches the two Quick Settings QML files over root SSH, and validates every known patch site. Review the summary and fingerprint; add `--diff` to explicitly include the raw QML diff. All edits are selected by default. Repeat `--patch ID` to select specific edits or use `--exclude ID` to omit edits.

Apply re-inspects the TV and refuses to deploy if the source, selection, connection settings, or resulting plan differs from the reviewed fingerprint. Replace the placeholder with `plan_fingerprint` from inspection:

```sh
cargo run -- device apply --expect-plan-fingerprint sha256:PASTE_FINGERPRINT --confirm-root-recovery --confirm-unverified
```

The unverified-firmware acknowledgement is required when firmware is not maintainer-tested. Display refresh is a separate action and requires separate approval:

```sh
cargo run -- device restart-display --confirm-owner-approval
cargo run -- report --structurally-verified 6 --deployment succeeded --refresh restarted-visual-check-pending --visual-result looks-good
```

Text mode writes results to stdout and errors to stderr. `--format json` writes one JSON object to stdout for success, operation errors, and usage errors. Exit codes are 0 for success, 1 for an operation failure, and 2 for invalid or blocked commands. The visual result is an owner-provided observation, not automated verification.

### GUI

Select edits and inspect the TV. Review the pre-install summary and optional raw diff. Structurally compatible but unverified firmware requires an explicit warning confirmation. Applying also requires confirming root SSH recovery is available.

The theme updates state in `/var/lib/webosbrew`; it does not write QML files into `/usr`, flash firmware, or install an IPK. A display-service restart can interrupt the TV UI. Visually check Quick Settings after a refresh; successful deployment is not visual verification.

Existing five-target installations migrate in place to the two Quick Settings targets. The update removes and verifies the three retired mounts while retaining original and pre-update backups.

Patch selections are independent. Missing, duplicate, or unexpected matches remain hard failures.

## Build and verify

```sh
cargo test
cargo run
cargo run -- --help
```

Maintainers with local, ignored QML fixtures can compare generated output against them with:

```sh
cargo run -- dev verify-local-preview .
```

That private development check requires fixtures that are intentionally absent from a clean checkout. It is not needed by owners.

## Safety and privacy

Both interfaces save connection settings outside the repository by default. The app does not send QML, logs, host details, or credentials to a project service. Root SSH recovery is required because runtime bind mounts and a display-service restart can disrupt the UI. Keep original backups and follow the on-screen or CLI state checks; do not treat a matching QML structure as official firmware support.
