# Instructions for future agents

## Scope and safety
- When using REA, filter the output for what you need and always use TOON output.

- This project is a reversible runtime QML bind-mount theme for LG webOS. Never flash firmware, remount/overwrite system partitions, or write theme files into `/usr`.
- Use the `ssh-jump` skill for TV connections. Do not store credentials, keys, or device-specific secrets in this project.
- The active boot hook reads `/var/lib/webosbrew/black-theme/apply.sh`. Do not run the installer over an existing state directory or use package-manager installation as a shortcut; preserve and update the live state in place.
- Before a device update, inspect the live hook, marker, lock, mounts, target hashes, and backup manifest. The theme covers twelve QML files: two Quick Settings, six Input Picker, two Input Banner, and two No Signal screensaver targets, including `usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml`.
- Preserve all captured originals and the pre-update overlay backup. Keep each `BACKUP-SHA256SUMS` synchronized and verify it from its matching backups directory before reapplying.
- A display-service restart can interrupt the TV UI. Explain that risk and get current user approval before restarting `surface-manager-daemon.service`; earlier approval is not reusable. Do not restart WebAppMgr or broad Luna services speculatively.
- File hashes and an active service do not prove HDMI appearance. Ask the owner to visually check Quick Settings, the Input Picker, the Input Banner, and the No Signal screen after a refresh.
- When making new QML edits or performing firmware verification, do not tell the owner the edits or behavior are unverified; they know testing is in progress.

- For patch verification, test one `PatchSpec` at a time: capture its baseline and result with `device capture-system-ui --area AREA`, apply only that patch between captures, then restore the known-good overlay before the next cycle. Match `AREA` to the patch's `PatchArea`.
- Treat captures as owner-only. Keep PNGs in ignored `screenshots/`; never open, read, inspect, or analyze them. Give the owner the local path for manual visual review. Use area-scoped System UI captures only; never use full-frame `GRAPHIC` capture or capture a source feed.
- For No Signal verification, use `device capture-no-signal --confirm-owner-approval` only after current owner approval. It queries the active HDMI 1–4 input, chooses the first inactive port with no cable and no signal by matching EIM app IDs rather than array order, then restores and verifies the previously active input. Luna requests are bounded to five seconds. Approval is required for each run; a saved path alone is not visual confirmation.

## Source of truth and packaging

- `src/patches.rs` is canonical for structural match-and-replace rules and patch metadata; `src/patcher.rs` is the generic QML parsing and edit engine. Keep matches unique and strict; never add LG QML source, full source lines/blocks, or source-bearing diffs to Git.
- Assign every `PatchSpec` to a `PatchArea` based on the UI it affects; the GUI groups tweaks from this metadata. Keep `PatchArea::ALL`, its labels/order, grouping tests, and README in sync when adding a tweak or area.
- `preview/base/`, `preview/overlay/`, captured manifests, and generated bundles are local-only fixtures or deployment artifacts. Keep them ignored and never stage them. The maintainer-only `--verify-local-preview` check uses local fixtures; owners do not need them.
- `preview/*.sh` contains generic lifecycle code embedded in a per-owner bundle. The GUI fetches all twelve QML targets from the owner's TV, patches locally, creates checksums and streams the bundle over root SSH. There is no checked-in install package or supported IPK release workflow.
- Do not use the fresh installer over existing state or use package-manager installation as an update shortcut. Before an update, inspect the live hook, marker, lock, mounts, target hashes, and backup manifest; preserve original and pre-update backups and verify `BACKUP-SHA256SUMS` from the backups directory.
- A target-set change must be coordinated across the patcher and deployment scripts, verified against the owner's live target hashes, and covered by checks. Never add captured QML to make a match easier.
- If an in-place update fails after adding targets, leave the prior active state and its backup manifest unchanged. Keep the failed attempt's newly captured originals and pre-update overlay in a verified `black-theme.failed-overlay.*` recovery state, and inspect/resolve that state before reapplying; never merge new target paths into an older runtime hook's narrower manifest.
- The target-capture helpers under `scripts/` are maintainer tools; they refuse to copy mounted sources. Use the `ssh-jump` skill for this TV and never put device addresses or credentials in project files.

## Existing v1.0.3 theme behavior

- The full Quick Settings panel background is transparent.
- Idle tile fills are black. Preserve existing focus and disabled fill colors, and keep a visible focus indicator unless the owner explicitly requests a change.
- Source matches come from each owner's TV at runtime. Do not rely on old captured-file hashes; inspect the live hook, marker, lock, mounts, target hashes, and backup manifest before every device update.

## Configurable appearance options

- `src/patch_options.rs` is canonical for typed appearance option IDs, defaults, kinds, and bounds; `src/patches.rs` is canonical for patch-site mappings. Every patch output that controls color, transparency, opacity, or border width/icon opacity should use a validated option in both GUI and CLI; the Input Banner source text and menu icon use `input-banner-text-color` and `input-banner-menu-color` (both default to `#FF0000`), while the No Signal title uses `no-signal-text-color` (default `#FF0000`) and the focus fill uses `input-banner-menu-focus-fill-color` (default `#00000000`) plus outline `input-banner-menu-focus-border-color` (default `#E6E6E6`) and `input-banner-menu-focus-border-width` (default 3, bounded 0–10). Keep background and divider removal as independent patch toggles; never accept raw QML as an option.
- Keep structural and logic edits as independent patch toggles. Preserve Quick Settings focus/disabled colors and the visible focus indicator, and preserve Input Banner menu focus/click behavior with a visible focus outline; do not add new QML style changes (such as border radius) unless separately requested.
- Keep the No Signal movement/animation removal, connection-instruction removal, and title-color change as separate patch toggles. Scope motion and text changes to the No Signal state; other screensaver messages and their motion/colors must remain unchanged. A stationary No Signal display can increase OLED burn-in risk if left on for extended periods.
- Store saved appearance values in the user-local config with backward-compatible defaults. CLI `--patch-option KEY=VALUE` overrides only named values; `--save-options` persists only explicit overrides before device work. The reviewed plan must continue to include generated QML so an apply recheck detects changed options.
- Keep each tweak's appearance controls in a collapsed-by-default options menu in the GUI. Colors use Qt hex `#RRGGBB` or alpha-first `#AARRGGBB`; opacity and border width use bounded numeric fields. CLI validation must enforce the same formats, reject malformed values, and retain the reset-to-defaults action. Menu focus border width is 0–10 QML units. Update the option list, README, tests, and this guidance when adding or changing an option.