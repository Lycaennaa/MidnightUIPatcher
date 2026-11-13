# Midnight UI Patcher for LG webOS TVs

A source-only Rust GUI and CLI for applying selected Quick Settings, Input Picker, Input Banner, and No Signal screensaver theme edits to an owner's own LG webOS TV. It fetches the TV's QML over root SSH, validates strict structural matches, patches locally, and transfers a per-install bundle directly to that TV.

This tool edits selected QML files that make up parts of webOS Quick Settings, the Input Picker, the Input Banner, and the No Signal screensaver.

This is an independent, unofficial project; it is not affiliated with or endorsed by LG Electronics.

This repo does not distribute the QML files or generated bundles.

## Owner workflow

`cargo run` opens the GUI. Use `cargo run -- --help` for the progressively disclosed CLI; top-level help lists the main command groups, and each group has focused subcommand help.

Both interfaces use the same local connection settings, saved appearance choices, and deployment logic. Connection settings and appearance options are stored in the user-local config; passwords and key contents are never saved.

### CLI

```sh
cargo run -- config set --ssh-target my-tv --ssh-config-file ~/.ssh/config
cargo run -- config set --patch-option 'input-tile-text-color=#FF0000'
cargo run -- config set --patch-option 'input-banner-text-color=#FF0000'
cargo run -- config set --patch-option 'no-signal-text-color=#FF0000'
cargo run -- config set --patch-option 'input-banner-menu-color=#FF0000'
cargo run -- config set --patch-option 'input-banner-menu-focus-fill-color=#00000000'
cargo run -- config set --patch-option 'input-banner-menu-focus-border-color=#E6E6E6'
cargo run -- config set --patch-option 'input-banner-menu-focus-border-width=3'
cargo run -- --format json patches list
cargo run -- --format json device inspect --patch-option 'input-tile-text-color=#00AACC'
```
Replace `my-tv` with an alias configured in the selected OpenSSH config.
Use `--ssh-config-file PATH` when the alias is in a non-default OpenSSH config; blank uses OpenSSH's default config.

`patches list` reports configurable option keys and defaults. Repeat `--patch-option KEY=VALUE` on `device inspect` or `device apply` to override saved values for that command; omitted keys use saved config. Add `--save-options` to persist supplied overrides before SSH/device work, or use `config set --patch-option KEY=VALUE` to save without contacting the TV. Temporary overrides must be repeated on apply to reproduce the inspected plan and on `report` to include those values in the status report; report overrides are not saved.

Colors accept Qt hex `#RRGGBB` or alpha-first `#AARRGGBB`; the GUI includes a cross-platform picker beside every color field. The picker changes RGB and preserves alpha from 8-digit values; use hex entry to change alpha. On Linux, the picker uses an available desktop dialog utility such as Zenity or KDialog. Panel/icon opacity accepts 0–1, selected-input border width accepts 0–100, and Input Banner menu focus border width accepts 0–10 QML units. Invalid values are rejected rather than inserted into QML.

Inspection does not change TV state; `--save-options` can write the local config. Inspection checks the live hook, lock, mounts, hashes, and backups, fetches all twelve Quick Settings, Input Picker, Input Banner, and No Signal screensaver QML targets over root SSH, and validates every known patch site. Review the summary and fingerprint; add `--diff` to explicitly include the raw QML diff. All edits are selected by default. Repeat `--patch ID` to select specific edits or use `--exclude ID` to omit edits.

Apply re-inspects the TV and refuses to deploy if the source, selection, connection settings, appearance options, or resulting plan differs from the reviewed fingerprint. Replace the placeholder with `plan_fingerprint` from inspection:

```sh
cargo run -- device apply --expect-plan-fingerprint sha256:PASTE_FINGERPRINT --confirm-root-recovery --confirm-unverified
```

The unverified-firmware acknowledgement is required when firmware is not maintainer-tested. Display refresh is a separate action and requires separate approval:

```sh
cargo run -- device restart-display --confirm-owner-approval
cargo run -- report --structurally-verified 17 --deployment succeeded --refresh restarted-visual-check-pending --visual-result looks-good
```

Text mode writes results to stdout and errors to stderr. `--format json` writes one JSON object to stdout for success, operation errors, and usage errors. Exit codes are 0 for success, 1 for an operation failure, and 2 for invalid or blocked commands. The visual result is an owner-provided observation, not automated verification.

### GUI

Select edits and inspect the TV. Tweak choices are grouped by area: Quick Settings, Input Picker, Input Banner, and No Signal. Edit connection and appearance settings, use the color picker beside any color hex field, adjust opacity and border width, and reset appearance defaults. The color picker changes RGB and preserves alpha; use hex entry to change alpha. Save settings to persist them. Review the pre-install summary and optional raw diff. Structurally compatible but unverified firmware requires an explicit warning confirmation. Applying also requires confirming root SSH recovery is available.

The theme updates state in `/var/lib/webosbrew`; it does not write QML files into `/usr`, flash firmware, or install an IPK. A display-service restart can interrupt the TV UI. Visually check Quick Settings, the Input Picker, the Input Banner, and the No Signal screen after a refresh; successful deployment is not visual verification.

Existing two-target installations still expand to eight targets by adding six Input Picker files; an eight-target install expands in place to ten by adding two Input Banner files; a ten-target install adds two No Signal screensaver files in place. Historical five-target installations also retire three obsolete mounts while retaining original and pre-update backups.

Patch selections are independent. Missing, duplicate, or unexpected matches remain hard failures.

The Input Picker shows a border only on the selected input, hides inactive external inputs, and hides the Universal Control settings button while keeping its associated input tile available.

The Input Banner is the source-information overlay toggled by OK on an external input. The theme removes its decorative background images and option divider, colors the source name and input label using `input-banner-text-color` (default `#FF0000`), and colors the vertical three-dot menu using `input-banner-menu-color` (default `#FF0000`). The focused menu tile fill defaults to transparent; its outline defaults to `#E6E6E6` at width 3, controlled by the `input-banner-menu-focus-fill-color`, `input-banner-menu-focus-border-color`, and `input-banner-menu-focus-border-width` options.

The No Signal screensaver appears when an input loses signal, such as when an HDMI cable is disconnected. Its three independent patches stop No Signal movement and animation, hide connection instructions, and set the translated title color with `no-signal-text-color` (default `#FF0000`). A stationary No Signal display left on for a long time can increase OLED burn-in risk.


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
