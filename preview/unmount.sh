#!/bin/sh
set -eu

RUN_MARKER=/run/black-theme-overlay.active
FILES='
usr/lib/qml/SystemUI/Notification/Toast.qml
usr/lib/qml/SystemUI/Notification/NotificationStyle.qml
usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml
usr/lib/qml/QuickSettings/Utils/Style.qml
usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml
'

if [ "$(id -u)" != 0 ]; then
    echo "Black theme rollback requires root." >&2
    exit 1
fi
if [ "$#" -gt 1 ] || { [ "$#" -eq 1 ] && [ "$1" != "--lock-held" ]; }; then
    echo "Unexpected unmount.sh argument." >&2
    exit 2
fi
LOCK_DIR=/run/black-theme-overlay.lock
LOCK_HELD=0
if [ "${1-}" = "--lock-held" ]; then
    if [ ! -d "$LOCK_DIR" ]; then
        echo "Lock-held unmount mode requires the shared operation lock." >&2
        exit 1
    fi
    LOCK_HELD=1
elif ! mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "Another black-theme operation is active; retry rollback shortly." >&2
    exit 1
fi
release_lock() {
    [ "$LOCK_HELD" -eq 1 ] || rmdir "$LOCK_DIR" 2>/dev/null || true
}
trap 'release_lock' 0
trap 'exit 1' HUP INT TERM

if [ ! -f "$RUN_MARKER" ]; then
    exit 0
fi

mount_present() {
    awk -v target="$1" '$2 == target { found=1 } END { exit !found }' /proc/mounts
}

status=0
for rel in $FILES; do
    target=/$rel
    if mount_present "$target" && ! umount "$target"; then
        echo "Could not unmount: $target" >&2
        status=1
    fi
done

if [ "$status" -eq 0 ]; then
    rm -f "$RUN_MARKER"
else
    echo "Some mounts remain; keep backups and retry rollback." >&2
    exit 1
fi
