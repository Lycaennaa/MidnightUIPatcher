# Runtime deployment scripts

This directory contains the generic install, update, apply, unmount, and uninstall scripts embedded by the desktop app in a per-owner transfer bundle. The app generates that bundle locally from QML fetched from the owner's TV; this checkout is not a ready-to-install archive.

No TV QML, patched QML, target hash manifest, or reusable IPK is distributed here. The generated bundle is streamed over root SSH to one TV and is not written as a shared release artifact.

## Safety behavior

- Fresh installation requires an explicit confirmation and refuses an existing state directory.
- Existing installations use the in-place update flow. It checks the current hook, lock, active marker, mount list, target hashes, and backup manifest before changing state.
- The update keeps the original backups and a verified pre-update overlay snapshot. If new activation fails, it attempts to restore the previous state; if restoration also fails, both the old state and retained backups remain available for recovery.
- Runtime changes use bind mounts; scripts do not overwrite files in `/usr` or flash firmware.
- Uninstall disables the hook and removes mounts while retaining backups. A busy-operation lock causes the operation to stop rather than race another update.

The app requires a working root SSH recovery path and separately asks before restarting `surface-manager-daemon.service`, which can interrupt the TV UI. After any refresh, check the actual Quick Settings appearance on the TV.

Do not run these scripts directly from a source checkout. They are embedded into a verified transfer bundle together with that owner's locally generated overlay and target hashes.
