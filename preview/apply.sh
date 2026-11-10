#!/bin/sh
set -eu

STATE=/var/lib/webosbrew/black-theme
RUN_MARKER=/run/black-theme-overlay.active
FILES='
usr/lib/qml/QuickSettings/Utils/Style.qml
usr/lib/qml/QuickSettings/Containers/QuickSettingsMainView.qml
usr/lib/qml/SystemUI/Notification/NotificationStyle.qml
usr/lib/qml/SystemUI/Notification/Toast.qml
usr/lib/qml/QuickSettings/Component/BaseControlPanelButton.qml
'

if [ "$(id -u)" != 0 ]; then
    echo "Black theme hook requires root." >&2
    exit 1
fi
if [ "$#" -gt 1 ] || { [ "$#" -eq 1 ] && [ "$1" != "--lock-held" ]; }; then
    echo "Unexpected apply.sh argument." >&2
    exit 2
fi
LOCK_DIR=/run/black-theme-overlay.lock
LOCK_HELD=0
if [ "${1-}" = "--lock-held" ]; then
    if [ ! -d "$LOCK_DIR" ]; then
        echo "Lock-held apply mode requires the shared operation lock." >&2
        exit 1
    fi
    LOCK_HELD=1
elif ! mkdir "$LOCK_DIR" 2>/dev/null; then
    echo "Another black-theme operation is active; skipping this hook run." >&2
    exit 0
fi
release_lock() {
    [ "$LOCK_HELD" -eq 1 ] || rmdir "$LOCK_DIR" 2>/dev/null || true
}
trap 'release_lock' 0
trap 'exit 1' HUP INT TERM

if [ -f "$STATE/disabled" ]; then
    exit 0
fi
if [ -f "$RUN_MARKER" ]; then
    exit 0
fi

mount_present() {
    awk -v target="$1" '$2 == target { found=1 } END { exit !found }' /proc/mounts
}

for rel in $FILES; do
    source=$STATE/overlay/$rel
    target=/$rel
    if [ ! -f "$source" ] || [ ! -f "$target" ] || [ -L "$target" ]; then
        echo "Overlay source or target missing/not regular: $target" >&2
        exit 1
    fi
    if mount_present "$target"; then
        echo "Refusing to stack a mount over $target" >&2
        exit 1
    fi
done
if ! (cd / && sha256sum -c "$STATE/TARGET-SHA256SUMS"); then
 echo "Target QML no longer matches the captured files; refusing to mount." >&2
 exit 1
fi

: > "$RUN_MARKER"
mounted=''
rollback_partial() {
    status=0
    for target in $mounted; do
        if mount_present "$target" && ! umount "$target"; then
            echo "Could not roll back mount: $target" >&2
            status=1
        fi
    done
    return "$status"
}

for rel in $FILES; do
    source=$STATE/overlay/$rel
    target=/$rel
    if ! mount --bind "$source" "$target"; then
        echo "Bind mount failed: $target" >&2
        if rollback_partial; then
            rm -f "$RUN_MARKER"
        else
            echo "Partial mount state retained for recovery; run unmount.sh as root." >&2
        fi
        exit 1
    fi
    mounted="$target $mounted"
done
