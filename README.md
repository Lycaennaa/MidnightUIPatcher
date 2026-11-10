# Midnight UI Patcher for LG webOS TVs

A source-only Rust desktop tool for applying selected Quick Settings theme edits to an owner's own LG webOS TV. It fetches the TV's QML over root SSH, validates strict structural matches, patches locally, and transfers a per-install bundle directly to that TV.

QML (Qt Modeling Language) is a declarative language for defining user interfaces. This tool edits selected QML files that make up parts of webOS Quick Settings.

This is an independent, unofficial project; it is not affiliated with or endorsed by LG Electronics.

This repo does not distribute the QML files or generated bundles.

## Owner workflow

1. Build and launch the GUI with `cargo run` on macOS. Other platforms are not currently validated.
2. Use **Edit connection settings** in the GUI to enter a root SSH target or OpenSSH alias, with optional port and private-key file path. Model and firmware labels are optional. These values are saved in the user-local config outside the repository; passwords and key contents are never saved.
3. Select edits and inspect the TV. The app fetches five QML files, requires every known patch site to match exactly once and have an expected old value, and stops on an unexpected source structure.
4. Review the pre-install summary and optional raw diff. Structurally compatible but unverified firmware requires an explicit warning confirmation. A separate confirmation is required before applying.
5. Keep working root SSH recovery available. The app checks existing live state and updates it in place when needed; it does not write into `/usr`, flash firmware, or install an IPK.
6. A display-service restart is a separate, explicitly confirmed action that can interrupt the TV UI. Visually check Quick Settings after a refresh; a successful deployment is not visual verification.

The patch selections are independent. Unchecking an edit leaves its matched QML unchanged; missing, duplicate, or unexpected matches remain hard failures.

## Build and verify

```sh
cargo test
cargo run
```

Maintainers with local, ignored QML fixtures can compare generated output against them with:

```sh
cargo run -- --verify-local-preview .
```

That private development check requires fixtures that are intentionally absent from a clean checkout. It is not needed by owners.

## Safety and privacy

The GUI saves connection settings outside this repository. The app does not send QML, logs, host details, or credentials to a project service. Root SSH recovery is required because runtime bind mounts and a display-service restart can disrupt the UI. Keep original backups and follow the on-screen state checks; do not treat a matching QML structure as official firmware support.
